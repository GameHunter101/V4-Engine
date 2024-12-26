use v4::{
    builtin_actions::{WorkloadAction, WorkloadOutputFreeAction},
    component,
    ecs::{
        component::{ComponentDetails, ComponentSystem},
        scene::Scene,
    },
    V4,
};

#[tokio::main]
pub async fn main() {
    let mut engine = V4::builder().build().await;

    let rendering_manager = engine.rendering_manager();

    let device = rendering_manager.device();
    let queue = rendering_manager.queue();
    let format = rendering_manager.format();

    let mut scene = Scene::new(device, queue, format);

    let workload_component = WorkloadTesterComponent::new(2);
    let workload_component_2 = WorkloadTesterComponent::new(3);

    scene.create_entity(
        None,
        vec![Box::new(workload_component), Box::new(workload_component_2)],
        None,
        true,
    );

    engine.attach_scene(scene);

    engine.main_loop().await;
}

#[derive(Debug)]
#[component]
struct WorkloadTesterComponent {
    initialized_time: std::time::Instant,
    duration: u64,
}

impl WorkloadTesterComponent {
    fn new(duration: u64) -> Self {
        Self {
            initialized_time: std::time::Instant::now(),
            duration,
            id: std::sync::OnceLock::new(),
            parent_entity_id: v4::ecs::entity::EntityId::MAX,
            is_initialized: false,
            is_enabled: true,
        }
    }
}

impl WorkloadTesterComponent {
    async fn create_workload(duration: u64) -> Box<dyn std::any::Any + Send> {
        let init_time = std::time::Instant::now();
        tokio::time::sleep(std::time::Duration::from_secs(duration)).await;
        Box::new(init_time)
    }
}

#[async_trait::async_trait]
impl ComponentSystem for WorkloadTesterComponent {
    fn initialize(&mut self, _device: &wgpu::Device) -> v4::ecs::actions::ActionQueue {
        self.initialized_time = std::time::Instant::now();
        self.set_initialized();
        Vec::new()
    }

    async fn update(
        &mut self,
        _device: &wgpu::Device,
        _queue: &wgpu::Queue,
        _input_manager: &winit_input_helper::WinitInputHelper,
        _other_components: &[&mut v4::ecs::component::Component],
        _active_camera_id: Option<v4::ecs::component::ComponentId>,
        _engine_details: &v4::EngineDetails,
        workload_outputs: std::sync::Arc<
            tokio::sync::Mutex<
                std::collections::HashMap<
                    v4::ecs::component::ComponentId,
                    Vec<v4::ecs::scene::WorkloadOutput>,
                >,
            >,
        >,
    ) -> v4::ecs::actions::ActionQueue {
        if self.initialized_time.elapsed().as_secs_f32() % 1.0 <= 0.01 {
            println!("Creating workload");
            return vec![Box::new(WorkloadAction(
                self.id(),
                Box::pin(Self::create_workload(self.duration)),
            ))];
        }
        let workload_outputs = workload_outputs.lock().await;
        if let Some(outputs) = workload_outputs.get(&self.id()) {
            let last = outputs.first();
            if let Some(last) = last {
                let start_time: std::time::Instant = *last.downcast_ref().unwrap();
                println!(
                    "Finished a workload after {} seconds",
                    start_time.elapsed().as_secs_f32()
                );
                return vec![Box::new(WorkloadOutputFreeAction(self.id(), 0))];
            }
        }
        Vec::new()
    }
}
