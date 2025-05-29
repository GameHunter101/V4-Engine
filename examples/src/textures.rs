use v4::{
    builtin_components::mesh_component::{MeshComponent, VertexDescriptor},
    engine_support::texture_support::Texture,
    scene, V4,
};

#[tokio::main]
pub async fn main() {
    let mut engine = V4::builder().build().await;
    let rendering_manager = engine.rendering_manager();
    let device = rendering_manager.device();
    let queue = rendering_manager.queue();

    scene! {
        _ = {
            material: {
                pipeline: {
                    vertex_shader_path: "shaders/textures/vertex.wgsl",
                    fragment_shader_path: "shaders/textures/fragment.wgsl",
                    vertex_layouts: [Vertex::vertex_layout()],
                    uses_camera: false,
                    geometry_details: {
                        polygon_mode: wgpu::PolygonMode::Fill,
                    }
                },
                attachments: [Texture (
                    texture: 
                        v4::ecs::material::GeneralTexture::Regular(
                            Texture::from_path(
                                "./assets/testing_textures/dude.png",
                                device,
                                queue,
                                wgpu::TextureFormat::Rgba8UnormSrgb,
                                false,
                                true,
                            ).await.unwrap()
                        ),
                    visibility: wgpu::ShaderStages::FRAGMENT,
                )],
            },
            components: [
                MeshComponent(
                    vertices: vec![vec![
                        Vertex {
                            pos: [-0.5, 0.5, 0.0],
                            tex_coords: [0.0, 0.0] ,
                        },
                        Vertex {
                            pos: [-0.5, -0.5, 0.0],
                            tex_coords: [0.0, 1.0] ,
                        },
                        Vertex {
                            pos: [0.5, -0.5, 0.0],
                            tex_coords: [1.0, 1.0] ,
                        },
                        Vertex {
                            pos: [0.5, 0.5, 0.0],
                            tex_coords: [1.0, 0.0] ,
                        },
                    ]],
                    indices: vec![vec![0,1,2,0,2,3]],
                    enabled_models: vec![(0, None)],
                ),
            ]
        }
    }

    engine.attach_scene(scene);

    engine.main_loop().await;
}

#[repr(C)]
#[derive(Debug, bytemuck::Pod, bytemuck::Zeroable, Clone, Copy)]
struct Vertex {
    pos: [f32; 3],
    tex_coords: [f32; 2],
}

impl VertexDescriptor for Vertex {
    const ATTRIBUTES: &[wgpu::VertexAttribute] =
        &wgpu::vertex_attr_array![0=>Float32x3, 1=>Float32x2];

    fn from_pos_normal_coords(pos: Vec<f32>, _normal: Vec<f32>, tex_coords: Vec<f32>) -> Self {
        Self {
            pos: pos.try_into().unwrap(),
            tex_coords: tex_coords.try_into().unwrap(),
        }
    }
}
