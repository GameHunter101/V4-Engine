use std::{
    any::Any,
    collections::{HashMap, HashSet},
    fmt::Debug,
    future::Future,
    ops::Range,
    pin::Pin,
};

use crossbeam_channel::{Receiver, Sender};
use uuid::Uuid;
use wgpu::{BindGroup, Buffer, Device, Queue, RenderPipeline};
use winit_input_helper::WinitInputHelper;

use thiserror::Error;

use crate::{
    EngineDetails,
    engine_management::{engine_action::EngineAction, pipeline::PipelineManager},
};

use super::{
    actions::ActionQueue,
    component::{Component, ComponentDetails, ComponentSystem},
    compute::Compute,
    entity::Entity,
    material::{Material, ShaderAttachment},
};

static mut SCENE_COUNT: usize = 0;

pub type Id = Uuid;

#[derive(Error, Debug)]
pub enum SceneError {
    #[error("Workload receiver has not been initialized")]
    WorkloadRecvInitError,
    #[error("Could not send workload packet over to worker thread: {0}")]
    WorkloadSendError(#[from] crossbeam_channel::TrySendError<WorkloadPacket>),
    #[error("Could not receive workload result from worker thread")]
    WorkloadRecvError,
    #[error("The specified material ID ({0}) is invalid")]
    InvalidMaterialId(Id),
    #[error("Failed to send engine action: {0}")]
    SendEngineActionFailure(#[from] crossbeam_channel::TrySendError<Box<dyn EngineAction>>),
    #[error("The specified entity ID ({0}) is invalid")]
    InvalidEntityId(Id),
    #[error("The specified pipeline ID ({0}) is invalid")]
    InvalidPipelineId(Id),
}

pub struct Scene {
    scene_index: usize,
    components: Vec<Component>,
    entities: HashMap<Id, Entity>,
    entity_component_groupings: HashMap<Id, Range<usize>>,
    ui_components: Vec<Id>,
    materials: HashMap<Id, Material>,
    pipeline_manager: PipelineManager,
    pipeline_to_corresponding_materials: HashMap<Id, Vec<Id>>,
    workload_sender: Option<Sender<WorkloadPacket>>,
    workload_output_receiver: Option<Receiver<(Id, WorkloadOutput)>>,
    workload_outputs: WorkloadOutputCollection,
    engine_action_sender: Option<Sender<Box<dyn EngineAction>>>,
    pub new_pipelines_needed: bool,
    active_camera: Option<Id>,
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
pub type WorkloadOutputCollection = HashMap<Id, Vec<WorkloadOutput>>;
pub type Workload = Pin<Box<dyn Future<Output = WorkloadOutput> + Send>>;

pub struct WorkloadPacket {
    pub scene_index: usize,
    pub component_id: Id,
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
            materials: HashMap::new(),
            pipeline_manager: PipelineManager::default(),
            pipeline_to_corresponding_materials: HashMap::new(),
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
    pub fn initialize(
        &mut self,
        device: &Device,
        workload_sender: Sender<WorkloadPacket>,
        workload_output_receiver: Receiver<(Id, WorkloadOutput)>,
        engine_action_sender: Sender<Box<dyn EngineAction>>,
    ) -> ActionQueue {
        self.workload_sender = Some(workload_sender);
        self.workload_output_receiver = Some(workload_output_receiver);
        self.engine_action_sender = Some(engine_action_sender);

        self.initialize_components(device)
    }

    fn initialize_components(&mut self, device: &Device) -> ActionQueue {
        let comp_action_queue: ActionQueue = self
            .components
            .iter_mut()
            .filter(|comp| !comp.is_initialized())
            .flat_map(|comp| comp.initialize(device))
            .collect();

        let mat_action_queue: ActionQueue = self
            .materials
            .values_mut()
            .filter(|mat| !mat.is_initialized())
            .flat_map(|mat| mat.initialize(device))
            .collect();

        let compute_action_queue: ActionQueue = self
            .computes
            .iter_mut()
            .filter(|compute| !compute.is_initialized())
            .flat_map(|compute| compute.initialize(device))
            .collect();

        comp_action_queue
            .into_iter()
            .chain(mat_action_queue)
            .chain(compute_action_queue)
            .collect()
    }

    pub fn update(
        &mut self,
        device: &Device,
        queue: &Queue,
        input_manager: &WinitInputHelper,
        engine_details: &EngineDetails,
    ) -> Result<ActionQueue, SceneError> {
        let Some(workload_recv) = self.workload_output_receiver.as_ref() else {
            return Err(SceneError::WorkloadRecvInitError);
        };

        while let Ok((component_id, workload_output)) = workload_recv.try_recv() {
            if let Some(outputs) = self.workload_outputs.get_mut(&component_id) {
                outputs.push(workload_output);
            } else {
                self.workload_outputs
                    .insert(component_id, vec![workload_output]);
            }
        }

        let active_camera = self.active_camera();
        let entities = &self.entities;

        let enabled_components: Vec<usize> = (0..self.components.len())
            .filter(|i| self.is_component_enabled(&*self.components[*i]))
            .collect();

        let all_components: &mut Vec<Component> = &mut self.components;

        Ok(enabled_components
            .into_iter()
            .flat_map(|i| {
                let (previous_components, rest_of_components) = all_components.split_at_mut(i);
                let Some((current_component, later_components)) =
                    rest_of_components.split_first_mut()
                else {
                    return Vec::new();
                };
                let mut other_components: Vec<&mut Component> = previous_components
                    .iter_mut()
                    .chain(later_components.iter_mut())
                    .collect::<Vec<_>>();

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

                current_component.update(super::component::UpdateParams {
                    device,
                    queue,
                    input_manager,
                    other_components: &mut other_components,
                    computes: &mut self.computes,
                    materials: &mut self.materials,
                    engine_details,
                    workload_outputs,
                    entities,
                    entity_component_groupings,
                    active_camera,
                })
            })
            .collect())
    }

    pub fn update_materials(
        &mut self,
        device: &Device,
        queue: &Queue,
        input_manager: &WinitInputHelper,
        engine_details: &EngineDetails,
    ) {
        let active_camera = self.active_camera();
        let entities = &self.entities;
        let entity_component_groupings: HashMap<Id, Range<usize>> = self
            .entity_component_groupings
            .clone()
            .into_iter()
            .filter(|(ent, _)| self.is_entity_enabled(*ent))
            .collect();

        let workload_outputs = &self.workload_outputs;

        let material_ids: Vec<Id> = self.materials.keys().copied().collect();
        for id in material_ids {
            let mut current_material = self.materials.remove(&id).unwrap();

            let mut all_components: Vec<&mut Component> =
                self.components.iter_mut().collect::<Vec<_>>();
            let entity_component_groupings = entity_component_groupings.clone();

            current_material.update(super::component::UpdateParams {
                device,
                queue,
                input_manager,
                other_components: &mut all_components,
                computes: &mut self.computes,
                materials: &mut self.materials,
                engine_details,
                workload_outputs,
                entities,
                entity_component_groupings,
                active_camera,
            });

            self.materials.insert(id, current_material);
        }
    }

    pub async fn attach_workload(
        &mut self,
        component_id: Id,
        workload: Workload,
    ) -> Result<(), SceneError> {
        if let Some(sender) = &self.workload_sender {
            sender.try_send(WorkloadPacket {
                scene_index: self.scene_index,
                component_id,
                workload,
            })?;
        }

        Ok(())
    }

    pub async fn free_workload_output(
        &mut self,
        component_id: Id,
        workload_output_index: usize,
    ) -> Result<(), SceneError> {
        let Some(outputs) = self.workload_outputs.get_mut(&component_id) else {
            return Err(SceneError::WorkloadRecvError);
        };

        if !outputs.is_empty() {
            outputs.remove(workload_output_index);
        }

        Ok(())
    }

    pub fn create_material(
        &mut self,
        device: &Device,
        pipeline: super::material::PipelineOptions,
        attachments: Vec<ShaderAttachment>,
        entities_attached: Vec<Id>,
        immediate_data: Vec<u8>,
        is_enabled: bool,
        id: Option<Id>,
    ) -> Result<Id, SceneError> {
        let id = id.unwrap_or(Id::new_v4());

        let (pipeline_descriptor, pipeline_id) = match pipeline {
            super::material::PipelineOptions::Descriptor(descriptor) => (descriptor, Id::new_v4()),
            super::material::PipelineOptions::Id(uuid) => {
                let Some((descriptor, _)) = self.pipeline_manager.get_pipeline(uuid) else {
                    return Err(SceneError::InvalidPipelineId(uuid));
                };

                (descriptor.clone(), uuid)
            }
        };

        let new_material = Material::new(
            id,
            pipeline_id,
            attachments,
            entities_attached,
            immediate_data,
            is_enabled,
        );

        if let Some(pipeline_materials) = self
            .pipeline_to_corresponding_materials
            .get_mut(&pipeline_id)
        {
            pipeline_materials.push(id);
        } else {
            self.pipeline_to_corresponding_materials
                .insert(pipeline_id, vec![new_material.id()]);
            self.pipeline_manager.create_render_pipeline(
                Some(pipeline_id),
                device,
                &pipeline_descriptor,
                new_material.bind_group_layout(),
            );
        }

        self.materials.insert(id, new_material);

        Ok(id)
    }

    pub fn get_pipeline_materials(&self, pipeline_id: Id) -> Vec<&Material> {
        self.pipeline_to_corresponding_materials
            .get(&pipeline_id)
            .cloned()
            .unwrap_or_default()
            .iter()
            .flat_map(|id| self.materials.get(id))
            .collect()
    }

    pub fn create_entity(
        &mut self,
        parent: Option<Id>,
        mut components: Vec<Component>,
        computes: Vec<Compute>,
        material: Option<Id>,
        is_enabled: bool,
        id: Option<Id>,
    ) -> Result<Id, SceneError> {
        let entity = Entity::new(
            id.unwrap_or(Id::new_v4()),
            Vec::new(),
            parent.unwrap_or_default(),
            is_enabled,
            material,
        );
        let id = entity.id();

        if let Some(parent) = parent {
            let Some(parent_entity) = self.entities.get_mut(&parent) else {
                return Err(SceneError::InvalidEntityId(parent));
            };
            parent_entity.push_child(id);
        }

        self.entities.insert(id, entity);

        components
            .iter_mut()
            .for_each(|comp| comp.set_parent_entity(id));
        components.sort_by_key(|a| a.rendering_order());
        self.entity_component_groupings.insert(
            id,
            self.components.len()..(self.components.len() + components.len()),
        );

        if let Some(mat_id) = material {
            let Some(material) = self.materials.get_mut(&mat_id) else {
                return Err(SceneError::InvalidMaterialId(mat_id));
            };
            material.attach_entity(id);
        }

        self.components.append(&mut components);
        self.computes.extend(computes);

        Ok(id)
    }

    pub fn get_entity(&self, entity_id: Id) -> Option<&Entity> {
        self.entities.get(&entity_id)
    }

    pub fn get_entity_mut(&mut self, entity_id: Id) -> Option<&mut Entity> {
        self.entities.get_mut(&entity_id)
    }

    pub fn get_component(&self, component_id: Id) -> Option<&Component> {
        self.components
            .iter()
            .find(|comp| comp.id() == component_id)
    }

    pub fn get_component_mut(&mut self, component_id: Id) -> Option<&mut Component> {
        self.components
            .iter_mut()
            .find(|comp| comp.id() == component_id)
    }

    pub fn get_material(&self, material_id: Id) -> Option<&Material> {
        self.materials.get(&material_id)
    }

    pub fn enabled_ui_components(&self) -> HashSet<Id> {
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
    ) -> Result<(), SceneError> {
        let actions = if action_queue.is_empty() {
            self.initialize_components(device)
        } else {
            action_queue
        };
        for action in actions {
            action.execute_async(self, device, queue).await?;
        }

        Ok(())
    }

    pub fn register_ui_component(&mut self, component_id: Id) {
        self.ui_components.push(component_id);
    }

    pub fn send_engine_action(&self, action: Box<dyn EngineAction>) -> Result<(), SceneError> {
        if let Some(engine_action_sender) = &self.engine_action_sender {
            engine_action_sender.try_send(action)?;
        }

        Ok(())
    }

    pub fn set_active_camera(&mut self, camera: Option<Id>) {
        self.active_camera = camera;
    }

    pub fn active_camera(&self) -> Option<Id> {
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

    pub fn screen_space_materials(&self) -> Vec<Id> {
        self.pipeline_to_corresponding_materials
            .iter()
            .flat_map(|(pipeline_id, materials)| {
                if let Some((descriptor, _)) = self.pipeline_manager.get_pipeline(*pipeline_id)
                    && descriptor.is_screen_space
                {
                    materials.clone()
                } else {
                    Vec::new()
                }
            })
            .collect()
    }

    pub fn all_components(&self) -> Vec<&Component> {
        self.components.iter().collect::<Vec<_>>()
    }

    pub fn all_components_mut(&mut self) -> Vec<&mut Component> {
        self.components.iter_mut().collect::<Vec<_>>()
    }

    pub fn computes(&self) -> &[Compute] {
        &self.computes
    }

    pub fn attach_compute(&mut self, compute: Compute) {
        self.computes.push(compute);
    }

    pub fn materials(&self) -> &HashMap<Id, Material> {
        &self.materials
    }

    pub fn is_entity_enabled(&self, entity: Id) -> bool {
        let mut predecessor_entity_id = entity;
        while !predecessor_entity_id.is_nil() {
            let ent = &self.entities[&predecessor_entity_id];
            if !ent.is_enabled() {
                return false;
            }
            predecessor_entity_id = ent.parent_entity_id();
        }

        true
    }
    pub fn is_component_enabled(&self, component: &dyn ComponentSystem) -> bool {
        if !component.is_enabled() {
            false
        } else {
            self.is_entity_enabled(component.parent_entity_id())
        }
    }

    pub fn pipeline_manager(&self) -> &PipelineManager {
        &self.pipeline_manager
    }
}
