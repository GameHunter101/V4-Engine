use winit::window::Window;

pub struct V4Mutable<'a> {
    pub window: &'a Window,
    pub active_scene: &'a mut usize,
    pub initialized_scene: &'a mut bool,
}

pub trait EngineAction: Send + Sync {
    fn execute(self: Box<Self>, engine: V4Mutable);
}
