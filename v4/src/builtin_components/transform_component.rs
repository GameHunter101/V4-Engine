use crate::v4;
use algoe::rotor::Rotor3;
use bytemuck::{cast_slice, Pod, Zeroable};
use nalgebra::{Matrix3, Matrix4, Vector3, Vector4};
use v4_core::ecs::component::ComponentSystem;
use v4_macros::component;
use wgpu::{util::DeviceExt, vertex_attr_array, BufferUsages, VertexAttribute, VertexBufferLayout};

use super::mesh_component::VertexDescriptor;

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
    pub fn vertex_layout<V: VertexDescriptor>() -> VertexBufferLayout<'static> {
        /* const LAST_VERT_POS: u32 = V::vertex_layout().attributes.len() as u32;
        const ATTRIBUTES: [wgpu::VertexAttribute; 4] = vertex_attr_array![
            LAST_VERT_POS => Float32x4, LAST_VERT_POS => Float32x4,LAST_VERT_POS => Float32x4,LAST_VERT_POS => Float32x4
        ]; */
        const TEST:impl V = unsafe {core::mem::zeroed()};
        /* let test: &'static [VertexAttribute; 2] = &[
            (wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 0,
                shader_location: 1,
            }),
            /* (wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: (0 + wgpu::VertexFormat::Float32x4.size()),
                shader_location: 2,
            }), */
        ]; */
        const FLOAT: u64 = wgpu::VertexFormat::Float32x4.size();
        // let other: &'static [u64; 2] = &[FLOAT, len];
        /* wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<V>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: test,
        } */
        todo!()
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
        dbg!(&raw_data);
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
            Vector4::z(),
            value.position.to_homogeneous(),
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
