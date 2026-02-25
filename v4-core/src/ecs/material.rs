use std::ops::Range;

use wgpu::{
    BindGroup, BindGroupEntry, BindGroupLayout, BindGroupLayoutEntry, Buffer, CommandEncoder,
    Device, Queue, Sampler, ShaderStages, util::DeviceExt,
};

use crate::{
    ecs::compute::Compute,
    engine_management::pipeline::PipelineId,
    engine_support::texture_support::{TextureBundle, TextureProperties},
};

use super::{
    actions::ActionQueue,
    component::{Component, ComponentDetails, ComponentId, ComponentSystem, UpdateParams},
    entity::EntityId,
};

#[derive(Debug, Clone)]
pub struct ShaderTextureAttachment {
    pub texture_bundle: TextureBundle,
    pub visibility: ShaderStages,
}

#[derive(Debug, Clone)]
pub struct ShaderBufferAttachment {
    pub buffer: Buffer,
    pub visibility: ShaderStages,
    pub buffer_type: wgpu::BufferBindingType,
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
                label: Some(&format!("{:?} Shader Buffer", visibility)),
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

    pub fn update_buffer(&mut self, contents: &[u8], device: &Device, queue: &Queue) {
        crate::engine_support::misc_utils::update_buffer(
            &mut self.buffer,
            contents,
            device,
            queue,
            Some(&format!("{:?} Shader Buffer", self.visibility)),
        );
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
    bind_group_layout: Option<BindGroupLayout>,
    bind_group: Option<BindGroup>,
    immediate_data: Vec<u8>,
    is_initialized: bool,
    is_enabled: bool,
}

impl Material {
    pub fn new(
        id: ComponentId,
        pipeline_id: PipelineId,
        attachments: Vec<ShaderAttachment>,
        entities_attached: Vec<EntityId>,
        immediate_data: Vec<u8>,
        is_enabled: bool,
    ) -> Self {
        Self {
            id,
            attachments,
            entities_attached,
            component_ranges: Vec::new(),
            pipeline_id,
            bind_group_layout: None,
            bind_group: None,
            immediate_data,
            is_initialized: false,
            is_enabled,
        }
    }

    pub fn create_attachment_bind_group_layout_entry(
        attachment: &ShaderAttachment,
        binding: u32,
    ) -> BindGroupLayoutEntry {
        match attachment {
            ShaderAttachment::Texture(tex) => wgpu::BindGroupLayoutEntry {
                binding,
                visibility: tex.visibility,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float {
                        filterable: tex.texture_bundle.properties().is_filtered,
                    },
                    view_dimension: if tex.texture_bundle.properties().is_cubemap {
                        wgpu::TextureViewDimension::Cube
                    } else {
                        wgpu::TextureViewDimension::D2
                    },
                    multisampled: false,
                },
                count: None,
            },
            ShaderAttachment::Buffer(buf) => wgpu::BindGroupLayoutEntry {
                binding,
                visibility: buf.visibility,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        }
    }

    fn create_attachment_bind_group_entry<'a>(
        attachment: &'a ShaderAttachment,
        binding: u32,
    ) -> BindGroupEntry<'a> {
        let resource = match attachment {
            ShaderAttachment::Texture(tex) => {
                wgpu::BindingResource::TextureView(tex.texture_bundle.view())
            }
            ShaderAttachment::Buffer(buf) => buf.buffer.as_entire_binding(),
        };

        BindGroupEntry { binding, resource }
    }

    fn create_sampler_entries<'a>(
        sampler: &'a wgpu::Sampler,
        is_filtering: bool,
        visibility: ShaderStages,
        binding: u32,
    ) -> (BindGroupLayoutEntry, BindGroupEntry<'a>) {
        (
            BindGroupLayoutEntry {
                binding,
                visibility,
                ty: wgpu::BindingType::Sampler(if is_filtering {
                    wgpu::SamplerBindingType::Filtering
                } else {
                    wgpu::SamplerBindingType::NonFiltering
                }),
                count: None,
            },
            BindGroupEntry {
                binding,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        )
    }

    /// Index 0: non-filtering, index 1: filtering
    fn create_samplers(&self, device: &Device) -> Vec<(Sampler, bool, ShaderStages)> {
        let samplers_needed =
            self.attachments
                .iter()
                .fold([(false, ShaderStages::empty()); 2], |acc, attachment| {
                    if let ShaderAttachment::Texture(ShaderTextureAttachment {
                        texture_bundle: texture,
                        visibility,
                    }) = attachment
                    {
                        let TextureProperties {
                            is_sampled,
                            is_filtered,
                            ..
                        } = texture.properties();

                        [
                            (
                                acc[0].0 | (is_sampled && !is_filtered),
                                acc[0].1 | *visibility,
                            ),
                            (
                                acc[1].0 | (is_sampled && is_filtered),
                                acc[1].1 | *visibility,
                            ),
                        ]
                    } else {
                        acc
                    }
                });

        samplers_needed
            .iter()
            .enumerate()
            .flat_map(|(filtering_as_index, (sampler_needed, visibility))| {
                if *sampler_needed {
                    let is_filtering = filtering_as_index == 1;
                    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
                        label: Some(&format!(
                            "Material {} | {} sampler",
                            self.id,
                            if is_filtering {
                                "filtering"
                            } else {
                                "non-filtering"
                            }
                        )),
                        address_mode_u: wgpu::AddressMode::ClampToEdge,
                        address_mode_v: wgpu::AddressMode::ClampToEdge,
                        address_mode_w: wgpu::AddressMode::ClampToEdge,
                        mag_filter: if is_filtering {
                            wgpu::FilterMode::Linear
                        } else {
                            wgpu::FilterMode::Nearest
                        },
                        min_filter: if is_filtering {
                            wgpu::FilterMode::Linear
                        } else {
                            wgpu::FilterMode::Nearest
                        },
                        mipmap_filter: wgpu::MipmapFilterMode::Nearest,
                        ..Default::default()
                    });
                    Some((sampler, is_filtering, *visibility))
                } else {
                    None
                }
            })
            .collect()
    }

    pub fn attach_entity(&mut self, entity_id: EntityId) {
        self.entities_attached.push(entity_id);
    }

    pub fn bind_group_layout(&self) -> Option<&BindGroupLayout> {
        self.bind_group_layout.as_ref()
    }

    pub fn bind_group(&self) -> Option<&BindGroup> {
        self.bind_group.as_ref()
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

    pub fn get_immediate_data<'a>(&'a self) -> &'a [u8] {
        &self.immediate_data
    }

    pub fn set_immediate_data(&mut self, immediate_data: &[u8]) {
        self.immediate_data = immediate_data.to_vec();
    }
}

// #[async_trait::async_trait]
impl ComponentSystem for Material {
    fn initialize(&mut self, device: &Device) -> ActionQueue {
        let (bind_group_layout_entries, bind_group_entries): (
            Vec<BindGroupLayoutEntry>,
            Vec<BindGroupEntry>,
        ) = self
            .attachments
            .iter()
            .enumerate()
            .map(|(binding, attachment)| {
                (
                    Self::create_attachment_bind_group_layout_entry(attachment, binding as u32),
                    Self::create_attachment_bind_group_entry(attachment, binding as u32),
                )
            })
            .unzip();

        let samplers = self.create_samplers(device);

        let (samplers_bind_group_layout_entries, samplers_bind_group_entries): (
            Vec<BindGroupLayoutEntry>,
            Vec<BindGroupEntry>,
        ) = samplers
            .iter()
            .enumerate()
            .map(|(i, (sampler, is_filtering, visibility))| {
                Self::create_sampler_entries(
                    sampler,
                    *is_filtering,
                    *visibility,
                    (i + bind_group_layout_entries.len()) as u32,
                )
            })
            .unzip();

        let all_bind_group_layout_entries: Vec<BindGroupLayoutEntry> = bind_group_layout_entries
            .into_iter()
            .chain(samplers_bind_group_layout_entries)
            .collect();

        let all_bind_group_entries: Vec<BindGroupEntry> = bind_group_entries
            .into_iter()
            .chain(samplers_bind_group_entries)
            .collect();

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some(&format!("Material {} | Bind group layout", self.id)),
            entries: &all_bind_group_layout_entries,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(&format!("Material {} | Bind group", self.id)),
            layout: &bind_group_layout,
            entries: &all_bind_group_entries,
        });

        self.bind_group_layout = Some(bind_group_layout);
        self.bind_group = Some(bind_group);

        self.is_initialized = true;

        Vec::new()
    }

    fn update(
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
        render_pass.set_bind_group(bind_group_offset, self.bind_group.as_ref().expect("The material bind group was not created. Remember to initialize the material before executing it."), &[]);

        for range in &self.component_ranges {
            for component in &other_components[range.clone()] {
                if !component.is_enabled() {
                    continue;
                }
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
                component.command_encoder_operations(
                    device,
                    queue,
                    encoder,
                    other_components,
                    materials,
                    computes,
                );
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
        self.is_enabled
    }

    fn set_enabled_state(&mut self, enabled_state: bool) {
        self.is_enabled = enabled_state;
    }
}
