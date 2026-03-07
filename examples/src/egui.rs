use egui::Context;
use v4::{
    V4,
    builtin_actions::RegisterUiComponentAction,
    component,
    ecs::{
        actions::ActionQueue,
        component::{ComponentDetails, ComponentSystem},
    },
    scene,
};
use winit::window::WindowAttributes;

#[tokio::main]
pub async fn main() {
    let mut engine = V4::builder()
        .window_attributes(
            WindowAttributes::default()
                .with_surface_size(winit::dpi::PhysicalSize::new(800, 800))
                .with_title("V4 egui demo"),
        )
        .clear_color(wgpu::Color {
            r: 0.5,
            g: 0.04,
            b: 0.04,
            a: 1.0,
        })
        .egui_clear_color(wgpu::Color::RED)
        .build()
        .await;

    scene! {
        scene: egui_scene,
        "ui" = {
            components: [
            EguiUiComponent()
            ]
        }
    };

    engine.attach_scene(egui_scene);

    engine.main_loop().await;
}

#[component(custom_debug)]
struct EguiUiComponent {
    #[default(0.5)]
    val: f32
}

impl std::fmt::Debug for EguiUiComponent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EguiUiComponent")
            .field("id", &self.id)
            .field("parent_entity_id", &self.parent_entity_id)
            .field("is_initialized", &self.is_initialized)
            .field("is_enabled", &self.is_enabled)
            .finish()
    }
}

impl ComponentSystem for EguiUiComponent {
    fn initialize(&mut self, _device: &wgpu::Device) -> ActionQueue {
        self.set_initialized();
        vec![Box::new(RegisterUiComponentAction {
            component_id: self.id,
            text_component_properties: None,
        })]
    }

    fn ui_render(&mut self, ctx: &Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add(egui::Slider::new(&mut self.val, 0.0..=1.0));
            ui.label("Hello, world!");
            if ui.button("Thing").clicked() {
                println!("Clicked!");
            }
            // egui_demo_lib::DemoWindows::default().ui(ctx);
        });
    }
}
