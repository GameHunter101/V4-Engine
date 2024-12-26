use async_scoped::TokioScope;
use ecs::scene::{Scene, WorkloadOutputCollection, WorkloadPacket};
use engine_management::rendering_management::RenderingManager;
use std::{
    collections::{HashMap, HashSet},
    fmt::Debug,
    sync::{mpsc, Arc},
    time::Instant,
};
use tokio::sync::Mutex;

use winit::{
    event::{Event::WindowEvent, MouseButton},
    event_loop::EventLoop,
    window::{Fullscreen, Window, WindowBuilder},
};
use winit_input_helper::WinitInputHelper;

pub mod engine_management {
    pub mod rendering_management;
}

pub mod engine_support {
    pub mod texture_support;
}

pub mod ecs {
    pub mod actions;
    pub mod component;
    pub mod entity;
    pub mod material;
    pub mod pipeline;
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
}

#[derive(Debug)]
pub struct EngineDetails {
    pub initialization_time: Instant,
    pub frames_elapsed: u128,
    pub last_frame_instant: Instant,
    pub window_resolution: (u32, u32),
    pub cursor_position: (u32, u32),
    pub mouse_state: HashSet<MouseButton>,
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

        let (workload_sender, workload_outputs, _handle) = Self::launch_workload_executor();

        self.event_loop
            .run(move |event, elwt| {
                self.input_manager.update(&event);
                match &event {
                    WindowEvent { event, .. } => match event {
                        winit::event::WindowEvent::Resized(new_size) => {
                            self.rendering_manager
                                .resize(new_size.width, new_size.height);
                            self.scenes[self.active_scene].update_text_viewport(
                                self.rendering_manager.queue(),
                                (new_size.width, new_size.height),
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
                    winit::event::Event::AboutToWait => {
                        if self.scenes.is_empty() {
                            return;
                        }
                        if !self.initialized_scene {
                            let device = self.rendering_manager.device();
                            TokioScope::scope_and_block(|scope| {
                                let proc = self.scenes[self.active_scene].initialize(
                                    device,
                                    workload_sender.clone(),
                                    workload_outputs.clone(),
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
                        let scene = &mut self.scenes[self.active_scene];
                        self.rendering_manager.render(scene);
                        let device = self.rendering_manager.device();
                        let queue = self.rendering_manager.queue();

                        TokioScope::scope_and_block(|scope| {
                            scope.spawn(async {
                                scene
                                    .update(device, queue, &self.input_manager, &self.details)
                                    .await;
                            });
                        });
                        self.details.frames_elapsed += 1;
                        self.details.last_frame_instant = Instant::now();
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
        mpsc::Sender<WorkloadPacket>,
        WorkloadOutputCollection,
        std::thread::JoinHandle<()>,
    ) {
        let (workload_sender, workload_receiver): (
            mpsc::Sender<WorkloadPacket>,
            mpsc::Receiver<WorkloadPacket>,
        ) = mpsc::channel();

        let outputs: WorkloadOutputCollection = Arc::new(Mutex::new(HashMap::new()));

        let workload_outputs_arc = outputs.clone();
        let handle = std::thread::spawn(move || {
            let runtime = tokio::runtime::Runtime::new()
                .expect("Failed to create tokio runtime for workloads.");
            runtime.block_on(async move {
                TokioScope::scope_and_block(|async_scope| loop {
                    if let Ok(workload_packet) = workload_receiver.try_recv() {
                        let workload_outputs_arc = workload_outputs_arc.clone();
                        async_scope.spawn(async move {
                            let workload_result = workload_packet.workload.await;
                            let mut workload_outputs = workload_outputs_arc.lock().await;
                            if let Some(component_outputs) =
                                workload_outputs.get_mut(&workload_packet.component_id)
                            {
                                component_outputs.push(workload_result);
                            } else {
                                workload_outputs
                                    .insert(workload_packet.component_id, vec![workload_result]);
                            }
                        });
                    }
                });
            });
        });

        (workload_sender, outputs, handle)
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
            RenderingManager::new(&window, self.antialiasing_enabled, self.clear_color).await;

        V4 {
            rendering_manager,
            event_loop,
            input_manager,
            scenes: Vec::new(),
            active_scene: 0,
            initialized_scene: false,
            window,
            details: Default::default(),
        }
    }
}
