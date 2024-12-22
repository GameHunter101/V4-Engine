use v4::{
    component,
    ecs::{
        component::{ComponentDetails, ComponentId, ComponentSystem},
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
    let toggle_component = ToggleComponent::new(text_component.id());
    let _text_entity_id = scene.create_entity(
        None,
        vec![Box::new(text_component), Box::new(toggle_component)],
        None,
        true,
    );

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
            id: std::sync::OnceLock::new(),
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

#[derive(Debug)]
#[component]
struct ToggleComponent {
    text_component: ComponentId,
}

impl ToggleComponent {
    fn new(text_component: ComponentId) -> Self {
        Self {
            text_component,
            parent_entity_id: 0,
            is_initialized: false,
            is_enabled: true,
            id: std::sync::OnceLock::new(),
        }
    }
}

#[async_trait::async_trait]
impl ComponentSystem for ToggleComponent {
    async fn update(
        &mut self,
        _device: &wgpu::Device,
        _queue: &wgpu::Queue,
        input_manager: &winit_input_helper::WinitInputHelper,
        _other_components: &[&mut v4::ecs::component::Component],
        _active_camera_id: Option<ComponentId>,
        _engine_details: &v4::EngineDetails,
    ) -> v4::ecs::actions::ActionQueue {
        let text = input_manager.text();

        if !text.is_empty()
            && text
                .iter()
                .any(|c| *c == winit::keyboard::Key::Named(winit::keyboard::NamedKey::Escape))
        {
            println!("escape pressed");
            return vec![Box::new(v4::builtin_actions::ComponentToggleAction(
                self.text_component,
                None,
            ))];
        }
        Vec::new()
    }
}
