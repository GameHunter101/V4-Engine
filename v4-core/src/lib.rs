use async_scoped::TokioScope;
use ecs::scene::Scene;
use egui::{FontDefinitions, Style};
use egui_winit_platform::{Platform, PlatformDescriptor};
use engine_management::{
    engine_action::V4Mutable,
    font_management::FontState,
    pipeline::{PipelineId, create_render_pipeline},
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
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event::{DeviceEvent, DeviceId, MouseButton, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    window::{Window, WindowAttributes},
};
use winit_input_helper::WinitInputHelper;

use crate::{
    engine_management::rendering_management::RenderingManagerDetails,
    engine_support::core_communication_support::CoreCommunication,
};

pub mod engine_management;

pub mod engine_support;

pub mod ecs;

/// The main engine struct. Contains the state for the whole engine.
#[derive(Debug)]
pub struct V4 {
    event_loop: EventLoop,
    app: V4App,
}

struct V4App {
    window_attributes: WindowAttributes,
    input_manager: WinitInputHelper,
    rendering_manager: RenderingManager,
    scenes: Vec<Scene>,
    last_active_scene_index: usize,
    active_scene: usize,
    initialized_scene: bool,
    window: Option<Box<dyn Window>>,
    details: EngineDetails,
    pipelines: HashMap<PipelineId, RenderPipeline>,
    font_state: Option<FontState>,
    hide_cursor: bool,
    core_communication: CoreCommunication,
    egui_platform: Option<Platform>,
    egui_clear_color: Option<wgpu::Color>,
}

#[derive(Debug)]
pub struct EngineDetails {
    pub initialization_time: Instant,
    pub frames_elapsed: u128,
    pub last_frame_instant: Instant,
    pub window_resolution: (u32, u32),
    pub scale_factor: f32,
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
            scale_factor: 1.0,
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
        self.app.details.initialization_time = Instant::now();

        self.event_loop
            .run_app(self.app)
            .expect("An error occured in the main loop.");
    }

    pub fn attach_scene(&mut self, scene: Scene) -> usize {
        let index = self.app.scenes.len();
        self.app.scenes.push(scene);

        index
    }

    pub fn scene_count(&self) -> usize {
        self.app.scenes.len()
    }

    pub fn rendering_manager(&self) -> &RenderingManager {
        &self.app.rendering_manager
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
                    let attachment_bind_group_layout =
                        active_scene.get_pipeline_materials(pipeline_id)[0].bind_group_layout();

                    pipelines.insert(
                        pipeline_id.clone(),
                        create_render_pipeline(
                            device,
                            pipeline_id,
                            attachment_bind_group_layout,
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

impl V4App {
    fn resize(&mut self, new_size: PhysicalSize<u32>) {
        self.rendering_manager.resize(
            new_size.width,
            new_size.height,
            self.window.as_ref().unwrap().scale_factor() as f32,
        );

        self.font_state.as_mut().unwrap().viewport.update(
            self.rendering_manager.queue(),
            glyphon::Resolution {
                width: new_size.width,
                height: new_size.height,
            },
        );

        self.details.window_resolution = (new_size.width, new_size.height);
        self.window.as_ref().unwrap().request_redraw();

        self.egui_platform = Some(Platform::new(PlatformDescriptor {
            physical_width: new_size.width,
            physical_height: new_size.width,
            scale_factor: self.details.scale_factor as f64,
            font_definitions: FontDefinitions::default(),
            style: Style::default(),
        }));
    }
}

impl ApplicationHandler for V4App {
    fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        let Ok(window) = event_loop.create_window(self.window_attributes.clone()) else {
            panic!("Failed to create window.")
        };

        self.rendering_manager.initialize_surface_data(&*window);

        let device = self.rendering_manager.device();
        let queue = self.rendering_manager.queue();
        let format = self.rendering_manager.format().unwrap();

        let font_system = FontSystem::new();
        let swash_cache = SwashCache::new();
        let cache = glyphon::Cache::new(device);
        let viewport = glyphon::Viewport::new(device, &cache);
        let mut atlas = TextAtlas::new(device, queue, &cache, format);
        let text_renderer =
            TextRenderer::new(&mut atlas, device, wgpu::MultisampleState::default(), None);

        window.set_cursor_visible(!self.hide_cursor);
        if self.hide_cursor {
            window
                .set_cursor_grab(winit::window::CursorGrabMode::Locked)
                .unwrap_or_else(|_| {
                    window
                        .set_cursor_grab(winit::window::CursorGrabMode::Confined)
                        .unwrap();
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

        self.egui_platform = Some(Platform::new(PlatformDescriptor {
            physical_width: window.surface_size().width,
            physical_height: window.surface_size().width,
            scale_factor: window.scale_factor(),
            font_definitions: FontDefinitions::default(),
            style: Style::default(),
        }));

        self.details.scale_factor = window.scale_factor() as f32;
        self.window = Some(window);
        self.font_state = Some(font_state);
    }

    fn window_event(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        _window_id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        let egui_platform = self.egui_platform.as_mut().unwrap();
        egui_platform.handle_event(&event);
        self.input_manager.process_window_event(&event);
        if self.input_manager.close_requested() || self.input_manager.destroyed() {
            event_loop.exit();
            return;
        }
        match event {
            WindowEvent::SurfaceResized(new_size) => {
                self.resize(new_size);
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                let new_size: PhysicalSize<u32> = self
                    .window_attributes
                    .surface_size
                    .unwrap()
                    .to_physical(scale_factor);
                self.details.window_resolution = new_size.into();
                self.details.scale_factor = scale_factor as f32;
                self.resize(new_size);
            }
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::PointerMoved { position, .. } => {
                self.details.cursor_position = (position.x as u32, position.y as u32);
            }
            WindowEvent::PointerButton { button, .. } => {
                let button = button.clone().mouse_button().unwrap();
                if self.details.mouse_state.contains(&button) {
                    self.details.mouse_state.remove(&button);
                } else {
                    self.details.mouse_state.insert(button);
                }
            }
            WindowEvent::RedrawRequested => {
                if self.scenes.is_empty() {
                    return;
                }
                egui_platform
                    .update_time(self.details.initialization_time.elapsed().as_secs_f64());
                let rendering_manager = &mut self.rendering_manager;
                if !self.initialized_scene {
                    let device = rendering_manager.device();
                    let queue = rendering_manager.queue();
                    let workload_output_receiver =
                        self.core_communication.workload_output_receiver();

                    let action_queue = self.scenes[self.active_scene].initialize(
                        device,
                        self.core_communication.workload_sender(),
                        workload_output_receiver,
                        self.core_communication.engine_action_sender(),
                    );
                    TokioScope::scope_and_block(|scope| {
                        scope.spawn(self.scenes[self.active_scene].execute_action_queue(
                            action_queue,
                            device,
                            queue,
                        ));
                    });
                    self.initialized_scene = true;
                }
                if self.active_scene != self.last_active_scene_index {
                    self.initialized_scene = false;
                    self.last_active_scene_index = self.active_scene;
                    return;
                }

                while let Ok(engine_action) =
                    self.core_communication.engine_action_receiver().try_recv()
                {
                    engine_action.execute(V4Mutable {
                        window: self.window.as_deref().unwrap(),
                        active_scene: &mut self.active_scene,
                        initialized_scene: &mut self.initialized_scene,
                        font_state: self.font_state.as_mut().unwrap(),
                    });
                }

                let scene = &mut self.scenes[self.active_scene];
                let device = rendering_manager.device();
                let queue = rendering_manager.queue();

                let action_queue = scene.update(device, queue, &self.input_manager, &self.details);
                pollster::block_on(scene.execute_action_queue(action_queue, device, queue));

                scene.update_materials(device, queue, &self.input_manager, &self.details);
                rendering_manager.individual_compute_execution(scene.computes());

                V4::create_new_pipelines(
                    device,
                    rendering_manager.format().unwrap(),
                    scene,
                    &mut self.pipelines,
                );

                pollster::block_on(rendering_manager.render(
                    scene,
                    &self.pipelines,
                    self.font_state.as_mut().unwrap(),
                    egui_platform,
                    self.window.as_deref(),
                    self.egui_clear_color,
                ));

                self.details.frames_elapsed += 1;
                self.details.last_frame_instant = Instant::now();
                self.details.cursor_delta = (0.0, 0.0);
            }
            _ => {}
        }
    }

    fn device_event(
        &mut self,
        _event_loop: &dyn ActiveEventLoop,
        _device_id: Option<DeviceId>,
        event: DeviceEvent,
    ) {
        self.input_manager.process_device_event(&event);
        match event {
            DeviceEvent::PointerMotion { delta } => {
                self.details.cursor_delta = (delta.0 as f32, delta.1 as f32);
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &dyn ActiveEventLoop) {
        self.input_manager.end_step();
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }

    fn new_events(&mut self, _event_loop: &dyn ActiveEventLoop, _cause: winit::event::StartCause) {
        self.input_manager.step();
    }
}

impl Debug for V4App {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("V4App")
            .field("window_attributes", &self.window_attributes)
            .field("rendering_manager", &self.rendering_manager)
            .field("scenes", &self.scenes)
            .field("active_scene", &self.active_scene)
            .field("initialized_scene", &self.initialized_scene)
            .field("window", &self.window)
            .field("details", &self.details)
            .field("pipelines", &self.pipelines)
            .field("font_state", &self.font_state)
            .field("hide_cursor", &self.hide_cursor)
            .field("core_communication", &self.core_communication)
            .finish()
    }
}

#[derive(Debug)]
pub struct V4Builder {
    window_attributes: WindowAttributes,
    antialiasing_enabled: bool,
    clear_color: wgpu::Color,
    features: wgpu::Features,
    hide_cursor: bool,
    limits: wgpu::Limits,
    backends: wgpu::Backends,
    egui_clear_color: Option<wgpu::Color>,
}

impl Default for V4Builder {
    fn default() -> Self {
        Self {
            window_attributes: WindowAttributes::default().with_surface_size(
                winit::dpi::Size::Physical(winit::dpi::PhysicalSize::new(800, 800)),
            ),
            antialiasing_enabled: false,
            clear_color: wgpu::Color::BLACK,
            features: wgpu::Features::default(),
            hide_cursor: false,
            limits: wgpu::Limits::default(),
            backends: wgpu::Backends::all(),
            egui_clear_color: None,
        }
    }
}

impl V4Builder {
    pub fn window_attributes(mut self, window_attributes: WindowAttributes) -> Self {
        self.window_attributes = window_attributes;
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

    pub fn backends(mut self, backends: wgpu::Backends) -> Self {
        self.backends = backends;
        self
    }

    pub fn egui_clear_color(mut self, color: wgpu::Color) -> Self {
        self.egui_clear_color = Some(color);
        self
    }

    pub async fn build(self) -> V4 {
        let event_loop = EventLoop::new().expect("Failed to create event loop.");
        event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);
        let input_manager = WinitInputHelper::new();
        let window_attributes = self.window_attributes;

        let window_size = window_attributes.surface_size.unwrap().to_physical(1.0);
        let rendering_manager = RenderingManager::new(
            window_size,
            RenderingManagerDetails {
                antialiasing_enabled: self.antialiasing_enabled,
                clear_color: self.clear_color,
                features: self.features,
                limits: self.limits,
                backends: self.backends,
            },
        )
        .await;

        let app = V4App {
            window_attributes,
            input_manager,
            rendering_manager,
            scenes: Vec::new(),
            last_active_scene_index: usize::MAX,
            active_scene: 0,
            initialized_scene: false,
            window: None,
            details: Default::default(),
            pipelines: HashMap::new(),
            font_state: None,
            hide_cursor: self.hide_cursor,
            core_communication: CoreCommunication::default(),
            egui_platform: None,
            egui_clear_color: self.egui_clear_color,
        };

        V4 { event_loop, app }
    }
}
