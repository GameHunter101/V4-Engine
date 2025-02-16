use std::collections::HashMap;

use proc_macro2::{TokenStream as TokenStream2, TokenTree};
use quote::{format_ident, quote, ToTokens};
use syn::{
    braced, bracketed, parenthesized,
    parse::{Parse, ParseStream},
    parse2,
    punctuated::Punctuated,
    spanned::Spanned,
    AngleBracketedGenericArguments, Expr, ExprCall, ExprPath, Ident, Lit, LitBool, LitStr, Token,
};
use v4_core::ecs::{component::ComponentId, entity::EntityId};

pub struct SceneDescriptor {
    scene_ident: Option<Ident>,
    entities: Vec<TransformedEntityDescriptor>,
    idents: HashMap<Lit, Id>,
    relationships: HashMap<EntityId, Vec<EntityId>>,
    materials: Vec<TransformedMaterialDescriptor>,
    screen_space_materials: Vec<MaterialDescriptor>,
    pipelines: Vec<PipelineIdDescriptor>,
    active_camera: Option<Lit>,
}

impl Parse for SceneDescriptor {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let scene_ident: Option<Ident> = if input.peek(Ident) && input.peek2(Token![:]) {
            let keyword: Ident = input.parse()?;
            if &keyword.to_string() != "scene" {
                return Err(syn::Error::new(keyword.span(), "Invalid specifier found. If you meant to specify a scene name you can do so using 'scene: {name}'"));
            }
            let _: Token![:] = input.parse()?;
            let ident: Ident = input.parse()?;
            let _: Token![,] = input.parse()?;
            Some(ident)
        } else {
            None
        };

        let active_camera = if input.peek(syn::Ident) {
            let ident: Ident = input.parse()?;
            if &ident.to_string() == "active_camera" {
                let _: Token![:] = input.parse()?;
                let entity_ident: Lit = input.parse()?;
                let _: Token![,] = input.parse()?;
                Ok(Some(entity_ident))
            } else {
                Err(syn::Error::new(ident.span(), "Invalid specifier found. In order to specify the active camera, use the `active_camera` field"))
            }
        } else {
            Ok(None)
        }?;

        let screen_space_materials: Vec<MaterialDescriptor> = if input.peek(syn::Ident)
            && input.peek2(Token![:])
        {
            let ident: Ident = input.parse()?;
            if &ident.to_string() == "screen_space_materials" {
                let _: Token![:] = input.parse()?;
                let content;
                bracketed!(content in input);
                let materials =
                    content.parse_terminated(MaterialDescriptor::parse_screen_space, Token![,])?;
                let _: Token![,] = input.parse()?;
                Ok(materials.into_iter().collect())
            } else {
                Err(syn::Error::new(ident.span(), "Invalid specifier found. If you meant to specify screen-space materials, use the `screen_space_materials` field"))
            }
        } else {
            Ok(Vec::new())
        }?;

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

            let material_id = if let Some(material) = entity.material {
                let pipeline_id = match &material.pipeline_id {
                    PipelineIdVariants::Ident(pipeline_ident) => match idents.get(pipeline_ident) {
                        Some(id) => {
                            if let Id::Pipeline(id) = *id {
                                Ok(id)
                            } else {
                                return Err(syn::Error::new(pipeline_ident.span(), format!("Two objects share the same identifier: \"{pipeline_ident:?}\"")));
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
                    PipelineIdVariants::ScreenSpace(_) => return Err(input.error("Screen-space materials are not valid here")),
                }?;

                if let Some(ident) = &material.ident {
                    idents.insert(ident.clone(), Id::Material(materials.len() as u32));
                }

                materials.push(TransformedMaterialDescriptor {
                    pipeline_id,
                    attachments: material.attachments,
                    entities_attached: vec![current_ident],
                });

                Some(materials.len() as u32 - 1)
            } else {
                None
            };

            let transformed_entity = TransformedEntityDescriptor {
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

        if let Some(active_camera_ident) = active_camera.as_ref() {
            if !idents.contains_key(active_camera_ident) {
                return Err(syn::Error::new(active_camera_ident.span(), "The identifier was not found. Make sure to specify which the identifier on an entity"));
            }
        }

        Ok(Self {
            scene_ident,
            entities: transformed_entities,
            idents,
            relationships,
            materials,
            screen_space_materials,
            pipelines,
            active_camera,
        })
    }
}

impl quote::ToTokens for SceneDescriptor {
    fn to_tokens(&self, tokens: &mut TokenStream2) {
        let scene_name = match &self.scene_ident {
            Some(ident) => quote! {#ident},
            None => quote! {scene},
        };

        let screen_space_material_initializations: Vec<TokenStream2> = self
            .screen_space_materials
            .iter()
            .map(|mat| {
                let MaterialDescriptor {
                    pipeline_id,
                    attachments,
                    ..
                } = mat;
                let PipelineIdVariants::ScreenSpace(pipeline_id) = pipeline_id else {
                    panic!("Invalid pipeline ID found for a screen-space material");
                };

                quote! {
                    #scene_name.create_material(
                        #pipeline_id,
                        vec![#(#attachments),*],
                        Vec::new(),
                    );
                }
            })
            .collect();

        let pipeline_id_initializations: Vec<TokenStream2> = self
            .pipelines
            .iter()
            .map(|pipeline| {
                quote! {#pipeline}
            })
            .collect();

        let material_initializations = self.materials.iter().map(|mat| {
            let pipeline_id_index = mat.pipeline_id;
            let pipeline_id = &pipeline_id_initializations[pipeline_id_index];
            let attachments = &mat.attachments;
            let entities_attached = &mat.entities_attached;
            quote! {
                #scene_name.create_material(
                    #pipeline_id,
                    vec![#(#attachments),*],
                    vec![#(#entities_attached),*]
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
                if let Some(generics) = &component.generics {
                    quote!{::#generics}
                } else {
                    quote! {}
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
                let #entity_ident = #scene_name.create_entity(
                    #parent_id,
                    vec![#(#component_initializations),*],
                    #material,
                    #is_enabled,
                );
            }
        });

        let camera_set = if let Some(active_camera) = &self.active_camera {
            let id = &self.idents[active_camera];
            quote! {
                #scene_name.set_active_camera(Some(#id));
            }
        } else {
            quote! {}
        };

        tokens.extend(quote! {
            let mut #scene_name = v4::ecs::scene::Scene::default();

            #(#material_initializations)*

            #(#screen_space_material_initializations)*

            #(#entity_initializations)*

            #camera_set
        });
    }
}

struct TransformedEntityDescriptor {
    components: Vec<ComponentDescriptor>,
    material_id: Option<ComponentId>,
    parent: Option<EntityId>,
    id: EntityId,
    is_enabled: bool,
    ident: Option<Lit>,
}
pub enum Id {
    Entity(EntityId),
    Component(ComponentId),
    Material(ComponentId),
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
    generics: Option<AngleBracketedGenericArguments>,
    params: Vec<SimpleField>,
    custom_constructor: Option<ComponentConstructor>,
    ident: Option<Lit>,
}

impl Parse for ComponentDescriptor {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let component_type: Ident = input.parse()?;
        let generics: Option<AngleBracketedGenericArguments> = if input.peek(Token![<]) {
            Some(input.parse()?)
        } else {
            None
        };

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

        let postfix: Option<TokenStream2> = if input.is_empty() || input.peek(Token![,]) {
            None
        } else {
            let mut tokens = quote! {};

            while !input.peek(Token![,]) && !input.is_empty() {
                let token: TokenTree = input.parse()?;
                tokens.extend(token.to_token_stream());
            }

            Some(tokens)
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
    attachments: Vec<ShaderAttachmentDescriptor>,
    ident: Option<Lit>,
}

impl MaterialDescriptor {
    fn parse_screen_space(input: ParseStream) -> syn::Result<Self> {
        let content;
        braced!(content in input);
        let params = content.parse_terminated(MaterialParameters::parse_screen_space, Token![,])?;

        let mut pipeline_id: Option<ScreenSpacePipelineIdDescriptor> = None;
        let mut attachments: Vec<ShaderAttachmentDescriptor> = Vec::new();
        let mut ident: Option<Lit> = None;

        for param in params {
            match param {
                MaterialParameters::Pipeline(specified_pipeline_id) => {
                    match specified_pipeline_id {
                        PipelineIdVariants::ScreenSpace(screen_space_pipeline_id_descriptor) => {
                            pipeline_id = Some(screen_space_pipeline_id_descriptor)
                        }
                        _ => return Err(input.error("Only screen-space pipelines are valid here")),
                    }
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
            pipeline_id: PipelineIdVariants::ScreenSpace(pipeline_id),
            attachments,
            ident,
        })
    }
}

impl Parse for MaterialDescriptor {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let content;
        braced!(content in input);
        let params = content.parse_terminated(MaterialParameters::parse, Token![,])?;

        let mut pipeline_id: Option<PipelineIdVariants> = None;
        let mut attachments: Vec<ShaderAttachmentDescriptor> = Vec::new();
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
    attachments: Vec<ShaderAttachmentDescriptor>,
    entities_attached: Vec<EntityId>,
}

enum MaterialParameters {
    Pipeline(PipelineIdVariants),
    Attachments(Vec<ShaderAttachmentDescriptor>),
    Ident(Lit),
}

impl MaterialParameters {
    fn parse_screen_space(input: ParseStream) -> syn::Result<Self> {
        let field_identifier: Ident = input.parse()?;
        let _: Token![:] = input.parse()?;

        match field_identifier.to_string().as_str() {
            "pipeline" => Ok(Self::Pipeline(PipelineIdVariants::ScreenSpace(
                input.parse()?,
            ))),
            "attachments" => {
                let content;
                bracketed!(content in input);
                Ok(Self::Attachments(
                    content
                        .parse_terminated(ShaderAttachmentDescriptor::parse, Token![,])?
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
                        .parse_terminated(ShaderAttachmentDescriptor::parse, Token![,])?
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
    ScreenSpace(ScreenSpacePipelineIdDescriptor),
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
struct ScreenSpacePipelineIdDescriptor {
    fragment_shader_path: LitStr,
}

impl Parse for ScreenSpacePipelineIdDescriptor {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let content;
        braced!(content in input);
        let fields = content.parse_terminated(SimpleField::parse, Token![,])?;
        let mut fragment_shader_path: Option<LitStr> = None;

        for field in fields {
            match field.ident.to_string().as_str() {
                "vertex_shader_path" => {
                    return Err(syn::Error::new(
                        field.ident.span(),
                        "No vertex shader should be specified for a screen-space effect",
                    ))
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
                    return Err(syn::Error::new(
                        field.ident.span(),
                        "No vertex layouts should be specified for a screen-space effect",
                    ))
                }
                "uses_camera" => {
                    return Err(syn::Error::new(
                        field.ident.span(),
                        "No camera usage should be specified for a screen-space effect",
                    ))
                }
                "geometry_details" => {
                    return Err(syn::Error::new(
                        field.ident.span(),
                        "No camera usage should be specified for a screen-space effect",
                    ))
                }
                "ident" => {
                    return Err(syn::Error::new(
                        field.ident.span(),
                        "Identifiers are not valid here, as they can not be safely checked",
                    ))
                }
                _ => {
                    return Err(syn::Error::new_spanned(
                        field.ident,
                        "Invalid argument passed into the pipeline descriptor",
                    ))
                }
            }
        }

        let Some(fragment_shader_path) = fragment_shader_path else {
            return Err(input.error("A fragment shader path must be specified"));
        };

        Ok(ScreenSpacePipelineIdDescriptor {
            fragment_shader_path,
        })
    }
}

impl quote::ToTokens for ScreenSpacePipelineIdDescriptor {
    fn to_tokens(&self, tokens: &mut TokenStream2) {
        let ScreenSpacePipelineIdDescriptor {
            fragment_shader_path,
        } = self;
        tokens.extend(quote! {
            v4::engine_management::pipeline::PipelineId {
                vertex_shader: v4::engine_management::pipeline::PipelineShader::Path(""),
                fragment_shader: v4::engine_management::pipeline::PipelineShader::Path(#fragment_shader_path),
                vertex_layouts: Vec::new(),
                uses_camera: false,
                is_screen_space: true,
                geometry_details: Default::default(),
            }
        });
    }
}

#[derive(Clone)]
struct PipelineIdDescriptor {
    vertex_shader_path: LitStr,
    fragment_shader_path: LitStr,
    vertex_layouts: Vec<ExprCall>,
    uses_camera: LitBool,
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
        let mut uses_camera: Option<LitBool> = None;
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
                "uses_camera" => {
                    if let Some(SimpleFieldValue::Literal(Lit::Bool(bool))) = field.value {
                        uses_camera = Some(bool);
                    }
                }
                "geometry_details" => geometry_details = Some(input.parse()?),
                "ident" => {
                    if let Some(SimpleFieldValue::Literal(lit)) = field.value {
                        ident = Some(lit);
                    }
                }
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

        let Some(uses_camera) = uses_camera else {
            return Err(input.error("The usage of the camera must be specified"));
        };

        Ok(PipelineIdDescriptor {
            vertex_shader_path,
            fragment_shader_path,
            vertex_layouts,
            uses_camera,
            geometry_details,
            ident,
        })
    }
}

impl quote::ToTokens for PipelineIdDescriptor {
    fn to_tokens(&self, tokens: &mut TokenStream2) {
        let PipelineIdDescriptor {
            vertex_shader_path,
            fragment_shader_path,
            vertex_layouts,
            uses_camera,
            geometry_details,
            ..
        } = self;
        let geometry_details = match geometry_details {
            Some(geo) => quote! {#geo},
            None => quote! {v4::engine_management::pipeline::GeometryDetails::default()},
        };
        tokens.extend(quote! {
            v4::engine_management::pipeline::PipelineId {
                vertex_shader: v4::engine_management::pipeline::PipelineShader::Path(#vertex_shader_path),
                fragment_shader: v4::engine_management::pipeline::PipelineShader::Path(#fragment_shader_path),
                vertex_layouts: vec![#(#vertex_layouts),*],
                uses_camera: #uses_camera,
                is_screen_space: false,
                geometry_details: #geometry_details,
            }
        });
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

impl quote::ToTokens for GeometryDetailsDescriptor {
    fn to_tokens(&self, tokens: &mut TokenStream2) {
        let GeometryDetailsDescriptor {
            topology,
            strip_index_format,
            front_face,
            cull_mode,
            polygon_mode,
        } = self;

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

        tokens.extend(quote! {
            v4::engine_management::pipeline::GeometryDetails {
            #topology,
            #strip_index_format,
            #front_face,
            #cull_mode,
            #polygon_mode,
            }
        });
    }
}

enum ShaderAttachmentDescriptor {
    Texture(ShaderTextureAttachmentDescriptor),
    Buffer(ShaderBufferAttachmentDescriptor),
}

impl Parse for ShaderAttachmentDescriptor {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let ident: Ident = input.parse()?;
        match input.to_string().as_str() {
            "Texture" => Ok(ShaderAttachmentDescriptor::Texture(input.parse()?)),
            "Buffer" => Ok(ShaderAttachmentDescriptor::Buffer(input.parse()?)),
            _ => Err(syn::Error::new(ident.span(), "Invalid Material Attachment")),
        }
    }
}

impl quote::ToTokens for ShaderAttachmentDescriptor {
    fn to_tokens(&self, tokens: &mut TokenStream2) {
        match self {
            ShaderAttachmentDescriptor::Texture(material_texture_attachment_descriptor) => {
                let texture = match &material_texture_attachment_descriptor.texture {
                    Some(tex) => quote! {texture: #tex},
                    None => quote! {texture,},
                };
                let visibility = match &material_texture_attachment_descriptor.visibility {
                    Some(vis) => quote! {visibility: #vis},
                    None => quote! {visibility,},
                };
                tokens.extend(quote! {
                    v4::ecs::material::ShaderTextureAttachment {
                        #texture,
                        #visibility,
                    }
                })
            }
            ShaderAttachmentDescriptor::Buffer(material_buffer_attachment_descriptor) => {
                let buffer = match &material_buffer_attachment_descriptor.buffer {
                    Some(buf) => quote! {buffer: #buf},
                    None => quote! {buffer,},
                };
                let visibility = match &material_buffer_attachment_descriptor.visibility {
                    Some(vis) => quote! {visibility: #vis},
                    None => quote! {visibility,},
                };
                tokens.extend(quote! {
                    v4::ecs::material::ShaderBufferAttachment {
                        #buffer,
                        #visibility,
                    }
                })
            }
        };
    }
}

struct ShaderTextureAttachmentDescriptor {
    texture: Option<Expr>,
    visibility: Option<ExprPath>,
}

impl Parse for ShaderTextureAttachmentDescriptor {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let content;
        parenthesized!(content in input);
        let fields = content.parse_terminated(SimpleField::parse, Token![,])?;
        let mut texture: Option<Expr> = None;
        let mut visibility: Option<ExprPath> = None;
        for field in fields {
            match field.ident.to_string().as_str() {
                "texture" => {
                    texture = match field.value {
                        Some(value) => Some(match value {
                            SimpleFieldValue::Expression(expr) => Ok(expr),
                            SimpleFieldValue::Literal(lit) => {
                                Err(syn::Error::new(lit.span(), "Invalid texture value"))
                            }
                        }?),
                        None => None,
                    }
                }
                "visibility" => {
                    visibility = match field.value {
                        Some(value) => Some(match value {
                            SimpleFieldValue::Expression(expr) => {
                                if let Expr::Path(path) = expr {
                                    Ok(path)
                                } else {
                                    Err(syn::Error::new(
                                        expr.span(),
                                        "Invalid texture visibility value",
                                    ))
                                }
                            }
                            SimpleFieldValue::Literal(lit) => Err(syn::Error::new(
                                lit.span(),
                                "Invalid texture visibility value",
                            )),
                        }?),
                        None => None,
                    }
                }
                _ => {}
            }
        }
        Ok(ShaderTextureAttachmentDescriptor {
            texture,
            visibility,
        })
    }
}

struct ShaderBufferAttachmentDescriptor {
    buffer: Option<Expr>,
    visibility: Option<ExprPath>,
}

impl Parse for ShaderBufferAttachmentDescriptor {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let content;
        parenthesized!(content in input);
        let fields = content.parse_terminated(SimpleField::parse, Token![,])?;
        let mut buffer: Option<Expr> = None;
        let mut visibility: Option<ExprPath> = None;
        for field in fields {
            match field.ident.to_string().as_str() {
                "buffer" => {
                    buffer = match field.value {
                        Some(value) => Some(match value {
                            SimpleFieldValue::Expression(expr) => Ok(expr),
                            SimpleFieldValue::Literal(lit) => {
                                Err(syn::Error::new(lit.span(), "Invalid buffer value"))
                            }
                        }?),
                        None => None,
                    }
                }
                "visibility" => {
                    visibility = match field.value {
                        Some(value) => Some(match value {
                            SimpleFieldValue::Expression(expr) => {
                                if let Expr::Path(path) = expr {
                                    Ok(path)
                                } else {
                                    Err(syn::Error::new(
                                        expr.span(),
                                        "Invalid texture visibility value",
                                    ))
                                }
                            }
                            SimpleFieldValue::Literal(lit) => Err(syn::Error::new(
                                lit.span(),
                                "Invalid buffer visibility value",
                            )),
                        }?),
                        None => None,
                    }
                }
                _ => {}
            }
        }
        Ok(ShaderBufferAttachmentDescriptor { buffer, visibility })
    }
}
