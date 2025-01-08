use winit::window::Window;

use crate::ecs::component::ComponentId;

use super::font_management::{FontState, TextAttributes, TextComponentProperties, TextDisplayInfo};

pub struct V4Mutable<'a> {
    pub window: &'a Window,
    pub active_scene: &'a mut usize,
    pub initialized_scene: &'a mut bool,
    pub font_state: &'a mut FontState,
}

pub trait EngineAction: Send + Sync {
    fn execute(self: Box<Self>, engine: V4Mutable);
}

pub struct CreateTextBufferEngineAction {
    pub component_id: ComponentId,
    pub text_component_properties: TextComponentProperties,
}

impl EngineAction for CreateTextBufferEngineAction {
    fn execute(self: Box<Self>, engine: V4Mutable) {
        engine.font_state.create_text_buffer(
            self.component_id,
            &self.text_component_properties.text,
            self.text_component_properties.text_attributes,
            self.text_component_properties.text_metrics,
            self.text_component_properties.text_display_info,
        );
    }
}

pub struct UpdateTextBufferEngineAction {
    pub component_id: ComponentId,
    pub text: Option<String>,
    pub text_attributes: Option<TextAttributes>,
    pub text_metrics: Option<glyphon::Metrics>,
    pub text_display_info: Option<TextDisplayInfo>,
}

impl EngineAction for UpdateTextBufferEngineAction {
    fn execute(self: Box<Self>, engine: V4Mutable) {
        engine.font_state.update_text_buffer(
            self.component_id,
            self.text,
            self.text_attributes,
            self.text_metrics,
            self.text_display_info,
        );
    }
}
