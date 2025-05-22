use algoe::bivector::Bivector;
use nalgebra::Vector3;
use v4::{
    builtin_components::{
        camera_component::CameraComponent,
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

    scene! {
        scene: hello_scene,
        active_camera: "cam",
        screen_space_materials: [
            {
                pipeline: {
                    fragment_shader_path: "shaders/hello_world/screen_space.wgsl",
                }
            },
            {
                pipeline: {
                    fragment_shader_path: "shaders/hello_world/screen_space_blur.wgsl"
                }
            }
        ],
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
                TransformComponent(position: Vector3::new(0.0, 0.0, 2.0), ident: "thing"),
                MeshComponent<Vertex>::from_obj("assets/models/basic_cube.obj", true).ident("unused ident").await.unwrap(),
            ]
        },
        "cam_ent" = {
            components: [
                CameraComponent(field_of_view: 80.0, aspect_ratio: 1.0, near_plane: 0.1, far_plane: 50.0, ident: "cam"),
                TransformComponent(position: Vector3::new(1.0, 0.0, 0.0), rotation: (Bivector::new(0.0, 1.0, 0.0) * std::f32::consts::FRAC_PI_8).exponentiate(), uses_buffer: false),
            ]
        }
    }

    engine.attach_scene(hello_scene);

    engine.main_loop().await;
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
