use std::{fmt::Debug, ops::Range};

use bytemuck::{Pod, Zeroable};
use v4_core::ecs::{
    component::{ComponentId, ComponentSystem},
    entity::EntityId,
};
use v4_macros::component;
use wgpu::{util::DeviceExt, Buffer, Device, Queue, RenderPass, VertexBufferLayout};

pub trait VertexDescriptor: Debug + Pod + Zeroable {
    fn vertex_layout() -> VertexBufferLayout<'static>;
}

#[derive(Debug)]
#[component(rendering_order = 500)]
pub struct MeshComponent<V> {
    vertices: Vec<V>,
    indices: Vec<u16>,
    vertex_buffer: Option<Buffer>,
    index_buffer: Option<Buffer>,
}

impl<V: VertexDescriptor> MeshComponent<V> {
    pub fn new(vertices: Vec<V>, indices: Vec<u16>, is_enabled: bool) -> Self {
        Self {
            vertices,
            indices,
            vertex_buffer: None,
            index_buffer: None,
            parent_entity_id: EntityId::MAX,
            component_id: ComponentId::MAX,
            is_initialized: false,
            is_enabled,
        }
    }
}

impl<V: VertexDescriptor> ComponentSystem for MeshComponent<V> {
    fn initialize(&mut self, device: &Device) {
        self.vertex_buffer = Some(
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(&format!("Component {} | Vertex Buffer", self.component_id)),
                contents: bytemuck::cast_slice(&self.vertices),
                usage: wgpu::BufferUsages::VERTEX,
            }),
        );
        self.index_buffer = Some(device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(&format!("Component {} | Index Buffer", self.component_id)),
            contents: bytemuck::cast_slice(&self.indices),
            usage: wgpu::BufferUsages::INDEX,
        }));
        // dbg!(self.vertex_buffer.as_ref().unwrap().size());
        // dbg!(self.index_buffer.as_ref().unwrap().size());
        self.is_initialized = true;
    }

    fn render(&self, _device: &Device, _queue: &Queue, render_pass: &mut RenderPass) {
        render_pass.set_vertex_buffer(
            0,
            self.vertex_buffer
                .as_ref()
                .expect("Attempted to render an uninitialized mesh.")
                .slice(..),
        );
        render_pass.set_index_buffer(
            self.index_buffer
                .as_ref()
                .expect("Attempted to render an uninitialized mesh.")
                .slice(..),
            wgpu::IndexFormat::Uint16,
        );
        render_pass.draw_indexed(0..(self.indices.len() as u32), 0, 0..1);
    }
}
