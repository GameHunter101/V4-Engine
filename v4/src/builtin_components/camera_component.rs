use std::any::TypeId;

use crate::{
    builtin_actions::{
        SetCursorPositionAction, SetCursorVisibilityAction, UpdateCameraBufferAction,
    },
    v4,
};
use algoe::{bivector::Bivector, vector::GeometricOperations};
use nalgebra::{Matrix4, Vector3, Vector4};
use v4_core::{
    ecs::{
        actions::ActionQueue,
        component::{ComponentDetails, ComponentSystem, UpdateParams},
    },
    EngineDetails,
};
use v4_macros::component;
use winit::{dpi::PhysicalPosition, keyboard::KeyCode};

use super::transform_component::TransformComponent;

#[component]
pub struct CameraComponent {
    field_of_view: f32,
    aspect_ratio: f32,
    near_plane: f32,
    far_plane: f32,
    #[default(1.0)]
    sensitivity: f32,
    #[default(1.0)]
    movement_speed: f32,
    #[default(false)]
    frozen: bool,
}

#[async_trait::async_trait]
impl ComponentSystem for CameraComponent {
    async fn update(
        &mut self,
        UpdateParams {
            other_components,
            entity_component_groupings,
            active_camera,
            engine_details:
                EngineDetails {
                    cursor_position,
                    window_resolution,
                    ..
                },
            input_manager,
            ..
        }: UpdateParams<'_, '_>,
    ) -> ActionQueue {
        let cursor_delta = (
            cursor_position.0 as f32 - window_resolution.0 as f32 / 2.0,
            cursor_position.1 as f32 - window_resolution.0 as f32 / 2.0,
        );

        if let Some(active) = active_camera {
            if input_manager.key_pressed(KeyCode::Escape) {
                self.frozen = !self.frozen;
                return vec![Box::new(SetCursorVisibilityAction(self.frozen))];
            }
            if active == self.id() && !self.frozen {
                let sibling_components = &mut other_components.lock().unwrap()
                    [entity_component_groupings[&self.parent_entity_id].clone()];

                let transform_component: Option<&mut TransformComponent> = sibling_components
                    .into_iter()
                    .flat_map(|comp| {
                        if comp.type_id() == TypeId::of::<TransformComponent>() {
                            comp.downcast_mut()
                        } else {
                            None
                        }
                    })
                    .next();

                let comp: Option<&TransformComponent> = if let Some(transform) = transform_component
                {
                    let rotation = transform.get_rotation();

                    let mut forward = rotation * Vector3::z();
                    let up = Vector3::y();

                    let pitch_rotation = (up.wedge(&forward) * self.sensitivity * cursor_delta.1
                        / -2.0)
                        .exponentiate();
                    let yaw_rotation =
                        Bivector::new(0.0, 0.0, self.sensitivity * cursor_delta.0 / -2.0)
                            .exponentiate();

                    let new_rotation = (yaw_rotation * pitch_rotation * rotation).normalize();
                    transform.set_rotation(new_rotation);
                    forward = rotation * Vector3::z();

                    let right = rotation * Vector3::x();

                    let forward_diff = ((input_manager.key_held(KeyCode::KeyW) as i32)
                        - (input_manager.key_held(KeyCode::KeyS) as i32))
                        as f32
                        * self.movement_speed;

                    let right_diff = ((input_manager.key_held(KeyCode::KeyD) as i32)
                        - (input_manager.key_held(KeyCode::KeyA) as i32))
                        as f32
                        * self.movement_speed;

                    let up_diff = ((input_manager.key_held(KeyCode::Space) as i32)
                        - (input_manager.key_held(KeyCode::ControlLeft) as i32))
                        as f32
                        * self.movement_speed;

                    let translation = forward * forward_diff + right * right_diff + up * up_diff;
                    transform.set_position(transform.get_position() + translation);

                    Some(transform)
                } else {
                    None
                };

                let raw_camera = RawCameraData::from_component(self, comp);
                return vec![
                    Box::new(UpdateCameraBufferAction(raw_camera.matrix, raw_camera.pos)),
                    Box::new(SetCursorPositionAction(
                        PhysicalPosition::new(window_resolution.0 / 2, window_resolution.1 / 2)
                            .into(),
                    )),
                ];
            }
        }
        Vec::new()
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct RawCameraData {
    matrix: [[f32; 4]; 4],
    pos: [f32; 3],
}

impl RawCameraData {
    fn from_component(comp: &CameraComponent, transform: Option<&TransformComponent>) -> Self {
        let c = 1.0 / (comp.field_of_view * std::f32::consts::PI / 360.0).tan();
        let aspect_ratio = comp.aspect_ratio;
        let far_plane = comp.far_plane;
        let near_plane = comp.near_plane;
        let difference = far_plane - near_plane;

        let (view_matrix, pos) = if let Some(transform) = transform {
            if let Some(inverted) = transform.create_matrix().try_inverse() {
                (inverted, transform.get_position())
            } else {
                (Matrix4::identity(), Vector3::zeros())
            }
        } else {
            (Matrix4::identity(), Vector3::zeros())
        };

        let projection_matrix = Matrix4::from_columns(&[
            Vector4::new(c / aspect_ratio, 0.0, 0.0, 0.0),
            Vector4::new(0.0, c, 0.0, 0.0),
            Vector4::new(0.0, 0.0, far_plane / difference, 1.0),
            Vector4::new(0.0, 0.0, -(far_plane * near_plane) / difference, 0.0),
        ]);

        Self {
            matrix: (projection_matrix * view_matrix).into(),
            pos: pos.into(),
        }
    }
}
