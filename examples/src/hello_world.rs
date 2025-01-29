use std::sync::Arc;

use algoe::rotor::Rotor3;
use nalgebra::Vector3;
use tokio::sync::Mutex;
use v4::{
    builtin_components::{
        mesh_component::{MeshComponent, VertexDescriptor},
        transform_component::TransformComponent,
    },
    scene, V4,
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

    scene! {
        scene: hello_scene
        _ = {
            material: {
                pipeline: {
                    vertex_shader_path: "shaders/hello_world/vertex.wgsl",
                    fragment_shader_path: "shaders/hello_world/fragment.wgsl",
                    vertex_layouts: [Vertex::vertex_layout(), TransformComponent::vertex_layout::<1>()],
                    uses_camera: true,
                },
            },
            components: [
                MeshComponent<Vertex>::from_obj("assets/models/basic_cube.obj", true).await.unwrap(),
                TransformComponent(position: Vector3::new(0.0, 0.0, 0.5)),
            ]
        }
    }

    engine.attach_scene(hello_scene);

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
    const ATTRIBUTES: &[wgpu::VertexAttribute] = &vertex_attr_array![0 => Float32x3];

    fn from_pos_normal_coords(pos: Vec<f32>, _normal: Vec<f32>, _tex_coords: Vec<f32>) -> Self {
        Self {
            pos: pos.try_into().unwrap(),
        }
    }
}
