use std::collections::{HashMap, HashSet};

use proc_macro2::{Span, TokenTree};
use quote::{ToTokens, quote};
use syn::{
    Error, Expr, ExprStruct, FieldValue, Ident, ItemMacro, LitBool, LitStr, Path, Token,
    parse::{Parse, ParseStream, Parser},
    parse_quote, parse2,
    punctuated::Punctuated,
    spanned::Spanned,
};

struct ModularStruct {
    ident: Ident,
    fields: HashMap<String, FieldValue>,
    span: Span,
}

impl ModularStruct {
    fn parse(
        input: ParseStream,
        mandatory_fields: Vec<&'static str>,
        optional_fields: Vec<&'static str>,
    ) -> syn::Result<Self> {
        let base_struct: ExprStruct = input.parse()?;
        let span = base_struct.span();

        let Some(ident) = base_struct.path.get_ident().cloned() else {
            return Err(Error::new_spanned(
                base_struct.path,
                "This modular struct needs to have an identifier.",
            ));
        };

        let fields: Vec<&Ident> = base_struct
            .fields
            .iter()
            .flat_map(|field| match &field.member {
                syn::Member::Named(ident) => Some(ident),
                syn::Member::Unnamed(_) => None,
            })
            .collect();

        let mandatory_fields_set = HashSet::<&str>::from_iter(mandatory_fields);
        let optional_fields_set = HashSet::from_iter(optional_fields);

        let complete_names_set: HashSet<&str> = mandatory_fields_set
            .clone()
            .union(&optional_fields_set)
            .copied()
            .collect();

        if let Some(extraneous_field) = fields
            .iter()
            .find(|ident| !complete_names_set.contains(ident.to_string().as_str()))
        {
            return Err(Error::new_spanned(
                extraneous_field,
                format!("No such field '{extraneous_field}'"),
            ));
        }

        let field_names: HashSet<String> = fields.iter().map(|ident| ident.to_string()).collect();
        let field_names: HashSet<&str> = field_names.iter().map(|name| name.as_str()).collect();

        let missing_fields: Vec<&&str> = mandatory_fields_set.difference(&field_names).collect();

        if !missing_fields.is_empty() {
            return Err(Error::new_spanned(
                base_struct,
                "Missing modular struct fields: {missing_fields:?}",
            ));
        }

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

    fn get_optional_field<T: Parse>(&self, field: &str) -> syn::Result<Option<T>> {
        let expr = self.fields.get(field).map(|field| &field.expr);

        if let Some(expr) = expr {
            Ok(Some(parse2(quote! {#expr})?))
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

pub struct SceneDescriptor {
    attributes: SceneAttributes,
    entities: Vec<EntityDescriptor>,
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
                        let parser = |input: ParseStream| {
                            Punctuated::<MaterialDescriptor, Token![,]>::parse_terminated_with(
                                input,
                                |input: ParseStream| MaterialDescriptor::parse(input, true),
                            )
                        };

                        let punctuated = parser.parse2(quote! {#(attribute.expr)})?;
                        screen_space_materials = Vec::from_iter(punctuated);
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

impl ToTokens for SceneDescriptor {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {}
}

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

struct MaterialDescriptor {
    pipeline: PipelineOptions,
    attachments: Vec<ShaderAttachmentOptions>,
    immediate_data: Option<Expr>,
    enabled: Option<Expr>,
}

impl MaterialDescriptor {
    fn parse(input: ParseStream, screenspace: bool) -> syn::Result<Self> {
        let modular_struct = ModularStruct::parse(
            input,
            vec!["pipeline"],
            vec!["attachments", "immediate_data", "enabled"],
        )?;

        Ok(Self {
            pipeline: modular_struct.get_mandatory_field("pipeline")?,
            attachments: if let Some(attachments) =
                modular_struct.get_optional_field("attachments")?
            {
                attachments
            } else {
                Vec::new()
            },
            immediate_data: modular_struct.get_optional_field("immediate_data")?,
            enabled: modular_struct.get_optional_field("enabled")?,
        })
    }
}

enum PipelineOptions {
    Screenspace(ScreenspacePipelineDescriptor),
    Normal(NormalPipelineDescriptor),
    Id(ItemMacro),
}

impl Parse for PipelineOptions {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        if let Ok(screenspace) = input.fork().parse::<ScreenspacePipelineDescriptor>() {
            Ok(PipelineOptions::Screenspace(screenspace))
        } else if let Ok(normal) = input.fork().parse::<NormalPipelineDescriptor>() {
            Ok(PipelineOptions::Normal(normal))
        } else if let Ok(id) = input.fork().parse::<ItemMacro>() {
            Ok(PipelineOptions::Id(id))
        } else {
            Err(Error::new_spanned(input.parse::<TokenTree>()?, "Invalid pipeline specified."))
        }
    }
}

struct ScreenspacePipelineDescriptor {
    shader_path: LitStr,
    spirv_shader: Option<LitBool>,
    immediate_size: Option<Expr>,
}

impl Parse for ScreenspacePipelineDescriptor {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let modular_struct = ModularStruct::parse(
            input,
            vec!["shader_path"],
            vec!["spirv_shader", "immediate_size"],
        )?;

        Ok(Self {
            shader_path: modular_struct.get_mandatory_field("shader_path")?,
            spirv_shader: modular_struct.get_optional_field("spirv_shader")?,
            immediate_size: modular_struct.get_optional_field("immediate_size")?,
        })
    }
}

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
}

impl Parse for NormalPipelineDescriptor {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let modular_struct = ModularStruct::parse(
            input,
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
        })
    }
}

struct GeometryDetailsDescriptor {
    topology: Option<Path>,
    strip_index_format: Option<Path>,
    front_face: Option<Path>,
    cull_mode: Option<Path>,
    polygon_mode: Option<Path>,
}

impl Parse for GeometryDetailsDescriptor {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let modular_struct = ModularStruct::parse(
            input,
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

enum ShaderAttachmentOptions {
    Texture(ShaderTextureAttachment),
    Buffer
}

struct ShaderTextureAttachment {

}

struct EntityDescriptor {
    id: LitStr,
    components: Vec<ComponentDescriptor>,
}

impl Parse for EntityDescriptor {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        todo!()
    }
}

struct ComponentDescriptor {}
