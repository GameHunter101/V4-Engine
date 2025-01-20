use wgpu::{BindGroup, BindGroupLayout, Buffer, Device, ShaderStages};

use crate::engine_support::texture_support::Texture;

use super::pipeline::PipelineId;


#[derive(Debug)]
pub struct MaterialTextureAttachment {
    pub texture: Texture,
    pub visibility: ShaderStages,
}

#[derive(Debug)]
pub struct MaterialBufferAttachment {
    pub buffer: Buffer,
    pub visibility: ShaderStages,
}

#[derive(Debug)]
pub enum MaterialAttachment {
    Texture(MaterialTextureAttachment),
    Buffer(MaterialBufferAttachment),
}

pub type MaterialId = usize;

#[derive(Debug)]
pub struct Material {
    id: MaterialId,
    pipeline_id: PipelineId,
    attachments: Vec<MaterialAttachment>,
    bind_group_layouts: Vec<BindGroupLayout>,
    bind_groups: Vec<BindGroup>,
}

impl Material {
    pub fn new(
        id: MaterialId,
        pipeline_id: PipelineId,
        attachments: Vec<MaterialAttachment>,
    ) -> Self {

        Self {
            id,
            attachments,
            pipeline_id,
            bind_group_layouts: Vec::new(),
            bind_groups: Vec::new(),
        }
    }

    pub fn initialize(&mut self, device: &Device) {
        let (bind_group_layouts, bind_groups): (Vec<BindGroupLayout>, Vec<BindGroup>) = self.attachments
            .iter()
            .map(|attachment| {
                let bind_group_layout =
                    Self::create_attachment_bind_group_layout(device, self.id, attachment);
                let bind_group =
                    Self::create_attachment_bind_group(device, self.id, attachment, &bind_group_layout);
                (bind_group_layout, bind_group)
            })
            .unzip();

        self.bind_group_layouts = bind_group_layouts;
        self.bind_groups = bind_groups;
    }

    pub fn create_attachment_bind_group_layout(
        device: &Device,
        material_id: MaterialId,
        attachment: &MaterialAttachment,
    ) -> BindGroupLayout {
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some(&format!(
                "Material {material_id} | attachment {attachment:?} Bind Group Layout"
            )),
            entries: &match attachment {
                MaterialAttachment::Texture(tex) => vec![
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
                MaterialAttachment::Buffer(buf) => vec![wgpu::BindGroupLayoutEntry {
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
        material_id: MaterialId,
        attachment: &MaterialAttachment,
        bind_group_layout: &BindGroupLayout,
    ) -> BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(&format!(
                "Material {material_id} | attachment {attachment:?} bind group"
            )),
            layout: bind_group_layout,
            entries: &match attachment {
                MaterialAttachment::Texture(tex) => vec![
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(tex.texture.view_ref()),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(tex.texture.sampler_ref()),
                    },
                ],
                MaterialAttachment::Buffer(buf) => {
                    vec![wgpu::BindGroupEntry {
                        binding: 0,
                        resource: buf.buffer.as_entire_binding(),
                    }]
                }
            },
        })
    }

    pub fn bind_group_layouts(&self) -> &[BindGroupLayout] {
        self.bind_group_layouts.as_ref()
    }

    pub fn id(&self) -> usize {
        self.id
    }

    pub fn bind_groups(&self) -> &[BindGroup] {
        self.bind_groups.as_ref()
    }

    pub fn attachments(&self) -> &[MaterialAttachment] {
        self.attachments.as_ref()
    }
}
