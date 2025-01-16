use wgpu::{BindGroupLayout, Device, RenderPipeline, TextureFormat, VertexBufferLayout};

use crate::engine_support::texture_support::Texture;

// pub type PipelineId = (&'static str, &'static str);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PipelineId {
    pub vertex_shader_path: &'static str,
    pub fragment_shader_path: &'static str,
    pub vertex_layouts: Vec<wgpu::VertexBufferLayout<'static>>,
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
) -> RenderPipeline {
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(&format!("{id:?} Pipeline Layout")),
        bind_group_layouts: &bind_group_layouts.iter().collect::<Vec<_>>(),
        push_constant_ranges: &[],
    });

    let vertex_shader_module = load_shader_module_descriptor(device, id.vertex_shader_path);
    if let Err(error) = vertex_shader_module {
        panic!(
            "Vertex shader error for shader {}: {error}",
            id.vertex_shader_path
        );
    }
    let vertex_shader_module = vertex_shader_module.unwrap();

    let fragment_shader_module = load_shader_module_descriptor(device, id.fragment_shader_path);
    if let Err(error) = fragment_shader_module {
        panic!(
            "Fragment shader error for shader {}: {error}",
            id.fragment_shader_path
        );
    }
    let fragment_shader_module = fragment_shader_module.unwrap();

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(&format!("{id:?} Pipeline")),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &vertex_shader_module,
            entry_point: "main",
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
        depth_stencil: Some(wgpu::DepthStencilState {
            format: Texture::DEPTH_FORMAT,
            depth_write_enabled: true,
            depth_compare: wgpu::CompareFunction::Less,
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState {
            count: 1,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        fragment: Some(wgpu::FragmentState {
            module: &fragment_shader_module,
            entry_point: "main",
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
    shader_path: &'static str,
) -> Result<wgpu::ShaderModule, std::io::Error> {
    let shader_contents_bytes = std::fs::read(shader_path)?;
    let shader_contents = String::from_utf8_lossy(&shader_contents_bytes);
    Ok(device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(&format!("{shader_path} Shader Module")),
        source: wgpu::ShaderSource::Wgsl(shader_contents),
    }))
}
