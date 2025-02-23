use std::{
    any::Any,
    collections::{HashMap, HashSet},
    fmt::Debug,
    future::Future,
    ops::Range,
    pin::Pin,
};

use crossbeam_channel::{Receiver, Sender};
use wgpu::{BindGroup, Buffer, Device, Queue};
use winit_input_helper::WinitInputHelper;

use crate::{
    engine_management::{
        engine_action::EngineAction,
        pipeline::{PipelineId, PipelineShader},
    },
    EngineDetails,
};

use super::{
    actions::ActionQueue,
    component::{Component, ComponentDetails, ComponentId, ComponentSystem},
    compute::Compute,
    entity::{Entity, EntityId},
    material::{Material, ShaderAttachment},
};

static mut SCENE_COUNT: usize = 0;

pub struct Scene {
    scene_index: usize,
    components: Vec<Component>,
    entities: HashMap<EntityId, Entity>,
    entity_component_groupings: HashMap<EntityId, Range<usize>>,
    ui_components: Vec<ComponentId>,
    materials: Vec<Material>,
    screen_space_materials: Vec<ComponentId>,
    pipeline_to_corresponding_materials: HashMap<PipelineId, Vec<ComponentId>>,
    total_entities_created: u32,
    workload_sender: Option<Sender<WorkloadPacket>>,
    workload_output_receiver: Option<Receiver<(ComponentId, WorkloadOutput)>>,
    workload_outputs: WorkloadOutputCollection,
    engine_action_sender: Option<Sender<Box<dyn EngineAction>>>,
    pub new_pipelines_needed: bool,
    active_camera: Option<ComponentId>,
    active_camera_buffer: Option<Buffer>,
    active_camera_bind_group: Option<BindGroup>,
    computes: Vec<Compute>,
}

impl Debug for Scene {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Scene")
            .field("components", &self.components.len())
            .finish()
    }
}

pub type WorkloadOutput = Box<dyn Any + Send + Sync>;
pub type WorkloadOutputCollection = HashMap<ComponentId, Vec<WorkloadOutput>>;
pub type Workload = Pin<Box<dyn Future<Output = WorkloadOutput> + Send>>;

pub struct WorkloadPacket {
    pub scene_index: usize,
    pub component_id: ComponentId,
    pub workload: Workload,
}

impl Default for Scene {
    fn default() -> Self {
        let scene_index = unsafe {
            SCENE_COUNT += 1;
            SCENE_COUNT
        };

        Scene {
            scene_index,
            components: Vec::new(),
            entities: HashMap::new(),
            entity_component_groupings: HashMap::new(),
            ui_components: Vec::new(),
            materials: Vec::new(),
            screen_space_materials: Vec::new(),
            pipeline_to_corresponding_materials: HashMap::new(),
            total_entities_created: 0,
            workload_sender: None,
            workload_output_receiver: None,
            engine_action_sender: None,
            workload_outputs: HashMap::new(),
            new_pipelines_needed: false,
            active_camera: None,
            active_camera_buffer: None,
            active_camera_bind_group: None,
            computes: Vec::new(),
        }
    }
}

impl Scene {
    pub async fn initialize(
        &mut self,
        device: &Device,
        queue: &Queue,
        workload_sender: Sender<WorkloadPacket>,
        workload_output_receiver: Receiver<(ComponentId, WorkloadOutput)>,
        engine_action_sender: Sender<Box<dyn EngineAction>>,
    ) {
        self.workload_sender = Some(workload_sender);
        self.workload_output_receiver = Some(workload_output_receiver);
        self.engine_action_sender = Some(engine_action_sender);

        let action_queue: ActionQueue = self
            .components
            .iter_mut()
            .flat_map(|comp| comp.initialize(device))
            .collect();
        self.execute_action_queue(action_queue, device, queue).await;

        for material in &mut self.materials {
            material.initialize(device);
        }
    }

    pub async fn update(
        &mut self,
        device: &Device,
        queue: &Queue,
        input_manager: &WinitInputHelper,
        engine_details: &EngineDetails,
    ) {
        while let Ok((component_id, workload_output)) = self
            .workload_output_receiver
            .as_ref()
            .expect("Failed to initialize workload output receiver.")
            .try_recv()
        {
            if let Some(outputs) = self.workload_outputs.get_mut(&component_id) {
                outputs.push(workload_output);
            } else {
                self.workload_outputs
                    .insert(component_id, vec![workload_output]);
            }
        }

        let active_camera = self.active_camera();
        let entities = &self.entities;
        let all_components: &mut Vec<Component> = &mut self.components;
        let all_materials: Vec<&mut Material> = self.materials.iter_mut().collect();

        let actions: Vec<_> = (0..all_components.len())
            .map(|i| {
                if !all_components[i].is_enabled() {
                    return Vec::new();
                }
                let (previous_components, rest_of_components) = all_components.split_at_mut(i);
                let Some((current_component, later_components)) =
                    rest_of_components.split_first_mut()
                else {
                    return Vec::new();
                };
                let other_components: Vec<&mut Component> = previous_components
                    .iter_mut()
                    .chain(later_components.iter_mut())
                    .collect();

                let mut entity_component_groupings = self.entity_component_groupings.clone();
                for grouping in entity_component_groupings.values_mut() {
                    if grouping.start > i {
                        grouping.start -= 1;
                    }
                    if grouping.end > i {
                        grouping.end -= 1;
                    }
                }
                let workload_outputs = &self.workload_outputs;

                let (_, outputs) = async_scoped::TokioScope::scope_and_block(|scope| {
                    scope.spawn(async {
                        current_component
                            .update(
                                device,
                                queue,
                                input_manager,
                                &other_components,
                                &all_materials,
                                engine_details,
                                workload_outputs,
                                entities,
                                entity_component_groupings,
                                active_camera,
                            )
                            .await
                    });
                });
                outputs
            })
            .collect();

        let action_queue: ActionQueue = actions
            .into_iter()
            .flatten()
            .flat_map(|queue| queue.unwrap_or_default())
            .collect();

        self.execute_action_queue(action_queue, device, queue).await;
    }

    pub async fn update_materials(
        &mut self,
        device: &Device,
        queue: &Queue,
        input_manager: &WinitInputHelper,
        engine_details: &EngineDetails,
    ) {
        let active_camera = self.active_camera();
        let entities = &self.entities;
        let all_components: Vec<&mut Component> = self.components.iter_mut().collect();
        let all_materials: &mut Vec<Material> = &mut self.materials;

        let workload_outputs = &self.workload_outputs;

        for i in 0..all_materials.len() {
            let (previous_materials, all_other_materials) = all_materials.split_at_mut(i);
            let (current_material, later_materials) =
                all_other_materials.split_first_mut().unwrap();

            let other_materials: Vec<&mut Material> = previous_materials
                .iter_mut()
                .chain(later_materials.iter_mut())
                .collect();

            let _ = async_scoped::TokioScope::scope_and_block(|scope| {
                let entity_component_groupings = self.entity_component_groupings.clone();
                scope.spawn(async {
                    current_material
                        .update(
                            device,
                            queue,
                            input_manager,
                            &all_components,
                            &other_materials,
                            engine_details,
                            workload_outputs,
                            entities,
                            entity_component_groupings,
                            active_camera,
                        )
                        .await
                });
            });
        }
    }

    pub async fn update_computes(&mut self, device: &Device, queue: &Queue) {}

    pub async fn attach_workload(&mut self, component_id: ComponentId, workload: Workload) {
        if let Some(sender) = &self.workload_sender {
            sender
                .try_send(WorkloadPacket {
                    scene_index: self.scene_index,
                    component_id,
                    workload,
                })
                .expect("Failed to send workload");
        }
    }

    pub async fn free_workload_output(
        &mut self,
        component_id: ComponentId,
        workload_output_index: usize,
    ) {
        let outputs = self
            .workload_outputs
            .get_mut(&component_id)
            .expect("Failed to get workloads assigned to the given component ID.");
        if !outputs.is_empty() {
            outputs.remove(workload_output_index);
        }
    }

    pub fn create_material(
        &mut self,
        mut pipeline_id: PipelineId,
        attachments: Vec<ShaderAttachment>,
        entities_attached: Vec<EntityId>,
    ) -> ComponentId {
        let id = self.materials.len() as u32;

        if pipeline_id.is_screen_space {
            const ATTRIBUTES: &[wgpu::VertexAttribute] =
                &wgpu::vertex_attr_array![0=>Float32x3, 1=>Float32x2];
            pipeline_id.vertex_layouts = vec![wgpu::VertexBufferLayout {
                array_stride: 4 * 5,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: ATTRIBUTES,
            }];
            pipeline_id.vertex_shader = PipelineShader::Raw(std::borrow::Cow::Owned(
                "
struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) tex_coords: vec2<f32>,
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
}

@vertex
fn main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    output.position = vec4f(input.position, 1.0);
    output.tex_coords = input.tex_coords;
    return output;
}
"
                .to_string(),
            ));
            pipeline_id.spirv_vertex_shader = false;
            self.screen_space_materials.push(id);
        }

        let new_material = Material::new(id, pipeline_id.clone(), attachments, entities_attached);

        if let Some(entry) = self
            .pipeline_to_corresponding_materials
            .get_mut(&pipeline_id)
        {
            entry.push(new_material.id());
        } else {
            self.pipeline_to_corresponding_materials
                .insert(pipeline_id, vec![new_material.id()]);
            self.new_pipelines_needed = true;
        }

        self.materials.push(new_material);

        id
    }

    pub fn get_pipeline_ids(&self) -> Vec<&PipelineId> {
        self.pipeline_to_corresponding_materials.keys().collect()
    }

    pub fn get_pipeline_materials(&self, pipeline_id: &PipelineId) -> Vec<&Material> {
        let material_ids = self.pipeline_to_corresponding_materials.get(pipeline_id);
        match material_ids {
            Some(material_ids) => self
                .materials
                .iter()
                .filter(|mat| material_ids.contains(&mat.id()))
                .collect(),
            None => Vec::new(),
        }
    }

    pub fn get_components_per_material(&self) -> HashMap<ComponentId, Vec<&Component>> {
        self.materials
            .iter()
            .flat_map(|material| {
                if material.pipeline_id().is_screen_space {
                    return None;
                }
                let components: Vec<&Component> = self
                    .entities
                    .iter()
                    .flat_map(|(id, ent)| {
                        if let Some(mat) = ent.active_material() {
                            if mat == material.id() {
                                Some(&self.components[self.entity_component_groupings[id].clone()])
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    })
                    .flatten()
                    .collect();
                Some((material.id(), components))
            })
            .collect()
    }

    pub fn create_entity(
        &mut self,
        parent: Option<EntityId>,
        mut components: Vec<Component>,
        material: Option<ComponentId>,
        is_enabled: bool,
    ) -> EntityId {
        let entity = Entity::new(
            self.total_entities_created + 1,
            Vec::new(),
            parent.unwrap_or(0),
            is_enabled,
            material,
        );
        let id = entity.id();

        if let Some(parent) = parent {
            self.entities.get_mut(&parent).unwrap().push_child(id);
        }

        self.entities.insert(id, entity);
        self.total_entities_created += 1;

        components
            .iter_mut()
            .for_each(|comp| comp.set_parent_entity(id));
        components.sort_by_key(|a| a.rendering_order());
        self.entity_component_groupings.insert(
            id,
            self.components.len()..(self.components.len() + components.len()),
        );
        self.components.append(&mut components);

        id
    }

    pub fn get_entity(&self, entity_id: EntityId) -> Option<&Entity> {
        self.entities.get(&entity_id)
    }

    pub fn get_entity_mut(&mut self, entity_id: EntityId) -> Option<&mut Entity> {
        self.entities.get_mut(&entity_id)
    }

    pub fn get_component(&self, component_id: ComponentId) -> Option<&Component> {
        self.components
            .iter()
            .find(|comp| comp.id() == component_id)
    }

    pub fn get_component_mut(&mut self, component_id: ComponentId) -> Option<&mut Component> {
        self.components
            .iter_mut()
            .find(|comp| comp.id() == component_id)
    }

    pub fn get_material(&self, material_id: ComponentId) -> Option<&Material> {
        self.materials.get(material_id as usize)
    }

    pub fn enabled_ui_components(&self) -> HashSet<ComponentId> {
        self.components
            .iter()
            .filter_map(|comp| {
                if comp.is_enabled() && self.ui_components.contains(&comp.id()) {
                    Some(comp.id())
                } else {
                    None
                }
            })
            .collect()
    }

    pub async fn execute_action_queue(
        &mut self,
        action_queue: ActionQueue,
        device: &Device,
        queue: &Queue,
    ) {
        for action in action_queue {
            action.execute_async(self, device, queue).await;
        }
    }

    pub fn register_ui_component(&mut self, component_id: ComponentId) {
        self.ui_components.push(component_id);
    }

    pub fn send_engine_action(&self, action: Box<dyn EngineAction>) {
        if let Some(engine_action_sender) = &self.engine_action_sender {
            engine_action_sender
                .try_send(action)
                .expect("Failed to send engine action.");
        }
    }

    pub fn set_active_camera(&mut self, camera: Option<ComponentId>) {
        self.active_camera = camera;
    }

    pub fn active_camera(&self) -> Option<u32> {
        self.active_camera
    }

    pub fn active_camera_buffer(&self) -> Option<&Buffer> {
        self.active_camera_buffer.as_ref()
    }

    pub fn active_camera_bind_group(&self) -> Option<&BindGroup> {
        self.active_camera_bind_group.as_ref()
    }

    pub fn set_active_camera_buffer(&mut self, active_camera_buffer: Option<Buffer>) {
        self.active_camera_buffer = active_camera_buffer;
    }

    pub fn set_active_camera_bind_group(&mut self, active_camera_bind_group: Option<BindGroup>) {
        self.active_camera_bind_group = active_camera_bind_group;
    }

    pub fn scene_index(&self) -> usize {
        self.scene_index
    }

    pub fn screen_space_materials(&self) -> &[ComponentId] {
        &self.screen_space_materials
    }

    pub fn all_components(&self) -> Vec<&Component> {
        self.components.iter().collect::<Vec<_>>()
    }
}
