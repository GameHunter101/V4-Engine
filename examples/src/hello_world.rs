use std::sync::Arc;

use tokio::sync::Mutex;
use v4::{
    builtin_components::mesh_component::{MeshComponent, VertexDescriptor},
    ecs::{
        pipeline::{GeometryDetails, PipelineId},
        scene::Scene,
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

    let rendering_manager = engine.rendering_manager();
    let device = rendering_manager.device();

    scene! {
        {
            material: {
                pipeline: {
                    vertex_shader_path: "shaders/hello_world/vertex.wgsl",
                    fragment_shader_path: "shaders/hello_world/fragment.wgsl",
                    vertex_layouts: [Vertex::vertex_layout()]
                },
            },
            components: [MeshComponent<Vertex>()]
        }
    }
        MeshComponent::<Vertex>::from_obj("assets/models/basic_cube.obj", true)
            .await
            .unwrap();

    let mut scene = Scene::default();
    let material = scene.create_material(
        device,
        PipelineId {
            vertex_shader_path: "shaders/hello_world/vertex.wgsl",
            fragment_shader_path: "shaders/hello_world/fragment.wgsl",
            vertex_layouts: vec![Vertex::vertex_layout()],
            geometry_details: GeometryDetails::default(),
        },
        Vec::new(),
    );

    let _mesh_component = MeshComponent::new(
        vec![vec![
            Vertex {
                pos: [-1.0, 1.0, 0.0],
            },
            Vertex {
                pos: [-1.0, -1.0, 0.0],
            },
            Vertex {
                pos: [1.0, -1.0, 0.0],
            },
        ]],
        vec![vec![0, 1, 2]],
        true,
        vec![0],
    );

    let cube_mesh_component =
        MeshComponent::<Vertex>::from_obj("assets/models/basic_cube.obj", true)
            .await
            .unwrap();

    scene.create_entity(
        None,
        vec![Box::new(cube_mesh_component)],
        Some(material),
        true,
    );

    engine.attach_scene(scene);

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
    fn vertex_layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &vertex_attr_array![0 => Float32x3],
        }
    }

    fn from_pos_normal_coords(pos: Vec<f32>, _normal: Vec<f32>, _tex_coords: Vec<f32>) -> Self {
        Self {
            pos: pos.try_into().unwrap(),
        }
    }
}
