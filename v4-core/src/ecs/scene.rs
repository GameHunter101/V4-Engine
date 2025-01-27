use std::{
    any::Any,
    collections::{HashMap, HashSet},
    fmt::Debug,
    future::Future,
    pin::Pin,
};

use crossbeam_channel::{Receiver, Sender};
use wgpu::{BindGroup, Buffer, Device, Queue};
use winit_input_helper::WinitInputHelper;

use crate::{engine_management::engine_action::EngineAction, EngineDetails};

use super::{
    actions::ActionQueue,
    component::{Component, ComponentId},
    entity::{Entity, EntityId},
    material::{Material, MaterialAttachment, MaterialId},
    pipeline::PipelineId,
};

static mut SCENE_COUNT: usize = 0;

pub struct Scene {
    scene_index: usize,
    components: Vec<Component>,
    entities: HashMap<EntityId, Entity>,
    ui_components: Vec<ComponentId>,
    materials: Vec<Material>,
    pipeline_to_corresponding_materials: HashMap<PipelineId, Vec<MaterialId>>,
    total_entities_created: u32,
    workload_sender: Option<Sender<WorkloadPacket>>,
    workload_output_receiver: Option<Receiver<(ComponentId, WorkloadOutput)>>,
    workload_outputs: WorkloadOutputCollection,
    engine_action_sender: Option<Sender<Box<dyn EngineAction>>>,
    pub new_pipelines_needed: bool,
    active_camera: Option<ComponentId>,
    active_camera_buffer: Option<Buffer>,
    active_camera_bind_group: Option<BindGroup>,
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
            ui_components: Vec::new(),
            materials: Vec::new(),
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
        }
    }
}

impl Scene {
    pub async fn initialize(
        &mut self,
        device: &Device,
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
            .flat_map(|component| component.initialize(device))
            .collect();
        self.execute_action_queue(action_queue).await;

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
        let all_components = &mut self.components;
        let actions: Vec<_> = (0..all_components.len())
            .map(|i| {
                if !all_components[i].is_enabled() {
                    return Vec::new();
                }
                let (_, outputs) = async_scoped::TokioScope::scope_and_block(|scope| {
                    let (components_before, components_after_and_this) =
                        all_components.split_at_mut(i);
                    let workload_outputs = &self.workload_outputs;
                    scope.spawn(async move {
                        if let Some((component, components_after)) =
                            components_after_and_this.split_first_mut()
                        {
                            let chain: Vec<&mut Component> = components_before
                                .iter_mut()
                                .chain(components_after.iter_mut())
                                .collect();
                            component
                                .update(
                                    device,
                                    queue,
                                    input_manager,
                                    &chain,
                                    engine_details,
                                    workload_outputs,
                                )
                                .await
                        } else {
                            Vec::new()
                        }
                    })
                });
                outputs
            })
            .collect();
        let action_queue: ActionQueue = actions
            .into_iter()
            .flatten()
            .flat_map(|actions| actions.unwrap_or_default())
            .collect();

        self.execute_action_queue(action_queue).await;
    }

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
        pipeline_id: PipelineId,
        attachments: Vec<MaterialAttachment>,
    ) -> MaterialId {
        let new_material = Material::new(
            self.materials.len(),
            pipeline_id.clone(),
            attachments,
        );

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

        let id = new_material.id();

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

    pub fn get_components_per_material(&self) -> HashMap<MaterialId, Vec<&Component>> {
        self.materials
            .iter()
            .map(|material| {
                let components: Vec<&Component> = self
                    .entities
                    .values()
                    .flat_map(|ent| {
                        if let Some(mat) = ent.active_material() {
                            if mat == material.id() {
                                Some(ent.id())
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    })
                    .flat_map(|ent_id| {
                        let mut components = self
                            .components
                            .iter()
                            .filter(|comp| comp.parent_entity_id() == ent_id)
                            .collect::<Vec<&Component>>();
                        components.sort_by_key(|a| a.rendering_order());
                        components
                    })
                    .collect();
                (material.id(), components)
            })
            .collect()
    }

    pub fn create_entity(
        &mut self,
        parent: Option<EntityId>,
        components: Vec<Component>,
        material: Option<MaterialId>,
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

        self.entities.insert(id, entity);
        self.total_entities_created += 1;

        for component in components {
            let insert_index = self
                .components
                .iter()
                .position(|comp| comp.rendering_order() >= component.rendering_order())
                .unwrap_or(self.components.len());
            self.components.insert(insert_index, component);
            self.components
                .get_mut(insert_index)
                .unwrap()
                .set_parent_entity(id);
        }

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
            .find(|&comp| comp.id() == component_id)
    }

    pub fn get_component_mut(&mut self, component_id: ComponentId) -> Option<&mut Component> {
        self.components
            .iter_mut()
            .find(|comp| comp.id() == component_id)
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

    pub async fn execute_action_queue(&mut self, action_queue: ActionQueue) {
        for action in action_queue {
            action.execute_async(self).await;
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
}
