#![allow(clippy::large_enum_variant)]
use std::collections::{HashMap, HashSet};

use proc_macro2::{Span, TokenStream, TokenTree};
use quote::{ToTokens, quote};
use syn::{
    Error, Expr, ExprMethodCall, ExprStruct, FieldValue, Ident, LitBool, LitStr, Macro, Path,
    Token, braced, bracketed,
    parse::{Parse, ParseStream, Parser},
    parse_quote,
    spanned::Spanned,
};
use uuid::Uuid;

#[derive(Debug)]
enum Id {
    Raw(LitStr),
    Processed(Uuid),
}

impl Parse for Id {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        Ok(Id::Raw(input.parse()?))
    }
}

trait GetId {
    fn populate_id_map(&mut self, id_map: &mut HashMap<LitStr, Uuid>);
}

#[derive(Debug)]
struct ModularStruct {
    ident: Option<Ident>,
    fields: HashMap<String, FieldValue>,
    span: Span,
}

impl ModularStruct {
    fn parse(
        input: ParseStream,
        struct_ident: &str,
        mandatory_fields: Vec<&str>,
        optional_fields: Vec<&str>,
    ) -> syn::Result<Self> {
        let modular_struct = Self::parse_everything(input)?;

        if let Some(ident) = &modular_struct.ident
            && ident != struct_ident
        {
            return Err(Error::new_spanned(
                modular_struct.ident,
                format!("Expected identifier '{struct_ident}'"),
            ));
        }

        let mandatory_fields_set = HashSet::<&str>::from_iter(mandatory_fields);
        let optional_fields_set = HashSet::from_iter(optional_fields);

        let complete_names_set: HashSet<&str> = mandatory_fields_set
            .clone()
            .union(&optional_fields_set)
            .copied()
            .collect();

        if let Some(extraneous_field) = modular_struct
            .fields
            .keys()
            .find(|ident| !complete_names_set.contains(ident.as_str()))
        {
            return Err(Error::new_spanned(
                extraneous_field,
                format!("No such field '{extraneous_field}'"),
            ));
        }

        let field_names: HashSet<&str> = modular_struct
            .fields
            .keys()
            .map(|ident| ident.as_str())
            .collect();

        let missing_fields: Vec<&&str> = mandatory_fields_set.difference(&field_names).collect();

        if !missing_fields.is_empty() {
            return Err(Error::new(
                modular_struct.span,
                format!("Missing modular struct fields: {missing_fields:?}"),
            ));
        }

        Ok(modular_struct)
    }

    fn parse_everything(input: ParseStream) -> syn::Result<Self> {
        let base_struct: ExprStruct = input.parse()?;
        let span = base_struct.span();

        let ident = base_struct.path.get_ident().cloned();

        if ident.is_none() {
            return Err(Error::new_spanned(
                base_struct.path,
                "This modular struct needs to have an identifier.",
            ));
        };

        let fields_map: HashMap<String, FieldValue> = base_struct
            .fields
            .into_iter()
            .flat_map(|field| match &field.member {
                syn::Member::Named(ident) => Some((ident.to_string(), field)),
                syn::Member::Unnamed(_) => None,
            })
            .collect();

        Ok(ModularStruct {
            ident,
            fields: fields_map,
            span,
        })
    }

    fn parse_no_ident(
        input: ParseStream,
        mandatory_fields: Vec<&str>,
        optional_fields: Vec<&str>,
    ) -> syn::Result<Self> {
        let span = input.span();
        let contents;
        braced!(contents in input);

        let all_fields: HashSet<&str> = mandatory_fields
            .iter()
            .chain(&optional_fields)
            .copied()
            .collect();

        let fields = contents
            .parse_terminated(FieldValue::parse, Token![,])?
            .into_iter()
            .flat_map(|field| match &field.member {
                syn::Member::Named(ident) => {
                    if all_fields.contains(ident.to_string().as_str()) {
                        Some(Ok((ident.to_string(), field)))
                    } else {
                        Some(Err(Error::new_spanned(
                            ident,
                            format!("Unexpected field '{ident}'"),
                        )))
                    }
                }
                syn::Member::Unnamed(_) => None,
            })
            .collect::<syn::Result<HashMap<String, FieldValue>>>()?;

        Ok(Self {
            ident: None,
            fields,
            span,
        })
    }

    fn get_optional_field<T: Parse>(&self, field: &str) -> syn::Result<Option<T>> {
        self.get_optional_field_with(field, T::parse)
    }

    fn get_optional_field_with<T>(
        &self,
        field: &str,
        parser: impl Fn(ParseStream) -> syn::Result<T>,
    ) -> syn::Result<Option<T>> {
        let expr = self.fields.get(field).map(|field| &field.expr);

        if let Some(expr) = expr {
            Ok(Some(parser.parse2(quote! {#expr})?))
        } else {
            Ok(None)
        }
    }

    fn get_mandatory_field<T: Parse>(&self, field: &str) -> syn::Result<T> {
        if let Some(val) = self.get_optional_field::<T>(field)? {
            Ok(val)
        } else {
            Err(Error::new(self.span, format!("Field not found: '{field}'")))
        }
    }
}

fn expr_to_array<T>(input: Expr, parser: fn(ParseStream) -> syn::Result<T>) -> syn::Result<Vec<T>> {
    let array_parse = |input: ParseStream| {
        let array_contents;
        bracketed!(array_contents in input);
        Ok(Vec::from_iter(
            array_contents.parse_terminated(parser, Token![,])?,
        ))
    };

    array_parse.parse2(quote! {#input})
}

fn id_to_tokens(id: Uuid) -> TokenStream {
    let raw_id = id.to_u128_le();
    quote! {v4::ecs::scene::Id::from_u128_le(#raw_id)}
}

fn quote_or<T: ToTokens>(val: Option<&T>, default: TokenStream) -> TokenStream {
    val.map(|v| quote! {#v}).unwrap_or(default)
}

pub struct SceneDescriptor {
    attributes: SceneAttributes,
    entities: Vec<EntityDescriptor>,
    id_map: HashMap<LitStr, Uuid>,
}

impl SceneDescriptor {
    fn get_all_ids(
        screenspace_materials: &mut [MaterialDescriptor],
        entities: &mut [EntityDescriptor],
    ) -> HashMap<LitStr, Uuid> {
        let mut id_map = HashMap::new();

        for screenspace_mat in screenspace_materials {
            screenspace_mat.populate_id_map(&mut id_map);
        }

        for entity in entities {
            entity.populate_id_map(&mut id_map);
        }

        id_map
    }
}

impl Parse for SceneDescriptor {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let items = input.parse_terminated(SceneItems::parse, Token![,])?;

        let (attributes, entities): (Vec<Option<FieldValue>>, Vec<Option<EntityDescriptor>>) =
            items
                .into_iter()
                .map(|item| match item {
                    SceneItems::SceneAttribute(field_value) => (Some(field_value), None),
                    SceneItems::Entity(entity) => (None, Some(entity)),
                })
                .unzip();

        let mut attributes =
            SceneAttributes::validate_attributes(attributes.into_iter().flatten().collect())?;
        let mut entities: Vec<EntityDescriptor> = entities.into_iter().flatten().collect();

        let id_map = Self::get_all_ids(&mut attributes.screenspace_materials, &mut entities);

        Ok(SceneDescriptor {
            attributes,
            entities,
            id_map,
        })
    }
}

impl ToTokens for SceneDescriptor {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        let active_cam = if let Some(active_cam) = &self.attributes.active_camera {
            quote! {Some(#active_cam)}
        } else {
            quote! {None}
        };

        let screenspace_materials: TokenStream = self
            .attributes
            .screenspace_materials
            .iter()
            .map(|mat| quote! {#mat;})
            .collect();

        let id_macro_fields: TokenStream = self
            .id_map
            .iter()
            .map(|(raw, id)| {
                let id_tokens = id_to_tokens(*id);
                quote! {(#raw) => {#id_tokens};}
            })
            .collect();

        let id_macro = quote! {
            macro_rules! ID {
                #id_macro_fields
                ($($fallback:tt)*) => {compile_error!("Invalid ID provided")};
            }
        };

        let entities = &self.entities;

        tokens.extend(quote! {
            {
                #id_macro

                let mut scene = v4::ecs::scene::Scene::default();

                #screenspace_materials

                #(#entities)*

                scene.set_active_camera(#active_cam);

                scene
            }
        });
    }
}

pub struct SceneAttributes {
    active_camera: Option<Macro>,
    screenspace_materials: Vec<MaterialDescriptor>,
}

impl SceneAttributes {
    fn validate_attributes(attributes: Vec<FieldValue>) -> syn::Result<SceneAttributes> {
        let mut active_camera = None;
        let mut screen_space_materials = Vec::new();

        for attribute in attributes {
            match attribute.member {
                syn::Member::Named(ref ident) => match ident.to_string().as_str() {
                    "active_camera" => {
                        let expr = attribute.expr;
                        active_camera = Some(parse_quote!(#expr));
                    }
                    "screen_space_materials" => {
                        let array_contents =
                            expr_to_array(attribute.expr, |input: ParseStream| {
                                MaterialDescriptor::parse(input, true)
                            })?;

                        screen_space_materials = Vec::from_iter(array_contents);
                    }
                    _ => {
                        let error = format!("Invalid attribute {ident}");
                        return Err(Error::new_spanned(attribute, error));
                    }
                },
                syn::Member::Unnamed(_) => {
                    return Err(Error::new_spanned(
                        attribute,
                        "Unnamed fields are not allowed for scene attributes",
                    ));
                }
            }
        }

        Ok(SceneAttributes {
            active_camera,
            screenspace_materials: screen_space_materials,
        })
    }
}

#[derive(Debug)]
enum SceneItems {
    SceneAttribute(FieldValue),
    Entity(EntityDescriptor),
}

impl Parse for SceneItems {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        if input.peek2(Token![=]) {
            Ok(Self::Entity(input.parse()?))
        } else {
            Ok(Self::SceneAttribute(input.parse()?))
        }
    }
}

#[derive(Debug)]
struct MaterialDescriptor {
    pipeline: PipelineOptions,
    attachments: Vec<ShaderAttachmentOptions>,
    immediate_data: Option<Expr>,
    enabled: Option<Expr>,
    id: Option<Id>,
}

impl MaterialDescriptor {
    fn parse(input: ParseStream, screenspace: bool) -> syn::Result<Self> {
        let modular_struct = ModularStruct::parse(
            input,
            "Material",
            vec!["pipeline"],
            vec!["attachments", "immediate_data", "enabled", "ID"],
        )?;

        let pipeline = modular_struct.get_mandatory_field("pipeline")?;

        match &pipeline {
            PipelineOptions::Screenspace(pipeline) => {
                if !screenspace {
                    return Err(Error::new(
                        pipeline.span,
                        "A standard material cannot receive a screenspace pipeline",
                    ));
                }
            }
            PipelineOptions::Normal(screenspace_pipeline) => {
                if screenspace {
                    return Err(Error::new(
                        screenspace_pipeline.span,
                        "A screenspace material cannot receive a normal pipeline",
                    ));
                }
            }
            PipelineOptions::Id(_) => {}
        }

        Ok(Self {
            pipeline,
            attachments: if let Some(attachments) =
                modular_struct.get_optional_field_with("attachments", |input: ParseStream| {
                    let contents;
                    bracketed!(contents in input);
                    contents.parse_terminated(ShaderAttachmentOptions::parse, Token![,])
                })? {
                Vec::from_iter(attachments)
            } else {
                Vec::new()
            },
            immediate_data: modular_struct.get_optional_field("immediate_data")?,
            enabled: modular_struct.get_optional_field("enabled")?,
            id: modular_struct.get_optional_field("ID")?,
        })
    }
}

impl GetId for MaterialDescriptor {
    fn populate_id_map(&mut self, id_map: &mut HashMap<LitStr, Uuid>) {
        if let Some(id) = self.id.as_mut() {
            let Id::Raw(raw_id) = id else {
                return;
            };
            let new_uuid = Uuid::new_v4();
            id_map.insert(raw_id.clone(), new_uuid);

            *id = Id::Processed(new_uuid);
        }

        self.pipeline.populate_id_map(id_map);
    }
}

impl ToTokens for MaterialDescriptor {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let Self {
            pipeline,
            attachments,
            immediate_data,
            enabled,
            id,
        } = self;

        let immediate_data = quote_or(immediate_data.as_ref(), quote! {Vec::new()});

        let enabled = quote_or(enabled.as_ref(), quote! {true});

        let id = id
            .as_ref()
            .map(|id| {
                if let Id::Processed(uuid) = id {
                    let id_tokens = id_to_tokens(*uuid);
                    quote! {Some(#id_tokens)}
                } else {
                    quote! {None}
                }
            })
            .unwrap_or(quote! {None});

        tokens.extend(quote! {
            scene.create_material(
                #pipeline,
                vec![#(#attachments),*],
                #immediate_data,
                #enabled,
                #id,
            ).unwrap()
        });
    }
}

#[derive(Debug)]
enum PipelineOptions {
    Screenspace(ScreenspacePipelineDescriptor),
    Normal(NormalPipelineDescriptor),
    Id(Macro),
}

impl Parse for PipelineOptions {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let ident: Ident = input.fork().parse()?;

        match ident.to_string().as_str() {
            "ScreenSpacePipeline" => Ok(Self::Screenspace(input.parse()?)),
            "Pipeline" => Ok(Self::Normal(input.parse()?)),
            "ID" => Ok(Self::Id(input.parse()?)),
            _ => Err(Error::new_spanned(ident, "Invalid pipeline specified.")),
        }
    }
}

impl GetId for PipelineOptions {
    fn populate_id_map(&mut self, id_map: &mut HashMap<LitStr, Uuid>) {
        match self {
            PipelineOptions::Screenspace(screenspace) => screenspace.populate_id_map(id_map),
            PipelineOptions::Normal(normal) => normal.populate_id_map(id_map),
            _ => {}
        }
    }
}

impl ToTokens for PipelineOptions {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        tokens.extend(match self {
            PipelineOptions::Screenspace(screenspace) => {
                quote! {v4::ecs::material::PipelineOptions::Descriptor(#screenspace)}
            }
            PipelineOptions::Normal(normal) => {
                quote! {v4::ecs::material::PipelineOptions::Descriptor(#normal)}
            }
            PipelineOptions::Id(id) => quote! {v4::ecs::material::PipelineOptions::Id(#id)},
        });
    }
}

#[derive(Debug)]
struct ScreenspacePipelineDescriptor {
    shader_path: LitStr,
    spirv_shader: Option<LitBool>,
    immediate_size: Option<Expr>,
    span: Span,
    id: Option<Id>,
}

impl Parse for ScreenspacePipelineDescriptor {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let modular_struct = ModularStruct::parse(
            input,
            "ScreenSpacePipeline",
            vec!["shader_path"],
            vec!["spirv_shader", "immediate_size", "ID"],
        )?;

        Ok(Self {
            shader_path: modular_struct.get_mandatory_field("shader_path")?,
            spirv_shader: modular_struct.get_optional_field("spirv_shader")?,
            immediate_size: modular_struct.get_optional_field("immediate_size")?,
            span: modular_struct.span,
            id: modular_struct.get_optional_field("ID")?,
        })
    }
}

impl GetId for ScreenspacePipelineDescriptor {
    fn populate_id_map(&mut self, id_map: &mut HashMap<LitStr, Uuid>) {
        if let Some(id) = self.id.as_mut() {
            let Id::Raw(raw_id) = id else {
                return;
            };
            let new_uuid = Uuid::new_v4();
            id_map.insert(raw_id.clone(), new_uuid);

            *id = Id::Processed(new_uuid);
        }
    }
}

impl ToTokens for ScreenspacePipelineDescriptor {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let Self {
            shader_path,
            spirv_shader,
            immediate_size,
            ..
        } = self;

        let spirv_shader = quote_or(spirv_shader.as_ref(), quote! {false});

        let immediate_size = quote_or(immediate_size.as_ref(), quote! {0});

        tokens.extend(quote! {
            v4::engine_management::pipeline::PipelineParameters::new_screenspace(
                #shader_path.to_string(),
                #spirv_shader,
                #immediate_size,
            )
        });
    }
}

#[derive(Debug)]
struct NormalPipelineDescriptor {
    vertex_shader: LitStr,
    spirv_vertex_shader: Option<LitBool>,
    fragment_shader: LitStr,
    spirv_fragment_shader: Option<LitBool>,
    vertex_layouts: Expr,
    uses_camera: LitBool,
    geometry_details: Option<GeometryDetailsDescriptor>,
    immediate_size: Option<Expr>,
    render_priority: Option<Expr>,
    span: Span,
    id: Option<Id>,
}

impl Parse for NormalPipelineDescriptor {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let modular_struct = ModularStruct::parse(
            input,
            "Pipeline",
            vec![
                "vertex_shader",
                "fragment_shader",
                "vertex_layouts",
                "uses_camera",
            ],
            vec![
                "spirv_vertex_shader",
                "spirv_fragment_shader",
                "geometry_details",
                "immediate_size",
                "render_priority",
                "ID",
            ],
        )?;

        Ok(Self {
            vertex_shader: modular_struct.get_mandatory_field("vertex_shader")?,
            fragment_shader: modular_struct.get_mandatory_field("fragment_shader")?,
            vertex_layouts: modular_struct.get_mandatory_field("vertex_layouts")?,
            uses_camera: modular_struct.get_mandatory_field("uses_camera")?,
            spirv_vertex_shader: modular_struct.get_optional_field("spirv_vertex_shader")?,
            spirv_fragment_shader: modular_struct.get_optional_field("spirv_fragment_shader")?,
            geometry_details: modular_struct.get_optional_field("geometry_details")?,
            immediate_size: modular_struct.get_optional_field("immediate_size")?,
            render_priority: modular_struct.get_optional_field("render_priority")?,
            span: modular_struct.span,
            id: modular_struct.get_optional_field("ID")?,
        })
    }
}

impl GetId for NormalPipelineDescriptor {
    fn populate_id_map(&mut self, id_map: &mut HashMap<LitStr, Uuid>) {
        if let Some(id) = self.id.as_mut() {
            let Id::Raw(raw_id) = id else {
                return;
            };
            let new_uuid = Uuid::new_v4();
            id_map.insert(raw_id.clone(), new_uuid);

            *id = Id::Processed(new_uuid);
        }
    }
}

impl ToTokens for NormalPipelineDescriptor {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let Self {
            vertex_shader,
            spirv_vertex_shader,
            fragment_shader,
            spirv_fragment_shader,
            vertex_layouts,
            uses_camera,
            geometry_details,
            immediate_size,
            render_priority,
            ..
        } = self;

        let spirv_vertex_shader = quote_or(spirv_vertex_shader.as_ref(), quote! {false});
        let spirv_fragment_shader = quote_or(spirv_fragment_shader.as_ref(), quote! {false});

        let geometry_details = quote_or(
            geometry_details.as_ref(),
            quote! {
                v4::engine_management::pipeline::GeometryDetails::default()
            },
        );

        let immediate_size = quote_or(immediate_size.as_ref(), quote! {0});
        let render_priority = quote_or(render_priority.as_ref(), quote! {i32::MAX});

        tokens.extend(quote! {
            v4::engine_management::pipeline::PipelineParameters {
                vertex_shader: #vertex_shader.to_string(),
                spirv_vertex_shader: #spirv_vertex_shader,
                fragment_shader: #fragment_shader.to_string(),
                spirv_fragment_shader: #spirv_fragment_shader,
                vertex_layouts: #vertex_layouts,
                uses_camera: #uses_camera,
                geometry_details: #geometry_details,
                immediate_size: #immediate_size,
                render_priority: #render_priority,
                is_screenspace: false,
            }
        });
    }
}

#[derive(Debug)]
struct GeometryDetailsDescriptor {
    topology: Option<Expr>,
    strip_index_format: Option<Path>,
    front_face: Option<Path>,
    cull_mode: Option<Path>,
    polygon_mode: Option<Path>,
}

impl Parse for GeometryDetailsDescriptor {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let modular_struct = ModularStruct::parse(
            input,
            "GeometryDetails",
            Vec::new(),
            vec![
                "topology",
                "strip_index_format",
                "front_face",
                "cull_mode",
                "polygon_mode",
            ],
        )?;

        Ok(Self {
            topology: modular_struct.get_optional_field("topology")?,
            strip_index_format: modular_struct.get_optional_field("strip_index_format")?,
            front_face: modular_struct.get_optional_field("front_face")?,
            cull_mode: modular_struct.get_optional_field("cull_mode")?,
            polygon_mode: modular_struct.get_optional_field("polygon_mode")?,
        })
    }
}

impl ToTokens for GeometryDetailsDescriptor {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let Self {
            topology,
            strip_index_format,
            front_face,
            cull_mode,
            polygon_mode,
            ..
        } = self;

        let topology = topology
            .as_ref()
            .map(|val| quote! {topology: #val,})
            .unwrap_or_default();
        let strip_index_format = strip_index_format
            .as_ref()
            .map(|val| quote! {strip_index_format: #val,})
            .unwrap_or_default();
        let front_face = front_face
            .as_ref()
            .map(|val| quote! {front_face: #val,})
            .unwrap_or_default();
        let cull_mode = cull_mode
            .as_ref()
            .map(|val| quote! {cull_mode: #val,})
            .unwrap_or_default();
        let polygon_mode = polygon_mode
            .as_ref()
            .map(|val| quote! {polygon_mode: #val,})
            .unwrap_or_default();

        tokens.extend(quote! {
            v4::engine_management::pipeline::GeometryDetails {
                #topology
                #strip_index_format
                #front_face
                #cull_mode
                #polygon_mode
                ..std::default::Default::default()
            }
        });
    }
}

#[derive(Debug)]
enum ShaderAttachmentOptions {
    Texture(ShaderTextureDescriptor),
    Buffer(ShaderBufferOptions),
}

impl Parse for ShaderAttachmentOptions {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let ident: Ident = input.fork().parse()?;
        if &ident.to_string() == "Buffer" {
            Ok(Self::Buffer(input.parse()?))
        } else if &ident.to_string() == "Texture" {
            Ok(Self::Texture(input.parse()?))
        } else {
            Err(Error::new_spanned(
                ident,
                "Invalid shader attachment specified",
            ))
        }
    }
}

impl ToTokens for ShaderAttachmentOptions {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        tokens.extend(match self {
            ShaderAttachmentOptions::Texture(texture) => quote! {#texture},
            ShaderAttachmentOptions::Buffer(buffer) => quote! {#buffer},
        });
    }
}

#[derive(Debug)]
struct ShaderTextureDescriptor {
    texture_bundle: Expr,
    visibility: Expr,
}

impl Parse for ShaderTextureDescriptor {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let modular_struct = ModularStruct::parse(
            input,
            "Texture",
            vec!["texture_bundle", "visibility"],
            Vec::new(),
        )?;

        Ok(Self {
            texture_bundle: modular_struct.get_mandatory_field("texture_bundle")?,
            visibility: modular_struct.get_mandatory_field("visibility")?,
        })
    }
}

impl ToTokens for ShaderTextureDescriptor {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let Self {
            texture_bundle,
            visibility,
            ..
        } = self;

        tokens.extend(quote! {
            v4::ecs::material::ShaderAttachment::Texture(
                v4::ecs::material::ShaderTextureAttachment {
                    texture_bundle: #texture_bundle,
                    visibility: #visibility
                }
            )
        });
    }
}

#[derive(Debug)]
enum ShaderBufferOptions {
    Descriptor(ShaderBufferDescriptor),
    Constructor(ShaderBufferConstructor),
}

impl Parse for ShaderBufferOptions {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        if input.fork().parse::<ShaderBufferDescriptor>().is_ok() {
            Ok(Self::Descriptor(input.parse()?))
        } else if input.fork().parse::<ShaderBufferConstructor>().is_ok() {
            Ok(Self::Constructor(input.parse()?))
        } else {
            Err(Error::new_spanned(
                input.parse::<TokenTree>()?,
                "Invalid shader buffer variant found. Use either the buffer descriptor (buffer, visibility, buffer_type), or the constructor (device, data, buffer_type, visibility, extra_usages)",
            ))
        }
    }
}

impl ToTokens for ShaderBufferOptions {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        tokens.extend(match self {
            ShaderBufferOptions::Descriptor(descriptor) => quote! {#descriptor},
            ShaderBufferOptions::Constructor(constructor) => quote! {#constructor},
        });
    }
}

#[derive(Debug)]
struct ShaderBufferDescriptor {
    buffer: Expr,
    visibility: Expr,
    buffer_type: Expr,
}

impl Parse for ShaderBufferDescriptor {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let modular_struct = ModularStruct::parse(
            input,
            "Buffer",
            vec!["buffer", "visibility", "buffer_type"],
            Vec::new(),
        )?;

        let buffer = modular_struct.get_mandatory_field("buffer")?;
        let visibility = modular_struct.get_mandatory_field("visibility")?;
        let buffer_type = modular_struct.get_mandatory_field("buffer_type")?;

        Ok(Self {
            buffer,
            visibility,
            buffer_type,
        })
    }
}

impl ToTokens for ShaderBufferDescriptor {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let Self {
            buffer,
            visibility,
            buffer_type,
        } = self;

        tokens.extend(quote! {
            v4::ecs::material::ShaderAttachment::Buffer(
                v4::ecs::material::ShaderBufferAttachment {
                    buffer: #buffer,
                    visibility: #visibility,
                    buffer_type: #buffer_type,
                }
            )
        });
    }
}

#[derive(Debug)]
struct ShaderBufferConstructor {
    device: Expr,
    data: Expr,
    buffer_type: Expr,
    visibility: Expr,
    extra_usages: Expr,
}

impl Parse for ShaderBufferConstructor {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let modular_struct = ModularStruct::parse(
            input,
            "Buffer",
            vec![
                "device",
                "data",
                "buffer_type",
                "visibility",
                "extra_usages",
            ],
            Vec::new(),
        )?;

        Ok(Self {
            device: modular_struct.get_mandatory_field("device")?,
            data: modular_struct.get_mandatory_field("data")?,
            buffer_type: modular_struct.get_mandatory_field("buffer_type")?,
            visibility: modular_struct.get_mandatory_field("visibility")?,
            extra_usages: modular_struct.get_mandatory_field("extra_usages")?,
        })
    }
}

impl ToTokens for ShaderBufferConstructor {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let Self {
            device,
            data,
            buffer_type,
            visibility,
            extra_usages,
        } = self;

        tokens.extend(quote! {
            v4::ecs::material::ShaderAttachment::Buffer(
                v4::ecs::material::ShaderBufferAttachment::new(
                    #device,
                    #data,
                    #buffer_type,
                    #visibility,
                    #extra_usages,
                )
            )
        });
    }
}

#[derive(Debug)]
struct EntityDescriptor {
    parent: Option<Macro>,
    id: Option<Id>,
    components: Vec<ComponentOptions>,
    material: Option<MaterialDescriptor>,
    computes: Vec<ComputeDescriptor>,
    is_enabled: Option<LitBool>,
}

impl Parse for EntityDescriptor {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let id: Option<Id> = if input.peek(Token![_]) {
            let _underscore_token: Token![_] = input.parse()?;
            None
        } else {
            Some(input.parse()?)
        };

        let _equal_token: Token![=] = input.parse()?;

        let entity_contents = ModularStruct::parse_no_ident(
            input,
            Vec::new(),
            vec!["material", "components", "computes", "parent", "is_enabled"],
        )?;

        let components = if let Some(components_field) =
            entity_contents.get_optional_field::<Expr>("components")?
        {
            expr_to_array(components_field.clone(), ComponentOptions::parse)?
        } else {
            Vec::new()
        };

        let material = entity_contents
            .get_optional_field_with("material", |input: ParseStream| {
                MaterialDescriptor::parse(input, false)
            })?;

        let computes =
            if let Some(computes_field) = entity_contents.get_optional_field::<Expr>("computes")? {
                expr_to_array(computes_field.clone(), ComputeDescriptor::parse)?
            } else {
                Vec::new()
            };

        Ok(Self {
            id,
            parent: entity_contents.get_optional_field("parent")?,
            components,
            material,
            computes,
            is_enabled: entity_contents.get_optional_field("is_enabled")?,
        })
    }
}

impl GetId for EntityDescriptor {
    fn populate_id_map(&mut self, id_map: &mut HashMap<LitStr, Uuid>) {
        if let Some(id) = self.id.as_mut() {
            let Id::Raw(raw_id) = id else {
                return;
            };
            let new_uuid = Uuid::new_v4();
            id_map.insert(raw_id.clone(), new_uuid);

            *id = Id::Processed(new_uuid);
        }

        if let Some(material) = &mut self.material {
            material.populate_id_map(id_map);
        }

        for component in &mut self.components {
            component.populate_id_map(id_map);
        }

        for compute in &mut self.computes {
            compute.populate_id_map(id_map);
        }
    }
}

impl ToTokens for EntityDescriptor {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let Self {
            parent,
            id,
            components,
            material,
            computes,
            is_enabled,
        } = self;

        let parent = parent
            .as_ref()
            .map(|parent| quote! {Some(#parent)})
            .unwrap_or(quote! {None});

        let material = material
            .as_ref()
            .map(|material| {
                quote! {Some(#material)}
            })
            .unwrap_or(quote! {None});

        let is_enabled = quote_or(is_enabled.as_ref(), quote! {true});

        let id = if let Some(Id::Processed(id)) = id {
            let id_tokens = id_to_tokens(*id);
            quote! {Some(#id_tokens)}
        } else {
            quote! {None}
        };

        tokens.extend(quote! {
            let material_id = #material;
            scene.create_entity(
                #parent,
                vec![#(Box::new(#components)),*],
                vec![#(#computes),*],
                material_id,
                #is_enabled,
                #id,
            ).unwrap();
        });
    }
}

#[derive(Debug)]
enum ComponentOptions {
    Descriptor(ComponentDescriptor),
    Constructor(ComponentConstructor),
}

impl GetId for ComponentOptions {
    fn populate_id_map(&mut self, id_map: &mut HashMap<LitStr, Uuid>) {
        let id_option = match self {
            ComponentOptions::Descriptor(descriptor) => descriptor.id.as_mut(),
            ComponentOptions::Constructor(constructor) => constructor.id.as_mut(),
        };

        if let Some(id) = id_option {
            let Id::Raw(raw_id) = id else {
                return;
            };
            let new_uuid = Uuid::new_v4();
            id_map.insert(raw_id.clone(), new_uuid);
            *id = Id::Processed(new_uuid);
        }
    }
}

impl Parse for ComponentOptions {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        if input.fork().parse::<ComponentDescriptor>().is_ok() {
            Ok(Self::Descriptor(input.parse()?))
        } else {
            Ok(Self::Constructor(input.parse()?))
        }
    }
}

impl ToTokens for ComponentOptions {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        tokens.extend(match self {
            ComponentOptions::Descriptor(descriptor) => quote! {#descriptor},
            ComponentOptions::Constructor(constructor) => quote! {#constructor},
        });
    }
}

#[derive(Debug)]
struct ComponentDescriptor {
    ident: Ident,
    fields: Vec<FieldValue>,
    id: Option<Id>,
}

impl Parse for ComponentDescriptor {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let modular_struct = ModularStruct::parse_everything(input)?;

        Ok(Self {
            fields: modular_struct
                .fields
                .iter()
                .flat_map(|(name, field)| {
                    if name != "ID" {
                        Some(field.clone())
                    } else {
                        None
                    }
                })
                .collect(),
            id: modular_struct.get_optional_field("ID")?,
            ident: modular_struct.ident.unwrap(),
        })
    }
}

impl ToTokens for ComponentDescriptor {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let ident = &self.ident;
        let fields: Vec<_> = self
            .fields
            .iter()
            .flat_map(|field| {
                let name = &field.member;
                let expr = if field.colon_token.is_some() {
                    let val = &field.expr;
                    quote! {#val}
                } else {
                    quote! {#name}
                };
                quote! {.#name(#expr)}
            })
            .collect();

        let id = if let Some(Id::Processed(id)) = self.id {
            let id_tokens = id_to_tokens(id);
            quote! {.id(#id_tokens)}
        } else {
            TokenStream::new()
        };

        tokens.extend(quote! {
            #ident::builder()#(#fields)*#id.build(),
        });
    }
}

#[derive(Debug)]
struct ComponentConstructor {
    constructor: Expr,
    id: Option<Id>,
}

impl Parse for ComponentConstructor {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        if input.fork().parse::<ExprMethodCall>().is_ok() {
            let method_call: ExprMethodCall = input.parse()?;
            let (id, filtered_constructor) = if method_call.method == "ID" {
                if let Expr::Lit(syn::PatLit { lit, .. }) = &method_call.args[0]
                    && let syn::Lit::Str(lit_str) = lit
                {
                    Ok((Some(Id::Raw(lit_str.clone())), *method_call.receiver))
                } else {
                    Err(Error::new_spanned(
                        &method_call.args[0],
                        "Invalid ID created",
                    ))
                }?
            } else {
                (None, Expr::MethodCall(method_call))
            };

            Ok(Self {
                constructor: filtered_constructor,
                id,
            })
        } else {
            Ok(Self {
                constructor: input.parse()?,
                id: None,
            })
        }
    }
}

impl ToTokens for ComponentConstructor {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        if let Some(Id::Processed(id)) = &self.id {
            let constructor = &self.constructor;
            let id_tokens = id_to_tokens(*id);
            tokens.extend(quote! {
                {
                    let mut comp = #constructor;
                    comp.set_id(#id_tokens);
                    comp
                }
            });
        } else {
            self.constructor.to_tokens(tokens);
        }
    }
}

#[derive(Debug)]
struct ComputeDescriptor {
    shader_path: LitStr,
    workgroup_counts: Expr,
    attachments: Vec<ShaderAttachmentOptions>,
    is_spirv: Option<LitBool>,
    iterate_count: Option<Expr>,
    continuous_execution: Option<LitBool>,
    id: Option<Id>,
}

impl Parse for ComputeDescriptor {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let modular_struct = ModularStruct::parse(
            input,
            "Compute",
            vec!["shader_path", "workgroup_counts"],
            vec![
                "attachments",
                "is_spirv",
                "iterate_count",
                "continuous_execution",
                "ID",
            ],
        )?;

        Ok(Self {
            shader_path: modular_struct.get_mandatory_field("shader_path")?,
            workgroup_counts: modular_struct.get_mandatory_field("workgroup_counts")?,
            attachments: if let Some(attachments_expr) =
                modular_struct.get_optional_field("attachments")?
            {
                expr_to_array(attachments_expr, ShaderAttachmentOptions::parse)?
            } else {
                Vec::new()
            },
            is_spirv: modular_struct.get_optional_field("is_spirv")?,
            iterate_count: modular_struct.get_optional_field("iterate_count")?,
            continuous_execution: modular_struct.get_optional_field("continuous_execution")?,
            id: modular_struct.get_optional_field("ID")?,
        })
    }
}

impl GetId for ComputeDescriptor {
    fn populate_id_map(&mut self, id_map: &mut HashMap<LitStr, Uuid>) {
        if let Some(id) = self.id.as_mut() {
            let Id::Raw(raw_id) = id else {
                return;
            };
            let new_uuid = Uuid::new_v4();
            id_map.insert(raw_id.clone(), new_uuid);
            *id = Id::Processed(new_uuid);
        }
    }
}

impl ToTokens for ComputeDescriptor {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let Self {
            shader_path,
            workgroup_counts,
            attachments,
            is_spirv,
            iterate_count,
            continuous_execution,
            ..
        } = self;

        let is_spirv = is_spirv
            .as_ref()
            .map(|is_spirv| {
                quote! {.is_spirv(#is_spirv)}
            })
            .unwrap_or_default();

        let iterate_count = iterate_count
            .as_ref()
            .map(|count| {
                quote! {.iterate_count(#count)}
            })
            .unwrap_or_default();

        let continuous_execution = continuous_execution
            .as_ref()
            .map(|execution| quote! {.continuous_execution(#execution)})
            .unwrap_or_default();

        tokens.extend(quote! {
            v4::ecs::compute::Compute::builder()
                .shader_path(#shader_path)
                .workgroup_counts(#workgroup_counts)
                .attachments(vec![#(#attachments),*])
                #is_spirv
                #iterate_count
                #continuous_execution
                .build().unwrap()
        });
    }
}
