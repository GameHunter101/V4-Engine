use std::hash::{DefaultHasher, Hash, Hasher};

use wgpu::{
    BindGroup, BindGroupEntry, BindGroupLayout, BindGroupLayoutEntry, ComputePass, ComputePipeline,
    Device, ShaderStages,
};

use crate::engine_management::pipeline::{PipelineShader, load_shader_module_descriptor};

use super::{
    component::{ComponentDetails, ComponentId, ComponentSystem},
    entity::EntityId,
    material::ShaderAttachment,
};

#[derive(Debug)]
pub struct Compute {
    attachments: Vec<ShaderAttachment>,
    shader_path: &'static str,
    is_spirv: bool,
    workgroup_counts: (u32, u32, u32),
    bind_group_layout: Option<BindGroupLayout>,
    bind_group: Option<BindGroup>,
    pipeline: Option<ComputePipeline>,
    id: ComponentId,
    is_enabled: bool,
    is_initialized: bool,
    parent_entity: EntityId,
    iterate_count: usize,
}

impl Compute {
    pub fn builder() -> ComputeBuilder {
        ComputeBuilder::default()
    }
    fn create_bind_group_layout_entry(
        attachment: &ShaderAttachment,
        binding: u32,
    ) -> BindGroupLayoutEntry {
        match attachment {
            ShaderAttachment::Texture(tex) => {
                let props = tex.texture.properties();
                let view_dimension = if props.is_cubemap {
                    wgpu::TextureViewDimension::D2Array
                } else {
                    wgpu::TextureViewDimension::D2
                };
                if let Some(storage_tex_access) = props.storage_texture {
                    BindGroupLayoutEntry {
                        binding,
                        visibility: ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::StorageTexture {
                            access: storage_tex_access,
                            format: props.format,
                            view_dimension,
                        },
                        count: None,
                    }
                } else {
                    BindGroupLayoutEntry {
                        binding,
                        visibility: tex.visibility,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float {
                                filterable: props.is_filtered,
                            },
                            view_dimension,
                            multisampled: false,
                        },
                        count: None,
                    }
                }
            }
            ShaderAttachment::Buffer(buf) => BindGroupLayoutEntry {
                binding,
                visibility: ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: buf.buffer_type(),
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        }
    }

    fn create_bind_group_entry<'a>(attachment: &'a ShaderAttachment, binding: u32) -> BindGroupEntry<'a> {
        match attachment {
            ShaderAttachment::Texture(tex) => BindGroupEntry {
                binding,
                resource: wgpu::BindingResource::TextureView(tex.texture.view()),
            },
            ShaderAttachment::Buffer(buf) => wgpu::BindGroupEntry {
                binding,
                resource: wgpu::BindingResource::Buffer(buf.buffer().as_entire_buffer_binding()),
            },
        }
    }

    pub fn create_compute_pipeline(
        device: &Device,
        bind_group_layout: &BindGroupLayout,
        shader_path: &'static str,
        compute_id: ComponentId,
        is_spirv: bool,
    ) -> ComputePipeline {
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some(&format!("Compute {compute_id} pipeline layout")),
            bind_group_layouts: &[bind_group_layout],
            immediate_size: 0,
        });

        let module =
            load_shader_module_descriptor(device, &PipelineShader::Path(shader_path), is_spirv)
                .unwrap();

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
        compute_pass.set_pipeline(self.pipeline.as_ref().expect(
            "The compute pipeline was not created. Remember to initialize the compute before executing it.",
        ));
        compute_pass.set_bind_group(0, self.bind_group.as_ref().expect("The compute bind group was not created. Remember to initialize the compute before executing it."), &[]);
        compute_pass.dispatch_workgroups(
            self.workgroup_counts.0,
            self.workgroup_counts.1,
            self.workgroup_counts.2,
        );
    }

    /* pub fn copy_after_calculate(&self, encoder: &mut CommandEncoder) {
        if let Some(output_copy_attachment) = self.output_copy_target.as_ref() {
            if let Some(output) = self.output.as_ref() {
                match output_copy_attachment {
                    ShaderAttachment::Texture(copy_tex) => match output {
                        ShaderAttachment::Buffer(_output_buf) => {
                            panic!("Buffer to texture copies are not yet supported!")
                        }
                        ShaderAttachment::Texture(output_tex) => {
                            let output_tex = output_tex.texture.texture();
                            let copy_tex = copy_tex.texture.texture();
                            encoder.copy_texture_to_texture(
                                output_tex.as_image_copy(),
                                copy_tex.as_image_copy(),
                                Extent3d {
                                    width: copy_tex.width().min(output_tex.width()),
                                    height: copy_tex.height().min(output_tex.height()),
                                    depth_or_array_layers: copy_tex
                                        .depth_or_array_layers()
                                        .min(output_tex.depth_or_array_layers()),
                                },
                            );
                        }
                    },
                    ShaderAttachment::Buffer(copy_buf) => match output {
                        ShaderAttachment::Buffer(output_buf) => {
                            let output_buf = output_buf.buffer();
                            let copy_buf = copy_buf.buffer();
                            encoder.copy_buffer_to_buffer(
                                output_buf,
                                0,
                                copy_buf,
                                0,
                                copy_buf.size().min(output_buf.size()),
                            );
                        }
                        ShaderAttachment::Texture(_output_tex) => {
                            panic!("Texture to buffer copies are not yet supported!")
                        }
                    },
                }
            }
        }
    } */

    pub fn attachments(&self) -> &[ShaderAttachment] {
        &self.attachments
    }

    pub fn iterate_count(&self) -> usize {
        self.iterate_count
    }
}

impl ComponentSystem for Compute {
    fn initialize(&mut self, device: &Device) -> super::actions::ActionQueue {
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

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some(&format!("Compute {} bind group layout", self.id)),
            entries: &bind_group_layout_entries,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(&format!("Compute {} bind group", self.id)),
            layout: &bind_group_layout,
            entries: &bind_group_entries,
        });

        self.pipeline = Some(Self::create_compute_pipeline(
            device,
            &bind_group_layout,
            self.shader_path,
            self.id,
            self.is_spirv,
        ));

        self.bind_group_layout = Some(bind_group_layout);
        self.bind_group = Some(bind_group);

        self.set_initialized();
        vec![]
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

#[derive(Debug)]
pub struct ComputeBuilder {
    attachments: Vec<ShaderAttachment>,
    shader_path: &'static str,
    is_spirv: bool,
    workgroup_counts: (u32, u32, u32),
    id: ComponentId,
    enabled: bool,
    iterate_count: usize,
}

impl Default for ComputeBuilder {
    fn default() -> Self {
        Self {
            attachments: Vec::new(),
            shader_path: "",
            is_spirv: false,
            workgroup_counts: (0, 0, 0),
            id: 0,
            enabled: true,
            iterate_count: 1,
        }
    }
}

impl ComputeBuilder {
    pub fn attachments(mut self, attachments: Vec<ShaderAttachment>) -> Self {
        self.attachments = attachments;
        self
    }

    pub fn shader_path(mut self, shader_path: &'static str) -> Self {
        self.shader_path = shader_path;
        self
    }

    pub fn is_spirv(mut self, is_spirv: bool) -> Self {
        self.is_spirv = is_spirv;
        self
    }

    pub fn workgroup_counts(mut self, workgroup_counts: (u32, u32, u32)) -> Self {
        self.workgroup_counts = workgroup_counts;
        self
    }

    pub fn id(mut self, id: ComponentId) -> Self {
        self.id = id;
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn iterate_count(mut self, iterate_count: usize) -> Self {
        self.iterate_count = iterate_count;
        self
    }

    pub fn build(self) -> Compute {
        Compute {
            attachments: self.attachments,
            shader_path: self.shader_path,
            is_spirv: self.is_spirv,
            workgroup_counts: self.workgroup_counts,
            bind_group_layout: None,
            bind_group: None,
            pipeline: None,
            id: if self.id == 0 {
                let mut hasher = DefaultHasher::new();
                std::time::Instant::now().hash(&mut hasher);
                hasher.finish()
            } else {
                self.id
            },
            is_enabled: self.enabled,
            is_initialized: false,
            parent_entity: 0,
            iterate_count: self.iterate_count,
        }
    }
}
