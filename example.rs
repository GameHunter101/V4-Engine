use std::sync::Arc;

use gamezap::{
    ecs::{
        pipeline::{GeometryDetails, PipelineDetails},
        scene::Scene,
    },
    Gamezap,
};
use tokio::sync::Mutex;

#[tokio::main]
async fn main() {
    let mut engine = Gamezap::builder()
        .window_settings(600, 600, "Example Gamezap Project", None)
        .clear_color(wgpu::Color {
            r: 0.8,
            g: 0.15,
            b: 0.2,
            a: 1.0,
        })
        .build()
        .await;

    let results = Arc::new(Mutex::new(Vec::new()));
    let clone = results.clone();
    std::thread::spawn(move || {
        async_test(clone);
    });

    let rendering_manager = engine.rendering_manager();
    let device = rendering_manager.device();
    let queue = rendering_manager.queue();
    let render_format = rendering_manager.format();

    let mut scene = Scene::new(device, queue, render_format);
    let material = scene.create_material(
        device,
        render_format,
        "examples/assets/shaders/vertex.wgsl",
        "examples/assets/shaders/fragment.wgsl",
        Vec::new(),
        PipelineDetails {
            vertex_layouts: &[],
            geometry_details: GeometryDetails::default(),
        },
    );
    scene.create_entity(None, Vec::new(), Some(material), true);

    engine.attach_scene(scene);

    // drop(scene);

    // futures::future::join_all(tasks).await;

    /* let all_items: Vec<Box<dyn ItemTrait + Send>> = [Item { field: -1.0 }; 10]
        .iter()
        .cloned()
        .map(|item| Box::new(item) as Box<dyn ItemTrait + Send>)
        .collect();
    let mut item_colllection = ItemCollection { all_items };
    item_colllection.update_all().await;

    let mut actions: Vec<Box<dyn TestAction>> = vec![
        Box::new(TestTransfer {
            item: Box::new(Item { field: -7.5 })
        });
        10
    ]
    .into_iter()
    .map(|action| action as Box<dyn TestAction>)
    .collect();

    actions
        .iter_mut()
        .for_each(|action| action.execute(&mut item_colllection)); */

    engine.main_loop().await;
}

#[tokio::main]
async fn async_test(results: Arc<Mutex<Vec<u64>>>) {
    async_scoped::TokioScope::scope_and_block(|scope| {
        (0..100).for_each(|i| {
            let res = results.clone();
            let task = async move {
                tokio::time::sleep(std::time::Duration::from_millis(i * 100)).await;
                res.lock().await.push(i);
                // println!("{i} finished");
                i
            };
            scope.spawn(task);
        });
    });
}

/* trait TestAction {
    fn execute(&self, items: &mut ItemCollection);
}

#[derive(Debug, Clone)]
struct TestTransfer {
    item: Box<Item>,
}

impl TestAction for TestTransfer {
    fn execute(&self, items: &mut ItemCollection) {
        items.all_items.push(self.item.clone());
    }
}

trait ItemTrait: std::fmt::Debug {
    fn update(&mut self, _other_items: &mut [&mut Box<dyn ItemTrait + Send>], _index: f32) {}
}

#[derive(Debug, Clone, Copy)]
struct Item {
    field: f32,
}

impl ItemTrait for Item {
    fn update(&mut self, other_items: &mut [&mut Box<dyn ItemTrait + Send>], index: f32) {
        // other_items[0].field = 12.0 * index;
        println!("{index} | {other_items:?}");
        self.field = index;
    }
}

#[derive(Debug)]
struct ItemCollection {
    all_items: Vec<Box<dyn ItemTrait + Send>>,
}

impl ItemCollection {
    async fn update_all(&mut self) {
        let all_items_ref = &mut self.all_items;
        for i in 0..all_items_ref.len() {
            unsafe {
                let value = all_items_ref.split_at_mut(i);
                async_scoped::TokioScope::scope_and_collect(|scope| {
                    let proc = async move {
                        let (previous_items, next_items_and_current_item) = value;
                        let current_item = next_items_and_current_item.split_first_mut();
                        if let Some((current_item, next_items)) = current_item {
                            let mut chain = previous_items
                                .iter_mut()
                                .chain(next_items.iter_mut())
                                .collect::<Vec<_>>();
                            current_item.update(&mut chain, i as f32);
                        }
                    };

                    scope.spawn(proc);
                })
                .await;
            }
        }
    }
}

struct Material {
    pub components: Vec<usize>
}

struct Component;
impl Component {
    pub fn render(&self, device: &wgpu::Device, render_pass: &mut wgpu::RenderPass) {
        println!("rendering");
    }
}

struct Stuff {
    components: Vec<Component>,
}

impl Stuff {
    fn render_component(&self, render_pass: &mut wgpu::RenderPass, device: wgpu::Device, material: &Material) {
        for component_id in &material.components {
            let component = &self.components[*component_id];
            component.render(&device, render_pass);
        }
    }

} */
