use v4::{
    builtin_actions::UpdateTextComponentAction,
    component,
    ecs::{
        component::{ComponentDetails, ComponentId, ComponentSystem},
        scene::Scene,
    },
    engine_management::font_management::{TextComponentProperties, TextDisplayInfo},
    scene, V4,
};

#[derive(Debug)]
#[component]
struct TextComponent {
    text: String,
}

#[derive(Debug)]
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

    let ident = scene! {
        TextComponent(text: "hi".to_string(), thing: ident("hi"))
    };

    dbg!(ident);

    let mut scene = Scene::default();

    /* let test = scene! {
        "thing" {
            components: [
                TextComponent(text: "hi".to_string()),
                ToggleComponent(text_component: 0),
            ]
        }
    }; */

    /* let other_comp = TextComponent!(text: ("hi".to_string()));
    dbg!(other_comp); */

    let text_component = TextComponent::builder().text("hi".to_string()).build();
    let toggle_component = ToggleComponent::builder()
        .text_component(text_component.id())
        .build();
    let _text_entity_id = scene.create_entity(
        None,
        vec![Box::new(text_component), Box::new(toggle_component)],
        None,
        true,
    );

    engine.attach_scene(scene);

    engine.main_loop().await;
}

#[async_trait::async_trait]
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

    async fn update(
        &mut self,
        _device: &wgpu::Device,
        _queue: &wgpu::Queue,
        input_manager: &winit_input_helper::WinitInputHelper,
        _other_components: &[&mut v4::ecs::component::Component],
        _engine_details: &v4::EngineDetails,
        _workload_outputs: &std::collections::HashMap<
            v4::ecs::component::ComponentId,
            Vec<v4::ecs::scene::WorkloadOutput>,
        >,
    ) -> v4::ecs::actions::ActionQueue {
        let text = input_manager.text();

        if input_manager.key_held(winit::keyboard::KeyCode::Backspace) {
            self.text.pop();
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
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

#[async_trait::async_trait]
impl ComponentSystem for ToggleComponent {
    async fn update(
        &mut self,
        _device: &wgpu::Device,
        _queue: &wgpu::Queue,
        input_manager: &winit_input_helper::WinitInputHelper,
        _other_components: &[&mut v4::ecs::component::Component],
        _engine_details: &v4::EngineDetails,
        _workload_outputs: &std::collections::HashMap<
            v4::ecs::component::ComponentId,
            Vec<v4::ecs::scene::WorkloadOutput>,
        >,
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
