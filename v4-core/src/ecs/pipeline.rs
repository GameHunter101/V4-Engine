use wgpu::{
    BindGroupLayout, Device, RenderPipeline, TextureFormat, VertexBufferLayout,
};

use crate::engine_support::texture_support::Texture;

pub type PipelineId = (&'static str, &'static str);

#[derive(Debug)]
pub struct GeometryDetails {
    topology: wgpu::PrimitiveTopology,
    strip_index_format: Option<wgpu::IndexFormat>,
    front_face: wgpu::FrontFace,
    cull_mode: Option<wgpu::Face>,
    polygon_mode: wgpu::PolygonMode,
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

pub struct PipelineDetails<'a> {
    pub vertex_layouts: &'a [wgpu::VertexBufferLayout<'a>],
    pub geometry_details: GeometryDetails,
}

impl<'a> PipelineDetails<'a> {
    pub fn vertex_layouts(&self) -> &[VertexBufferLayout<'_>] {
        self.vertex_layouts
    }

    pub fn geometry_details(&self) -> &GeometryDetails {
        &self.geometry_details
    }
}

pub fn create_render_pipeline(
    device: &Device,
    vertex_shader_path: &'static str,
    fragment_shader_path: &'static str,
    bind_group_layouts: &[BindGroupLayout],
    render_format: TextureFormat,
    pipeline_details: PipelineDetails,
) -> RenderPipeline {
    let id: PipelineId = (vertex_shader_path, fragment_shader_path);

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(&format!("{id:?} Pipeline Layout")),
        bind_group_layouts: &bind_group_layouts.iter().collect::<Vec<_>>(),
        push_constant_ranges: &[],
    });

    let vertex_shader_module = load_shader_module_descriptor(device, vertex_shader_path);
    if let Err(error) = vertex_shader_module {
        panic!("Vertex shader error for shader {vertex_shader_path}: {error}");
    }
    let vertex_shader_module = vertex_shader_module.unwrap();

    let fragment_shader_module = load_shader_module_descriptor(device, fragment_shader_path);
    if let Err(error) = fragment_shader_module {
        panic!("Fragment shader error for shader {fragment_shader_path}: {error}");
    }
    let fragment_shader_module = fragment_shader_module.unwrap();

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(&format!("{id:?} Pipeline")),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &vertex_shader_module,
            entry_point: "main",
            compilation_options: Default::default(),
            buffers: pipeline_details.vertex_layouts,
        },
        primitive: wgpu::PrimitiveState {
            topology: pipeline_details.geometry_details.topology,
            strip_index_format: pipeline_details.geometry_details.strip_index_format,
            front_face: pipeline_details.geometry_details.front_face,
            cull_mode: pipeline_details.geometry_details.cull_mode,
            unclipped_depth: false,
            polygon_mode: pipeline_details.geometry_details.polygon_mode,
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
