use std::{collections::HashMap, fmt::Debug};
use wgpu::{Device, Queue, RenderPass};
use winit_input_helper::WinitInputHelper;

use crate::EngineDetails;

use super::{actions::ActionQueue, entity::EntityId, scene::WorkloadOutput};

pub type ComponentId = u32;

pub type Component = Box<dyn ComponentSystem + Send + Sync>;

#[allow(unused)]
#[async_trait::async_trait]
pub trait ComponentSystem: ComponentDetails + Debug {
    fn initialize(&mut self, device: &Device) -> ActionQueue {
        self.set_initialized();
        Vec::new()
    }

    #[allow(clippy::too_many_arguments)]
    async fn update(
        &mut self,
        device: &Device,
        queue: &Queue,
        input_manager: &WinitInputHelper,
        other_components: &[&mut Component],
        engine_details: &EngineDetails,
        workload_outputs: &HashMap<ComponentId, Vec<WorkloadOutput>>,
    ) -> ActionQueue {
        Vec::new()
    }

    fn render(&self, device: &Device, queue: &Queue, render_pass: &mut RenderPass) {}
}

pub trait ComponentDetails {
    fn id(&self) -> ComponentId;

    fn is_initialized(&self) -> bool;

    fn set_initialized(&mut self);

    fn parent_entity_id(&self) -> EntityId;

    fn set_parent_entity(&mut self, parent_id: EntityId);

    fn is_enabled(&self) -> bool;

    fn set_enabled_state(&mut self, enabled_state: bool);

    /// Lower order means it is rendered earlier
    fn rendering_order(&self) -> i32 {
        0
    }
}
