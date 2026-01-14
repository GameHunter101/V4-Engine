use v4::{
    builtin_actions::{CreateEntityAction, WorkloadAction, WorkloadOutputFreeAction},
    component,
    ecs::{
        component::{ComponentDetails, ComponentSystem, UpdateParams},
        scene::WorkloadOutput,
    },
    scene, V4,
};

#[tokio::main]
pub async fn main() {
    let mut engine = V4::builder().build().await;

    scene! {
        _ = {
            components: [
                WorkloadTesterComponent(initialized_time: std::time::Instant::now(), duration: 2),
                WorkloadTesterComponent(initialized_time: std::time::Instant::now(), duration: 3),
                TempComponent()
            ]
        }
    }

    engine.attach_scene(scene);

    engine.main_loop().await;
}

#[component]
struct WorkloadTesterComponent {
    initialized_time: std::time::Instant,
    duration: u64,
}

impl WorkloadTesterComponent {
    async fn create_workload(duration: u64) -> WorkloadOutput {
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
        println!("Initialized!");
        Vec::new()
    }

    async fn update(
        &mut self,
        UpdateParams { workload_outputs, .. }: UpdateParams<'_, '_>,
    ) -> v4::ecs::actions::ActionQueue {
        if self.initialized_time.elapsed().as_secs_f32() % 1.0 <= 0.01 {
            return vec![Box::new(WorkloadAction(
                self.id(),
                Box::pin(Self::create_workload(self.duration)),
            ))];
        }
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

        if self.initialized_time.elapsed().as_secs_f32() >= 1.0 {
            return vec![Box::new(CreateEntityAction {
                entity_parent_id: Some(self.parent_entity_id()),
                components: vec![Box::new(
                    WorkloadTesterComponent::builder()
                        .initialized_time(std::time::Instant::now())
                        .duration(self.duration)
                        .build(),
                )],
                computes: Vec::new(),
                active_material: None,
                is_enabled: true,
            })];
        }
        Vec::new()
    }
}

#[component]
struct TempComponent {}

#[async_trait::async_trait]
impl ComponentSystem for TempComponent {
    async fn update(
        &mut self,
        UpdateParams { engine_details, .. }: UpdateParams<'_, '_>,
    ) -> v4::ecs::actions::ActionQueue {
        if engine_details.initialization_time.elapsed().as_millis() % 100 == 0 {
            println!("Check");
        }

        Vec::new()
    }
}
