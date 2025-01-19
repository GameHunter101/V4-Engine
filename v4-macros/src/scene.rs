#![allow(unused)]
use std::collections::HashMap;

use darling::FromMeta;
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::{format_ident, quote, ToTokens};
use syn::{
    braced, bracketed, parenthesized,
    parse::{discouraged::AnyDelimiter, Parse, ParseStream},
    parse2, parse_macro_input, parse_quote,
    punctuated::Punctuated,
    spanned::Spanned,
    Expr, ExprAwait, ExprCall, ExprField, ExprLit, ExprPath, FieldValue, Generics, Ident, Lit,
    LitStr, Member, PatLit, PatPath, Token,
};
use v4_core::ecs::{component::ComponentId, entity::EntityId, material::MaterialId, scene::Scene};

pub struct SceneDescriptor {
    entities: Vec<TransformedEntityDescriptor>,
    idents: HashMap<Lit, Id>,
    relationships: HashMap<EntityId, Vec<EntityId>>,
    materials: Vec<TransformedMaterialDescriptor>,
    pipelines: Vec<PipelineIdDescriptor>
}

impl Parse for SceneDescriptor {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let entities: Vec<EntityDescriptor> = input
            .parse_terminated(EntityDescriptor::parse, Token![,])?
            .into_iter()
            .collect();
        let mut idents: HashMap<Lit, Id> = HashMap::new();
        let mut relationships: HashMap<EntityId, Vec<EntityId>> = HashMap::new();
        let mut materials = Vec::new();
        let mut pipelines = Vec::new();

        let mut current_ident = 1;
        let transformed_entities = entities.into_iter().map(|entity| {

            let material_id = if let Some(mut material) = entity.material {
                let pipeline_id = match &material.pipeline_id {
                    PipelineIdVariants::Ident(pipeline_ident) => match idents.get(pipeline_ident) {
                        Some(id) => {
                            if let Id::Pipeline(id) = *id {
                                Ok(id)
                            } else {
                                return Err(syn::Error::new(pipeline_ident.span(), 
                                    format!("Two objects share the same identifier: \"{pipeline_ident:?}\"")));
                            }
                        },
                        None => Err(syn::Error::new(
                            pipeline_ident.span(),
                            format!("The pipeline \"{pipeline_ident:?}\" could not be found. If you declared it, make sure it is declared above the current entity")
                        )),
                    },
                    PipelineIdVariants::Specifier(pipeline_id_descriptor) => {
                        let pipeline_id = pipelines.len();
                        if let Some(pipeline_ident) = &pipeline_id_descriptor.ident {
                            idents.insert(pipeline_ident.clone(), Id::Pipeline(pipeline_id));
                        }
                        pipelines.push(pipeline_id_descriptor.clone());

                        Ok(pipeline_id)
                    },
                }?;

                if let Some(ident) = &material.ident {
                    idents.insert(ident.clone(), Id::Material(materials.len()));
                }

                materials.push(TransformedMaterialDescriptor {
                    pipeline_id: usize::MAX,
                    attachments: material.attachments,
                });
                Some(materials.len() - 1)
            } else {
                None
            };

            let mut transformed_entity = TransformedEntityDescriptor {
                components: entity.components,
                material_id,
                parent: None,
                id: current_ident,
                is_enabled: entity.is_enabled,
                ident: entity.ident,
            };

            if let Some(ident) = &transformed_entity.ident {
                idents.insert(ident.clone(), Id::Entity(current_ident));
                current_ident += 1;
            }

            if let Some(parent_ident) = &entity.parent {
                if let Some(parent_id) = idents.get(parent_ident) {
                    let Id::Entity(parent_id) = parent_id else {
                        return Err(syn::Error::new_spanned(
                            parent_ident,
                            format!("Two objects share the same identifier: \"{parent_ident:?}\""),
                        ));
                    };
                    if let Some(children) = relationships.get_mut(parent_id) {
                        children.push(transformed_entity.id);
                    } else {
                        relationships.insert(*parent_id, vec![transformed_entity.id]);
                    }
                } else {
                    return Err(syn::Error::new_spanned(parent_ident, format!("The parent entity \"{parent_ident:?}\" could not be found. If you declared it, make sure it is declared above the current entity")));
                }
            }

            for component in &transformed_entity.components {
                if let Some(ident) = &component.ident {
                    idents.insert(ident.clone(), Id::Component(current_ident));
                    current_ident += 1;
                }
            }

            Ok(transformed_entity)
        }).collect::<syn::Result<Vec<TransformedEntityDescriptor>>>()?;

        Ok(Self {
            entities: transformed_entities,
            idents,
            relationships,
            materials,
            pipelines,
        })
    }
}

impl quote::ToTokens for SceneDescriptor {
    fn to_tokens(&self, tokens: &mut TokenStream2) {

        let pipeline_id_initializations: Vec<TokenStream2> = self.pipelines.iter().map(
            |PipelineIdDescriptor { vertex_shader_path, fragment_shader_path, vertex_layouts, geometry_details, .. }| {
                let geometry_details = match geometry_details {
                    Some(GeometryDetailsDescriptor { topology, strip_index_format, front_face, cull_mode, polygon_mode }) => {
                        let topology = if let Some(topology) = topology {
                            quote! {topology: #topology}
                        } else {
                            quote! {topology: Default::default()}
                        };

                        let strip_index_format = if let Some(strip_index_format) = strip_index_format {
                            quote! {strip_index_format: #strip_index_format}
                        } else {
                            quote! {strip_index_format: Default::default()}
                        };

                        let front_face = if let Some(front_face) = front_face {
                            quote! {front_face: #front_face}
                        } else {
                            quote! {front_face: Default::default()}
                        };

                        let cull_mode = if let Some(cull_mode) = cull_mode {
                            quote! {cull_mode: #cull_mode}
                        } else {
                            quote! {cull_mode: Default::default()}
                        };

                        let polygon_mode = if let Some(polygon_mode) = polygon_mode {
                            quote! {polygon_mode: #polygon_mode}
                        } else {
                            quote! {polygon_mode: Default::default()}
                        };

                        quote! {
                        v4::ecs::pipeline::GeometryDetails {
                            #topology,
                            #strip_index_format,
                            #front_face,
                            #cull_mode,
                            #polygon_mode,
                        }
                    }},
                    None => quote! {v4::ecs::pipeline::GeometryDetails::default()},
                };
                quote! {
                    v4::ecs::pipeline::PipelineId {
                        vertex_shader_path: #vertex_shader_path,
                        fragment_shader_path: #fragment_shader_path,
                        vertex_layouts: vec![#(#vertex_layouts),*],
                        #geometry_details,
                    }
                }
        }).collect();

        let material_initializations = self.materials.iter().map(|mat| {
            let pipeline_id_index = mat.pipeline_id;
            let pipeline_id = &pipeline_id_initializations[pipeline_id_index];
            let attachments = &mat.attachments;
            quote! {
                scene.create_material(
                    device,
                    #pipeline_id,
                    attachments: #(#attachments),*,
                );
            }
        });

        let entity_initializations = self.entities.iter().map(|entity| {
            let parent_id = match entity.parent {
                Some(id) => quote! {Some(#id)},
                None => quote! {None},
            };

            let component_initializations = entity.components.iter().map(|component| {
                let component_type = &component.component_type;
                let component_generics =
                if component.generics.params.is_empty() {
                    quote! {}
                } else {
                    let generics = &component.generics;
                    quote!{::#generics}
                };

                if let Some(constructor) = &component.custom_constructor {

                    quote! {
                        Box::new(#component_type #component_generics::#constructor)
                    }
                } else {
                    let params = component.params.iter().map(|param| {
                        let field = &param.ident;
                        if let Some(value) = &param.value {
                            if let Some(ident) = value.get_ident() {
                                let id = self.idents.get(&ident).unwrap();
                                quote! {.#field(#id)}
                            } else {
                                quote! {.#field(#value)}
                            }
                        } else {
                            quote! {.#field(#field)}
                        }
                    });
                    let id_set = if let Some(ident) = &component.ident {
                        let id = self.idents.get(ident).unwrap();
                        quote! {.id(#id)}
                    } else {
                        quote! {}
                    };

                    quote! {
                        Box::new(#component_type #component_generics::builder()#(#params)*#id_set.build())
                    }
                }

            });

            let material = if let Some(material_id) = &entity.material_id {
                quote! {Some(#material_id)}
            } else {
                quote! {None}
            };

            let is_enabled = entity.is_enabled;

            let entity_ident = if let Some(ident) = &entity.ident {
                let entity_name = match ident {
                    Lit::Str(lit_str) => format!("entity_{}", lit_str.value()),
                    Lit::ByteStr(lit_byte_str) => format!(
                        "entity_{}",
                        String::from_utf8(lit_byte_str.value()).unwrap()
                    ),
                    Lit::CStr(lit_cstr) => format!("entity_{}", lit_cstr.value().to_str().unwrap()),
                    Lit::Byte(lit_byte) => format!("entity_{}", lit_byte.value()),
                    Lit::Char(lit_char) => format!("entity_{}", lit_char.value()),
                    Lit::Int(lit_int) => {
                        format!("entity_{}", lit_int.base10_parse::<u32>().unwrap())
                    }
                    Lit::Float(lit_float) => {
                        format!("entity_{}", lit_float.base10_parse::<f32>().unwrap())
                    }
                    Lit::Bool(lit_bool) => format!("entity_{}", lit_bool.value()),
                    Lit::Verbatim(literal) => format!("entity_{}", literal),
                    _ => "unnamed_entity".to_string(),
                };
                let ident = format_ident!("{}", entity_name);
                quote! {#ident}
            } else {
                quote! {_}
            };

            quote! {
                let #entity_ident = scene.create_entity(
                    #parent_id,
                    vec![#(#component_initializations,)*],
                    #material,
                    #is_enabled,
                );
            }
        });

        tokens.extend(quote! {
            let mut scene = v4::ecs::scene::Scene::default();

            #(#material_initializations)*

            #(#entity_initializations)*
        });
    }
}

struct TransformedEntityDescriptor {
    components: Vec<ComponentDescriptor>,
    material_id: Option<MaterialId>,
    parent: Option<EntityId>,
    id: EntityId,
    is_enabled: bool,
    ident: Option<Lit>,
}
pub enum Id {
    Entity(EntityId),
    Component(ComponentId),
    Material(MaterialId),
    Pipeline(usize),
}

impl quote::ToTokens for Id {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        tokens.extend(match self {
            Id::Entity(id) => quote! {#id},
            Id::Component(id) => quote! {#id},
            Id::Material(id) => quote! {#id},
            Id::Pipeline(id) => quote! {#id},
        });
    }
}

struct EntityDescriptor {
    ident: Option<Lit>,
    components: Vec<ComponentDescriptor>,
    material: Option<MaterialDescriptor>,
    parent: Option<Lit>,
    is_enabled: bool,
}

impl Parse for EntityDescriptor {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let ident = {
            if input.peek(Token![_]) {
                let _: Token![_] = input.parse()?;
                let _: Token![=] = input.parse()?;
                None
            } else if input.peek(syn::token::Brace) {
                None
            } else {
                let raw_ident: Lit = input.parse()?;
                let _: Token![=] = input.parse()?;
                match raw_ident {
                    Lit::Str(lit_str) => {
                        if lit_str.value() == *"_" {
                            None
                        } else {
                            Some(Lit::Str(lit_str))
                        }
                    }
                    lit => Some(lit),
                }
            }
        };

        let mut entity_descriptor = EntityDescriptor {
            ident,
            components: Vec::new(),
            material: None,
            parent: None,
            is_enabled: true,
        };

        let content;
        braced!(content in input);
        let parameters = content.parse_terminated(EntityParameters::parse, Token![,])?;
        for param in parameters {
            match param {
                EntityParameters::Components(vec) => entity_descriptor.components = vec,
                EntityParameters::Material(material_descriptor) => {
                    entity_descriptor.material = Some(material_descriptor)
                }
                EntityParameters::Parent(parent) => entity_descriptor.parent = Some(parent),
                EntityParameters::Enabled(is_enabled) => entity_descriptor.is_enabled = is_enabled,
            }
        }

        Ok(entity_descriptor)
    }
}

enum EntityParameters {
    Components(Vec<ComponentDescriptor>),
    Material(MaterialDescriptor),
    Parent(Lit),
    Enabled(bool),
}

impl Parse for EntityParameters {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let param_type: Ident = input.parse()?;
        input.parse::<Token![:]>()?;
        match param_type.to_string().as_str() {
            "components" => {
                let content;
                bracketed!(content in input);
                let components = content.parse_terminated(ComponentDescriptor::parse, Token![,])?;
                Ok(EntityParameters::Components(
                    components.into_iter().collect(),
                ))
            }
            "material" => Ok(EntityParameters::Material(input.parse()?)),
            "parent" => Ok(EntityParameters::Parent(input.parse()?)),
            "enabled" => {
                let lit: Lit = input.parse()?;
                if let Lit::Bool(lit_bool) = lit {
                    Ok(EntityParameters::Enabled(lit_bool.value))
                } else {
                    Err(syn::Error::new_spanned(lit, "Expected a boolean literal"))
                }
            }
            _ => Err(syn::Error::new_spanned(
                param_type,
                "Invalid argument passed into the entity descriptor",
            )),
        }
    }
}

struct ComponentDescriptor {
    component_type: Ident,
    generics: Generics,
    params: Vec<SimpleField>,
    custom_constructor: Option<ComponentConstructor>,
    ident: Option<Lit>,
}

impl Parse for ComponentDescriptor {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let component_type: Ident = input.parse()?;
        let generics: Generics = input.parse()?;

        if input.peek(Token![::]) {
            let _: Token![::] = input.parse()?;
            let mut custom_constructor: ComponentConstructor = input.parse()?;
            let ident = custom_constructor.component_ident.take();

            Ok(ComponentDescriptor {
                component_type,
                generics,
                params: Vec::new(),
                custom_constructor: Some(custom_constructor),
                ident,
            })
        } else {
            let content;
            parenthesized!(content in input);

            let mut params: Vec<SimpleField> = content
                .parse_terminated(SimpleField::parse, Token![,])?
                .into_iter()
                .collect();

            let ident = params
                .iter()
                .filter(|param| &param.ident.to_string() == "ident" && param.value.is_some())
                .flat_map(|param| {
                    if let SimpleFieldValue::Literal(ident) = param.value.as_ref().unwrap() {
                        Some(ident.clone())
                    } else {
                        None
                    }
                })
                .next();

            if let Some(ident) = &ident {
                params.remove(
                    params
                        .iter()
                        .position(|param| {
                            param.value == Some(SimpleFieldValue::Literal(ident.clone()))
                        })
                        .unwrap(),
                );
            }

            Ok(ComponentDescriptor {
                component_type,
                generics,
                params,
                custom_constructor: None,
                ident,
            })
        }
    }
}

struct ComponentConstructor {
    constructor_ident: Ident,
    parameters: Punctuated<Expr, Token![,]>,
    postfix: Option<TokenStream2>,
    component_ident: Option<Lit>,
}

impl Parse for ComponentConstructor {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let constructor_ident: Ident = input.parse()?;
        let content;
        parenthesized!(content in input);
        let mut parameters = content.parse_terminated(Expr::parse, Token![,])?;

        let ident_search = parameters
            .iter()
            .enumerate()
            .flat_map(|(i, param)| {
                if let Expr::Call(ExprCall { func, args, .. }) = param {
                    if let Expr::Path(ExprPath { path, .. }) = *func.clone() {
                        if let Some(possible_ident) = path.get_ident() {
                            if &possible_ident.to_string() == "ident" {
                                if let Some(Expr::Lit(lit)) = args.first() {
                                    return Some((i, lit.lit.clone()));
                                }
                            }
                        }
                    }
                }
                None
            })
            .next();

        let ident = if let Some((index, ident)) = ident_search {
            parameters =
                Punctuated::from_iter(parameters.into_iter().enumerate().flat_map(|(i, param)| {
                    if i != index {
                        Some(param)
                    } else {
                        None
                    }
                }));

            Some(ident)
        } else {
            None
        };

        let postfix: Option<TokenStream2> = if input.is_empty() {
            None
        } else {
            Some(input.parse()?)
        };

        Ok(ComponentConstructor {
            constructor_ident,
            parameters,
            postfix,
            component_ident: ident,
        })
    }
}

impl quote::ToTokens for ComponentConstructor {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        let constructor_ident = &self.constructor_ident;
        let parameters = &self.parameters;
        let postfix = &self.postfix;
        tokens.extend(quote! {
            #constructor_ident(#parameters)#postfix
        });
    }
}

struct SimpleField {
    ident: Ident,
    value: Option<SimpleFieldValue>,
}

impl Parse for SimpleField {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let ident = input.parse::<Ident>()?;
        let colon = input.parse::<Token![:]>();
        let mut value = None;
        if colon.is_ok() {
            if let Ok(expr) = input.parse::<Expr>() {
                if let Expr::Lit(lit) = expr {
                    value = Some(SimpleFieldValue::Literal(lit.lit));
                } else {
                    value = Some(SimpleFieldValue::Expression(expr));
                }
            }
            if let Ok(lit) = input.parse::<Lit>() {
                value = Some(SimpleFieldValue::Literal(lit));
            }
        }

        Ok(SimpleField { ident, value })
    }
}

#[derive(PartialEq)]
enum SimpleFieldValue {
    Expression(Expr),
    Literal(Lit),
}

impl SimpleFieldValue {
    fn get_ident(&self) -> Option<Lit> {
        if let SimpleFieldValue::Expression(Expr::Call(ExprCall { func, args, .. })) = &self {
            if let Expr::Path(ExprPath { path, .. }) = *func.clone() {
                if let Some(possible_ident) = path.get_ident() {
                    if &possible_ident.to_string() == "ident" {
                        if let Some(Expr::Lit(lit)) = args.first() {
                            return Some(lit.lit.clone());
                        }
                    }
                }
            }
        }

        None
    }
}

impl quote::ToTokens for SimpleFieldValue {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        tokens.extend(match self {
            SimpleFieldValue::Expression(expr) => quote! {#expr},
            SimpleFieldValue::Literal(lit) => quote! {#lit},
        });
    }
}

struct MaterialDescriptor {
    pipeline_id: PipelineIdVariants,
    attachments: Vec<MaterialAttachmentDescriptor>,
    ident: Option<Lit>,
}

impl Parse for MaterialDescriptor {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let content;
        braced!(content in input);
        let params = content.parse_terminated(MaterialParameters::parse, Token![,])?;

        let mut pipeline_id: Option<PipelineIdVariants> = None;
        let mut attachments: Vec<MaterialAttachmentDescriptor> = Vec::new();
        let mut ident: Option<Lit> = None;

        for param in params {
            match param {
                MaterialParameters::Pipeline(specified_pipeline_id) => {
                    pipeline_id = Some(specified_pipeline_id)
                }
                MaterialParameters::Attachments(specified_attachments) => {
                    attachments = specified_attachments
                }
                MaterialParameters::Ident(lit) => ident = Some(lit),
            }
        }

        let Some(pipeline_id) = pipeline_id else {
            return Err(input.error("A pipeline ID must be specified"));
        };

        Ok(MaterialDescriptor {
            pipeline_id,
            attachments,
            ident,
        })
    }
}

struct TransformedMaterialDescriptor {
    pipeline_id: usize,
    attachments: Vec<MaterialAttachmentDescriptor>,
}

enum MaterialParameters {
    Pipeline(PipelineIdVariants),
    Attachments(Vec<MaterialAttachmentDescriptor>),
    Ident(Lit),
}

impl Parse for MaterialParameters {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let field_identifier: Ident = input.parse()?;
        let _: Token![:] = input.parse()?;

        match field_identifier.to_string().as_str() {
            "pipeline" => Ok(Self::Pipeline(input.parse()?)),
            "attachments" => {
                let content;
                bracketed!(content in input);
                Ok(Self::Attachments(
                    content
                        .parse_terminated(MaterialAttachmentDescriptor::parse, Token![,])?
                        .into_iter()
                        .collect(),
                ))
            }
            "ident" => Ok(Self::Ident(input.parse()?)),
            _ => Err(syn::Error::new_spanned(
                field_identifier,
                "Invalid argument passed into the material descriptor",
            )),
        }
    }
}

#[derive(Clone)]
enum PipelineIdVariants {
    Ident(Lit),
    Specifier(PipelineIdDescriptor),
}

impl Parse for PipelineIdVariants {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        if input.peek(syn::token::Brace) {
            Ok(Self::Specifier(input.parse()?))
        } else {
            let val = SimpleFieldValue::Expression(input.parse()?);
            if let Some(ident) = val.get_ident() {
                Ok(Self::Ident(ident))
            } else {
                let span = match val {
                    SimpleFieldValue::Expression(expr) => expr.span(),
                    SimpleFieldValue::Literal(lit) => lit.span(),
                };
                Err(syn::Error::new(
                    span,
                    "Error getting an identifier for a pipeline ID",
                ))
            }
        }
    }
}

#[derive(Clone)]
struct PipelineIdDescriptor {
    vertex_shader_path: LitStr,
    fragment_shader_path: LitStr,
    vertex_layouts: Vec<ExprCall>,
    geometry_details: Option<GeometryDetailsDescriptor>,
    ident: Option<Lit>,
}

impl Parse for PipelineIdDescriptor {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let content;
        braced!(content in input);
        let fields = content.parse_terminated(SimpleField::parse, Token![,])?;
        let mut vertex_shader_path: Option<LitStr> = None;
        let mut fragment_shader_path: Option<LitStr> = None;
        let mut vertex_layouts: Vec<ExprCall> = Vec::new();
        let mut geometry_details: Option<GeometryDetailsDescriptor> = None;
        let mut ident: Option<Lit> = None;

        for field in fields {
            match field.ident.to_string().as_str() {
                "vertex_shader_path" => {
                    if let Some(value) = field.value {
                        match value {
                            SimpleFieldValue::Expression(expr) => {
                                return Err(syn::Error::new(
                                    expr.span(),
                                    "Only string literals are valid paths",
                                ))
                            }
                            SimpleFieldValue::Literal(lit) => {
                                if let Lit::Str(str) = lit {
                                    vertex_shader_path = Some(str)
                                } else {
                                    return Err(syn::Error::new(
                                        lit.span(),
                                        "Only string literals are valid paths",
                                    ));
                                }
                            }
                        }
                    }
                }
                "fragment_shader_path" => {
                    if let Some(value) = field.value {
                        match value {
                            SimpleFieldValue::Expression(expr) => {
                                return Err(syn::Error::new(
                                    expr.span(),
                                    "Only string literals are valid paths",
                                ))
                            }
                            SimpleFieldValue::Literal(lit) => {
                                if let Lit::Str(str) = lit {
                                    fragment_shader_path = Some(str)
                                } else {
                                    return Err(syn::Error::new(
                                        lit.span(),
                                        "Only string literals are valid paths",
                                    ));
                                }
                            }
                        }
                    }
                }
                "vertex_layouts" => {
                    if let Some(value) = field.value {
                        match value {
                            SimpleFieldValue::Expression(expr) => {
                                let stream = quote! {#expr};
                                vertex_layouts = parse2::<VertexLayoutsDescriptor>(stream)?.0;
                            }
                            SimpleFieldValue::Literal(lit) => {
                                return Err(syn::Error::new(
                                    lit.span(),
                                    "Invalid value for vertex layout",
                                ))
                            }
                        }
                    }
                }
                "geometry_details" => geometry_details = Some(input.parse()?),
                "ident" => ident = Some(input.parse()?),
                _ => {
                    return Err(syn::Error::new_spanned(
                        field.ident,
                        "Invalid argument passed into the pipeline descriptor",
                    ))
                }
            }
        }

        let Some(vertex_shader_path) = vertex_shader_path else {
            return Err(input.error("A vertex shader path must be specified"));
        };
        let Some(fragment_shader_path) = fragment_shader_path else {
            return Err(input.error("A fragment shader path must be specified"));
        };

        Ok(PipelineIdDescriptor {
            vertex_shader_path,
            fragment_shader_path,
            vertex_layouts,
            geometry_details,
            ident,
        })
    }
}

struct VertexLayoutsDescriptor(Vec<ExprCall>);

impl Parse for VertexLayoutsDescriptor {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let content;
        bracketed!(content in input);
        Ok(Self(
            content
                .parse_terminated(ExprCall::parse, Token![,])?
                .into_iter()
                .collect(),
        ))
    }
}

#[derive(Default, Clone)]
struct GeometryDetailsDescriptor {
    topology: Option<ExprPath>,
    strip_index_format: Option<ExprPath>,
    front_face: Option<ExprPath>,
    cull_mode: Option<ExprPath>,
    polygon_mode: Option<ExprPath>,
}

impl Parse for GeometryDetailsDescriptor {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let content;
        braced!(content in input);
        let fields = content.parse_terminated(SimpleField::parse, Token![,])?;

        let mut details = GeometryDetailsDescriptor::default();

        for field in fields {
            match field.ident.to_string().as_str() {
                "topology" => details.topology = Some(input.parse()?),
                "strip_index_format" => details.strip_index_format = Some(input.parse()?),
                "front_face" => details.front_face = Some(input.parse()?),
                "cull_mode" => details.cull_mode = Some(input.parse()?),
                "polygon_mode" => details.polygon_mode = Some(input.parse()?),
                _ => {
                    return Err(syn::Error::new_spanned(
                        field.ident,
                        "Invalid argument passed into the pipeline geometry details descriptor",
                    ))
                }
            }
        }

        Ok(details)
    }
}

enum MaterialAttachmentDescriptor {
    Texture(MaterialTextureAttachmentDescriptor),
    Buffer(MaterialBufferAttachmentDescriptor),
}

impl Parse for MaterialAttachmentDescriptor {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let ident: Ident = input.parse()?;
        match input.to_string().as_str() {
            "Texture" => Ok(MaterialAttachmentDescriptor::Texture(input.parse()?)),
            "Buffer" => Ok(MaterialAttachmentDescriptor::Buffer(input.parse()?)),
            _ => Err(syn::Error::new(ident.span(), "Invalid Material Attachment"))
        }
    }
}

impl quote::ToTokens for MaterialAttachmentDescriptor {
    fn to_tokens(&self, tokens: &mut TokenStream2) {
        match self {
            MaterialAttachmentDescriptor::Texture(material_texture_attachment_descriptor) => {
                let texture = match &material_texture_attachment_descriptor.texture {
                    Some(tex) => quote! {texture: #tex},
                    None => quote! {texture,},
                };
                let visibility = match &material_texture_attachment_descriptor.visibility {
                    Some(vis) => quote! {visibility: #vis},
                    None => quote! {visibility,},
                };
                tokens.extend(quote! {
                    v4::ecs::material::MaterialTextureAttachment {
                        #texture,
                        #visibility,
                    }
                })
            },
            MaterialAttachmentDescriptor::Buffer(material_buffer_attachment_descriptor) => {
                let texture = match &material_buffer_attachment_descriptor.buffer {
                    Some(tex) => quote! {texture: #tex},
                    None => quote! {texture,},
                };
                let visibility = match &material_buffer_attachment_descriptor.visibility {
                    Some(vis) => quote! {visibility: #vis},
                    None => quote! {visibility,},
                };
                tokens.extend(quote! {
                    v4::ecs::material::MaterialTextureAttachment {
                        #texture,
                        #visibility,
                    }
                })
            },
        };



    }
}

struct MaterialTextureAttachmentDescriptor {
    texture: Option<Expr>,
    visibility: Option<ExprPath>,
}

impl Parse for MaterialTextureAttachmentDescriptor {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let content;
        parenthesized!(content in input);
        let fields = content.parse_terminated(SimpleField::parse, Token![,])?;
        let mut texture: Option<Expr> = None;
        let mut visibility: Option<ExprPath> = None;
        for field in fields {
            match field.ident.to_string().as_str() {
                "texture" => texture = match field.value {
                    Some(value) =>  Some(match value {
                        SimpleFieldValue::Expression(expr) => Ok(expr),
                        SimpleFieldValue::Literal(lit) => Err(syn::Error::new(lit.span(), "Invalid texture value")),
                    }?),
                    None => None,
                },
                "visibility" => visibility = match field.value {
                    Some(value) => Some(match value {
                        SimpleFieldValue::Expression(expr) => if let Expr::Path(path) = expr {Ok(path)} else {Err(syn::Error::new(expr.span(), "Invalid texture visibility value"))},
                        SimpleFieldValue::Literal(lit) => Err(syn::Error::new(lit.span(), "Invalid texture visibility value")),
                    }?),
                    None => None,
                },
                    _ => {}
                }

            }
        Ok(MaterialTextureAttachmentDescriptor { texture, visibility })
    }
}

struct MaterialBufferAttachmentDescriptor {
    buffer: Option<Expr>,
    visibility: Option<ExprPath>,
}

impl Parse for MaterialBufferAttachmentDescriptor {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let content;
        parenthesized!(content in input);
        let fields = content.parse_terminated(SimpleField::parse, Token![,])?;
        let mut buffer: Option<Expr> = None;
        let mut visibility: Option<ExprPath> = None;
        for field in fields {
            match field.ident.to_string().as_str() {
                "buffer" => buffer = match field.value {
                    Some(value) =>  Some(match value {
                        SimpleFieldValue::Expression(expr) => Ok(expr),
                        SimpleFieldValue::Literal(lit) => Err(syn::Error::new(lit.span(), "Invalid buffer value")),
                    }?),
                    None => None,
                },
                "visibility" => visibility = match field.value {
                    Some(value) => Some(match value {
                        SimpleFieldValue::Expression(expr) => if let Expr::Path(path) = expr {Ok(path)} else {Err(syn::Error::new(expr.span(), "Invalid texture visibility value"))},
                        SimpleFieldValue::Literal(lit) => Err(syn::Error::new(lit.span(), "Invalid buffer visibility value")),
                    }?),
                    None => None,
                },
                    _ => {}
                }

            }
        Ok(MaterialBufferAttachmentDescriptor { buffer, visibility })
    }
}
