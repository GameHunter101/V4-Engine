use std::ops::Range;

use wgpu::{
    util::DeviceExt, BindGroup, BindGroupLayout, Buffer, CommandEncoder, Device, Queue,
    ShaderStages,
};

use crate::{
    ecs::compute::Compute, engine_management::pipeline::PipelineId, engine_support::texture_support::{StorageTexture, Texture}
};

use super::{
    actions::ActionQueue,
    component::{Component, ComponentDetails, ComponentId, ComponentSystem, UpdateParams},
    entity::EntityId,
};

#[derive(Debug, Clone)]
pub struct ShaderTextureAttachment {
    pub texture: GeneralTexture,
    pub visibility: ShaderStages,
    pub extra_usages: wgpu::TextureUsages,
}

#[derive(Debug, Clone)]
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

    pub fn is_sampled(&self) -> bool {
        match self {
            GeneralTexture::Regular(texture) => texture.is_sampled(),
            GeneralTexture::Storage(_) => false,
        }
    }
}

#[derive(Debug, Clone)]
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
        extra_usages: wgpu::BufferUsages,
    ) -> Self {
        Self {
            buffer: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(&format!("Shader Buffer | {data:?}")),
                contents: data,
                usage: match buffer_type {
                    wgpu::BufferBindingType::Uniform => wgpu::BufferUsages::UNIFORM,
                    wgpu::BufferBindingType::Storage { .. } => wgpu::BufferUsages::STORAGE,
                } | extra_usages,
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

#[derive(Debug, Clone)]
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
            entries: &[match attachment {
                ShaderAttachment::Texture(tex) => wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: tex.visibility,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                ShaderAttachment::Buffer(buf) => wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: buf.visibility,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            }],
        })
    }

    fn create_attachment_bind_group(
        device: &Device,
        material_id: ComponentId,
        attachment: &ShaderAttachment,
        bind_group_layout: &BindGroupLayout,
    ) -> BindGroup {
        let label = format!("Material {material_id} | attachment {attachment:?} bind group");

        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(&label),
            layout: bind_group_layout,
            entries: &[match attachment {
                ShaderAttachment::Texture(tex) => wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(tex.texture.view_ref()),
                },
                ShaderAttachment::Buffer(buf) => wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buf.buffer.as_entire_binding(),
                },
            }],
        })
    }

    pub fn attach_entity(&mut self, entity_id: EntityId) {
        self.entities_attached.push(entity_id);
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
        let (mut bind_group_layouts, mut bind_groups): (Vec<BindGroupLayout>, Vec<BindGroup>) =
            self.attachments
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
        let (has_sampler, sampler_visibility) =
            self.attachments()
                .iter()
                .fold((false, wgpu::ShaderStages::NONE), |acc, attachment| {
                    if let ShaderAttachment::Texture(ShaderTextureAttachment {
                        texture,
                        visibility,
                        ..
                    }) = attachment
                    {
                        if texture.is_sampled() {
                            (true, acc.1 | *visibility)
                        } else {
                            acc
                        }
                    } else {
                        acc
                    }
                });

        if has_sampler {
            let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some(&format!("Material {} | Sampler bind group layout", self.id)),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: sampler_visibility,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                }],
            });

            let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some(&format!("Material {} | Sampler", self.id)),
                address_mode_u: wgpu::AddressMode::ClampToEdge,
                address_mode_v: wgpu::AddressMode::ClampToEdge,
                address_mode_w: wgpu::AddressMode::ClampToEdge,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                mipmap_filter: wgpu::FilterMode::Nearest,
                ..Default::default()
            });

            bind_groups.push(device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(&format!("Material {} | Sampler bind group", self.id)),
                layout: &layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                }],
            }));

            bind_group_layouts.push(layout);
        }

        self.bind_group_layouts = bind_group_layouts;
        self.bind_groups = bind_groups;

        self.is_initialized = true;

        Vec::new()
    }

    async fn update(
        &mut self,
        UpdateParams {
            entity_component_groupings,
            ..
        }: UpdateParams<'_, '_>,
    ) -> crate::ecs::actions::ActionQueue {
        self.component_ranges = entity_component_groupings
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

    fn command_encoder_operations(
        &self,
        device: &Device,
        queue: &Queue,
        encoder: &mut CommandEncoder,
        other_components: &[&Component],
        materials: &[Material],
        computes: &[Compute],
    ) {
        for range in &self.component_ranges {
            for component in &other_components[range.clone()] {
                component.command_encoder_operations(device, queue, encoder, other_components, materials, computes);
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
