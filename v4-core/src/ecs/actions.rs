use std::fmt::Debug;

use wgpu::{Device, Queue};

use crate::ecs::scene::SceneError;

use super::scene::Scene;

#[allow(unused)]
#[async_trait::async_trait]
pub trait Action: Debug {
    async fn execute_async(
        self: Box<Self>,
        scene: &mut Scene,
        device: &Device,
        queue: &Queue,
    ) -> Result<(), SceneError> {
        self.execute(scene, device, queue)
    }

    fn execute(
        self: Box<Self>,
        scene: &mut Scene,
        device: &Device,
        queue: &Queue,
    ) -> Result<(), SceneError> {
        Ok(())
    }
}

pub type ActionQueue = Vec<Box<dyn Action + Send>>;
