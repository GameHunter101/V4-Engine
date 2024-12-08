use std::fmt::Debug;

use super::scene::Scene;

pub trait Action: Debug {
    fn execute(self: Box<Self>, scene: &mut Scene);
}

pub type ActionQueue = Vec<Box<dyn Action + Send>>;
