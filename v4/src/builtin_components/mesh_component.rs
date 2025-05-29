use std::{fmt::Debug, ops::Range};

use crate::v4;
use bytemuck::{Pod, Zeroable};
use v4_core::ecs::component::{Component, ComponentDetails, ComponentSystem};
use v4_macros::component;
use wgpu::{util::DeviceExt, Buffer, Device, Queue, RenderPass, VertexAttribute};

pub trait VertexDescriptor: Debug + Pod + Zeroable {
    const ATTRIBUTES: &[VertexAttribute];
    fn vertex_layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: Self::ATTRIBUTES,
        }
    }

    fn len() -> u64 {
        Self::ATTRIBUTES.len() as u64
    }

    fn from_pos_normal_coords(pos: Vec<f32>, normal: Vec<f32>, tex_coords: Vec<f32>) -> Self;
}

/// When specifying `enabled_models`, it is possible to specify the vertex range in the vertex buffer
/// from which to draw. The number of elements in `enabled_models` dictates the number of models
/// and consequently the number of draw calls
#[component(rendering_order = 500)]
pub struct MeshComponent<V: VertexDescriptor> {
    vertices: Vec<Vec<V>>,
    #[default]
    indices: Vec<Vec<u32>>,
    #[default]
    vertex_buffers: Option<Vec<Buffer>>,
    #[default]
    index_buffers: Option<Vec<Buffer>>,
    enabled_models: Vec<(usize, Option<Range<u64>>)>,
}

impl<V: VertexDescriptor> MeshComponent<V> {
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
            vertex_buffers: None,
            index_buffers: None,
            enabled_models: (0..model_count).map(|i| (i, None)).collect(),
            id: {
                use std::hash::{Hash, Hasher};
                let mut hasher = std::hash::DefaultHasher::new();
                std::time::Instant::now().hash(&mut hasher);
                hasher.finish()
            },
            parent_entity_id: 0,
            is_initialized: false,
            is_enabled,
        })
    }

    pub fn vertex_buffers(&self) -> Option<&Vec<Buffer>> {
        self.vertex_buffers.as_ref()
    }

    pub fn index_buffers(&self) -> Option<&Vec<Buffer>> {
        self.index_buffers.as_ref()
    }

    pub fn enabled_models(&self) -> &[(usize, Option<Range<u64>>)] {
        &self.enabled_models
    }

    pub fn enabled_models_mut(&mut self) -> &mut Vec<(usize, Option<Range<u64>>)> {
        &mut self.enabled_models
    }
}

impl<V: VertexDescriptor + Send + Sync> ComponentSystem for MeshComponent<V> {
    fn initialize(&mut self, device: &Device) -> v4_core::ecs::actions::ActionQueue {
        self.vertex_buffers = Some(
            self.enabled_models
                .iter()
                .map(|(index, _)| {
                    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some(&format!("Component {} | Vertex Buffer", self.id())),
                        contents: bytemuck::cast_slice(&self.vertices[*index]),
                        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    })
                })
                .collect(),
        );
        if !self.indices.is_empty() {
            self.index_buffers = Some(
                self.enabled_models
                    .iter()
                    .map(|(index, _)| {
                        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some(&format!("Component {} | Index Buffer", self.id())),
                            contents: bytemuck::cast_slice(&self.indices[*index]),
                            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
                        })
                    })
                    .collect(),
            );
        }
        self.is_initialized = true;

        Vec::new()
    }

    fn render(
        &self,
        _device: &Device,
        _queue: &Queue,
        render_pass: &mut RenderPass,
        _other_components: &[&Component],
    ) {
        for (index, range_opt) in &self.enabled_models {
            render_pass.set_vertex_buffer(
                0,
                if let Some(range) = range_opt {
                    let byte_range =
                        (range.start * size_of::<V>() as u64)..(range.end * size_of::<V>() as u64);
                    self.vertex_buffers
                        .as_ref()
                        .expect("Attempted to render an uninitialized mesh.")[*index]
                        .slice(byte_range)
                } else {
                    self.vertex_buffers
                        .as_ref()
                        .expect("Attempted to render an uninitialized mesh.")[*index]
                        .slice(..)
                },
            );
            if let Some(index_buffers) = &self.index_buffers {
                render_pass
                    .set_index_buffer(index_buffers[*index].slice(..), wgpu::IndexFormat::Uint32);
                render_pass.draw_indexed(0..(self.indices[*index].len() as u32), 0, 0..1);
            } else {
                render_pass.draw(
                    if let Some(range) = range_opt {
                        (range.start as u32)..(range.end as u32)
                    } else {
                        0..self.vertices[*index].len() as u32
                    },
                    0..1,
                );
            }
        }
    }
}
