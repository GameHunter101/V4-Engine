use std::collections::HashMap;

use wgpu::{
    BindGroupLayout, Device, RenderPipeline, ShaderStages, TextureFormat, VertexBufferLayout,
    util::make_spirv,
};

use thiserror::Error;

use crate::{ecs::scene::Id, engine_support::texture_support::TextureBundle};

#[derive(Error, Debug)]
pub enum PipelineError {
    #[error("Failed to read the shader at '{path}'")]
    ShaderModuleError { path: String, err: std::io::Error },
}

#[derive(Debug, Default)]
pub struct PipelineManager {
    /// Ready pipelines that have been constructed
    pipelines: HashMap<Id, (PipelineParameters, RenderPipeline)>,
    /// Pipelines that have been specified but are yet to be constructed
    pipelines_queue: Vec<(Id, PipelineParameters, Option<BindGroupLayout>)>,
}

impl PipelineManager {
    pub fn get_pipeline(&self, id: Id) -> Option<&(PipelineParameters, RenderPipeline)> {
        self.pipelines.get(&id)
    }

    fn create_special_bind_group_layouts(
        device: &Device,
        id: &PipelineParameters,
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

        let screenspace_layout = if id.is_screenspace {
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
            .chain(screenspace_layout)
            .collect()
    }

    pub fn add_pipeline_to_creation_queue(
        &mut self,
        pipeline_id: Id,
        pipeline_parameters: PipelineParameters,
        attachment_bind_group_layout: Option<BindGroupLayout>,
    ) {
        self.pipelines_queue.push((
            pipeline_id,
            pipeline_parameters,
            attachment_bind_group_layout,
        ));
    }

    pub fn construct_from_pipeline_queue(
        &mut self,
        device: &Device,
        render_format: TextureFormat,
    ) -> Result<(), PipelineError> {
        for (id, pipeline_parameters, attachment_bind_group_layout) in self.pipelines_queue.clone()
        {
            self.create_render_pipeline(
                Some(id),
                device,
                &pipeline_parameters,
                attachment_bind_group_layout.as_ref(),
                render_format,
            )?;
        }

        self.pipelines_queue = Vec::new();

        Ok(())
    }

    pub fn create_render_pipeline(
        &mut self,
        id: Option<Id>,
        device: &Device,
        descriptor: &PipelineParameters,
        attachment_bind_group_layout: Option<&BindGroupLayout>,
        render_format: TextureFormat,
    ) -> Result<Id, PipelineError> {
        let special_bind_group_layouts =
            Self::create_special_bind_group_layouts(device, descriptor);
        let bind_group_layouts: Vec<&BindGroupLayout> = special_bind_group_layouts
            .iter()
            .chain(attachment_bind_group_layout)
            .collect();

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some(&format!("{id:?} Pipeline Layout")),
            bind_group_layouts: &bind_group_layouts,
            immediate_size: descriptor.immediate_size,
        });

        let vertex_shader_module = Self::load_shader_module_descriptor(
            device,
            descriptor.vertex_shader,
            descriptor.spirv_vertex_shader,
        )?;

        let fragment_shader_module = Self::load_shader_module_descriptor(
            device,
            descriptor.fragment_shader,
            descriptor.spirv_fragment_shader,
        )?;

        let id = if let Some(id) = id { id } else { Id::new_v4() };

        let GeometryDetails {
            topology,
            strip_index_format,
            front_face,
            cull_mode,
            polygon_mode,
        } = descriptor.geometry_details;

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(&format!("{id:?} Pipeline")),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &vertex_shader_module,
                entry_point: Some("main"),
                compilation_options: Default::default(),
                buffers: &descriptor.vertex_layouts,
            },
            primitive: wgpu::PrimitiveState {
                topology,
                strip_index_format,
                front_face,
                cull_mode,
                unclipped_depth: false,
                polygon_mode,
                conservative: false,
            },
            depth_stencil: if descriptor.is_screenspace {
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
        });

        self.pipelines.insert(id, (descriptor.clone(), pipeline));

        Ok(id)
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

    pub fn sorted_pipelines(&self) -> Vec<(Id, &PipelineParameters, &RenderPipeline)> {
        let mut sorted_pipelines: Vec<(Id, &PipelineParameters, &RenderPipeline)> = self
            .pipelines
            .iter()
            .map(|(id, (parameters, pipeline))| (*id, parameters, pipeline))
            .collect();

        sorted_pipelines.sort_by_key(|(_, pipeline, _)| pipeline.render_priority);

        sorted_pipelines
    }

    pub fn pipeline_ids(&self) -> Vec<Id> {
        self.pipelines.keys().copied().collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PipelineAttachments {
    Texture(ShaderStages),
    Buffer(ShaderStages),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PipelineParameters {
    pub vertex_shader: &'static str,
    pub spirv_vertex_shader: bool,
    pub fragment_shader: &'static str,
    pub spirv_fragment_shader: bool,
    pub vertex_layouts: Vec<wgpu::VertexBufferLayout<'static>>,
    pub uses_camera: bool,
    pub is_screenspace: bool,
    pub geometry_details: GeometryDetails,
    pub immediate_size: u32,
    pub render_priority: i32,
}

impl PipelineParameters {
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
            vertex_shader: "../default_shaders/screenspace_vertex.wgsl",
            spirv_vertex_shader: false,
            fragment_shader: shader_path,
            spirv_fragment_shader: spirv_shader,
            vertex_layouts,
            uses_camera: true,
            is_screenspace: true,
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
