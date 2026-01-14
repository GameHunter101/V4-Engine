use crate::v4;
use algoe::rotor::Rotor3;
use bytemuck::{cast_slice, Pod, Zeroable};
use nalgebra::{Matrix3, Matrix4, Translation3, Vector3};
use v4_core::ecs::component::{Component, ComponentSystem};
use v4_macros::component;
use wgpu::{util::DeviceExt, BufferUsages, VertexAttribute, VertexBufferLayout};

#[component]
pub struct TransformComponent {
    position: Vector3<f32>,
    #[default]
    rotation: Rotor3,
    #[default(Vector3::new(1.0, 1.0, 1.0))]
    scale: Vector3<f32>,
    #[default(true)]
    uses_buffer: bool,
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

    pub fn create_matrix(&self) -> Matrix4<f32> {
        let rotation_matrix = Matrix3::from_columns(&[
            self.rotation * Vector3::x(),
            self.rotation * Vector3::y(),
            self.rotation * Vector3::z(),
        ])
        .to_homogeneous();

        let transformation_matrix = Translation3::from(self.position).to_homogeneous();

        let scale_matrix = Matrix3::from_columns(&[
            Vector3::x() * self.scale.x,
            Vector3::y() * self.scale.y,
            Vector3::z() * self.scale.z,
        ])
        .to_homogeneous();

        transformation_matrix * rotation_matrix * scale_matrix
    }

    pub fn set_position(&mut self, position: Vector3<f32>) {
        self.position = position;
    }

    pub fn set_rotation(&mut self, rotation: Rotor3) {
        self.rotation = rotation;
    }

    pub fn set_scale(&mut self, scale: Vector3<f32>) {
        self.scale = scale;
    }

    pub fn get_position(&self) -> Vector3<f32> {
        self.position
    }

    pub fn get_rotation(&self) -> Rotor3 {
        self.rotation
    }

    pub fn get_scale(&self) -> Vector3<f32> {
        self.scale
    }
}

impl ComponentSystem for TransformComponent {
    fn render(
        &self,
        device: &wgpu::Device,
        _queue: &wgpu::Queue,
        render_pass: &mut wgpu::RenderPass,
        _other_components: &[&Component],
    ) {
        if self.uses_buffer {
            let raw_data = RawTransformData::from_component(self);
            let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(&format!("Transform Component {} Buffer", self.id)),
                contents: cast_slice(&[raw_data]),
                usage: BufferUsages::VERTEX,
            });

            render_pass.set_vertex_buffer(1, buffer.slice(..));
        }
    }
}

#[repr(C)]
#[derive(Debug, Pod, Zeroable, Clone, Copy)]
pub struct RawTransformData {
    matrix: [[f32; 4]; 4],
}

impl RawTransformData {
    fn from_component(comp: &TransformComponent) -> Self {
        RawTransformData {
            matrix: comp.create_matrix().into(),
        }
    }
}
