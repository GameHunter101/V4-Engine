use std::{collections::HashMap, ops::Range};

use wgpu::{util::DeviceExt, BindGroup, BindGroupLayout, Buffer, Device, Queue, ShaderStages};

use crate::{
    engine_management::pipeline::PipelineId,
    engine_support::texture_support::{StorageTexture, Texture},
};

use super::{
    actions::ActionQueue,
    component::{Component, ComponentDetails, ComponentId, ComponentSystem},
    entity::{Entity, EntityId},
};

#[derive(Debug)]
pub struct ShaderTextureAttachment {
    pub texture: GeneralTexture,
    pub visibility: ShaderStages,
}

#[derive(Debug)]
pub enum GeneralTexture {
    Regular(Texture),
    Storage(StorageTexture),
}

impl GeneralTexture {
    pub fn texture(&self) -> &wgpu::Texture {
        match self {
            GeneralTexture::Regular(texture) => texture.texture_ref(),
            GeneralTexture::Storage(storage_texture) => storage_texture.texture_ref(),
        }
    }

    pub fn view_ref(&self) -> &wgpu::TextureView {
        match self {
            GeneralTexture::Regular(texture) => texture.view_ref(),
            GeneralTexture::Storage(storage_texture) => storage_texture.view_ref(),
        }
    }

    pub fn view_mut(&mut self) -> &mut wgpu::TextureView {
        match self {
            GeneralTexture::Regular(texture) => texture.view_mut(),
            GeneralTexture::Storage(storage_texture) => storage_texture.view_mut(),
        }
    }

    pub fn sampler_ref(&self) -> Option<&wgpu::Sampler> {
        match self {
            GeneralTexture::Regular(texture) => Some(texture.sampler_ref()),
            GeneralTexture::Storage(_storage_texture) => None,
        }
    }
}

#[derive(Debug)]
pub struct ShaderBufferAttachment {
    buffer: Buffer,
    visibility: ShaderStages,
    buffer_type: wgpu::BufferBindingType,
}

impl ShaderBufferAttachment {
    pub fn new(
        device: &Device,
        data: &[u8],
        buffer_type: wgpu::BufferBindingType,
        visibility: ShaderStages,
    ) -> Self {
        Self {
            buffer: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(&format!("Shader Buffer | {data:?}")),
                contents: data,
                usage: match buffer_type {
                    wgpu::BufferBindingType::Uniform => wgpu::BufferUsages::UNIFORM,
                    wgpu::BufferBindingType::Storage { .. } => wgpu::BufferUsages::STORAGE,
                },
            }),
            buffer_type,
            visibility,
        }
    }

    pub fn buffer(&self) -> &Buffer {
        &self.buffer
    }

    pub fn visibility(&self) -> ShaderStages {
        self.visibility
    }

    pub fn buffer_type(&self) -> wgpu::BufferBindingType {
        self.buffer_type
    }
}

#[derive(Debug)]
pub enum ShaderAttachment {
    Texture(ShaderTextureAttachment),
    Buffer(ShaderBufferAttachment),
}

#[derive(Debug)]
pub struct Material {
    id: ComponentId,
    pipeline_id: PipelineId,
    entities_attached: Vec<EntityId>,
    component_ranges: Vec<Range<usize>>,
    attachments: Vec<ShaderAttachment>,
    bind_group_layouts: Vec<BindGroupLayout>,
    bind_groups: Vec<BindGroup>,
    is_initialized: bool,
}

impl Material {
    pub fn new(
        id: ComponentId,
        pipeline_id: PipelineId,
        attachments: Vec<ShaderAttachment>,
        entities_attached: Vec<EntityId>,
    ) -> Self {
        Self {
            id,
            attachments,
            entities_attached,
            component_ranges: Vec::new(),
            pipeline_id,
            bind_group_layouts: Vec::new(),
            bind_groups: Vec::new(),
            is_initialized: false,
        }
    }

    pub fn create_attachment_bind_group_layout(
        device: &Device,
        material_id: ComponentId,
        attachment: &ShaderAttachment,
    ) -> BindGroupLayout {
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some(&format!(
                "Material {material_id} | attachment {attachment:?} Bind Group Layout"
            )),
            entries: &match attachment {
                ShaderAttachment::Texture(tex) => vec![
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: tex.visibility,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: tex.visibility,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
                ShaderAttachment::Buffer(buf) => vec![wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: buf.visibility,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            },
        })
    }

    fn create_attachment_bind_group(
        device: &Device,
        material_id: ComponentId,
        attachment: &ShaderAttachment,
        bind_group_layout: &BindGroupLayout,
    ) -> BindGroup {
        let label = format!("Material {material_id} | attachment {attachment:?} bind group");
        let entries = &match attachment {
            ShaderAttachment::Texture(tex) => {
                if let Some(sampler) = tex.texture.sampler_ref() {
                    vec![
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(tex.texture.view_ref()),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(sampler),
                        },
                    ]
                } else {
                    vec![wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(tex.texture.view_ref()),
                    }]
                }
            }
            ShaderAttachment::Buffer(buf) => {
                vec![wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buf.buffer.as_entire_binding(),
                }]
            }
        };
        dbg!(entries.len());
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(&label),
            layout: bind_group_layout,
            entries,
        })
    }

    pub fn bind_group_layouts(&self) -> &[BindGroupLayout] {
        self.bind_group_layouts.as_ref()
    }

    pub fn bind_groups(&self) -> &[BindGroup] {
        self.bind_groups.as_ref()
    }

    pub fn attachments(&self) -> &[ShaderAttachment] {
        self.attachments.as_ref()
    }

    pub fn uses_camera(&self) -> bool {
        self.pipeline_id.uses_camera
    }

    pub fn pipeline_id(&self) -> &PipelineId {
        &self.pipeline_id
    }
}

#[async_trait::async_trait]
impl ComponentSystem for Material {
    fn initialize(&mut self, device: &Device) -> ActionQueue {
        let (bind_group_layouts, bind_groups): (Vec<BindGroupLayout>, Vec<BindGroup>) = self
            .attachments
            .iter()
            .map(|attachment| {
                let bind_group_layout =
                    Self::create_attachment_bind_group_layout(device, self.id, attachment);
                let bind_group = Self::create_attachment_bind_group(
                    device,
                    self.id,
                    attachment,
                    &bind_group_layout,
                );
                (bind_group_layout, bind_group)
            })
            .unzip();

        self.bind_group_layouts = bind_group_layouts;
        self.bind_groups = bind_groups;

        self.is_initialized = true;

        Vec::new()
    }

    async fn update(
        &mut self,
        _device: &Device,
        _queue: &Queue,
        _input_manager: &winit_input_helper::WinitInputHelper,
        _other_components: &[&mut crate::ecs::component::Component],
        _materials: &[&mut Material],
        _engine_details: &crate::EngineDetails,
        _workload_outputs: &HashMap<ComponentId, Vec<crate::ecs::scene::WorkloadOutput>>,
        _entities: &HashMap<EntityId, Entity>,
        entity_component_groups: HashMap<EntityId, Range<usize>>,
        _active_camera: Option<ComponentId>,
    ) -> crate::ecs::actions::ActionQueue {
        self.component_ranges = entity_component_groups
            .iter()
            .flat_map(|(entity_id, range)| {
                if self.entities_attached.contains(entity_id) {
                    Some(range.clone())
                } else {
                    None
                }
            })
            .collect();
        Vec::new()
    }

    fn render(
        &self,
        device: &Device,
        queue: &Queue,
        render_pass: &mut wgpu::RenderPass,
        other_components: &[&Component],
    ) {
        let bind_group_offset = if self.uses_camera() { 1 } else { 0 };
        for (i, bind_group) in self.bind_groups.iter().enumerate() {
            render_pass.set_bind_group(i as u32 + bind_group_offset, bind_group, &[]);
        }

        for range in &self.component_ranges {
            for component in &other_components[range.clone()] {
                component.render(device, queue, render_pass, other_components);
            }
        }
    }
}

impl ComponentDetails for Material {
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
        0
    }

    fn set_parent_entity(&mut self, _parent_id: EntityId) {}

    fn is_enabled(&self) -> bool {
        true
    }

    fn set_enabled_state(&mut self, _enabled_state: bool) {}
}
