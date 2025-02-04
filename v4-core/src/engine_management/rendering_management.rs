use std::{collections::HashMap, fmt::Debug};

use smaa::SmaaTarget;
use wgpu::{
    rwh::{HasDisplayHandle, HasWindowHandle},
    util::DeviceExt,
    RenderPipeline, TextureUsages,
};

use crate::{
    ecs::{pipeline::PipelineId, scene::Scene},
    engine_support::texture_support,
};

use super::font_management::FontState;

pub struct RenderingManager {
    surface: wgpu::Surface<'static>,
    format: wgpu::TextureFormat,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    width: u32,
    height: u32,
    depth_texture: texture_support::Texture,
    clear_color: wgpu::Color,
    smaa_target: SmaaTarget,
}

impl<'a> RenderingManager {
    pub async fn new(
        window: &'a winit::window::Window,
        antialiasing_enabled: bool,
        clear_color: wgpu::Color,
    ) -> Self {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
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
        }
    }

    pub fn render(
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
                    | TextureUsages::COPY_DST,
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

            let components_sorted_by_material = scene.get_components_per_material();

            for (pipeline_id, pipeline) in pipelines {
                if pipeline_id.is_screen_space {
                    continue;
                }
                render_pass.set_pipeline(pipeline);
                let materials_for_pipeline = scene.get_pipeline_materials(pipeline_id);
                for material in materials_for_pipeline {
                    let material_bind_groups = material.bind_groups();

                    if material.uses_camera() {
                        render_pass.set_bind_group(
                            0,
                            scene
                                .active_camera_bind_group()
                                .expect("No active camera buffer set"),
                            &[],
                        );
                    }

                    for (i, bind_group) in material_bind_groups.iter().enumerate() {
                        render_pass.set_bind_group(i as u32, bind_group, &[]);
                    }
                    let material_id = material.id();
                    for component in &components_sorted_by_material[&material_id] {
                        component.render(&self.device, &self.queue, &mut render_pass);
                    }
                }
            }
        }

        smaa_frame.resolve();

        if !screen_space_materials.is_empty() {
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
            let screen_space_input_sampler = self.device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("Screen space render output sampler"),
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

            let screen_space_output_view =
                screen_space_output.create_view(&wgpu::TextureViewDescriptor::default());

            let screen_space_input_bind_group_layout =
                self.device
                    .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                        label: Some("Screen space render output bind grouop layout"),
                        entries: &[
                            wgpu::BindGroupLayoutEntry {
                                binding: 0,
                                visibility: wgpu::ShaderStages::FRAGMENT,
                                ty: wgpu::BindingType::Texture {
                                    sample_type: wgpu::TextureSampleType::Float {
                                        filterable: true,
                                    },
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

            let screen_space_input_bind_group =
                self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("Screen space render output bind group"),
                    layout: &screen_space_input_bind_group_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(&view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(&screen_space_input_sampler),
                        },
                    ],
                });

            let screen_triangle: [[f32; 5]; 3] = [
                [-0.5, 1.5, 0.0, 0.0, 2.0],
                [-0.5, -0.5, 0.0, 0.0, 0.0],
                [0.5, 1.5, 0.0, 2.0, 0.0],
            ];
            let screen_triangle_buffer =
                self.device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("Screen triangle vertex buffer"),
                        contents: bytemuck::cast_slice(&[screen_triangle]),
                        usage: wgpu::BufferUsages::VERTEX,
                    });

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

                    effect_pass.set_bind_group(0, &screen_space_input_bind_group, &[]);

                    for (i, bind_group) in material.bind_groups().iter().enumerate() {
                        effect_pass.set_bind_group(i as u32 + 1, bind_group, &[]);
                    }

                    effect_pass.set_vertex_buffer(0, screen_triangle_buffer.slice(..));
                    effect_pass.draw(0..3, 0..1);
                }
            }
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
                    view: &view,
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

        self.queue.submit(std::iter::once(encoder.finish()));
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

    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    pub fn queue(&self) -> &wgpu::Queue {
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
