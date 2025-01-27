use std::fmt::Debug;

use v4_core::{
    ecs::{
        actions::Action,
        component::{Component, ComponentId},
        entity::EntityId,
        material::MaterialId,
        scene::{Scene, Workload},
    },
    engine_management::{
        engine_action::{CreateTextBufferEngineAction, UpdateTextBufferEngineAction},
        font_management::{TextAttributes, TextComponentProperties, TextDisplayInfo},
    },
};

pub struct WorkloadAction(pub ComponentId, pub Workload);

impl Debug for WorkloadAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("WorkloadAction")
            .field(&self.0)
            .field(&"Future")
            .finish()
    }
}

#[async_trait::async_trait]
impl Action for WorkloadAction {
    async fn execute_async(self: Box<Self>, scene: &mut Scene) {
        scene.attach_workload(self.0, self.1).await;
    }
}

#[derive(Debug)]
pub struct WorkloadOutputFreeAction(pub ComponentId, pub usize);

#[async_trait::async_trait]
impl Action for WorkloadOutputFreeAction {
    async fn execute_async(self: Box<Self>, scene: &mut Scene) {
        scene.free_workload_output(self.0, self.1).await;
    }
}

#[derive(Debug)]
pub struct EntityToggleAction(pub EntityId, pub Option<bool>);

impl Action for EntityToggleAction {
    fn execute(self: Box<Self>, scene: &mut Scene) {
        let entity = scene.get_entity_mut(self.0);
        if let Some(entity) = entity {
            match self.1 {
                Some(desired_state) => entity.set_enabled_state(desired_state),
                None => entity.toggle_enabled_state(),
            }
        }
    }
}

#[derive(Debug)]
pub struct ComponentToggleAction(pub ComponentId, pub Option<bool>);

impl Action for ComponentToggleAction {
    fn execute(self: Box<Self>, scene: &mut Scene) {
        let component = scene.get_component_mut(self.0);
        if let Some(component) = component {
            let component_enabled = component.is_enabled();
            match self.1 {
                Some(desired_state) => component.set_enabled_state(desired_state),
                None => component.set_enabled_state(!component_enabled),
            }
        }
    }
}

#[derive(Debug)]
pub struct RegisterUiComponentAction {
    pub component_id: ComponentId,
    pub text_component_properties: Option<TextComponentProperties>,
}

impl Action for RegisterUiComponentAction {
    fn execute(self: Box<Self>, scene: &mut Scene) {
        if let Some(text_component_properties) = self.text_component_properties {
            scene.send_engine_action(Box::new(CreateTextBufferEngineAction {
                component_id: self.component_id,
                text_component_properties,
            }));
        }
        scene.register_ui_component(self.component_id);
    }
}

#[derive(Debug)]
pub struct UpdateTextComponentAction {
    pub component_id: ComponentId,
    pub text: Option<String>,
    pub text_attributes: Option<TextAttributes>,
    pub text_metrics: Option<glyphon::Metrics>,
    pub text_display_info: Option<TextDisplayInfo>,
}

impl Action for UpdateTextComponentAction {
    fn execute(self: Box<Self>, scene: &mut Scene) {
        /* scene.update_text_buffer(
            self.component_id,
            self.text,
            self.text_attributes,
            self.text_metrics,
            self.text_display_info,
        ); */
        scene.send_engine_action(Box::new(UpdateTextBufferEngineAction {
            component_id: self.component_id,
            text: self.text,
            text_attributes: self.text_attributes,
            text_metrics: self.text_metrics,
            text_display_info: self.text_display_info,
        }));
    }
}

#[derive(Debug)]
pub struct SetEntityActiveMaterialAction(pub EntityId, pub MaterialId);

impl Action for SetEntityActiveMaterialAction {
    fn execute(self: Box<Self>, scene: &mut Scene) {
        if let Some(entity) = scene.get_entity_mut(self.0) {
            entity.set_active_material(self.1);
        }
    }
}

#[derive(Debug)]
pub struct CreateEntityAction {
    pub entity_parent_id: Option<EntityId>,
    pub components: Vec<Component>,
    pub active_material: Option<MaterialId>,
    pub is_enabled: bool,
}

impl Action for CreateEntityAction {
    fn execute(self: Box<Self>, scene: &mut Scene) {
        scene.create_entity(
            self.entity_parent_id,
            self.components,
            self.active_material,
            self.is_enabled,
        );
    }
}

#[derive(Debug)]
pub struct SetActiveCameraAction(ComponentId);

impl Action for SetActiveCameraAction {
    fn execute(self: Box<Self>, scene: &mut Scene) {
        scene.set_active_camera(Some(self.0));
    }
}

#[derive(Debug)]
pub struct DisableCameraAction;

impl Action for DisableCameraAction {
    fn execute(self: Box<Self>, scene: &mut Scene) {
        scene.set_active_camera(None);
    }
}
