use std::collections::HashMap;

use crate::{builtin_actions::UpdateCameraBufferAction, v4};
use v4_core::{
    ecs::{
        actions::ActionQueue,
        component::{Component, ComponentDetails, ComponentId, ComponentSystem},
        scene::WorkloadOutput,
    },
    EngineDetails,
};
use v4_macros::component;
use wgpu::{Device, Queue};
use winit_input_helper::WinitInputHelper;

#[derive(Debug)]
#[component]
struct CameraComponent {
    field_of_view: f32,
    aspect_ratio: f32,
    near_plane: f32,
    far_plane: f32,
}

#[async_trait::async_trait]
impl ComponentSystem for CameraComponent {
    async fn update(
        &mut self,
        _device: &Device,
        _queue: &Queue,
        _input_manager: &WinitInputHelper,
        _other_components: &[&mut Component],
        _engine_details: &EngineDetails,
        _workload_outputs: &HashMap<ComponentId, Vec<WorkloadOutput>>,
        active_camera: Option<ComponentId>,
    ) -> ActionQueue {
        if let Some(active) = active_camera {
            if active == self.id() {
                return vec![Box::new(UpdateCameraBufferAction(
                    RawCameraData::from_component(self).matrix,
                ))];
            }
        }
        Vec::new()
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct RawCameraData {
    matrix: [[f32; 4]; 4],
}

impl RawCameraData {
    fn from_component(comp: &CameraComponent) -> Self {
        let c = 1.0 / (comp.field_of_view / 2.0).tan();
        let aspect_ratio = comp.aspect_ratio;
        let far_plane = comp.far_plane;
        let near_plane = comp.near_plane;
        let difference = far_plane - near_plane;

        #[rustfmt::skip]
        let matrix = [
            [c / aspect_ratio,  0.0,    0.0,                    0.0],
            [0.0,               c,      0.0,                    0.0],
            [0.0,               0.0,    far_plane / difference, -far_plane * near_plane / difference],
            [0.0,               0.0,    1.0,                    0.0],
        ];
        Self { matrix }
    }
}
