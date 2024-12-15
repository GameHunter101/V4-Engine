use std::sync::Arc;

use tokio::sync::Mutex;
use v4::{
    builtin_components::mesh_component::{MeshComponent, VertexDescriptor},
    ecs::{
        pipeline::{GeometryDetails, PipelineDetails},
        scene::Scene,
    },
    V4,
};
use wgpu::vertex_attr_array;

#[tokio::main]
pub async fn main() {
    let mut engine = V4::builder()
        .window_settings(600, 600, "Example V4 Project", None)
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
        "shaders/vertex.wgsl",
        "shaders/fragment.wgsl",
        Vec::new(),
        PipelineDetails {
            vertex_layouts: &[Vertex::vertex_layout()],
            geometry_details: GeometryDetails::default(),
        },
    );

    let mesh_component = MeshComponent::new(
        vec![
            Vertex {
                pos: [-1.0, 1.0, 0.0],
            },
            Vertex {
                pos: [-1.0, -1.0, 0.0],
            },
            Vertex {
                pos: [1.0, -1.0, 0.0],
            },
        ],
        vec![0, 1, 2],
        true,
    );

    scene.create_entity(None, vec![Box::new(mesh_component)], Some(material), true);

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

#[repr(C)]
#[derive(Debug, bytemuck::Pod, bytemuck::Zeroable, Clone, Copy)]
struct Vertex {
    pos: [f32; 3],
}

impl VertexDescriptor for Vertex {
    fn vertex_layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &vertex_attr_array![0 => Float32x3],
        }
    }
}
