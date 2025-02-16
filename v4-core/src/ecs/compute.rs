use wgpu::{BindGroupLayout, Device};

use super::{
    component::{ComponentDetails, ComponentId, ComponentSystem},
    entity::EntityId,
    material::ShaderAttachment,
};

#[derive(Debug)]
pub struct Compute {
    input: Vec<ShaderAttachment>,
    output: ShaderAttachment,
    bind_group_layouts: Option<BindGroupLayout>,
    id: ComponentId,
    is_enabled: bool,
    is_initialized: bool,
    parent_entity: EntityId,
}

impl Compute {
    fn create_bind_group_layouts(&mut self, device: &Device) {
        self.bind_group_layouts = Some(device.create_bind_group_layout(
            &wgpu::BindGroupLayoutDescriptor {
                label: Some(&format!("Compute {} bind group layout", self.id)),
                entries: &self.input.iter().enumerate().map(|(i, attachement)| {
                    wgpu::BindGroupLayoutEntry {
                        binding: i as u32,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: match attachement {
                            ShaderAttachment::Texture(tex_attachment) => match tex.texture {
                                super::material::GeneralTexture::Regular(tex) => {
                                    wgpu::BindingType::Texture {
                                        sample_type: (),
                                        view_dimension: (),
                                        multisampled: (),
                                    }
                                }
                                super::material::GeneralTexture::Storage(store_tex) => todo!(),
                            },
                            ShaderAttachment::Buffer(buf) => todo!(),
                        },
                        count: todo!(),
                    }
                }),
            },
        ));
    }
}

impl ComponentSystem for Compute {}

impl ComponentDetails for Compute {
    fn id(&self) -> ComponentId {
        self.id
    }

    fn is_initialized(&self) -> bool {
        self.is_initialized
    }

    fn set_initialized(&mut self) {
        self.is_initialized = true;
    }

    fn parent_entity_id(&self) -> EntityId {
        self.parent_entity
    }

    fn set_parent_entity(&mut self, parent_id: EntityId) {
        self.parent_entity = parent_id;
    }

    fn is_enabled(&self) -> bool {
        self.is_enabled
    }

    fn set_enabled_state(&mut self, enabled_state: bool) {
        self.is_enabled = enabled_state;
    }
}
