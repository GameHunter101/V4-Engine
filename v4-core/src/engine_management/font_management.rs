use crate::ecs::component::ComponentId;

use glyphon::{FontSystem, SwashCache, TextAtlas, TextRenderer};
use std::{collections::HashMap, fmt::Debug};

pub struct FontState {
    pub font_system: FontSystem,
    pub swash_cache: SwashCache,
    pub viewport: glyphon::Viewport,
    pub atlas: TextAtlas,
    pub text_renderer: TextRenderer,
    pub text_buffers: HashMap<ComponentId, TextRenderInfo>,
}

impl FontState {
    pub fn create_text_buffer(
        &mut self,
        component_id: ComponentId,
        text: &str,
        text_attributes: TextAttributes,
        text_metrics: glyphon::Metrics,
        text_display_info: TextDisplayInfo,
    ) {
        let font_system = &mut self.font_system;
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

        self.text_buffers.insert(
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
        let font_system = &mut self.font_system;
        if let Some(text_buffer) = self.text_buffers.get_mut(&component_id) {
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

#[derive(Debug)]
pub struct TextComponentProperties {
    pub text: String,
    pub text_attributes: TextAttributes,
    pub text_metrics: glyphon::Metrics,
    pub text_display_info: TextDisplayInfo,
}
