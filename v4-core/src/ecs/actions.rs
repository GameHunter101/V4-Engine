use std::fmt::Debug;

use super::scene::Scene;

#[allow(unused)]
#[async_trait::async_trait]
pub trait Action: Debug {
    async fn execute_async(self: Box<Self>, scene: &mut Scene) {
        self.execute(scene);
    }

    fn execute(self: Box<Self>, scene: &mut Scene) {}
}

pub type ActionQueue = Vec<Box<dyn Action + Send>>;
