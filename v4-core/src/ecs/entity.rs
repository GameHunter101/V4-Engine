use super::material::MaterialId;

pub type EntityId = u32;

#[derive(Debug)]
pub struct Entity {
    id: EntityId,
    children_ids: Vec<EntityId>,
    /// If this is set to 0 the entity is top-level and does not have a parent
    parent_entity_id: EntityId,
    is_enabled: bool,
    active_material: Option<MaterialId>,
}

impl Entity {
    pub fn new(
        id: EntityId,
        children_ids: Vec<EntityId>,
        parent_entity_id: EntityId,
        is_enabled: bool,
        active_material: Option<MaterialId>,
    ) -> Self {
        Self {
            id,
            children_ids,
            parent_entity_id,
            is_enabled,
            active_material,
        }
    }

    pub fn active_material(&self) -> Option<MaterialId> {
        self.active_material
    }

    pub fn set_active_material(&mut self, active_material: MaterialId) {
        self.active_material = Some(active_material);
    }

    pub fn id(&self) -> EntityId {
        self.id
    }

    pub fn toggle_enabled_state(&mut self) {
        self.is_enabled = !self.is_enabled;
    }

    pub fn set_enabled_state(&mut self, desired_state: bool) {
        self.is_enabled = desired_state;
    }

    pub fn is_enabled(&self) -> bool {
        self.is_enabled
    }

    pub fn children_ids(&self) -> &[EntityId] {
        &self.children_ids
    }

    pub fn parent_entity_id(&self) -> EntityId {
        self.parent_entity_id
    }

    pub fn push_child(&mut self, child: EntityId) {
        self.children_ids.push(child);
    }
}
