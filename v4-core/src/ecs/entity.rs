use super::scene::Id;

#[derive(Debug)]
pub struct Entity {
    id: Id,
    children_ids: Vec<Id>,
    /// If this is set to nil then the entity is top-level and does not have a parent
    parent_entity_id: Id,
    is_enabled: bool,
    active_material: Option<Id>,
}

impl Entity {
    pub fn new(
        id: Id,
        children_ids: Vec<Id>,
        parent_entity_id: Id,
        is_enabled: bool,
        active_material: Option<Id>,
    ) -> Self {
        Self {
            id,
            children_ids,
            parent_entity_id,
            is_enabled,
            active_material,
        }
    }

    pub fn active_material(&self) -> Option<Id> {
        self.active_material
    }

    pub fn set_active_material(&mut self, active_material: Id) {
        self.active_material = Some(active_material);
    }

    pub fn id(&self) -> Id {
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

    pub fn children_ids(&self) -> &[Id] {
        &self.children_ids
    }

    /// If this returns 0 then the entity is top-level and does not have a parent
    pub fn parent_entity_id(&self) -> Id {
        self.parent_entity_id
    }

    pub fn push_child(&mut self, child: Id) {
        self.children_ids.push(child);
    }
}
