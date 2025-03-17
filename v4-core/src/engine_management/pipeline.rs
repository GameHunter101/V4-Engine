use std::borrow::Cow;

use wgpu::{
    util::make_spirv, BindGroupLayout, Device, RenderPipeline, ShaderStages, TextureFormat,
    VertexBufferLayout,
};

use crate::engine_support::texture_support::Texture;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PipelineAttachments {
    SampledTexture(ShaderStages),
    StorageTexture(ShaderStages),
    Buffer(ShaderStages),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PipelineId {
    pub vertex_shader: PipelineShader,
    pub spirv_vertex_shader: bool,
    pub fragment_shader: PipelineShader,
    pub spirv_fragment_shader: bool,
    pub attachments: Vec<PipelineAttachments>,
    pub vertex_layouts: Vec<wgpu::VertexBufferLayout<'static>>,
    pub uses_camera: bool,
    pub is_screen_space: bool,
    pub geometry_details: GeometryDetails,
}

impl PipelineId {
    pub fn vertex_layouts(&self) -> &[VertexBufferLayout] {
        &self.vertex_layouts
    }

    pub fn geometry_details(&self) -> &GeometryDetails {
        &self.geometry_details
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PipelineShader {
    Path(&'static str),
    Raw(Cow<'static, str>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GeometryDetails {
    pub topology: wgpu::PrimitiveTopology,
    pub strip_index_format: Option<wgpu::IndexFormat>,
    pub front_face: wgpu::FrontFace,
    pub cull_mode: Option<wgpu::Face>,
    pub polygon_mode: wgpu::PolygonMode,
}

impl Default for GeometryDetails {
    fn default() -> Self {
        Self {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: None,
            polygon_mode: wgpu::PolygonMode::Fill,
        }
    }
}

pub fn create_render_pipeline(
    device: &Device,
    id: &PipelineId,
    bind_group_layouts: &[BindGroupLayout],
    render_format: TextureFormat,
    is_vert_spirv: bool,
    is_frag_spirv: bool,
) -> RenderPipeline {
    let camera_layout = if id.uses_camera {
        Some(
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some(&format!("{id:?} Pipeline Camera Bind Group Layout")),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            }),
        )
    } else {
        None
    };

    let screen_space_layout = if id.is_screen_space {
        Some(
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some(&format!("{id:?} Pipeline Screen Space Bind Group Layout")),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            }),
        )
    } else {
        None
    };

    let bind_group_layouts: Vec<&wgpu::BindGroupLayout> =
        if let Some(camera_layout) = &camera_layout {
            vec![camera_layout]
                .into_iter()
                .chain(bind_group_layouts.iter())
                .collect()
        } else if let Some(screen_space_layout) = &screen_space_layout {
            vec![screen_space_layout]
                .into_iter()
                .chain(bind_group_layouts.iter())
                .collect()
        } else {
            bind_group_layouts
                .iter()
                .collect()
        };


    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(&format!("{id:?} Pipeline Layout")),
        bind_group_layouts: &bind_group_layouts,
        push_constant_ranges: &[],
    });

    let vertex_shader_module =
        load_shader_module_descriptor(device, &id.vertex_shader, is_vert_spirv);
    if let Err(error) = vertex_shader_module {
        panic!(
            "Vertex shader error for shader {:?}: {error}",
            id.vertex_shader
        );
    }
    let vertex_shader_module = vertex_shader_module.unwrap();

    let fragment_shader_module =
        load_shader_module_descriptor(device, &id.fragment_shader, is_frag_spirv);
    if let Err(error) = fragment_shader_module {
        panic!(
            "Fragment shader error for shader {:?}: {error}",
            id.fragment_shader
        );
    }
    let fragment_shader_module = fragment_shader_module.unwrap();

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(&format!("{id:?} Pipeline")),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &vertex_shader_module,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            buffers: &id.vertex_layouts,
        },
        primitive: wgpu::PrimitiveState {
            topology: id.geometry_details.topology,
            strip_index_format: id.geometry_details.strip_index_format,
            front_face: id.geometry_details.front_face,
            cull_mode: id.geometry_details.cull_mode,
            unclipped_depth: false,
            polygon_mode: id.geometry_details.polygon_mode,
            conservative: false,
        },
        depth_stencil: if id.is_screen_space {
            None
        } else {
            Some(wgpu::DepthStencilState {
                format: Texture::DEPTH_FORMAT,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            })
        },
        multisample: wgpu::MultisampleState {
            count: 1,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        fragment: Some(wgpu::FragmentState {
            module: &fragment_shader_module,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: render_format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview: None,
        cache: None,
    })
}

pub fn load_shader_module_descriptor(
    device: &Device,
    shader: &PipelineShader,
    spirv: bool,
) -> Result<wgpu::ShaderModule, std::io::Error> {
    match shader {
        PipelineShader::Path(shader_path) => {
            let shader_contents_bytes = std::fs::read(shader_path)?;
            Ok(device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: None,
                source: if spirv {
                    make_spirv(&shader_contents_bytes)
                } else {
                    let contents = String::from_utf8_lossy(&shader_contents_bytes);
                    wgpu::ShaderSource::Wgsl(contents)
                },
            }))
        }
        PipelineShader::Raw(contents) => {
            Ok(device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: None,
                source: if spirv {
                    make_spirv(contents.as_bytes())
                } else {
                    wgpu::ShaderSource::Wgsl(contents.clone())
                },
            }))
        }
    }
}
