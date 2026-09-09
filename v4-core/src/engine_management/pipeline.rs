use wgpu::{
    BindGroupLayout, Device, RenderPipeline, ShaderStages, TextureFormat, VertexBufferLayout,
    util::make_spirv,
};

use thiserror::Error;

use crate::engine_support::texture_support::TextureBundle;

#[derive(Error, Debug)]
pub enum PipelineError {
    #[error("Failed to read the shader at '{path}'")]
    ShaderModuleError { path: String, err: std::io::Error },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PipelineAttachments {
    Texture(ShaderStages),
    Buffer(ShaderStages),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PipelineDescriptor {
    pub vertex_shader: &'static str,
    pub spirv_vertex_shader: bool,
    pub fragment_shader: &'static str,
    pub spirv_fragment_shader: bool,
    pub vertex_layouts: Vec<wgpu::VertexBufferLayout<'static>>,
    pub uses_camera: bool,
    pub is_screen_space: bool,
    pub geometry_details: GeometryDetails,
    pub immediate_size: u32,
    pub render_priority: i32,
}

impl PipelineDescriptor {
    pub fn vertex_layouts<'a>(&'a self) -> &'a [VertexBufferLayout<'a>] {
        &self.vertex_layouts
    }

    pub fn geometry_details(&self) -> &GeometryDetails {
        &self.geometry_details
    }

    pub fn screenspace_shader(
        shader_path: &'static str,
        spirv_shader: bool,
        immediate_size: u32,
    ) -> Self {
        const ATTRIBUTES: &[wgpu::VertexAttribute] =
            &wgpu::vertex_attr_array![0=>Float32x3, 1=>Float32x2];
        let vertex_layouts = vec![wgpu::VertexBufferLayout {
            array_stride: 4 * 5,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: ATTRIBUTES,
        }];

        Self {
            vertex_shader: "../default_shaders/screen_space_vertex.wgsl",
            spirv_vertex_shader: false,
            fragment_shader: shader_path,
            spirv_fragment_shader: spirv_shader,
            vertex_layouts,
            uses_camera: true,
            is_screen_space: true,
            geometry_details: Default::default(),
            immediate_size,
            render_priority: i32::MAX,
        }
    }
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

fn create_special_bind_group_layouts(
    device: &Device,
    id: &PipelineDescriptor,
) -> Vec<BindGroupLayout> {
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

    camera_layout
        .into_iter()
        .chain(screen_space_layout)
        .collect()
}

pub fn create_render_pipeline(
    device: &Device,
    id: &PipelineDescriptor,
    attachment_bind_group_layout: Option<&BindGroupLayout>,
    render_format: TextureFormat,
    is_vert_spirv: bool,
    is_frag_spirv: bool,
) -> Result<RenderPipeline, PipelineError> {
    let special_bind_group_layouts = create_special_bind_group_layouts(device, id);
    let bind_group_layouts: Vec<&BindGroupLayout> = special_bind_group_layouts
        .iter()
        .chain(attachment_bind_group_layout)
        .collect();

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(&format!("{id:?} Pipeline Layout")),
        bind_group_layouts: &bind_group_layouts,
        immediate_size: id.immediate_size,
    });

    let vertex_shader_module =
        load_shader_module_descriptor(device, id.vertex_shader, is_vert_spirv)?;

    let fragment_shader_module =
        load_shader_module_descriptor(device, id.fragment_shader, is_frag_spirv)?;

    Ok(
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
                    format: TextureBundle::DEPTH_FORMAT,
                    depth_write_enabled: true,
                    depth_compare: wgpu::CompareFunction::LessEqual,
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
            multiview_mask: None,
            cache: None,
        }),
    )
}

pub fn load_shader_module_descriptor(
    device: &Device,
    shader_path: &str,
    spirv: bool,
) -> Result<wgpu::ShaderModule, PipelineError> {
    let shader_contents_bytes =
        std::fs::read(shader_path).map_err(|err| PipelineError::ShaderModuleError {
            path: shader_path.to_string(),
            err,
        })?;

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
