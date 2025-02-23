use wgpu::{
    BindGroup, BindGroupLayout, ComputePass, ComputePipeline, Device, ShaderStages
};

use crate::engine_management::pipeline::{load_shader_module_descriptor, PipelineShader};

use super::{
    component::{ComponentDetails, ComponentId, ComponentSystem},
    entity::EntityId,
    material::ShaderAttachment,
};

#[derive(Debug)]
pub struct Compute {
    input: Vec<ShaderAttachment>,
    output: ShaderAttachment,
    shader_path: &'static str,
    is_spirv: bool,
    workgroup_counts: (u32, u32, u32),
    bind_group_layouts: Vec<BindGroupLayout>,
    bind_groups: Vec<BindGroup>,
    pipeline: ComputePipeline,
    id: ComponentId,
    is_enabled: bool,
    is_initialized: bool,
    parent_entity: EntityId,
}

impl Compute {
    fn create_bind_group_layout(
        attachment: &ShaderAttachment,
        device: &Device,
        compute_id: ComponentId,
    ) -> BindGroupLayout {
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some(&format!(
                "Compute {compute_id} | attachment {attachment:?} bind group layout"
            )),
            entries: &match attachment {
                ShaderAttachment::Texture(tex) => match &tex.texture {
                    super::material::GeneralTexture::Regular(_regular_tex) => vec![
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Texture {
                                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                                view_dimension: wgpu::TextureViewDimension::D2,
                                multisampled: false,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                            count: None,
                        },
                    ],
                    super::material::GeneralTexture::Storage(storage_tex) => {
                        vec![wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::StorageTexture {
                                access: storage_tex.access(),
                                format: storage_tex.format(),
                                view_dimension: wgpu::TextureViewDimension::D2,
                            },
                            count: None,
                        }]
                    }
                },
                ShaderAttachment::Buffer(buf) => vec![wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: buf.buffer_type,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            },
        })
    }

    fn create_bind_group(
        attachment: &ShaderAttachment,
        layout: &BindGroupLayout,
        device: &Device,
        compute_id: ComponentId,
    ) -> BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(&format!(
                "Compute {compute_id} | attachment {attachment:?} bind group"
            )),
            layout,
            entries: &match &attachment {
                ShaderAttachment::Texture(tex) => {
                    if let Some(sampler) = tex.texture.sampler_ref() {
                        vec![
                            wgpu::BindGroupEntry {
                                binding: 0,
                                resource: wgpu::BindingResource::TextureView(
                                    tex.texture.view_ref(),
                                ),
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
                ShaderAttachment::Buffer(buf) => vec![wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(buf.buffer.as_entire_buffer_binding()),
                }],
            },
        })
    }

    pub fn create_compute_pipeline(
        device: &Device,
        bind_group_layouts: &[&BindGroupLayout],
        shader_path: &'static str,
        compute_id: ComponentId,
        is_spirv: bool,
    ) -> ComputePipeline {
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some(&format!("Compute {compute_id} pipeline layout")),
            bind_group_layouts,
            push_constant_ranges: &[],
        });

        let module =
            load_shader_module_descriptor(device, &PipelineShader::Path(shader_path), is_spirv).unwrap();

        device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some(&format!("Compute {compute_id} pipeline")),
            layout: Some(&layout),
            module: &module,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        })
    }

    pub fn calculate(&self, compute_pass: &mut ComputePass) {
        for (i, bind_group) in self.bind_groups.iter().enumerate() {
            compute_pass.set_pipeline(&self.pipeline);
            compute_pass.set_bind_group(i as u32, bind_group, &[]);
            compute_pass.dispatch_workgroups(self.workgroup_counts.0, self.workgroup_counts.1, self.workgroup_counts.2);
        }
    }
}

impl ComponentSystem for Compute {
    fn initialize(&mut self, device: &Device) -> super::actions::ActionQueue {
        let (bind_group_layouts, bind_groups): (Vec<BindGroupLayout>, Vec<BindGroup>) = self
            .input
            .iter()
            .chain(Some(&self.output))
            .map(|attachment| {
                let bind_group_layout = Self::create_bind_group_layout(attachment, device, self.id);
                let bind_group =
                    Self::create_bind_group(attachment, &bind_group_layout, device, self.id);
                (bind_group_layout, bind_group)
            })
            .collect();

        self.pipeline = Self::create_compute_pipeline(
            device,
            &bind_group_layouts.iter().collect::<Vec<_>>(),
            self.shader_path,
            self.id,
            self.is_spirv,
        );

        self.bind_group_layouts = bind_group_layouts;
        self.bind_groups = bind_groups;

        self.set_initialized();
        Vec::new()
    }
}

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
