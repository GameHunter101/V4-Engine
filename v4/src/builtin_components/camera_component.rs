use std::any::TypeId;

use crate::{builtin_actions::UpdateCameraBufferAction, v4};
use nalgebra::Matrix4;
use v4_core::ecs::{
    actions::ActionQueue,
    component::{ComponentDetails, ComponentSystem, UpdateParams},
};
use v4_macros::component;

use super::transform_component::TransformComponent;

#[component]
pub struct CameraComponent {
    field_of_view: f32,
    aspect_ratio: f32,
    near_plane: f32,
    far_plane: f32,
}

#[async_trait::async_trait]
impl ComponentSystem for CameraComponent {
    async fn update(
        &mut self,
        UpdateParams {
            other_components,
            entity_component_groupings,
            active_camera,
            ..
        }: UpdateParams<'_>,
    ) -> ActionQueue {
        if let Some(active) = active_camera {
            if active == self.id() {
                let sibling_components =
                    &other_components[entity_component_groupings[&self.parent_entity_id].clone()];

                let transform_component: Option<&TransformComponent> = sibling_components
                    .iter()
                    .flat_map(|comp| {
                        if comp.type_id() == TypeId::of::<TransformComponent>() {
                            comp.downcast_ref()
                        } else {
                            None
                        }
                    })
                    .next();
                return vec![Box::new(UpdateCameraBufferAction(
                    RawCameraData::from_component(self, transform_component).matrix,
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
    fn from_component(comp: &CameraComponent, transform: Option<&TransformComponent>) -> Self {
        let c = 1.0 / (comp.field_of_view / 2.0).tan();
        let aspect_ratio = comp.aspect_ratio;
        let far_plane = comp.far_plane;
        let near_plane = comp.near_plane;
        let difference = far_plane - near_plane;

        let view_matrix = if let Some(transform) = transform {
            if let Some(inverted) = transform.create_matrix().try_inverse() {
                inverted
            } else {
                Matrix4::identity()
            }
        } else {
            Matrix4::identity()
        };

        #[rustfmt::skip]
        let projection_matrix= Matrix4::new(
            c * aspect_ratio,  0.0,    0.0,                     0.0,
            0.0,               c,      0.0,                     0.0,
            0.0,               0.0,    far_plane / difference,  - (far_plane * near_plane) / difference,
            0.0,               0.0,    1.0,                     0.0,
        );

        Self {
            matrix: (projection_matrix * view_matrix).into(),
        }
    }
}
