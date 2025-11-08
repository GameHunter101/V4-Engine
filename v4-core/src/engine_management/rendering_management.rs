use std::{collections::HashMap, fmt::Debug};

use smaa::SmaaTarget;
use wgpu::{
    rwh::{HasDisplayHandle, HasWindowHandle},
    util::DeviceExt,
    Device, Queue, RenderPipeline, TextureUsages,
};

use crate::{
    ecs::{component::ComponentSystem, scene::Scene},
    engine_management::pipeline::{create_render_pipeline, PipelineId},
    engine_support::texture_support,
};

use super::font_management::FontState;

pub struct RenderingManager {
    surface: wgpu::Surface<'static>,
    format: wgpu::TextureFormat,
    device: Device,
    queue: Queue,
    config: wgpu::SurfaceConfiguration,
    width: u32,
    height: u32,
    depth_texture: texture_support::Texture,
    clear_color: wgpu::Color,
    smaa_target: SmaaTarget,
    screen_space_input_texture: wgpu::Texture,
    screen_space_bind_group: wgpu::BindGroup,
    screen_space_output_pipeline: wgpu::RenderPipeline,
    screen_triangle_buffer: wgpu::Buffer,
}

impl RenderingManager {
    pub async fn new(
        window: &winit::window::Window,
        antialiasing_enabled: bool,
        clear_color: wgpu::Color,
        features: wgpu::Features,
    ) -> Self {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        let window_size = window.inner_size();

        let surface = unsafe {
            instance.create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle {
                raw_display_handle: window.display_handle().unwrap().into(),
                raw_window_handle: window.window_handle().unwrap().into(),
            })
        }
        .expect("Error creating the surface for the given window.");

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptionsBase {
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: false,
                compatible_surface: Some(&surface),
            })
            .await
            .unwrap();

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("Renderer device descriptor"),
                    memory_hints: wgpu::MemoryHints::Performance,
                    required_features: features,
                    ..Default::default()
                },
                None,
            )
            .await
            .unwrap();

        let surface_caps = surface.get_capabilities(&adapter);

        let format = surface_caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(surface_caps.formats[0]);

        let config = wgpu::SurfaceConfiguration {
            usage: TextureUsages::RENDER_ATTACHMENT,
            format,
            width: window_size.width,
            height: window_size.height,
            present_mode: wgpu::PresentMode::AutoNoVsync,
            desired_maximum_frame_latency: 2,
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
        };

        surface.configure(&device, &config);

        let depth_texture = texture_support::Texture::create_depth_texture(&device, &config);

        let smaa_target = SmaaTarget::new(
            &device,
            &queue,
            window_size.width,
            window_size.height,
            config.format,
            if antialiasing_enabled {
                smaa::SmaaMode::Smaa1X
            } else {
                smaa::SmaaMode::Disabled
            },
        );

        let screen_space_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Screen-space render output bind group layout"),
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
            });

        let screen_space_texture_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Screen-space render output sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            lod_min_clamp: 0.0,
            lod_max_clamp: 100.0,
            ..Default::default()
        });

        let screen_space_input_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Screen-space effect output texture"),
            size: wgpu::Extent3d {
                width: window_size.width,
                height: window_size.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
            view_formats: &[],
        });

        let screen_space_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Screen-space render bind group"),
            layout: &screen_space_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(
                        &screen_space_input_texture
                            .create_view(&wgpu::TextureViewDescriptor::default()),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&screen_space_texture_sampler),
                },
            ],
        });

        const SCREEN_SPACE_VERTEX_ATTRIBUTES: &[wgpu::VertexAttribute] =
            &wgpu::vertex_attr_array![0=>Float32x3, 1=>Float32x2];

        let screen_space_output_pipeline_id = PipelineId {
            vertex_shader: crate::engine_management::pipeline::PipelineShader::Raw(
                std::borrow::Cow::Borrowed(
                    "
struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) tex_coords: vec2<f32>,
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
}

@vertex
fn main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    output.position = vec4f(input.position, 1.0);
    output.tex_coords = input.tex_coords;
    return output;
}
",
                ),
            ),
            spirv_vertex_shader: false,
            fragment_shader: crate::engine_management::pipeline::PipelineShader::Raw(
                std::borrow::Cow::Borrowed(
                    "
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
}

@group(0) @binding(0)
var input_tex: texture_2d<f32>;

@group(0) @binding(1)
var input_sampler: sampler;

@fragment
fn main(input: VertexOutput) -> @location(0) vec4<f32> {
    return textureSample(input_tex, input_sampler, input.tex_coords);
}
",
                ),
            ),
            spirv_fragment_shader: false,
            attachments: Vec::new(),
            vertex_layouts: vec![wgpu::VertexBufferLayout {
                array_stride: 4 * 5,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: SCREEN_SPACE_VERTEX_ATTRIBUTES,
            }],
            uses_camera: false,
            is_screen_space: true,
            geometry_details: Default::default(),
        };

        let screen_space_output_pipeline = create_render_pipeline(
            &device,
            &screen_space_output_pipeline_id,
            &[],
            format,
            false,
            false,
        );

        let screen_triangle: [[f32; 5]; 3] = [
            [-1.0, 3.0, 0.0, 0.0, 2.0],
            [-1.0, -1.0, 0.0, 0.0, 0.0],
            [3.0, -1.0, 0.0, 2.0, 0.0],
        ];
        let screen_triangle_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Screen triangle vertex buffer"),
            contents: bytemuck::cast_slice(&[screen_triangle]),
            usage: wgpu::BufferUsages::VERTEX,
        });

        Self {
            surface,
            format,
            device,
            queue,
            config,
            width: window_size.width,
            height: window_size.height,
            depth_texture,
            clear_color,
            smaa_target,
            screen_space_input_texture,
            screen_space_bind_group,
            screen_space_output_pipeline,
            screen_triangle_buffer,
        }
    }

    pub async fn render(
        &mut self,
        scene: &mut Scene,
        pipelines: &HashMap<PipelineId, RenderPipeline>,
        font_state: &mut FontState,
    ) {
        let screen_space_materials = scene.screen_space_materials();
        let output = self.surface.get_current_texture().unwrap();
        let raw_render_tex = if screen_space_materials.is_empty() {
            &output.texture
        } else {
            &self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some(&format!(
                    "Scene {} raw render output texture",
                    scene.scene_index()
                )),
                size: wgpu::Extent3d {
                    width: self.width,
                    height: self.height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: self.format,
                usage: TextureUsages::RENDER_ATTACHMENT
                    | TextureUsages::TEXTURE_BINDING
                    | TextureUsages::COPY_SRC,
                view_formats: &[],
            })
        };

        let view = raw_render_tex.create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render encoder"),
            });

        let smaa_frame = self
            .smaa_target
            .start_frame(&self.device, &self.queue, &view);

        let all_components = scene.all_components();

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Main render pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &smaa_frame,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(self.clear_color),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: self.depth_texture.view_ref(),
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });


            for (pipeline_id, pipeline) in pipelines {
                if pipeline_id.is_screen_space {
                    continue;
                }
                render_pass.set_pipeline(pipeline);
                let materials_for_pipeline = scene.get_pipeline_materials(pipeline_id);
                for material in materials_for_pipeline {
                    if material.uses_camera() {
                        render_pass.set_bind_group(
                            0,
                            scene
                                .active_camera_bind_group()
                                .expect("No active camera buffer set"),
                            &[],
                        );
                    }

                    material.render(&self.device, &self.queue, &mut render_pass, &all_components);
                }
            }
        }

        for material in scene.materials() {
            material.command_encoder_operations(
                &self.device,
                &self.queue,
                &mut encoder,
                &all_components,
                scene.materials(),
                scene.computes(),
            );
        }

        smaa_frame.resolve();

        let output_view = if screen_space_materials.is_empty() {
            view
        } else {
            output
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default())
        };

        if !screen_space_materials.is_empty() {
            encoder.copy_texture_to_texture(
                raw_render_tex.as_image_copy(),
                self.screen_space_input_texture.as_image_copy(),
                wgpu::Extent3d {
                    width: self.width,
                    height: self.height,
                    depth_or_array_layers: 1,
                },
            );
            let screen_space_output = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Screen space effect output texture"),
                size: wgpu::Extent3d {
                    width: self.width,
                    height: self.height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: self.format,
                usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
                view_formats: &[],
            });

            let screen_space_output_view =
                screen_space_output.create_view(&wgpu::TextureViewDescriptor::default());

            for material_id in scene.screen_space_materials() {
                let material = scene
                    .get_material(*material_id)
                    .expect("Invalid material ID");
                if let Some(pipeline) = pipelines.get(material.pipeline_id()) {
                    let mut effect_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("Effect render pass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &screen_space_output_view,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        depth_stencil_attachment: None,
                        timestamp_writes: None,
                        occlusion_query_set: None,
                    });

                    effect_pass.set_pipeline(pipeline);

                    effect_pass.set_bind_group(0, &self.screen_space_bind_group, &[]);

                    for (i, bind_group) in material.bind_groups().iter().enumerate() {
                        effect_pass.set_bind_group(i as u32 + 1, bind_group, &[]);
                    }

                    effect_pass.set_vertex_buffer(0, self.screen_triangle_buffer.slice(..));
                    effect_pass.draw(0..3, 0..1);
                }
                encoder.copy_texture_to_texture(
                    screen_space_output.as_image_copy(),
                    self.screen_space_input_texture.as_image_copy(),
                    wgpu::Extent3d {
                        width: self.width,
                        height: self.height,
                        depth_or_array_layers: 1,
                    },
                );
            }

            let mut screen_space_application_render_pass =
                encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Screen-space display render pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &output_view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });

            screen_space_application_render_pass.set_pipeline(&self.screen_space_output_pipeline);
            screen_space_application_render_pass.set_bind_group(
                0,
                &self.screen_space_bind_group,
                &[],
            );
            screen_space_application_render_pass
                .set_vertex_buffer(0, self.screen_triangle_buffer.slice(..));
            screen_space_application_render_pass.draw(0..3, 0..1);
        }

        let enabled_components = scene.enabled_ui_components();
        let text_areas = font_state
            .text_buffers
            .iter()
            .filter(|(id, _)| enabled_components.contains(id))
            .map(|(_, data)| glyphon::TextArea {
                buffer: &data.buffer,
                left: data.top_left_pos[0],
                top: data.top_left_pos[1],
                scale: data.scale,
                bounds: data.bounds,
                default_color: data.attributes.color,
                custom_glyphs: &[],
            })
            .collect::<Vec<_>>();

        font_state
            .text_renderer
            .prepare(
                &self.device,
                &self.queue,
                &mut font_state.font_system,
                &mut font_state.atlas,
                &font_state.viewport,
                text_areas,
                &mut font_state.swash_cache,
            )
            .expect("Failed to prepare text for rendering.");

        {
            let mut ui_render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("UI Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &output_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            font_state
                .text_renderer
                .render(&font_state.atlas, &font_state.viewport, &mut ui_render_pass)
                .expect("Failed to render text.");
        }

        self.queue.submit(Some(encoder.finish()));
        output.present();
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.width = width;
        self.height = height;
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
        self.depth_texture =
            texture_support::Texture::create_depth_texture(&self.device, &self.config);
        self.smaa_target.resize(&self.device, width, height);
    }

    pub fn device(&self) -> &Device {
        &self.device
    }

    pub fn queue(&self) -> &Queue {
        &self.queue
    }

    pub fn format(&self) -> wgpu::TextureFormat {
        self.format
    }

    pub fn surface(&self) -> &wgpu::Surface<'static> {
        &self.surface
    }

    pub fn smaa_target_mut(&mut self) -> &mut SmaaTarget {
        &mut self.smaa_target
    }
}

impl Debug for RenderingManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RenderingManager")
            .field("surface", &self.surface)
            .field("format", &self.format)
            .field("device", &self.device)
            .field("queue", &self.queue)
            .field("config", &self.config)
            .field("width", &self.width)
            .field("height", &self.height)
            .field("depth_texture", &self.depth_texture)
            .field("clear_color", &self.clear_color)
            .field("smaa_target", &"smaa target".to_string())
            .finish()
    }
}
