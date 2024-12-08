use std::fmt::Debug;

use smaa::SmaaTarget;
use wgpu::rwh::{HasDisplayHandle, HasWindowHandle};

use crate::{ecs::scene::Scene, engine_support::texture_support};

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
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: window_size.width,
            height: window_size.height,
            present_mode: surface_caps.present_modes[0],
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

    pub fn render(&mut self, scene: &mut Scene) {
        {
            let enabled_components = scene.enabled_ui_components();
            let font_state = scene.font_state_mut();
            font_state
                .text_renderer
                .prepare(
                    &self.device,
                    &self.queue,
                    &mut font_state.font_system,
                    &mut font_state.atlas,
                    &font_state.viewport,
                    font_state
                        .text_buffers
                        .iter()
                        .filter(|(id, _)| enabled_components.contains(id))
                        .map(|(_, data)| glyphon::TextArea {
                            buffer: &data.buffer,
                            left: data.top_left_pos[0],
                            top: data.top_left_pos[1],
                            scale: data.scale,
                            bounds: data.bounds,
                            default_color: glyphon::Color::rgb(0, 0, 0),
                            custom_glyphs: &[],
                        }),
                    &mut font_state.swash_cache,
                )
                .expect("Failed to prepare text for rendering.");
        }

        let output = self.surface.get_current_texture().unwrap();

        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

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

            for (pipeline_id, pipeline) in scene.pipelines() {
                render_pass.set_pipeline(pipeline);
                let materials_for_pipeline = scene.get_pipeline_materials(*pipeline_id);
                for material in materials_for_pipeline {
                    let material_bind_groups = material.bind_groups();
                    let mut bind_group_count_accumulated = 0;
                    let material_attachments = material.attachments();
                    for (i, bind_group) in material_bind_groups.iter().enumerate() {
                        render_pass.set_bind_group(bind_group_count_accumulated, bind_group, &[]);
                        bind_group_count_accumulated += match material_attachments[i] {
                            crate::ecs::material::MaterialAttachment::Texture(_) => 2,
                            crate::ecs::material::MaterialAttachment::Buffer(_) => 1,
                        };
                    }
                    let material_id = material.id();
                    for component in &components_sorted_by_material[&material_id] {
                        component.render(&self.device, &self.queue);
                    }
                }
            }

            let font_state = scene.font_state_mut();

            font_state
                .text_renderer
                .render(&font_state.atlas, &font_state.viewport, &mut render_pass)
                .expect("Failed to render text.");
        }
        smaa_frame.resolve();

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
