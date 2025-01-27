use crate::v4;
use algoe::rotor::Rotor3;
use bytemuck::{cast_slice, Pod, Zeroable};
use nalgebra::{Matrix3, Matrix4, Vector3, Vector4};
use v4_core::ecs::component::ComponentSystem;
use v4_macros::component;
use wgpu::{util::DeviceExt, BufferUsages, VertexAttribute, VertexBufferLayout};

#[derive(Debug)]
#[component]
pub struct TransformComponent {
    position: Vector3<f32>,
    #[default]
    rotation: Rotor3,
    #[default(Vector3::new(1.0, 1.0, 1.0))]
    scale: Vector3<f32>,
}

impl TransformComponent {
    pub fn vertex_layout<const VERTEX_ATTRIBUTE_COUNT: u32>() -> VertexBufferLayout<'static> {
        const VECTOR_SIZE: u64 = wgpu::VertexFormat::Float32x4.size();
        let attributes: &'static [VertexAttribute] = &[
            (wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 0,
                shader_location: (VERTEX_ATTRIBUTE_COUNT),
            }),
            (wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: VECTOR_SIZE,
                shader_location: (VERTEX_ATTRIBUTE_COUNT + 1),
            }),
            (wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: VECTOR_SIZE * 2,
                shader_location: (VERTEX_ATTRIBUTE_COUNT + 2),
            }),
            (wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: VECTOR_SIZE * 3,
                shader_location: (VERTEX_ATTRIBUTE_COUNT + 3),
            }),
        ];
        wgpu::VertexBufferLayout {
            array_stride: VECTOR_SIZE * 4,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes,
        }
    }
}

impl ComponentSystem for TransformComponent {
    fn render(
        &self,
        device: &wgpu::Device,
        _queue: &wgpu::Queue,
        render_pass: &mut wgpu::RenderPass,
    ) {
        let raw_data = RawTransformData::from_component(self);
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(&format!("Transform Component {} Buffer", self.id)),
            contents: cast_slice(&[raw_data]),
            usage: BufferUsages::VERTEX,
        });

        render_pass.set_vertex_buffer(1, buffer.slice(..));
    }
}

#[repr(C)]
#[derive(Debug, Pod, Zeroable, Clone, Copy)]
pub struct RawTransformData {
    matrix: [[f32; 4]; 4],
}

impl RawTransformData {
    fn from_component(value: &TransformComponent) -> Self {
        let rotation_matrix = Matrix3::from_columns(&[
            value.rotation * Vector3::x(),
            value.rotation * Vector3::y(),
            value.rotation * Vector3::z(),
        ])
        .to_homogeneous();

        let transformation_matrix = Matrix4::from_columns(&[
            Vector4::x(),
            Vector4::y(),
            value.position.to_homogeneous(),
            Vector4::w(),
        ]);

        let scale_matrix = Matrix3::from_columns(&[
            Vector3::x() * value.scale.x,
            Vector3::y() * value.scale.y,
            Vector3::z() * value.scale.z,
        ])
        .to_homogeneous();

        let matrix = (transformation_matrix * rotation_matrix * scale_matrix).into();

        RawTransformData { matrix }
    }
}
