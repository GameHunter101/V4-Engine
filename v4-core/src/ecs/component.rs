use downcast_rs::{impl_downcast, DowncastSync};
use egui::Context;
use super::scene::Id;
use std::{collections::HashMap, fmt::Debug, ops::Range};
use wgpu::{CommandEncoder, Device, Queue, RenderPass};
use winit_input_helper::WinitInputHelper;

use crate::EngineDetails;

use super::{
    actions::ActionQueue,
    compute::Compute,
    entity::Entity,
    material::Material,
    scene::WorkloadOutput,
};

pub type Component = Box<dyn ComponentSystem>;

pub struct UpdateParams<'a: 'b, 'b> {
    pub device: &'a Device,
    pub queue: &'a Queue,
    pub input_manager: &'a WinitInputHelper,
    pub other_components: &'a mut[&'b mut Component],
    pub computes: &'a mut [Compute],
    pub materials: &'a mut HashMap<Id, Material>,
    pub engine_details: &'a EngineDetails,
    pub workload_outputs: &'a HashMap<Id, Vec<WorkloadOutput>>,
    pub entities: &'a HashMap<Id, Entity>,
    pub entity_component_groupings: HashMap<Id, Range<usize>>,
    pub active_camera: Option<Id>,
}

#[allow(unused)]
pub trait ComponentSystem: ComponentDetails + Debug + DowncastSync + Send + Sync {
    fn initialize(&mut self, device: &Device) -> ActionQueue {
        self.set_initialized();
        Vec::new()
    }

    fn update(&mut self, params: UpdateParams<'_, '_>) -> ActionQueue {
        Vec::new()
    }

    fn render(
        &self,
        device: &Device,
        queue: &Queue,
        render_pass: &mut RenderPass,
        other_components: &[&Component],
    ) {
    }

    fn command_encoder_operations(
        &self,
        device: &Device,
        queue: &Queue,
        encoder: &mut CommandEncoder,
        other_components: &[&Component],
        materials: &HashMap<Id, Material>,
        computes: &[Compute],
    ) {
    }

    fn ui_render(&mut self, ctx: &Context) {}
}
impl_downcast!(sync ComponentSystem);

pub trait ComponentDetails {
    fn id(&self) -> Id;

    fn is_initialized(&self) -> bool;

    fn set_initialized(&mut self);

    fn parent_entity_id(&self) -> Id;

    fn set_parent_entity(&mut self, parent_id: Id);

    fn is_enabled(&self) -> bool;

    fn set_enabled_state(&mut self, enabled_state: bool);

    /// Lower order means it is rendered earlier
    fn rendering_order(&self) -> i32 {
        0
    }
}
