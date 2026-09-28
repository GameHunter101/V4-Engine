use v4::{V4, ecs::material::BufferBundle, scene};

#[tokio::main]
pub async fn main() {
    let mut engine = V4::builder().build().await.unwrap();

    let device = engine.rendering_manager().device();

    let compute_scene = scene! {
        "comp" = {
            computes: [
                Compute {
                    attachments: [
                        Buffer {
                            buffer: BufferBundle::new(
                                device, bytemuck::cast_slice(&[1.0_f32,2.0,3.0,4.0, 5.0, 6.0, 7.0, 8.0]),
                                wgpu::BufferBindingType::Storage { read_only: true },
                                wgpu::BufferUsages::empty(),
                            ),
                            visibility: wgpu::ShaderStages::COMPUTE,
                        },
                        Buffer {
                            buffer: BufferBundle::new(
                                device, bytemuck::cast_slice(&[0.0_f32,0.0,0.0,0.0, 0.0, 0.0, 0.0, 0.0]),
                                wgpu::BufferBindingType::Storage { read_only: false },
                                wgpu::BufferUsages::empty(),
                            ),
                            visibility: wgpu::ShaderStages::COMPUTE,
                        },
                    ],
                    shader_path: "shaders/compute/compute.wgsl",
                    workgroup_counts: v4::ecs::compute::WorkgroupCounts::Static(8, 1, 1),
                    ID: "temp"
                }
            ]
        },
    };

    engine.attach_scene(compute_scene);

    engine.main_loop().await.unwrap();
}
