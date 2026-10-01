use thiserror::Error;
use wgpu::{
    BindGroup, BindGroupEntry, BindGroupLayout, BindGroupLayoutEntry, Buffer, Device, Extent3d,
    Queue, util::DeviceExt,
};

use crate::ecs::{
    material::{RawAttachment, ShaderAttachment},
    scene::Id,
};

#[derive(Debug, Error)]
pub enum AttachmentError {
    #[error(
        "The pipeline was not initialized. Remember to initialize it before it is used. (Component {0})"
    )]
    PipelineNotInitialized(Id),
    #[error("The targeted attachment ({target}, {}) does not match the specified type ({}).", if *.target_is_texture {"Texture"} else {"Buffer"}, if *.target_is_texture {"Buffer"} else {"Texture"})]
    InvalidAttachmentUpdate {
        target: usize,
        target_is_texture: bool,
    },
    #[error("The targeted attachment ({0}) does not exist.")]
    AttachmentNotFound(usize),
    #[error(
        "The updated texture is larger than the original texture, but no new texture size was specified."
    )]
    NoNewTextureSize,
}

pub fn create_bind_group_entry<'a>(
    attachment: &'a RawAttachment<'a>,
    binding: u32,
) -> BindGroupEntry<'a> {
    let resource = match attachment {
        RawAttachment::Texture(tex) => wgpu::BindingResource::TextureView(tex.view()),
        RawAttachment::Buffer(buf) => buf.as_entire_binding(),
    };

    BindGroupEntry { binding, resource }
}

pub fn create_bind_group_layout_entry(
    attachment: &ShaderAttachment,
    binding: u32,
) -> BindGroupLayoutEntry {
    match attachment {
        ShaderAttachment::Texture(tex) => BindGroupLayoutEntry {
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
        ShaderAttachment::Buffer(buf) => BindGroupLayoutEntry {
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

/// Create a bind group entry for every attachment. This is useful for updating attachments, as
/// a new bind group needs to be created when new data exceeds the old capacity.
pub fn attachment_bind_group_entries<'a>(
    raw_attachments: &'a [RawAttachment<'a>],
) -> Vec<BindGroupEntry<'a>> {
    raw_attachments
        .iter()
        .enumerate()
        .map(|(binding, attachment)| create_bind_group_entry(attachment, binding as u32))
        .collect()
}

/// Updates the specified buffer with new data. Returns `true` if a the new data exceeds the
/// buffer's capacity and a new buffer is created.
pub fn update_buffer(
    buf: &mut Buffer,
    data: &[u8],
    device: &Device,
    queue: &Queue,
    label: Option<&str>,
) -> bool {
    if data.len() as u64 <= buf.size() {
        queue.write_buffer(buf, 0, data);
        false
    } else {
        *buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label,
            contents: data,
            usage: buf.usage(),
        });

        true
    }
}

pub fn attachments_to_raw<'a>(attachments: &'a [ShaderAttachment]) -> Vec<RawAttachment<'a>> {
    attachments
        .iter()
        .map(|attachment| match attachment {
            ShaderAttachment::Texture(tex) => RawAttachment::Texture(&tex.texture_bundle),
            ShaderAttachment::Buffer(buf) => RawAttachment::Buffer(buf.buffer()),
        })
        .collect()
}

/// Update the specified buffer attachment with raw byte data. Will error if either the
/// attachment could not be found, the pipeline is not initialized or if the selected
/// attachment is not a buffer.
pub(crate) fn update_buffer_attachment(
    attachments: &mut [ShaderAttachment],
    attachment_index: usize,
    data: &[u8],
    device: &Device,
    queue: &Queue,
    bind_group_layout: Option<&BindGroupLayout>,
    id: Id,
) -> Result<Option<BindGroup>, AttachmentError> {
    if let Some(attachment) = attachments.get_mut(attachment_index) {
        if let ShaderAttachment::Buffer(buf) = attachment {
            if buf.update_buffer(data, device, queue) {
                let raw_attachments = attachments_to_raw(attachments);
                let attachments = attachment_bind_group_entries(&raw_attachments);
                let Some(bind_group_layout) = bind_group_layout else {
                    return Err(AttachmentError::PipelineNotInitialized(id));
                };

                let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some(&format!("{id} | Bind group")),
                    layout: bind_group_layout,
                    entries: &attachments,
                });
                drop(raw_attachments);

                Ok(Some(bind_group))
            } else {
                Ok(None)
            }
        } else {
            Err(AttachmentError::InvalidAttachmentUpdate {
                target: attachment_index,
                target_is_texture: false,
            })
        }
    } else {
        Err(AttachmentError::AttachmentNotFound(attachment_index))
    }
}

/// Update the specified texture attachment with raw byte data. Will error if either the attachment
/// could not be found, the pipeline is not initialized, the selected attachment is not a texture,
/// or if the new data overflows the existing texture and no `new_tex_size` was specified.
pub(crate) fn update_texture_attachment(
    attachments: &mut [ShaderAttachment],
    attachment_index: usize,
    data: &[u8],
    new_tex_size: Option<Extent3d>,
    device: &Device,
    queue: &Queue,
    bind_group_layout: Option<&BindGroupLayout>,
    id: Id,
) -> Result<Option<BindGroup>, AttachmentError> {
    if let Some(attachment) = attachments.get_mut(attachment_index) {
        if let ShaderAttachment::Texture(tex) = attachment {
            if tex.update_texture(
                data,
                device,
                queue,
                Some(&format!("{:?} Shader Buffer", tex.visibility)),
                new_tex_size,
            )? {
                let raw_attachments = attachments_to_raw(attachments);
                let attachments = attachment_bind_group_entries(&raw_attachments);
                let Some(bind_group_layout) = &bind_group_layout else {
                    return Err(AttachmentError::PipelineNotInitialized(id));
                };

                let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some(&format!("{id} | Bind group")),
                    layout: bind_group_layout,
                    entries: &attachments,
                });
                drop(raw_attachments);

                Ok(Some(bind_group))
            } else {
                Ok(None)
            }
        } else {
            Err(AttachmentError::InvalidAttachmentUpdate {
                target: attachment_index,
                target_is_texture: true,
            })
        }
    } else {
        Err(AttachmentError::AttachmentNotFound(attachment_index))
    }
}
