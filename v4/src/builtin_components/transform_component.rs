use crate::v4;
use algoe::rotor::Rotor3;
use bytemuck::{cast_slice, Pod, Zeroable};
use nalgebra::{Matrix3, Vector3};
use v4_core::ecs::component::ComponentSystem;
use v4_macros::component;
use wgpu::{util::DeviceExt, BufferUsages};

#[derive(Debug)]
#[component]
pub struct TransformComponent {
    position: Vector3<f32>,
    rotation: Rotor3,
    scale: Vector3<f32>,
}

impl ComponentSystem for TransformComponent {
    fn render(
        &self,
        device: &wgpu::Device,
        _queue: &wgpu::Queue,
        render_pass: &mut wgpu::RenderPass,
    ) {
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(&format!("Transform Component {} Buffer", self.id)),
            contents: cast_slice(&[RawTransformData::from_component(self)]),
            usage: BufferUsages::VERTEX,
        });

        render_pass.set_vertex_buffer(1, buffer.slice(..));
    }
}

#[repr(C)]
#[derive(Debug, Pod, Zeroable, Clone, Copy)]
pub struct RawTransformData {
    position: [f32; 3],
    rotation_matrix: [[f32; 4]; 4],
    scale: [f32; 3],
}

impl RawTransformData {
    fn from_component(value: &TransformComponent) -> Self {
        let rotation_matrix = Matrix3::from_columns(&[
            value.rotation * Vector3::x(),
            value.rotation * Vector3::y(),
            value.rotation * Vector3::z(),
        ])
        .to_homogeneous()
        .into();

        RawTransformData {
            position: value.position.into(),
            rotation_matrix,
            scale: value.scale.into(),
        }
    }
}
