use v4::{
    component,
    ecs::{
        component::{ComponentDetails, ComponentSystem},
        entity::EntityId,
        scene::{Scene, TextDisplayInfo},
    },
    V4,
};

#[tokio::main]
pub async fn main() {
    let mut engine = V4::builder()
        .clear_color(wgpu::Color::BLACK)
        .window_settings(500, 500, "V4 Font Render Example", None)
        .build()
        .await;

    let rendering_manager = engine.rendering_manager();

    let mut scene = Scene::new(
        rendering_manager.device(),
        rendering_manager.queue(),
        rendering_manager.format(),
    );

    let text_component = TextComponent::new("hi".to_string());
    let _text_entity_id = scene.create_entity(None, vec![Box::new(text_component)], None, true);

    engine.attach_scene(scene);

    engine.main_loop().await;
}

#[derive(Debug)]
#[component]
struct TextComponent {
    text: String,
}

impl TextComponent {
    fn new(text: String) -> Self {
        Self {
            text,
            parent_entity_id: EntityId::MAX,
            is_initialized: false,
            is_enabled: true,
        }
    }
}

impl ComponentSystem for TextComponent {
    fn initialize(&mut self, _device: &wgpu::Device) -> v4::ecs::actions::ActionQueue {
        self.is_initialized = true;

        vec![Box::new(v4::builtin_actions::RegisterUiComponentAction {
            component_id: self.id(),
            text: self.text.clone(),
            text_attributes: glyphon::Attrs::new().color(glyphon::Color::rgb(255, 0, 0)),
            text_metrics: glyphon::Metrics {
                font_size: 20.0,
                line_height: 40.0,
            },
            text_display_info: TextDisplayInfo {
                on_screen_width: 1000.0,
                on_screen_height: 1000.0,
                top_left_pos: [20.0; 2],
                scale: 1.0,
            },
            advanced_rendering: false,
        })]
    }
}
