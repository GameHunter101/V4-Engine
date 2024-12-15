use std::fmt::Debug;
use wgpu::{Device, Queue, RenderPass};
use winit_input_helper::WinitInputHelper;

use crate::EngineDetails;

use super::{actions::ActionQueue, entity::EntityId};

pub type ComponentId = u32;

pub type Component = Box<dyn ComponentSystem + Send>;

#[allow(unused)]
#[async_trait::async_trait]
pub trait ComponentSystem: ComponentDetails + Debug {
    fn initialize(&mut self, device: &Device) {}

    async fn update(
        &mut self,
        device: &Device,
        queue: &Queue,
        input_manager: &WinitInputHelper,
        other_components: &[&mut Component],
        active_camera_id: Option<ComponentId>,
        engine_details: &EngineDetails,
    ) -> ActionQueue {
        Vec::new()
    }

    fn render(&self, device: &Device, queue: &Queue, render_pass: &mut RenderPass) {}

}

pub trait ComponentDetails {
    fn id(&self) -> ComponentId;

    fn set_id(&mut self, new_id: ComponentId);

    fn is_initialized(&self) -> bool;

    fn parent_entity_id(&self) -> EntityId;

    fn set_parent_entity(&mut self, parent_id: EntityId);

    fn is_enabled(&self) -> bool;

    fn set_enabled_state(&mut self, enabled_state: bool);

    /// Lower order means it is rendered earlier
    fn rendering_order(&self) -> i32 {
        0
    }
}
