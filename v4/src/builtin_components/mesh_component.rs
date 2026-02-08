use std::{fmt::Debug, ops::Range};

use crate::v4;
use bytemuck::{Pod, Zeroable};
use nalgebra::{Vector2, Vector3};
use v4_core::ecs::component::{Component, ComponentDetails, ComponentSystem};
use v4_macros::component;
use wgpu::{Buffer, Device, Queue, RenderPass, VertexAttribute, util::DeviceExt};

pub struct VertexData {
    pub pos: [f32; 3],
    pub normal: [f32; 3],
    pub tex_coords: [f32; 2],
    pub tangent: [f32; 3],
    pub bitangent: [f32; 3],
}

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

    fn from_data(data: VertexData) -> Self;
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
                let mut verts: Vec<VertexData> = (0..model.mesh.positions.len() / 3)
                    .map(|i| VertexData {
                        pos: [
                            *model.mesh.positions.get(i * 3).unwrap_or(&0.0),
                            *model.mesh.positions.get(i * 3 + 1).unwrap_or(&0.0),
                            *model.mesh.positions.get(i * 3 + 2).unwrap_or(&0.0),
                        ],
                        normal: [
                            *model.mesh.normals.get(i * 3).unwrap_or(&0.0),
                            *model.mesh.normals.get(i * 3 + 1).unwrap_or(&0.0),
                            *model.mesh.normals.get(i * 3 + 2).unwrap_or(&0.0),
                        ],
                        tex_coords: [
                            *model.mesh.texcoords.get(i * 2).unwrap_or(&0.0),
                            *model.mesh.texcoords.get(i * 2 + 1).unwrap_or(&0.0),
                        ],
                        tangent: [0.0; 3],
                        bitangent: [0.0; 3],
                    })
                    .collect();

                let mut total_vertex_uses = vec![0.0_f32; verts.len()];

                for chunk in model.mesh.indices.chunks(3) {
                    let indices = [chunk[0] as usize, chunk[1] as usize, chunk[2] as usize];
                    let tri_verts = indices.map(|i| &verts[i]);
                    let [pos0, pos1, pos2] = tri_verts.map(|vert| Vector3::from(vert.pos));
                    let [tex0, tex1, tex2] = tri_verts.map(|vert| Vector2::from(vert.tex_coords));

                    let delta_pos_1 = pos1 - pos0;
                    let delta_pos_2 = pos2 - pos0;

                    let delta_tex_1 = tex1 - tex0;
                    let delta_tex_2 = tex2 - tex0;

                    let inv = 1.0 / (delta_tex_1.x * delta_tex_2.y - delta_tex_1.y * delta_tex_2.x);
                    let tangent = inv * (delta_tex_2.y * delta_pos_1 - delta_tex_1.y * delta_pos_2);
                    let bitangent = inv * (-delta_tex_2.x * delta_pos_1 + delta_tex_1.x * delta_pos_2);
                    for i in indices {
                        verts[i].tangent = (tangent + Vector3::from(verts[i].tangent)).into();
                        verts[i].bitangent = (bitangent + Vector3::from(verts[i].bitangent)).into();
                        total_vertex_uses[i] += 1.0;
                    }
                }

                for (i, vert) in verts.iter_mut().enumerate() {
                    vert.tangent = (Vector3::from(vert.tangent) / total_vertex_uses[i]).into();
                    vert.bitangent = (Vector3::from(vert.bitangent) / total_vertex_uses[i]).into();
                }

                (
                    verts.into_iter().map(|data| V::from_data(data)).collect(),
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
                        0..(range.end - range.start) as u32
                    } else {
                        0..self.vertices[*index].len() as u32
                    },
                    0..1,
                );
            }
        }
    }
}
