use async_scoped::TokioScope;
use crossbeam_channel::{Receiver, Sender};
use ecs::{
    component::ComponentId,
    scene::{Scene, WorkloadOutput, WorkloadPacket},
};
use engine_management::{
    engine_action::{EngineAction, V4Mutable},
    font_management::FontState,
    pipeline::{create_render_pipeline, PipelineId},
    rendering_management::RenderingManager,
};
use glyphon::{FontSystem, SwashCache, TextAtlas, TextRenderer};
use std::{
    collections::{HashMap, HashSet},
    fmt::Debug,
    time::Instant,
};
use wgpu::Device;
use wgpu::{RenderPipeline, TextureFormat};

use winit::{
    event::{Event, MouseButton},
    event_loop::EventLoop,
    window::{Fullscreen, Window, WindowBuilder},
};
use winit_input_helper::WinitInputHelper;

pub mod engine_management {
    pub mod engine_action;
    pub mod font_management;
    pub mod pipeline;
    pub mod rendering_management;
}

pub mod engine_support {
    pub mod texture_support;
}

pub mod ecs {
    pub mod actions;
    pub mod component;
    pub mod compute;
    pub mod entity;
    pub mod material;
    pub mod scene;
}

/// The main engine struct. Contains the state for the whole engine.
pub struct V4 {
    event_loop: EventLoop<()>,
    input_manager: WinitInputHelper,
    rendering_manager: RenderingManager,
    scenes: Vec<Scene>,
    active_scene: usize,
    initialized_scene: bool,
    window: Window,
    details: EngineDetails,
    pipelines: HashMap<PipelineId, RenderPipeline>,
    font_state: FontState,
}

#[derive(Debug)]
pub struct EngineDetails {
    pub initialization_time: Instant,
    pub frames_elapsed: u128,
    pub last_frame_instant: Instant,
    pub window_resolution: (u32, u32),
    pub cursor_position: (u32, u32),
    pub mouse_state: HashSet<MouseButton>,
    pub cursor_delta: (f32, f32),
}

impl Default for EngineDetails {
    fn default() -> Self {
        Self {
            initialization_time: Instant::now(),
            frames_elapsed: 0,
            last_frame_instant: Instant::now(),
            window_resolution: (0, 0),
            cursor_position: (0, 0),
            mouse_state: HashSet::new(),
            cursor_delta: (0.0, 0.0),
        }
    }
}

impl V4 {
    pub fn builder() -> V4Builder {
        V4Builder::default()
    }

    pub async fn main_loop(mut self) {
        let mut last_active_scene_index = self.active_scene;
        self.details.initialization_time = Instant::now();

        let (workload_sender, workload_output_receiver, _handle) = Self::launch_workload_executor();

        let (engine_action_sender, engine_action_receiver): (
            Sender<Box<dyn EngineAction>>,
            Receiver<_>,
        ) = crossbeam_channel::unbounded();


        self.event_loop
            .run(move |event, elwt| {
                self.input_manager.update(&event);
                match &event {
                    Event::WindowEvent { event, .. } => match event {
                        winit::event::WindowEvent::Resized(new_size) => {
                            self.rendering_manager
                                .resize(new_size.width, new_size.height);

                            self.font_state.viewport.update(
                                self.rendering_manager.queue(),
                                glyphon::Resolution {
                                    width: new_size.width,
                                    height: new_size.height,
                                },
                            );

                            self.details.window_resolution = (new_size.width, new_size.height);
                        }
                        winit::event::WindowEvent::CloseRequested => {
                            elwt.exit();
                        }
                        winit::event::WindowEvent::CursorMoved { position, .. } => {
                            self.details.cursor_position = (position.x as u32, position.y as u32);
                        }
                        winit::event::WindowEvent::MouseInput { button, .. } => {
                            if self.details.mouse_state.contains(button) {
                                self.details.mouse_state.remove(button);
                            } else {
                                self.details.mouse_state.insert(*button);
                            }
                        }
                        _ => {}
                    },
                    Event::DeviceEvent { event, ..} => match event {
                        winit::event::DeviceEvent::MouseMotion { delta } => {
                            self.details.cursor_delta = (delta.0 as f32, delta.1 as f32);
                        },
                        _ => {}
                    }
                    Event::AboutToWait => {
                        if self.scenes.is_empty() {
                            return;
                        }
                        if !self.initialized_scene {
                            let device = self.rendering_manager.device();
                            let queue = self.rendering_manager.queue();
                            let workload_output_receiver = workload_output_receiver.clone();
                            TokioScope::scope_and_block(|scope| {
                                let proc = self.scenes[self.active_scene].initialize(
                                    device,
                                    queue,
                                    workload_sender.clone(),
                                    workload_output_receiver,
                                    engine_action_sender.clone(),
                                );

                                scope.spawn(proc);
                            });
                            self.initialized_scene = true;
                        }
                        if self.active_scene != last_active_scene_index {
                            self.initialized_scene = false;
                            last_active_scene_index = self.active_scene;
                            return;
                        }

                        while let Ok(engine_action) = engine_action_receiver.try_recv() {
                            engine_action.execute(V4Mutable {
                                window: &self.window,
                                active_scene: &mut self.active_scene,
                                initialized_scene: &mut self.initialized_scene,
                                font_state: &mut self.font_state,
                            });
                        }

                        let scene = &mut self.scenes[self.active_scene];
                        let device = self.rendering_manager.device();
                        let queue = self.rendering_manager.queue();

                        TokioScope::scope_and_block(|scope| {
                            scope.spawn(async {
                                scene
                                    .update(device, queue, &self.input_manager, &self.details)
                                    .await;
                                scene
                                    .update_materials(
                                        device,
                                        queue,
                                        &self.input_manager,
                                        &self.details,
                                    )
                                    .await;
                                scene.execute_computes(device, queue);
                            });
                        });

                        Self::create_new_pipelines(
                            device,
                            self.rendering_manager.format(),
                            scene,
                            &mut self.pipelines,
                        );

                        async_scoped::TokioScope::scope_and_block(|scope| {
                            scope.spawn(self.rendering_manager.render(
                                scene,
                                &self.pipelines,
                                &mut self.font_state,
                            ))
                        });

                        self.details.frames_elapsed += 1;
                        self.details.last_frame_instant = Instant::now();
                        self.details.cursor_delta = (0.0, 0.0);
                    }
                    _ => {}
                }
            })
            .expect("An error occured in the main loop.");
    }

    pub fn attach_scene(&mut self, scene: Scene) -> usize {
        let index = self.scenes.len();
        self.scenes.push(scene);

        index
    }

    pub fn scene_count(&self) -> usize {
        self.scenes.len()
    }

    pub fn rendering_manager(&self) -> &RenderingManager {
        &self.rendering_manager
    }

    pub fn launch_workload_executor() -> (
        Sender<WorkloadPacket>,
        Receiver<(ComponentId, WorkloadOutput)>,
        std::thread::JoinHandle<()>,
    ) {
        let (workload_sender, workload_receiver): (
            Sender<WorkloadPacket>,
            Receiver<WorkloadPacket>,
        ) = crossbeam_channel::unbounded();

        let (workload_output_sender, workload_output_receiver): (
            Sender<(ComponentId, WorkloadOutput)>,
            Receiver<_>,
        ) = crossbeam_channel::unbounded();

        let handle = std::thread::spawn(move || {
            let runtime = tokio::runtime::Runtime::new()
                .expect("Failed to create tokio runtime for workloads.");
            runtime.block_on(async move {
                TokioScope::scope_and_block(|async_scope| {
                    if let Ok(workload_packet) = workload_receiver.try_recv() {
                        let sender = workload_output_sender.clone();
                        async_scope.spawn(async move {
                            let workload_result = workload_packet.workload.await;
                            sender
                                .send((workload_packet.component_id, workload_result))
                                .unwrap_or_else(|_| {
                                    panic!(
                                        "Failed to send workload output for component {}",
                                        workload_packet.component_id
                                    )
                                });
                        });
                    }
                });
            });
        });

        (workload_sender, workload_output_receiver, handle)
    }

    fn create_new_pipelines(
        device: &Device,
        render_format: TextureFormat,
        active_scene: &mut Scene,
        pipelines: &mut HashMap<PipelineId, RenderPipeline>,
    ) {
        if active_scene.new_pipelines_needed {
            let active_scene_pipelines = active_scene.get_pipeline_ids();
            for pipeline_id in active_scene_pipelines {
                if !pipelines.contains_key(pipeline_id) {
                    let bind_group_layouts =
                        active_scene.get_pipeline_materials(pipeline_id)[0].bind_group_layouts();

                    pipelines.insert(
                        pipeline_id.clone(),
                        create_render_pipeline(
                            device,
                            pipeline_id,
                            bind_group_layouts,
                            render_format,
                            pipeline_id.spirv_vertex_shader,
                            pipeline_id.spirv_fragment_shader,
                        ),
                    );
                }
            }
            active_scene.new_pipelines_needed = false;
        }
    }
}

impl Debug for V4 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WindowAndEventManager")
            .field("event_loop", &self.event_loop)
            .field("window", &self.window)
            .field("input_manager", &"Input Manager")
            .finish()
    }
}

#[derive(Debug)]
pub struct V4Builder {
    width: u32,
    height: u32,
    title: &'static str,
    fullscreen: Option<Fullscreen>,
    antialiasing_enabled: bool,
    clear_color: wgpu::Color,
    features: wgpu::Features,
    hide_cursor: bool,
    limits: wgpu::Limits,
}

impl Default for V4Builder {
    fn default() -> Self {
        Self {
            width: 800,
            height: 600,
            title: "V4 Program",
            fullscreen: None,
            antialiasing_enabled: false,
            clear_color: wgpu::Color::BLACK,
            features: wgpu::Features::default(),
            hide_cursor: false,
            limits: wgpu::Limits::default(),
        }
    }
}

impl V4Builder {
    pub fn window_settings(
        mut self,
        width: u32,
        height: u32,
        title: &'static str,
        fullscreen: Option<Fullscreen>,
    ) -> Self {
        self.width = width;
        self.height = height;
        self.title = title;
        self.fullscreen = fullscreen;
        self
    }

    pub fn antialiasing_enabled(mut self, enabled: bool) -> Self {
        self.antialiasing_enabled = enabled;
        self
    }

    pub fn clear_color(mut self, color: wgpu::Color) -> Self {
        self.clear_color = color;
        self
    }

    pub fn features(mut self, features: wgpu::Features) -> Self {
        self.features = features;
        self
    }

    pub fn hide_cursor(mut self, hide_cursor: bool) -> Self {
        self.hide_cursor = hide_cursor;
        self
    }

    pub fn limits(mut self, limits: wgpu::Limits) -> Self {
        self.limits = limits;
        self
    }

    pub async fn build(self) -> V4 {
        let event_loop = EventLoop::new().expect("Failed to create event loop.");
        event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);
        let window = WindowBuilder::new()
            .with_inner_size(winit::dpi::LogicalSize::new(self.width, self.height))
            .with_title(self.title)
            .with_fullscreen(self.fullscreen)
            .build(&event_loop)
            .expect("Failed to create window.");
        let input_manager = WinitInputHelper::new();

        let rendering_manager =
            RenderingManager::new(&window, self.antialiasing_enabled, self.clear_color, self.features, self.limits).await;

        let device = rendering_manager.device();
        let queue = rendering_manager.queue();
        let format = rendering_manager.format();

        let font_system = FontSystem::new();
        let swash_cache = SwashCache::new();
        let cache = glyphon::Cache::new(device);
        let viewport = glyphon::Viewport::new(device, &cache);
        let mut atlas = TextAtlas::new(device, queue, &cache, format);
        let text_renderer =
            TextRenderer::new(&mut atlas, device, wgpu::MultisampleState::default(), None);

        window.set_cursor_visible(!self.hide_cursor);
        if self.hide_cursor {
            window.set_cursor_grab(winit::window::CursorGrabMode::Locked).unwrap_or_else(|_| {
                window.set_cursor_grab(winit::window::CursorGrabMode::Confined).unwrap();
            });
        }

        let font_state = FontState {
            font_system,
            swash_cache,
            viewport,
            atlas,
            text_renderer,
            text_buffers: HashMap::new(),
        };

        V4 {
            rendering_manager,
            event_loop,
            input_manager,
            scenes: Vec::new(),
            active_scene: 0,
            initialized_scene: false,
            window,
            details: Default::default(),
            pipelines: HashMap::new(),
            font_state,
        }
    }
}
