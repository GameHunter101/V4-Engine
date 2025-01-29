use std::fmt::Debug;

use wgpu::{Device, Queue};

use super::scene::Scene;

#[allow(unused)]
#[async_trait::async_trait]
pub trait Action: Debug {
    async fn execute_async(self: Box<Self>, scene: &mut Scene, device: &Device, queue: &Queue) {
        self.execute(scene, device, queue);
    }

    fn execute(self: Box<Self>, scene: &mut Scene, device: &Device, queue: &Queue) {}
}

pub type ActionQueue = Vec<Box<dyn Action + Send>>;
