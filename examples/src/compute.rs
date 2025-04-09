use v4::{
    ecs::{compute::Compute, material::{ShaderAttachment, ShaderBufferAttachment}},
    scene, V4,
};

#[tokio::main]
pub async fn main() {
    let mut engine = V4::builder().build().await;

    let device = engine.rendering_manager().device();

    scene! {
        scene: thing,
        "comp" = {
            computes: [
                Compute(input:
                    vec![
                        ShaderAttachment::Buffer(ShaderBufferAttachment::new(
                            device,
                            bytemuck::cast_slice(&[1.0_f32,2.0,3.0,4.0, 5.0, 6.0, 7.0, 8.0]),
                            wgpu::BufferBindingType::Storage { read_only: true },
                            wgpu::ShaderStages::COMPUTE,
                            wgpu::BufferUsages::empty(),
                        ))
                    ],
                    output: 
                        ShaderAttachment::Buffer(ShaderBufferAttachment::new(
                            device,
                            bytemuck::cast_slice(&[0.0_f32,0.0,0.0,0.0, 0.0, 0.0, 0.0, 0.0]),
                            wgpu::BufferBindingType::Storage { read_only: false },
                            wgpu::ShaderStages::COMPUTE,
                            wgpu::BufferUsages::empty(),
                        )),
                    shader_path: "shaders/compute/compute.wgsl",
                    workgroup_counts: (8, 1, 1),
                ),
            ]
        },
    }

    engine.attach_scene(thing);

    engine.main_loop().await;
}
