use std::{
    any::Any,
    collections::{HashMap, HashSet},
    fmt::Debug,
    future::Future,
    pin::Pin,
    sync::Arc,
};

use glyphon::{FontSystem, SwashCache, TextAtlas, TextRenderer, Viewport};
use tokio::sync::Mutex;
use wgpu::{Device, Queue, RenderPipeline, TextureFormat};
use winit_input_helper::WinitInputHelper;

use crate::EngineDetails;

use super::{
    actions::ActionQueue,
    component::{Component, ComponentId},
    entity::{Entity, EntityId},
    material::{Material, MaterialAttachment, MaterialId},
    pipeline::{create_render_pipeline, PipelineDetails, PipelineId},
};

pub struct Scene {
    components: Vec<Component>,
    entities: HashMap<EntityId, Entity>,
    ui_components: Vec<ComponentId>,
    pipelines: HashMap<PipelineId, RenderPipeline>,
    materials: Vec<Material>,
    pipeline_to_corresponding_materials: HashMap<PipelineId, Vec<MaterialId>>,
    active_camera_id: Option<ComponentId>,
    total_entities_created: u32,
    font_state: FontState,
    workloads: HashMap<ComponentId, Workload>,
    workload_outputs: Arc<Mutex<HashMap<ComponentId, Vec<WorkloadOutput>>>>,
}

pub struct FontState {
    pub font_system: FontSystem,
    pub swash_cache: SwashCache,
    pub viewport: glyphon::Viewport,
    pub atlas: TextAtlas,
    pub text_renderer: TextRenderer,
    pub text_buffers: HashMap<ComponentId, TextRenderInfo>,
}

#[derive(Debug)]
pub struct TextDisplayInfo {
    pub on_screen_width: f32,
    pub on_screen_height: f32,
    pub top_left_pos: [f32; 2],
    pub scale: f32,
}

#[derive(Debug)]
pub struct TextRenderInfo {
    pub buffer: glyphon::Buffer,
    pub top_left_pos: [f32; 2],
    pub scale: f32,
    pub bounds: glyphon::TextBounds,
    pub attributes: TextAttributes,
}

#[derive(Debug, Clone)]
pub struct TextAttributes {
    pub color: glyphon::Color,
    pub family: FontFamily,
    pub stretch: glyphon::Stretch,
    pub style: glyphon::Style,
    pub weight: glyphon::Weight,
}

impl<'a> From<glyphon::Attrs<'a>> for TextAttributes {
    fn from(val: glyphon::Attrs<'a>) -> Self {
        TextAttributes {
            color: val.color_opt.unwrap_or(glyphon::Color::rgb(255, 255, 255)),
            family: val.family.into(),
            stretch: val.stretch,
            style: val.style,
            weight: val.weight,
        }
    }
}

#[allow(clippy::wrong_self_convention)]
impl TextAttributes {
    fn into_glyphon_attrs(&self) -> glyphon::Attrs {
        glyphon::Attrs::new()
            .color(self.color)
            .family(self.family.into_glyphon_family())
            .stretch(self.stretch)
            .style(self.style)
            .weight(self.weight)
    }
}

#[derive(Debug, Clone)]
pub enum FontFamily {
    Name(String),
    Serif,
    SansSerif,
    Cursive,
    Fantasy,
    Monospace,
}

impl<'a> From<glyphon::Family<'a>> for FontFamily {
    fn from(val: glyphon::Family<'a>) -> Self {
        match val {
            glyphon::Family::Name(name) => FontFamily::Name(name.to_owned()),
            glyphon::Family::Serif => FontFamily::Serif,
            glyphon::Family::SansSerif => FontFamily::SansSerif,
            glyphon::Family::Cursive => FontFamily::Cursive,
            glyphon::Family::Fantasy => FontFamily::Fantasy,
            glyphon::Family::Monospace => FontFamily::Monospace,
        }
    }
}

#[allow(clippy::wrong_self_convention)]
impl FontFamily {
    fn into_glyphon_family(&self) -> glyphon::Family {
        match self {
            FontFamily::Name(name) => glyphon::Family::Name(name),
            FontFamily::Serif => glyphon::Family::Serif,
            FontFamily::SansSerif => glyphon::Family::SansSerif,
            FontFamily::Cursive => glyphon::Family::Cursive,
            FontFamily::Fantasy => glyphon::Family::Fantasy,
            FontFamily::Monospace => glyphon::Family::Monospace,
        }
    }
}

impl Debug for Scene {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Scene")
            .field("components", &self.components.len())
            .finish()
    }
}

pub type WorkloadOutput = Box<dyn Any + Send>;
pub type Workload = Pin<Box<dyn Future<Output = WorkloadOutput> + Send>>;

impl Scene {
    pub fn new(device: &Device, queue: &Queue, format: TextureFormat) -> Self {
        let font_system = FontSystem::new();
        let swash_cache = SwashCache::new();
        let cache = glyphon::Cache::new(device);
        let viewport = Viewport::new(device, &cache);
        let mut atlas = TextAtlas::new(device, queue, &cache, format);
        let text_renderer =
            TextRenderer::new(&mut atlas, device, wgpu::MultisampleState::default(), None);

        let font_state = FontState {
            font_system,
            swash_cache,
            viewport,
            atlas,
            text_renderer,
            text_buffers: HashMap::new(),
        };

        Scene {
            components: Vec::new(),
            entities: HashMap::new(),
            ui_components: Vec::new(),
            pipelines: HashMap::new(),
            materials: Vec::new(),
            pipeline_to_corresponding_materials: HashMap::new(),
            active_camera_id: None,
            total_entities_created: 0,
            font_state,
            workloads: HashMap::new(),
            workload_outputs: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn initialize(&mut self, device: &Device) {
        let action_queue: ActionQueue = self
            .components
            .iter_mut()
            .flat_map(|component| component.initialize(device))
            .collect();
        self.execute_action_queue(action_queue).await;
    }

    pub async fn update(
        &mut self,
        device: &Device,
        queue: &Queue,
        input_manager: &WinitInputHelper,
        engine_details: &EngineDetails,
    ) {
        let active_camera_id = self.active_camera_id;
        let all_components = &mut self.components;
        let workload_outputs = self.workload_outputs.clone();
        let actions: Vec<_> = (0..all_components.len())
            .map(|i| {
                let workload_outputs = workload_outputs.clone();
                if !all_components[i].is_enabled() {
                    return Vec::new();
                }
                let (_, outputs) = async_scoped::TokioScope::scope_and_block(|scope| {
                    let (components_before, components_after_and_this) =
                        all_components.split_at_mut(i);
                    let proc = async move {
                        if let Some((component, components_after)) =
                            components_after_and_this.split_first_mut()
                        {
                            let chain: Vec<&mut Component> = components_before
                                .iter_mut()
                                .chain(components_after.iter_mut())
                                .collect();
                            component
                                .update(
                                    device,
                                    queue,
                                    input_manager,
                                    &chain,
                                    active_camera_id,
                                    engine_details,
                                    workload_outputs,
                                )
                                .await
                        } else {
                            Vec::new()
                        }
                    };
                    scope.spawn(proc)
                });
                outputs
            })
            .collect();
        let action_queue: ActionQueue = actions
            .into_iter()
            .flatten()
            .flat_map(|actions| actions.unwrap_or_default())
            .collect();

        self.execute_action_queue(action_queue).await;
    }

    pub fn attach_workload(&mut self, component_id: ComponentId, workload: Workload) {
        self.workloads.insert(component_id, workload);
    }

    #[tokio::main]
    pub async fn run_workloads(&mut self) {
        let keys: Vec<ComponentId> = self.workloads.keys().cloned().collect();
        let workloads = self.workload_outputs.clone();

        async_scoped::TokioScope::scope_and_block(|scope| {
            for component_id in keys {
                let workload = self.workloads.remove(&component_id).unwrap();
                let workloads = workloads.clone();
                let proc = async move {
                    let mut workloads = workloads.lock().await;
                    if let Some(outputs) = workloads.get_mut(&component_id) {
                        outputs.push(workload.await);
                    } else {
                        workloads.insert(component_id, vec![workload.await]);
                    }
                };
                scope.spawn(proc);
            }
        });
    }

    pub async fn free_workload_output(
        &mut self,
        component_id: ComponentId,
        workload_output_index: usize,
    ) {
        self.workload_outputs
            .lock()
            .await
            .get_mut(&component_id)
            .expect("Failed to get workloads assigned to the given component ID.")
            .remove(workload_output_index);
    }

    pub fn create_material(
        &mut self,
        device: &Device,
        render_format: TextureFormat,
        vertex_shader_path: &'static str,
        fragment_shader_path: &'static str,
        attachments: Vec<MaterialAttachment>,
        pipeline_details: PipelineDetails,
    ) -> MaterialId {
        let new_material = Material::new(
            device,
            self.materials.len(),
            vertex_shader_path,
            fragment_shader_path,
            attachments,
        );
        if let std::collections::hash_map::Entry::Vacant(e) = self
            .pipelines
            .entry((vertex_shader_path, fragment_shader_path))
        {
            e.insert(create_render_pipeline(
                device,
                vertex_shader_path,
                fragment_shader_path,
                new_material.bind_group_layouts(),
                render_format,
                pipeline_details,
            ));
            self.pipeline_to_corresponding_materials
                .insert((vertex_shader_path, fragment_shader_path), Vec::new());
        }

        let id = new_material.id();

        self.materials.push(new_material);

        if let Some(indices) = self
            .pipeline_to_corresponding_materials
            .get_mut(&(vertex_shader_path, fragment_shader_path))
        {
            indices.push(id);
        }

        id
    }

    pub fn pipelines(&self) -> &HashMap<PipelineId, RenderPipeline> {
        &self.pipelines
    }

    pub fn get_pipeline_materials(&self, pipeline_id: PipelineId) -> Vec<&Material> {
        let material_ids = self.pipeline_to_corresponding_materials.get(&pipeline_id);
        match material_ids {
            Some(material_ids) => self
                .materials
                .iter()
                .filter(|mat| material_ids.contains(&mat.id()))
                .collect(),
            None => Vec::new(),
        }
    }

    pub fn get_components_per_material(&self) -> HashMap<MaterialId, Vec<&Component>> {
        let mut output: HashMap<MaterialId, Vec<&Component>> = self
            .materials
            .iter()
            .map(|mat| (mat.id(), Vec::new()))
            .collect();

        for component in &self.components {
            let component_parent_entity_id = component.parent_entity_id();
            let parent_entity_material_id =
                self.entities[&component_parent_entity_id].active_material();
            if let Some(parent_entity_material_id) = parent_entity_material_id {
                if self.entities[&component_parent_entity_id].is_enabled() && component.is_enabled()
                {
                    output
                        .get_mut(&parent_entity_material_id)
                        .unwrap()
                        .push(component);
                }
            }
        }

        output
    }

    pub fn create_entity(
        &mut self,
        parent: Option<EntityId>,
        components: Vec<Component>,
        material: Option<MaterialId>,
        is_enabled: bool,
    ) -> EntityId {
        let entity = Entity::new(
            self.total_entities_created + 1,
            Vec::new(),
            parent.unwrap_or(0),
            is_enabled,
            material,
        );
        let id = entity.id();

        self.entities.insert(id, entity);
        self.total_entities_created += 1;

        for component in components {
            let insert_index = self
                .components
                .iter()
                .position(|comp| comp.rendering_order() >= component.rendering_order())
                .unwrap_or(self.components.len());
            self.components.insert(insert_index, component);
            self.components
                .get_mut(insert_index)
                .unwrap()
                .set_parent_entity(id);
        }

        id
    }

    pub fn get_entity(&self, entity_id: EntityId) -> Option<&Entity> {
        self.entities.get(&entity_id)
    }

    pub fn get_entity_mut(&mut self, entity_id: EntityId) -> Option<&mut Entity> {
        self.entities.get_mut(&entity_id)
    }

    pub fn get_component(&self, component_id: ComponentId) -> Option<&Component> {
        self.components
            .iter()
            .find(|&comp| comp.id() == component_id)
    }

    pub fn get_component_mut(&mut self, component_id: ComponentId) -> Option<&mut Component> {
        self.components
            .iter_mut()
            .find(|comp| comp.id() == component_id)
    }

    pub fn create_text_buffer(
        &mut self,
        component_id: ComponentId,
        text: &str,
        text_attributes: TextAttributes,
        text_metrics: glyphon::Metrics,
        text_display_info: TextDisplayInfo,
    ) {
        let font_system = &mut self.font_state.font_system;
        let mut text_buffer = glyphon::Buffer::new(font_system, text_metrics);
        text_buffer.set_size(
            font_system,
            Some(text_display_info.on_screen_height),
            Some(text_display_info.on_screen_height),
        );
        text_buffer.set_text(
            font_system,
            text,
            text_attributes.into_glyphon_attrs(),
            glyphon::Shaping::Advanced,
        );

        self.font_state.text_buffers.insert(
            component_id,
            TextRenderInfo {
                buffer: text_buffer,
                top_left_pos: text_display_info.top_left_pos,
                bounds: glyphon::TextBounds {
                    left: text_display_info.top_left_pos[0] as i32,
                    top: text_display_info.top_left_pos[1] as i32,
                    right: (text_display_info.top_left_pos[0] + text_display_info.on_screen_width)
                        as i32,
                    bottom: (text_display_info.top_left_pos[1] + text_display_info.on_screen_height)
                        as i32,
                },
                scale: text_display_info.scale,
                attributes: text_attributes,
            },
        );
    }

    pub fn update_text_buffer(
        &mut self,
        component_id: ComponentId,
        text: Option<String>,
        text_attributes: Option<TextAttributes>,
        text_metrics: Option<glyphon::Metrics>,
        text_display_info: Option<TextDisplayInfo>,
    ) {
        let font_system = &mut self.font_state.font_system;
        if let Some(text_buffer) = self.font_state.text_buffers.get_mut(&component_id) {
            if let Some(new_text) = text {
                let attrs = text_attributes.as_ref().unwrap_or(&text_buffer.attributes);
                text_buffer.buffer.set_text(
                    font_system,
                    &new_text,
                    attrs.into_glyphon_attrs(),
                    glyphon::Shaping::Advanced,
                );
                text_buffer.attributes = attrs.clone();
            }
            if let Some(new_text_metrics) = text_metrics {
                text_buffer
                    .buffer
                    .set_metrics(font_system, new_text_metrics);
            }
            if let Some(new_text_display_info) = text_display_info {
                text_buffer.bounds = glyphon::TextBounds {
                    left: new_text_display_info.top_left_pos[0] as i32,
                    top: new_text_display_info.top_left_pos[1] as i32,
                    right: (new_text_display_info.top_left_pos[0]
                        + new_text_display_info.on_screen_width) as i32,
                    bottom: (new_text_display_info.top_left_pos[1]
                        + new_text_display_info.on_screen_height)
                        as i32,
                };
            }
        }
    }

    pub fn update_text_viewport(&mut self, queue: &Queue, new_size: (u32, u32)) {
        self.font_state.viewport.update(
            queue,
            glyphon::Resolution {
                width: new_size.0,
                height: new_size.1,
            },
        );
    }

    pub fn font_state_mut(&mut self) -> &mut FontState {
        &mut self.font_state
    }

    pub fn enabled_ui_components(&self) -> HashSet<ComponentId> {
        self.components
            .iter()
            .filter_map(|comp| {
                if comp.is_enabled() && self.ui_components.contains(&comp.id()) {
                    Some(comp.id())
                } else {
                    None
                }
            })
            .collect()
    }

    pub async fn execute_action_queue(&mut self, action_queue: ActionQueue) {
        for action in action_queue {
            action.execute_async(self).await;
        }
        if !self.workloads.is_empty() {
            std::thread::scope(|scope| {
                scope.spawn(|| {
                    self.run_workloads();
                });
            });
        }
    }

    pub fn register_ui_component(&mut self, component_id: ComponentId) {
        self.ui_components.push(component_id);
    }
}
