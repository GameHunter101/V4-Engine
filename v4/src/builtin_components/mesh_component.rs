use std::fmt::Debug;

use crate::v4;
use bytemuck::{Pod, Zeroable};
use v4_core::ecs::{
    component::{ComponentDetails, ComponentId, ComponentSystem},
    entity::EntityId,
};
use v4_macros::component;
use wgpu::{util::DeviceExt, Buffer, Device, Queue, RenderPass, VertexBufferLayout};

pub trait VertexDescriptor: Debug + Pod + Zeroable {
    fn vertex_layout() -> VertexBufferLayout<'static>;

    fn from_pos_normal_coords(pos: Vec<f32>, normal: Vec<f32>, tex_coords: Vec<f32>) -> Self;
}

#[derive(Debug)]
#[component(rendering_order = 500)]
pub struct MeshComponent<V> {
    vertices: Vec<Vec<V>>,
    indices: Vec<Vec<u32>>,
    vertex_buffer: Option<Vec<Buffer>>,
    index_buffer: Option<Vec<Buffer>>,
    enabled_models: Vec<usize>,
}

impl<V: VertexDescriptor> MeshComponent<V> {
    pub fn new(
        vertices: Vec<Vec<V>>,
        indices: Vec<Vec<u32>>,
        is_enabled: bool,
        enabled_models: Vec<usize>,
    ) -> Self {
        Self {
            vertices,
            indices,
            vertex_buffer: None,
            index_buffer: None,
            enabled_models,
            parent_entity_id: EntityId::MAX,
            is_initialized: false,
            is_enabled,
            id: ComponentId::MAX,
        }
    }

    pub async fn from_obj(path: &str, is_enabled: bool) -> Result<Self, tobj::LoadError> {
        let (models, _materials) = tobj::load_obj(
            path,
            &tobj::LoadOptions {
                single_index: true,
                triangulate: true,
                ignore_points: true,
                ignore_lines: true,
            },
        )?;

        let model_count = models.len();

        let (vertices, indices): (Vec<Vec<V>>, Vec<Vec<u32>>) = models
            .into_iter()
            .map(|model| {
                (
                    (0..model.mesh.positions.len() / 3)
                        .map(|i| {
                            let vert: V = VertexDescriptor::from_pos_normal_coords(
                                vec![
                                    *model.mesh.positions.get(i * 3).unwrap_or(&0.0),
                                    *model.mesh.positions.get(i * 3 + 1).unwrap_or(&0.0),
                                    *model.mesh.positions.get(i * 3 + 2).unwrap_or(&0.0),
                                ],
                                vec![
                                    *model.mesh.normals.get(i * 3).unwrap_or(&0.0),
                                    *model.mesh.normals.get(i * 3 + 1).unwrap_or(&0.0),
                                    *model.mesh.normals.get(i * 3 + 2).unwrap_or(&0.0),
                                ],
                                vec![
                                    *model.mesh.texcoords.get(i * 2).unwrap_or(&0.0),
                                    *model.mesh.texcoords.get(i * 2 + 1).unwrap_or(&0.0),
                                ],
                            );
                            vert
                        })
                        .collect::<Vec<V>>(),
                    model.mesh.indices,
                )
            })
            .unzip();

        Ok(Self {
            vertices,
            indices,
            vertex_buffer: None,
            index_buffer: None,
            enabled_models: (0..model_count).collect(),
            id: ComponentId::MAX,
            parent_entity_id: 0,
            is_initialized: false,
            is_enabled,
        })
    }
}

impl<V: VertexDescriptor> ComponentSystem for MeshComponent<V> {
    fn initialize(&mut self, device: &Device) -> v4_core::ecs::actions::ActionQueue {
        self.vertex_buffer = Some(
            self.enabled_models
                .iter()
                .map(|index| {
                    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some(&format!("Component {} | Vertex Buffer", self.id())),
                        contents: bytemuck::cast_slice(&self.vertices[*index]),
                        usage: wgpu::BufferUsages::VERTEX,
                    })
                })
                .collect(),
        );
        self.index_buffer = Some(
            self.enabled_models
                .iter()
                .map(|index| {
                    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some(&format!("Component {} | Index Buffer", self.id())),
                        contents: bytemuck::cast_slice(&self.indices[*index]),
                        usage: wgpu::BufferUsages::INDEX,
                    })
                })
                .collect(),
        );
        self.is_initialized = true;

        Vec::new()
    }

    fn render(&self, _device: &Device, _queue: &Queue, render_pass: &mut RenderPass) {
        for index in &self.enabled_models {
            render_pass.set_vertex_buffer(
                0,
                self.vertex_buffer
                    .as_ref()
                    .expect("Attempted to render an uninitialized mesh.")[*index]
                    .slice(..),
            );
            render_pass.set_index_buffer(
                self.index_buffer
                    .as_ref()
                    .expect("Attempted to render an uninitialized mesh.")[*index]
                    .slice(..),
                wgpu::IndexFormat::Uint32,
            );
            render_pass.draw_indexed(0..(self.indices[*index].len() as u32), 0, 0..1);
        }
    }
}
