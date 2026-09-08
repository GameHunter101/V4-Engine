#![allow(clippy::large_enum_variant)]
use std::collections::{HashMap, HashSet};

use proc_macro2::{Span, TokenTree};
use quote::{ToTokens, quote};
use syn::{
    Error, Expr, ExprCall, ExprMethodCall, ExprStruct, FieldValue, Ident, ItemMacro, LitBool,
    LitStr, Path, Token, braced, bracketed,
    parse::{Parse, ParseStream, Parser},
    parse_quote,
    spanned::Spanned,
};

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
                        Some(Err(Error::new_spanned(
                            ident,
                            format!("Unexpected field '{ident}'"),
                        )))
                    } else {
                        Some(Ok((ident.to_string(), field)))
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

pub struct SceneDescriptor {
    attributes: SceneAttributes,
    entities: Vec<EntityDescriptor>,
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

        let attributes: Vec<FieldValue> = attributes.into_iter().flatten().collect();
        let entities: Vec<EntityDescriptor> = entities.into_iter().flatten().collect();

        Ok(SceneDescriptor {
            attributes: SceneAttributes::validate_attributes(attributes)?,
            entities,
        })
    }
}

pub struct SceneAttributes {
    active_camera: Option<ItemMacro>,
    screen_space_materials: Vec<MaterialDescriptor>,
}

impl SceneAttributes {
    fn validate_attributes(attributes: Vec<FieldValue>) -> syn::Result<SceneAttributes> {
        let mut active_camera = None;
        let mut screen_space_materials = Vec::new();

        for attribute in attributes {
            match attribute.member {
                syn::Member::Named(ref ident) => match ident.to_string().as_str() {
                    "active_camera" => {
                        active_camera = Some(parse_quote!(quote! {#(attribute.expr)}))
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
            screen_space_materials,
        })
    }
}

impl ToTokens for SceneDescriptor {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {}
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
    id: Option<LitStr>,
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

#[derive(Debug)]
enum PipelineOptions {
    Screenspace(ScreenspacePipelineDescriptor),
    Normal(NormalPipelineDescriptor),
    Id(ItemMacro),
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

#[derive(Debug)]
struct ScreenspacePipelineDescriptor {
    shader_path: LitStr,
    spirv_shader: Option<LitBool>,
    immediate_size: Option<Expr>,
    span: Span,
    id: Option<LitStr>,
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
    id: Option<LitStr>,
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

#[derive(Debug)]
struct GeometryDetailsDescriptor {
    topology: Option<Path>,
    strip_index_format: Option<Path>,
    front_face: Option<Path>,
    cull_mode: Option<Path>,
    polygon_mode: Option<Path>,
    span: Span,
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
            span: modular_struct.span,
        })
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

#[derive(Debug)]
struct ShaderTextureDescriptor {
    texture_bundle: Expr,
    visibility: Expr,
    span: Span,
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
            span: modular_struct.span,
        })
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

#[derive(Debug)]
struct EntityDescriptor {
    id: Option<LitStr>,
    components: Vec<ComponentOptions>,
    material: Option<MaterialDescriptor>,
    computes: Vec<ComputeDescriptor>,
}

impl Parse for EntityDescriptor {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let id: Option<LitStr> = if input.peek(Token![_]) {
            let _underscore_token: Token![_] = input.parse()?;
            None
        } else {
            Some(input.parse()?)
        };

        let _equal_token: Token![=] = input.parse()?;

        let entity_contents = ModularStruct::parse_no_ident(
            input,
            Vec::new(),
            vec!["material", "components", "computes"],
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
            components,
            material,
            computes,
        })
    }
}

#[derive(Debug)]
enum ComponentOptions {
    Descriptor(ComponentDescriptor),
    Constructor(ComponentConstructor),
}

impl Parse for ComponentOptions {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        if input.fork().parse::<ComponentConstructor>().is_ok() {
            Ok(Self::Constructor(input.parse()?))
        } else {
            Ok(Self::Descriptor(input.parse()?))
        }
    }
}

#[derive(Debug)]
struct ComponentDescriptor {
    ident: Ident,
    fields: Vec<FieldValue>,
    id: Option<LitStr>,
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

#[derive(Debug)]
struct ComponentConstructor {
    constructor: Expr,
    id: Option<LitStr>,
}

impl Parse for ComponentConstructor {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        if input.fork().parse::<ExprCall>().is_ok() {
            Ok(Self {
                constructor: input.parse()?,
                id: None,
            })
        } else if input.fork().parse::<ExprMethodCall>().is_ok() {
            let method_call: ExprMethodCall = input.parse()?;
            let (id, filtered_constructor) = if method_call.method == "ID" {
                let expr_parser = |input: ParseStream| input.parse::<LitStr>();

                (
                    Some(expr_parser.parse2(quote! {method_call.args[0]})?),
                    *method_call.receiver,
                )
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

#[derive(Debug)]
struct ComputeDescriptor {
    shader_path: LitStr,
    workgroup_counts: Expr,
    attachments: Vec<ShaderAttachmentOptions>,
    is_spirv: Option<LitBool>,
    iterate_count: Option<Expr>,
    continuous_execution: Option<LitBool>,
    id: Option<LitStr>,
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
