use v4::{
    builtin_actions::UpdateTextComponentAction,
    component,
    ecs::component::{ComponentDetails, ComponentId, ComponentSystem, UpdateParams},
    engine_management::font_management::{TextComponentProperties, TextDisplayInfo},
    scene, V4,
};

#[component]
struct TextComponent {
    text: String,
}

#[component]
struct ToggleComponent {
    text_component: ComponentId,
}

#[tokio::main]
pub async fn main() {
    let mut engine = V4::builder()
        .clear_color(wgpu::Color::BLACK)
        .window_settings(500, 500, "V4 Font Render Example", None)
        .build()
        .await;

    scene! {
        "main" = {
            components: [
                TextComponent(text: "something".to_string(), ident: "text"),
                ToggleComponent(text_component: ident("text")),
            ]
        },
    };

    engine.attach_scene(scene);

    engine.main_loop().await;
}

impl ComponentSystem for TextComponent {
    fn initialize(&mut self, _device: &wgpu::Device) -> v4::ecs::actions::ActionQueue {
        self.is_initialized = true;

        vec![Box::new(v4::builtin_actions::RegisterUiComponentAction {
            component_id: self.id(),
            text_component_properties: Some(TextComponentProperties {
                text: self.text.clone(),
                text_attributes: glyphon::Attrs::new()
                    .color(glyphon::Color::rgb(255, 0, 0))
                    .family(glyphon::Family::Name("AntiquarianScribeW01-Reg"))
                    .into(),
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
            }),
        })]
    }

    fn update(
        &mut self,
        UpdateParams { input_manager, .. }: UpdateParams<'_, '_>,
    ) -> v4::ecs::actions::ActionQueue {
        let text = input_manager.text();

        if input_manager.key_held(winit::keyboard::KeyCode::Backspace) {
            self.text.pop();
            std::thread::sleep(std::time::Duration::from_millis(100));
            return vec![Box::new(UpdateTextComponentAction {
                component_id: self.id(),
                text: Some(self.text.clone()),
                text_attributes: None,
                text_metrics: None,
                text_display_info: None,
            })];
        }

        if !text.is_empty() {
            for key in text {
                if let winit::keyboard::Key::Character(char) = key {
                    self.text.push_str(char);
                }
                if let winit::keyboard::Key::Named(named) = key {
                    if *named == winit::keyboard::NamedKey::Space {
                        self.text.push(' ');
                    }
                }
            }
            return vec![Box::new(UpdateTextComponentAction {
                component_id: self.id(),
                text: Some(self.text.clone()),
                text_attributes: None,
                text_metrics: None,
                text_display_info: None,
            })];
        }
        Vec::new()
    }
}

impl ComponentSystem for ToggleComponent {
    fn update(
        &mut self,
        UpdateParams { input_manager, .. }: UpdateParams<'_, '_>,
    ) -> v4::ecs::actions::ActionQueue {
        let text = input_manager.text();

        if !text.is_empty()
            && text
                .iter()
                .any(|c| *c == winit::keyboard::Key::Named(winit::keyboard::NamedKey::Escape))
        {
            return vec![Box::new(v4::builtin_actions::ComponentToggleAction(
                self.text_component,
                None,
            ))];
        }
        Vec::new()
    }
}
