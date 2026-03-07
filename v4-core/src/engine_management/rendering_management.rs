use std::{collections::HashMap, fmt::Debug};

use egui::TexturesDelta;
use egui_wgpu_backend::{RenderPass as EguiRenderPass, ScreenDescriptor};
use egui_winit_platform::Platform;
use smaa::SmaaTarget;
use wgpu::{
    Adapter, BindGroup, Buffer, CommandEncoder, Device, Instance, Queue, RenderPipeline, Texture,
    TextureFormat, TextureUsages, TextureView,
    rwh::{HasDisplayHandle, HasWindowHandle},
    util::DeviceExt,
};
use winit::{dpi::PhysicalSize, window::Window};

use crate::{
    ecs::{
        component::{Component, ComponentDetails, ComponentSystem},
        compute::Compute,
        scene::Scene,
    },
    engine_management::pipeline::{PipelineId, create_render_pipeline},
    engine_support::texture_support,
};

use super::font_management::FontState;

#[derive(Debug)]
pub struct RenderingManager {
    instance: Instance,
    adapter: Adapter,
    device: Device,
    queue: Queue,
    width: u32,
    height: u32,
    clear_color: wgpu::Color,
    antialiasing_enabled: bool,
    surface_data: Option<SurfaceData>,
}

pub struct SurfaceData {
    surface: wgpu::Surface<'static>,
    format: wgpu::TextureFormat,
    config: wgpu::SurfaceConfiguration,
    smaa_target: SmaaTarget,
    depth_texture: texture_support::CompleteTexture,
    screen_space_attachments: ScreenSpaceAttachments,
    egui_render_pass: EguiRenderPass,
    egui_screen_descriptor: ScreenDescriptor,
}

impl Debug for SurfaceData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SurfaceData")
            .field("surface", &self.surface)
            .field("format", &self.format)
            .field("config", &self.config)
            .field("smaa_target", &"smaa_target")
            .field("depth_texture", &self.depth_texture)
            .field("screen_space_attachments", &self.screen_space_attachments)
            .finish()
    }
}

#[derive(Debug, Clone)]
pub struct RenderingManagerDetails {
    pub antialiasing_enabled: bool,
    pub clear_color: wgpu::Color,
    pub features: wgpu::Features,
    pub limits: wgpu::Limits,
    pub backends: wgpu::Backends,
}

impl RenderingManager {
    pub async fn new(
        window_size: PhysicalSize<u32>,
        RenderingManagerDetails {
            antialiasing_enabled,
            clear_color,
            features,
            limits,
            backends,
        }: RenderingManagerDetails,
    ) -> Self {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends,
            ..Default::default()
        });

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptionsBase {
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: false,
                compatible_surface: None,
            })
            .await
            .unwrap();

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("Renderer device descriptor"),
                memory_hints: wgpu::MemoryHints::Performance,
                required_features: features,
                required_limits: limits,
                ..Default::default()
            })
            .await
            .unwrap();
        let (width, height): (u32, u32) = window_size.into();

        RenderingManager {
            instance,
            adapter,
            width,
            height,
            device,
            queue,
            clear_color,
            antialiasing_enabled,
            surface_data: None,
        }
    }

    pub fn initialize_surface_data(&mut self, window: &dyn Window) {
        let surface = unsafe {
            self.instance
                .create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle {
                    raw_display_handle: window.display_handle().unwrap().into(),
                    raw_window_handle: window.window_handle().unwrap().into(),
                })
        }
        .expect("Error creating the surface for the given window.");

        let surface_caps = surface.get_capabilities(&self.adapter);

        let format = surface_caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(surface_caps.formats[0]);

        let config = wgpu::SurfaceConfiguration {
            usage: TextureUsages::RENDER_ATTACHMENT,
            format,
            width: self.width,
            height: self.height,
            present_mode: wgpu::PresentMode::AutoNoVsync,
            desired_maximum_frame_latency: 2,
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
        };

        surface.configure(&self.device, &config);

        let depth_texture =
            texture_support::TextureBundle::create_depth_texture(&self.device, &config);

        let smaa_target = SmaaTarget::new(
            &self.device,
            &self.queue,
            self.width,
            self.height,
            format,
            if self.antialiasing_enabled {
                smaa::SmaaMode::Smaa1X
            } else {
                smaa::SmaaMode::Disabled
            },
        );

        let screen_space_attachments =
            ScreenSpaceAttachments::new(&self.device, self.width, self.height, format);

        let egui_render_pass = EguiRenderPass::new(&self.device, format, 1);
        let egui_screen_descriptor = ScreenDescriptor {
            physical_width: self.width,
            physical_height: self.height,
            scale_factor: window.scale_factor() as f32,
        };

        self.surface_data = Some(SurfaceData {
            surface,
            format,
            config,
            smaa_target,
            depth_texture,
            screen_space_attachments,
            egui_render_pass,
            egui_screen_descriptor,
        });
    }

    pub async fn render(
        &mut self,
        scene: &mut Scene,
        pipelines: &HashMap<PipelineId, RenderPipeline>,
        font_state: &mut FontState,
        egui_platform: &mut Platform,
        window: Option<&dyn Window>,
        egui_clear_color: Option<wgpu::Color>,
    ) {
        let screen_space_materials = scene.screen_space_materials();
        let surface_data = self.surface_data.as_mut().unwrap();
        let output = surface_data.surface.get_current_texture().unwrap();
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
                format: surface_data.format,
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

        let smaa_frame = surface_data
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
                    depth_slice: None,
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: surface_data.depth_texture.1.view(),
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            let mut sorted_pipelines: Vec<(&PipelineId, &RenderPipeline)> =
                Vec::from_iter(pipelines);
            sorted_pipelines.sort_by(|(a, _), (b, _)| a.render_priority.cmp(&b.render_priority));

            for (pipeline_id, pipeline) in sorted_pipelines {
                if pipeline_id.is_screen_space {
                    continue;
                }
                render_pass.set_pipeline(pipeline);
                let materials_for_pipeline = scene.get_pipeline_materials(pipeline_id);
                for material in materials_for_pipeline
                    .iter()
                    .filter(|mat| scene.is_component_enabled(**mat))
                {
                    if material.uses_camera() {
                        render_pass.set_bind_group(
                            0,
                            scene
                                .active_camera_bind_group()
                                .expect("No active camera buffer set"),
                            &[],
                        );
                    }

                    if pipeline_id.immediate_size != 0 {
                        render_pass.set_immediates(0, material.get_immediate_data());
                    }

                    material.render(&self.device, &self.queue, &mut render_pass, &all_components);
                }
            }
        }

        for material in scene.materials().iter().filter(|mat| mat.is_enabled()) {
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
            Self::render_screen_space_effects(
                scene,
                self.width,
                self.height,
                pipelines,
                &output_view,
                &self.device,
                raw_render_tex,
                surface_data,
                &mut encoder,
            );
        }

        let enabled_ui_components = scene.enabled_ui_components();
        let text_areas = font_state
            .text_buffers
            .iter()
            .filter(|(id, _)| enabled_ui_components.contains(id))
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
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            font_state
                .text_renderer
                .render(&font_state.atlas, &font_state.viewport, &mut ui_render_pass)
                .expect("Failed to render text.");
        }
        let tdelta_option = if !enabled_ui_components.is_empty() {
            let all_comps = scene.all_components_mut();
            let ui_components: Vec<&mut Component> = all_comps
                .into_iter()
                .filter(|comp| enabled_ui_components.contains(&comp.id()))
                .collect();
            Some(Self::egui_render(
                ui_components,
                egui_platform,
                &mut surface_data.egui_render_pass,
                &surface_data.egui_screen_descriptor,
                window.as_deref(),
                &self.device,
                &self.queue,
                &mut encoder,
                &output_view,
                egui_clear_color
            ))
        } else {
            None
        };

        self.queue.submit(Some(encoder.finish()));
        output.present();

        if let Some(tdelta) = tdelta_option {
            surface_data
                .egui_render_pass
                .remove_textures(tdelta)
                .unwrap();
        }
    }

    fn render_screen_space_effects(
        scene: &Scene,
        width: u32,
        height: u32,
        pipelines: &HashMap<PipelineId, RenderPipeline>,
        output_view: &TextureView,
        device: &Device,
        raw_render_tex: &Texture,
        surface_data: &SurfaceData,
        encoder: &mut CommandEncoder,
    ) {
        encoder.copy_texture_to_texture(
            raw_render_tex.as_image_copy(),
            surface_data
                .screen_space_attachments
                .screen_space_input_texture
                .as_image_copy(),
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        let screen_space_output = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Screen space effect output texture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: surface_data.format,
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
                surface_data
                    .screen_space_attachments
                    .execute_effect_render_pass(
                        encoder,
                        &screen_space_output_view,
                        pipeline,
                        material.bind_group().unwrap(),
                    );
            }
            encoder.copy_texture_to_texture(
                screen_space_output.as_image_copy(),
                surface_data
                    .screen_space_attachments
                    .screen_space_input_texture
                    .as_image_copy(),
                wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
            );
        }

        surface_data
            .screen_space_attachments
            .execute_output_render_pass(encoder, &output_view);
    }

    fn egui_render(
        ui_components: Vec<&mut Component>,
        platform: &mut Platform,
        render_pass: &mut EguiRenderPass,
        screen_descriptor: &ScreenDescriptor,
        window: Option<&dyn Window>,
        device: &Device,
        queue: &Queue,
        encoder: &mut CommandEncoder,
        output_view: &TextureView,
        egui_clear_color: Option<wgpu::Color>,
    ) -> TexturesDelta {
        platform.begin_pass();
        for component in ui_components {
            component.ui_render(&platform.context());
        }
        let full_output = platform.end_pass(window);
        let paint_jobs = platform
            .context()
            .tessellate(full_output.shapes, full_output.pixels_per_point);
        let tdelta = full_output.textures_delta;
        render_pass.add_textures(device, queue, &tdelta).unwrap();
        render_pass.update_buffers(device, queue, &paint_jobs, screen_descriptor);
        render_pass
            .execute(
                encoder,
                output_view,
                &paint_jobs,
                screen_descriptor,
                egui_clear_color,
            )
            .unwrap();
        tdelta
    }

    pub fn resize(&mut self, physical_width: u32, physical_height: u32, scale_factor: f32,) {
        self.width = physical_width;
        self.height = physical_height;
        let surface_data = self.surface_data.as_mut().unwrap();

        surface_data.config.width = physical_width;
        surface_data.config.height = physical_height;
        surface_data
            .surface
            .configure(&self.device, &surface_data.config);
        surface_data.depth_texture = texture_support::TextureBundle::create_depth_texture(
            &self.device,
            &surface_data.config,
        );
        surface_data.smaa_target.resize(&self.device, physical_width, physical_height);
        surface_data.egui_screen_descriptor = ScreenDescriptor {
            physical_width,
            physical_height,
            scale_factor,
        }
    }

    pub fn device(&self) -> &Device {
        &self.device
    }

    pub fn queue(&self) -> &Queue {
        &self.queue
    }

    pub fn format(&self) -> Option<wgpu::TextureFormat> {
        self.surface_data
            .as_ref()
            .map(|surface_data| surface_data.format)
    }

    pub fn surface(&self) -> Option<&wgpu::Surface<'static>> {
        self.surface_data
            .as_ref()
            .map(|surface_data| &surface_data.surface)
    }

    pub fn smaa_target_mut(&mut self) -> Option<&mut SmaaTarget> {
        self.surface_data
            .as_mut()
            .map(|surface_data| &mut surface_data.smaa_target)
    }

    pub fn individual_compute_execution(&self, computes: &[Compute]) {
        let mut encoder =
            self.device
                .create_command_encoder(&wgpu::wgt::CommandEncoderDescriptor {
                    label: Some("Individual compute encoder"),
                });

        {
            let mut compute_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Compute pass"),
                timestamp_writes: None,
            });

            for compute in computes {
                for _ in 0..compute.iterate_count() {
                    compute.calculate(&mut compute_pass);
                }
            }
        }

        self.queue.submit(Some(encoder.finish()));
    }
}

#[derive(Debug)]
struct ScreenSpaceAttachments {
    screen_space_input_texture: Texture,
    screen_space_bind_group: BindGroup,
    screen_triangle_buffer: Buffer,
    screen_space_output_pipeline: RenderPipeline,
}

impl ScreenSpaceAttachments {
    fn new(device: &Device, width: u32, height: u32, format: TextureFormat) -> Self {
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
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            lod_min_clamp: 0.0,
            lod_max_clamp: 100.0,
            ..Default::default()
        });

        let screen_space_input_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Screen-space effect output texture"),
            size: wgpu::Extent3d {
                width,
                height,
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

        let screen_space_output_pipeline_id = PipelineId {
            vertex_shader: crate::engine_management::pipeline::PipelineShader::Raw(
                std::borrow::Cow::Borrowed(include_str!(
                    "../default_shaders/screen_space_vertex.wgsl"
                )),
            ),
            spirv_vertex_shader: false,
            fragment_shader: crate::engine_management::pipeline::PipelineShader::Raw(
                std::borrow::Cow::Borrowed(include_str!(
                    "../default_shaders/screen_space_output_fragment.wgsl"
                )),
            ),
            spirv_fragment_shader: false,
            vertex_layouts: vec![wgpu::VertexBufferLayout {
                array_stride: 4 * 5,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: SCREEN_SPACE_VERTEX_ATTRIBUTES,
            }],
            uses_camera: false,
            is_screen_space: true,
            geometry_details: Default::default(),
            immediate_size: 0,
            render_priority: i32::MAX,
        };

        let screen_space_output_pipeline = create_render_pipeline(
            &device,
            &screen_space_output_pipeline_id,
            None,
            format,
            false,
            false,
        );

        ScreenSpaceAttachments {
            screen_space_input_texture,
            screen_space_bind_group,
            screen_triangle_buffer,
            screen_space_output_pipeline,
        }
    }

    fn execute_effect_render_pass(
        &self,
        encoder: &mut CommandEncoder,
        screen_space_output_view: &TextureView,
        pipeline: &RenderPipeline,
        material_bind_group: &BindGroup,
    ) {
        let mut effect_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Effect render pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: screen_space_output_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });

        effect_pass.set_pipeline(pipeline);

        effect_pass.set_bind_group(0, &self.screen_space_bind_group, &[]);

        effect_pass.set_bind_group(1, material_bind_group, &[]);

        effect_pass.set_vertex_buffer(0, self.screen_triangle_buffer.slice(..));
        effect_pass.draw(0..3, 0..1);
    }

    fn execute_output_render_pass(&self, encoder: &mut CommandEncoder, output_view: &TextureView) {
        let mut screen_space_application_render_pass =
            encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Screen-space display render pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: output_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

        screen_space_application_render_pass.set_pipeline(&self.screen_space_output_pipeline);
        screen_space_application_render_pass.set_bind_group(0, &self.screen_space_bind_group, &[]);
        screen_space_application_render_pass
            .set_vertex_buffer(0, self.screen_triangle_buffer.slice(..));
        screen_space_application_render_pass.draw(0..3, 0..1);
    }
}
