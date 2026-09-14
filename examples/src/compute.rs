use v4::{V4, scene};

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
                            device,
                            data: bytemuck::cast_slice(&[1.0_f32,2.0,3.0,4.0, 5.0, 6.0, 7.0, 8.0]),
                            buffer_type: wgpu::BufferBindingType::Storage { read_only: true },
                            visibility: wgpu::ShaderStages::COMPUTE,
                            extra_usages: wgpu::BufferUsages::empty(),
                        },
                        Buffer {
                            device,
                            data: bytemuck::cast_slice(&[0.0_f32,0.0,0.0,0.0, 0.0, 0.0, 0.0, 0.0]),
                            buffer_type: wgpu::BufferBindingType::Storage { read_only: false },
                            visibility: wgpu::ShaderStages::COMPUTE,
                            extra_usages: wgpu::BufferUsages::empty(),
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
