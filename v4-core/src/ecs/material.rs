use std::{
    collections::{HashMap, HashSet},
    ops::Range,
};

use wgpu::{
    BindGroup, BindGroupEntry, BindGroupLayout, BindGroupLayoutEntry, Buffer, CommandEncoder,
    Device, Extent3d, Queue, Sampler, ShaderStages, util::DeviceExt,
};

use crate::{
    engine_management::pipeline::PipelineParameters,
    engine_support::texture_support::{TextureBundle, TextureProperties},
};

use super::{
    actions::ActionQueue,
    component::{Component, ComponentDetails, ComponentSystem, UpdateParams},
    compute::Compute,
    scene::Id,
};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum MaterialError {
    #[error(
        "The render pipeline was not created. Remember to initialize the material before executing it. (Material {0})"
    )]
    PipelineNotInitialized(Id),
    #[error("The targeted attachment ({target}, {}) does not match the specified type ({}).", if *.target_is_texture {"Texture"} else {"Buffer"}, if *.target_is_texture {"Buffer"} else {"Texture"})]
    InvalidAttachmentUpdate {
        target: usize,
        target_is_texture: bool,
    },
    #[error("The targeted attachment ({0}) does not exist.")]
    AttachmentNotFound(usize),
    #[error("Error from shader attachment: {0:?}")]
    ShaderAttachmentError(#[from] ShaderAttachmentError),
}

#[derive(Debug, Error)]
pub enum ShaderAttachmentError {
    #[error(
        "The updated texture is larger than the original texture, but no new texture size was specified."
    )]
    NoNewTextureSize,
}

#[derive(Debug, Clone)]
pub struct ShaderTextureAttachment {
    pub texture_bundle: TextureBundle,
    pub visibility: ShaderStages,
}

impl ShaderTextureAttachment {
    pub fn update_texture(
        &mut self,
        data: &[u8],
        device: &Device,
        queue: &Queue,
        label: Option<&str>,
        new_tex_size: Option<Extent3d>,
    ) -> Result<bool, ShaderAttachmentError> {
        let texture = self.texture_bundle.view().texture();
        let tex_pixel_size =
            texture.format().block_copy_size(None).unwrap() * texture.format().components() as u32;
        let tex_size = texture.size();
        if data.len() as u64 <= (tex_pixel_size * tex_size.width * tex_size.height) as u64 {
            queue.write_texture(
                texture.as_image_copy(),
                data,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(texture.size().width * tex_pixel_size),
                    rows_per_image: Some(texture.size().height),
                },
                texture.size(),
            );
            Ok(false)
        } else {
            let Some(size) = new_tex_size else {
                return Err(ShaderAttachmentError::NoNewTextureSize);
            };

            *self.texture_bundle.view_mut() = device
                .create_texture(&wgpu::TextureDescriptor {
                    label,
                    size,
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: texture.dimension(),
                    format: texture.format(),
                    usage: texture.usage(),
                    view_formats: &[],
                })
                .create_view(&wgpu::TextureViewDescriptor {
                    array_layer_count: Some(tex_size.depth_or_array_layers),
                    ..Default::default()
                });

            Ok(true)
        }
    }
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
                label: Some(&format!("{visibility:?} Shader Buffer")),
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

    pub fn update_buffer(&mut self, contents: &[u8], device: &Device, queue: &Queue) -> bool {
        crate::engine_support::misc_utils::update_buffer(
            &mut self.buffer,
            contents,
            device,
            queue,
            Some(&format!("{:?} Shader Buffer", self.visibility)),
        )
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
    id: Id,
    entities_attached: HashSet<Id>,
    component_ranges: Vec<Range<usize>>,
    attachments: Vec<ShaderAttachment>,
    bind_group_layout: Option<BindGroupLayout>,
    bind_group: Option<BindGroup>,
    immediate_data: Vec<u8>,
    is_initialized: bool,
    is_enabled: bool,
    parent_entity: Id,
}

#[derive(Debug)]
pub enum PipelineOptions {
    Descriptor(PipelineParameters),
    Id(Id),
}

impl Material {
    pub fn new(
        id: Id,
        attachments: Vec<ShaderAttachment>,
        immediate_data: Vec<u8>,
        is_enabled: bool,
    ) -> Self {
        Self {
            id,
            attachments,
            entities_attached: HashSet::new(),
            component_ranges: Vec::new(),
            bind_group_layout: None,
            bind_group: None,
            immediate_data,
            is_initialized: false,
            is_enabled,
            parent_entity: Id::nil(),
        }
    }

    pub fn create_bind_group_layout_entry(
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

    fn create_bind_group_entry<'a>(
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

    pub fn create_bind_group(
        &self,
        layout: &BindGroupLayout,
        entries: &[BindGroupEntry],
        device: &Device,
    ) -> BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(&format!("Material {} | Bind group", self.id)),
            layout,
            entries,
        })
    }

    /// Update the specified buffer attachment with raw byte data. Will error if either the
    /// attachment could not be found, the pipeline is not initialized or if the selected
    /// attachment is not a buffer.
    pub fn update_buffer_attachment(
        &mut self,
        attachment_index: usize,
        data: &[u8],
        device: &Device,
        queue: &Queue,
    ) -> Result<(), MaterialError> {
        if let Some(attachment) = self.attachments_mut().get_mut(attachment_index) {
            if let ShaderAttachment::Buffer(buf) = attachment {
                if buf.update_buffer(data, device, queue) {
                    let attachments = self.attachment_bind_group_entries();
                    let Some(bind_group_layout) = &self.bind_group_layout else {
                        return Err(MaterialError::PipelineNotInitialized(self.id));
                    };

                    self.bind_group = Some(self.create_bind_group(bind_group_layout, &attachments, device));
                }
                Ok(())
            } else {
                Err(MaterialError::InvalidAttachmentUpdate {
                    target: attachment_index,
                    target_is_texture: false,
                })
            }
        } else {
            Err(MaterialError::AttachmentNotFound(attachment_index))
        }
    }

    /// Create a bind group entry for every attachment. This is useful for updating attachments, as
    /// a new bind group needs to be created when new data exceeds the old capacity.
    fn attachment_bind_group_entries(&self) -> Vec<BindGroupEntry<'_>> {
        self.attachments
            .iter()
            .enumerate()
            .map(|(binding, attachment)| Self::create_bind_group_entry(attachment, binding as u32))
            .collect()
    }

    /// Update the specified texture attachment with raw byte data. Will error if either the attachment
    /// could not be found, the pipeline is not initialized, the selected attachment is not a texture,
    /// or if the new data overflows the existing texture and no `new_tex_size` was specified.
    pub fn update_texture_attachment(
        &mut self,
        attachment_index: usize,
        data: &[u8],
        new_tex_size: Option<Extent3d>,
        device: &Device,
        queue: &Queue,
    ) -> Result<(), MaterialError> {
        if let Some(attachment) = self.attachments_mut().get_mut(attachment_index) {
            if let ShaderAttachment::Texture(tex) = attachment {
                if tex.update_texture(
                    data,
                    device,
                    queue,
                    Some(&format!("{:?} Shader Buffer", tex.visibility)),
                    new_tex_size,
                )? {
                    let attachments = self.attachment_bind_group_entries();
                    let Some(bind_group_layout) = &self.bind_group_layout else {
                        return Err(MaterialError::PipelineNotInitialized(self.id));
                    };

                    self.bind_group = Some(self.create_bind_group(bind_group_layout, &attachments, device));
                }

                Ok(())
            } else {
                Err(MaterialError::InvalidAttachmentUpdate {
                    target: attachment_index,
                    target_is_texture: true,
                })
            }
        } else {
            Err(MaterialError::AttachmentNotFound(attachment_index))
        }
    }

    pub fn attach_entity(&mut self, entity_id: Id) {
        self.entities_attached.insert(entity_id);
    }

    pub fn remove_entity(&mut self, entity_id: Id) {
        self.entities_attached.remove(&entity_id);
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

    pub fn attachments_mut(&mut self) -> &mut [ShaderAttachment] {
        self.attachments.as_mut()
    }

    pub fn get_immediate_data(&self) -> &[u8] {
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
                    Self::create_bind_group_layout_entry(attachment, binding as u32),
                    Self::create_bind_group_entry(attachment, binding as u32),
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

        let bind_group =
            self.create_bind_group(&bind_group_layout, &all_bind_group_entries, device);

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
        pipeline_parameters: &PipelineParameters,
    ) {
        let bind_group_offset = pipeline_parameters.uses_camera as u32;
        let bind_group = self.bind_group.as_ref().expect("The material bind group was not created. Remember to initialize the material before executing it.");
        render_pass.set_bind_group(bind_group_offset, bind_group, &[]);

        for range in &self.component_ranges {
            for component in &other_components[range.clone()] {
                if !component.is_enabled() {
                    continue;
                }
                component.render(
                    device,
                    queue,
                    render_pass,
                    other_components,
                    pipeline_parameters,
                );
            }
        }
    }

    fn command_encoder_operations(
        &self,
        device: &Device,
        queue: &Queue,
        encoder: &mut CommandEncoder,
        other_components: &[&Component],
        materials: &HashMap<Id, Material>,
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
    fn id(&self) -> Id {
        self.id
    }

    fn set_id(&mut self, id: Id) {
        self.id = id;
    }

    fn is_initialized(&self) -> bool {
        self.is_initialized
    }

    fn set_initialized(&mut self) {
        self.is_initialized = true;
    }

    fn parent_entity_id(&self) -> Id {
        self.parent_entity
    }

    fn set_parent_entity(&mut self, _parent_id: Id) {}

    fn is_enabled(&self) -> bool {
        self.is_enabled
    }

    fn set_enabled_state(&mut self, enabled_state: bool) {
        self.is_enabled = enabled_state;
    }
}
