use std::collections::HashMap;

use proc_macro2::{TokenStream as TokenStream2, TokenTree};
use quote::{ToTokens, format_ident, quote};
use syn::{
    AngleBracketedGenericArguments, Expr, ExprCall, ExprPath, Ident, Lit, LitBool,
    LitStr, Token, braced, bracketed, parenthesized,
    parse::{Parse, ParseStream, discouraged::Speculative},
    parse2,
    punctuated::Punctuated,
    spanned::Spanned,
};
use v4_core::ecs::{component::ComponentId, entity::EntityId};

pub struct SceneDescriptor {
    scene_ident: Option<Ident>,
    entities: Vec<TransformedEntityDescriptor>,
    idents: HashMap<Lit, Id>,
    materials: Vec<TransformedMaterialDescriptor>,
    screen_space_materials: Vec<MaterialDescriptor>,
    pipelines: Vec<PipelineIdDescriptor>,
    active_camera: Option<Lit>,
}

impl SceneDescriptor {
    fn get_scene_ident(input: ParseStream) -> syn::Result<Option<Ident>> {
        if input.peek(Ident) && input.peek2(Token![:]) {
            let keyword: Ident = input.parse()?;
            if &keyword.to_string() != "scene" {
                return Err(syn::Error::new(
                    keyword.span(),
                    "Invalid specifier found. If you meant to specify a scene name you can do so using 'scene: {name}'",
                ));
            }
            let _: Token![:] = input.parse()?;
            let ident: Ident = input.parse()?;
            let _: Token![,] = input.parse()?;
            Ok(Some(ident))
        } else {
            Ok(None)
        }
    }

    fn get_active_camera(input: ParseStream) -> syn::Result<Option<Lit>> {
        if input.peek(syn::Ident) {
            let ident: Ident = input.parse()?;
            if &ident.to_string() == "active_camera" {
                let _: Token![:] = input.parse()?;
                if input.peek(Token![_]) {
                    input.parse::<Token![_]>()?;
                    input.parse::<Token![,]>()?;
                    Ok(None)
                } else {
                    let entity_ident: Lit = input.parse()?;
                    let _: Token![,] = input.parse()?;
                    Ok(Some(entity_ident))
                }
            } else {
                Err(syn::Error::new(
                    ident.span(),
                    "Invalid specifier found. In order to specify the active camera, use the `active_camera` field",
                ))
            }
        } else {
            Ok(None)
        }
    }

    fn get_screen_space_materials(input: ParseStream) -> syn::Result<Vec<MaterialDescriptor>> {
        if input.peek(syn::Ident) && input.peek2(Token![:]) {
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
                Err(syn::Error::new(
                    ident.span(),
                    "Invalid specifier found. If you meant to specify screen-space materials, use the `screen_space_materials` field",
                ))
            }
        } else {
            Ok(Vec::new())
        }
    }
}

impl Parse for SceneDescriptor {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let scene_ident: Option<Ident> = Self::get_scene_ident(input)?;

        let active_camera = Self::get_active_camera(input)?;

        let screen_space_materials: Vec<MaterialDescriptor> =
            Self::get_screen_space_materials(input)?;

        let entities: Vec<EntityDescriptor> = input
            .parse_terminated(EntityDescriptor::parse, Token![,])?
            .into_iter()
            .collect();

        let mut idents: HashMap<Lit, Id> = HashMap::new();
        let mut relationships: HashMap<EntityId, Vec<EntityId>> = HashMap::new();
        let mut materials = Vec::new();
        let mut pipelines = Vec::new();

        let mut current_entity_id = 1;
        let mut current_component_id = 1;
        let transformed_entities = entities.into_iter().map(|entity| {
            let material_id = if let Some(material) = entity.material {
                Some(material.initialize_and_get_id(current_entity_id, input, &mut idents, &mut pipelines, &mut materials)?)
            } else {
                None
            };

            let mut parent: Option<EntityId> = None;

            if let Some(parent_ident) = &entity.parent {
                if let Some(parent_id) = idents.get(parent_ident) {
                    let Id::Entity(parent_id) = parent_id else {
                        return Err(syn::Error::new_spanned(
                            parent_ident,
                            format!("Two objects share the same identifier: \"{parent_ident:?}\""),
                        ));
                    };
                    if let Some(children) = relationships.get_mut(parent_id) {
                        children.push(current_entity_id);
                    } else {
                        relationships.insert(*parent_id, vec![current_entity_id]);
                    }
                    parent = Some(*parent_id);
                } else {
                    return Err(syn::Error::new_spanned(parent_ident, format!("The parent entity \"{parent_ident:?}\" could not be found. If you declared it, make sure it is declared above the current entity")));
                }
            }

            let transformed_entity = TransformedEntityDescriptor {
                components: entity.components,
                computes: entity.computes,
                material_id,
                parent,
                _id: current_entity_id,
                is_enabled: entity.is_enabled,
                ident: entity.ident,
            };

            if let Some(ident) = &transformed_entity.ident {
                idents.insert(ident.clone(), Id::Entity(current_entity_id));
                current_entity_id += 1;
            }


            for component in &transformed_entity.components {
                if let Some(ident) = &component.ident {
                    idents.insert(ident.clone(), Id::Component(current_component_id));
                    current_component_id += 1;
                }
            }

            for compute in &transformed_entity.computes {
                if let Some(ident) = &compute.ident {
                    idents.insert(ident.clone(), Id::Component(current_component_id));
                    current_component_id += 1;
                }
            }

            Ok(transformed_entity)
        }).collect::<syn::Result<Vec<TransformedEntityDescriptor>>>()?;

        if let Some(active_camera_ident) = active_camera.as_ref() {
            if !idents.contains_key(active_camera_ident) {
                return Err(syn::Error::new(
                    active_camera_ident.span(),
                    "The identifier was not found. Make sure to specify which the identifier on an entity",
                ));
            }
        }

        Ok(Self {
            scene_ident,
            entities: transformed_entities,
            idents,
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
            let immediate_data = if let Some(data) = mat.immediate_data.as_ref() {
                quote! {#data}
            } else {
                quote! {Vec::new()}
            };
            let is_enabled = if let Some(enabled_lit) = mat.is_enabled.as_ref() {
                quote! {#enabled_lit}
            } else {
                quote! {true}
            };

            quote! {
                #scene_name.create_material(
                    #pipeline_id,
                    vec![#(#attachments),*],
                    vec![#(#entities_attached),*],
                    #immediate_data,
                    #is_enabled,
                );
            }
        });

        let entity_initializations = self.entities.iter().map(|entity| {
            let parent_id = match entity.parent {
                Some(id) => quote! {Some(#id)},
                None => quote! {None},
            };

            let component_initializations = entity.components.iter().map(|component| {
                let mut token_stream = TokenStream2::new();
                component.to_tokens(&mut token_stream, &self.idents);
                quote! {#token_stream}
            });

            let compute_initializations = entity.computes.iter().map(|compute| {
                let mut token_stream = TokenStream2::new();
                compute.to_tokens(&mut token_stream, &self.idents);
                quote! {#token_stream}
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
                    vec![#(#compute_initializations),*],
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
    computes: Vec<ComputeDescriptor>,
    material_id: Option<ComponentId>,
    parent: Option<EntityId>,
    _id: EntityId,
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
    computes: Vec<ComputeDescriptor>,
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
            computes: Vec::new(),
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
                EntityParameters::Computes(vec) => entity_descriptor.computes = vec,
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
    Computes(Vec<ComputeDescriptor>),
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
            "computes" => {
                let content;
                bracketed!(content in input);
                let computes = content.parse_terminated(ComputeDescriptor::parse, Token![,])?;
                Ok(EntityParameters::Computes(computes.into_iter().collect()))
            }
            "material" => Ok(EntityParameters::Material(input.parse()?)),
            "parent" => Ok(EntityParameters::Parent(input.parse()?)),
            "is_enabled" => {
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
    parameters: Vec<SimpleField>,
    custom_constructor: Option<ComponentConstructor>,
    ident: Option<Lit>,
}

impl ComponentDescriptor {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream, idents: &HashMap<Lit, Id>) {
        let component_type = &self.component_type;
        let component_generics = if let Some(generics) = &self.generics {
            quote! {::#generics}
        } else {
            quote! {}
        };

        if let Some(constructor) = &self.custom_constructor {
            let params = constructor.parameters.iter().map(|param| {
                if let Expr::Call(ExprCall { func, args, .. }) = param {
                    if let Expr::Path(ExprPath { path, .. }) = *func.clone() {
                        if let Some(possible_ident) = path.get_ident() {
                            if &possible_ident.to_string() == "ident" {
                                if let Some(Expr::Lit(lit)) = args.first() {
                                    let id = idents.get(&lit.lit).unwrap();
                                    return syn::Expr::Verbatim(quote! {#id});
                                }
                            }
                        }
                    }
                }

                return param.clone();
            });

            let new_constructor = ComponentConstructor {
                parameters: Punctuated::from_iter(params.into_iter()),
                ..(constructor.clone())
            };

            tokens.extend(quote! {
                Box::new(#component_type #component_generics::#new_constructor)
            });
        } else {
            let params = self.parameters.iter().map(|param| {
                let field = &param.ident;
                if let Some(value) = &param.value {
                    if let Some(ident) = value.get_ident() {
                        let id = idents.get(&ident).unwrap();
                        quote! {.#field(#id)}
                    } else {
                        quote! {.#field(#value)}
                    }
                } else {
                    quote! {.#field(#field)}
                }
            });
            let id_set = if let Some(ident) = &self.ident {
                let id = idents.get(ident).unwrap();
                quote! {.id(#id)}
            } else {
                quote! {}
            };

            tokens.extend(quote! {
                Box::new(#component_type #component_generics::builder()#(#params)*#id_set.build())
            });
        }
    }
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
                parameters: Vec::new(),
                custom_constructor: Some(custom_constructor),
                ident,
            })
        } else {
            let content;
            parenthesized!(content in input);

            let mut parameters: Vec<SimpleField> = content
                .parse_terminated(SimpleField::parse, Token![,])?
                .into_iter()
                .collect();

            let ident = parameters
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
                parameters.remove(
                    parameters
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
                parameters,
                custom_constructor: None,
                ident,
            })
        }
    }
}

#[derive(Clone)]
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
        let parameters = content.parse_terminated(Expr::parse, Token![,])?;

        let (postfix, ident): (Option<TokenStream2>, Option<Lit>) =
            if input.is_empty() || input.peek(Token![,]) {
                Ok::<(Option<TokenStream2>, Option<Lit>), syn::Error>((None, None))
            } else {
                let mut ident = None;
                let mut tokens = quote! {};

                let mut tail_getter = |stream: ParseStream| -> syn::Result<()> {
                    while !stream.peek(Token![,]) && !stream.is_empty() {
                        let token: TokenTree = stream.parse()?;
                        tokens.extend(token.to_token_stream());
                    }
                    Ok(())
                };

                let fork = input.fork();
                // Parse second token in fork to check for ident (constructor.ident("Temp ident"))
                if let Ok(_) = fork.parse::<Token![.]>() {
                    if let Ok(ident_func_name) = fork.parse::<Ident>() {
                        if &ident_func_name.to_string() == "ident" {
                            let ident_buf;
                            parenthesized!(ident_buf in fork);
                            ident = Some(ident_buf.parse()?);
                            tail_getter(&fork)?;
                        }
                    }
                } else {
                    tail_getter(&input)?;
                }

                input.advance_to(&fork);

                Ok((Some(tokens), ident))
            }?;

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

struct ComputeDescriptor {
    params: Vec<SimpleField>,
    ident: Option<Lit>,
}

impl ComputeDescriptor {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream, idents: &HashMap<Lit, Id>) {
        let params = self.params.iter().map(|param| {
            let field = &param.ident;
            if let Some(value) = &param.value {
                if let Some(ident) = value.get_ident() {
                    let id = idents.get(&ident).unwrap();
                    quote! {.#field(#id)}
                } else {
                    quote! {.#field(#value)}
                }
            } else {
                quote! {.#field(#field)}
            }
        });
        let id_set = if let Some(ident) = &self.ident {
            let id = idents.get(ident).unwrap();
            quote! {.id(#id)}
        } else {
            quote! {}
        };

        tokens.extend(quote! {
            Compute::builder()#(#params)*#id_set.build()
        });
    }
}

impl Parse for ComputeDescriptor {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let compute_ident: Ident = input.parse()?;
        if &compute_ident.to_string() != "Compute" {
            return Err(syn::Error::new(
                compute_ident.span(),
                "Only Compute components are valid in this field.",
            ));
        }

        let content;
        parenthesized!(content in input);

        let mut params: Vec<SimpleField> = content
            .parse_terminated(SimpleField::parse, Token![,])?
            .into_iter()
            .collect();

        let ident_and_index = params
            .iter()
            .enumerate()
            .filter(|(_, param)| &param.ident.to_string() == "ident" && param.value.is_some())
            .flat_map(|(i, param)| {
                if let SimpleFieldValue::Literal(ident) = param.value.as_ref().unwrap() {
                    Some((i, ident.clone()))
                } else {
                    None
                }
            })
            .next();

        if let Some((ident_index, _)) = &ident_and_index {
            params.remove(
                *ident_index, /* params
                              .iter()
                              .position(|param| param.value == Some(SimpleFieldValue::Literal(ident.clone())))
                              .unwrap(), */
            );
        }

        let ident = ident_and_index.into_iter().map(|(_, ident)| ident).next();

        Ok(ComputeDescriptor { params, ident })
    }
}

#[derive(Debug)]
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
            if input.peek(syn::token::Brace) {
                let content;
                braced!(content in input);
                value = Some(SimpleFieldValue::Group(content.parse()?));
            } else if Expr::peek(input) {
                let expr: Expr = input.parse()?;
                if let Expr::Lit(lit) = expr {
                    value = Some(SimpleFieldValue::Literal(lit.lit));
                } else {
                    value = Some(SimpleFieldValue::Expression(expr));
                }
            } else if input.peek(Lit) {
                let lit: Lit = input.parse()?;
                value = Some(SimpleFieldValue::Literal(lit));
            }
        }

        Ok(SimpleField { ident, value })
    }
}

#[derive(Debug)]
enum SimpleFieldValue {
    Expression(Expr),
    Literal(Lit),
    Group(TokenStream2),
}

impl PartialEq for SimpleFieldValue {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Expression(l0), Self::Expression(r0)) => l0 == r0,
            (Self::Literal(l0), Self::Literal(r0)) => l0 == r0,
            _ => false,
        }
    }
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
            SimpleFieldValue::Group(group) => quote! {#group},
        });
    }
}

struct MaterialDescriptor {
    pipeline_id: PipelineIdVariants,
    attachments: Vec<ShaderAttachmentDescriptor>,
    immediate_data: Option<Expr>,
    is_enabled: Option<LitBool>,
    ident: Option<Lit>,
}

impl MaterialDescriptor {
    fn parse_screen_space(input: ParseStream) -> syn::Result<Self> {
        let content;
        braced!(content in input);
        let params = content.parse_terminated(MaterialParameters::parse_screen_space, Token![,])?;

        let mut pipeline_id: Option<ScreenSpacePipelineIdDescriptor> = None;
        let mut attachments: Vec<ShaderAttachmentDescriptor> = Vec::new();
        let mut is_enabled: Option<LitBool> = None;
        let mut immediate_data: Option<Expr> = None;
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
                MaterialParameters::IsEnabled(enabled_lit) => is_enabled = Some(enabled_lit),
                MaterialParameters::ImmediateData(data) => immediate_data = Some(data),
                MaterialParameters::Ident(lit) => ident = Some(lit),
            }
        }

        let Some(pipeline_id) = pipeline_id else {
            return Err(input.error("A pipeline ID must be specified"));
        };

        Ok(MaterialDescriptor {
            pipeline_id: PipelineIdVariants::ScreenSpace(pipeline_id),
            attachments,
            immediate_data,
            is_enabled,
            ident,
        })
    }

    fn initialize_and_get_id(
        self,
        entity_ident: EntityId,
        input: ParseStream,
        idents: &mut HashMap<Lit, Id>,
        pipelines: &mut Vec<PipelineIdDescriptor>,
        materials: &mut Vec<TransformedMaterialDescriptor>,
    ) -> syn::Result<ComponentId> {
        let pipeline_id = self
            .pipeline_id
            .initialize_and_get_id(input, idents, pipelines)?;

        if let Some(ident) = self.ident {
            idents.insert(ident, Id::Material(materials.len() as ComponentId));
        }

        materials.push(TransformedMaterialDescriptor {
            pipeline_id,
            attachments: self.attachments,
            entities_attached: vec![entity_ident],
            immediate_data: self.immediate_data,
            is_enabled: self.is_enabled,
        });

        Ok(materials.len() as ComponentId - 1)
    }
}

impl Parse for MaterialDescriptor {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let content;
        braced!(content in input);
        let params = content.parse_terminated(MaterialParameters::parse, Token![,])?;

        let mut pipeline_id: Option<PipelineIdVariants> = None;
        let mut attachments: Vec<ShaderAttachmentDescriptor> = Vec::new();
        let mut immediate_data: Option<Expr> = None;
        let mut is_enabled: Option<LitBool> = None;
        let mut ident: Option<Lit> = None;

        for param in params {
            match param {
                MaterialParameters::Pipeline(specified_pipeline_id) => {
                    pipeline_id = Some(specified_pipeline_id)
                }
                MaterialParameters::Attachments(specified_attachments) => {
                    attachments = specified_attachments
                }
                MaterialParameters::ImmediateData(data) => immediate_data = Some(data),
                MaterialParameters::IsEnabled(enabled_lit) => is_enabled = Some(enabled_lit),
                MaterialParameters::Ident(lit) => ident = Some(lit),
            }
        }

        let Some(pipeline_id) = pipeline_id else {
            return Err(input.error("A pipeline ID must be specified"));
        };

        Ok(MaterialDescriptor {
            pipeline_id,
            attachments,
            immediate_data,
            is_enabled,
            ident,
        })
    }
}

struct TransformedMaterialDescriptor {
    pipeline_id: usize,
    attachments: Vec<ShaderAttachmentDescriptor>,
    entities_attached: Vec<EntityId>,
    immediate_data: Option<Expr>,
    is_enabled: Option<LitBool>,
}

enum MaterialParameters {
    Pipeline(PipelineIdVariants),
    Attachments(Vec<ShaderAttachmentDescriptor>),
    IsEnabled(LitBool),
    ImmediateData(Expr),
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
            "immediate_data" => Ok(Self::ImmediateData(input.parse()?)),
            "is_enabled" => Ok(Self::IsEnabled(input.parse()?)),
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
            "immediate_data" => Ok(Self::ImmediateData(input.parse()?)),
            "is_enabled" => Ok(Self::IsEnabled(input.parse()?)),
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

impl PipelineIdVariants {
    fn initialize_and_get_id(
        self,
        input: ParseStream,
        idents: &mut HashMap<Lit, Id>,
        pipelines: &mut Vec<PipelineIdDescriptor>,
    ) -> syn::Result<usize> {
        match self {
            PipelineIdVariants::Ident(pipeline_ident) => match idents.get(&pipeline_ident) {
                Some(id) => {
                    if let Id::Pipeline(id) = *id {
                        Ok(id)
                    } else {
                        Err(syn::Error::new(
                            pipeline_ident.span(),
                            format!(
                                "Two objects share the same identifier: \"{pipeline_ident:?}\""
                            ),
                        ))
                    }
                }
                None => Err(syn::Error::new(
                    pipeline_ident.span(),
                    format!(
                        "The pipeline \"{pipeline_ident:?}\" could not be found. If you declared it, make sure it is declared above the current entity"
                    ),
                )),
            },
            PipelineIdVariants::Specifier(pipeline_id_descriptor) => {
                let pipeline_id = pipelines.len();
                if let Some(pipeline_ident) = &pipeline_id_descriptor.ident {
                    idents.insert(pipeline_ident.clone(), Id::Pipeline(pipeline_id));
                }
                pipelines.push(pipeline_id_descriptor.clone());

                Ok(pipeline_id)
            }
            PipelineIdVariants::ScreenSpace(_) => {
                Err(input.error("Screen-space materials are not valid here"))
            }
        }
    }
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
                    SimpleFieldValue::Group(group) => group.span(),
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
                    ));
                }
                "fragment_shader_path" => {
                    if let Some(value) = field.value {
                        match value {
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
                            rest => {
                                return Err(syn::Error::new_spanned(
                                    rest,
                                    "Only string literals are valid paths",
                                ));
                            }
                        }
                    }
                }
                "vertex_layouts" => {
                    return Err(syn::Error::new(
                        field.ident.span(),
                        "No vertex layouts should be specified for a screen-space effect",
                    ));
                }
                "uses_camera" => {
                    return Err(syn::Error::new(
                        field.ident.span(),
                        "No camera usage should be specified for a screen-space effect",
                    ));
                }
                "geometry_details" => {
                    return Err(syn::Error::new(
                        field.ident.span(),
                        "No camera usage should be specified for a screen-space effect",
                    ));
                }
                "ident" => {
                    return Err(syn::Error::new(
                        field.ident.span(),
                        "Identifiers are not valid here, as they can not be safely checked",
                    ));
                }
                _ => {
                    return Err(syn::Error::new_spanned(
                        field.ident,
                        "Invalid argument passed into the pipeline descriptor",
                    ));
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
                spirv_vertex_shader: false,
                fragment_shader: v4::engine_management::pipeline::PipelineShader::Path(#fragment_shader_path),
                spirv_fragment_shader: false,
                vertex_layouts: Vec::new(),
                uses_camera: false,
                is_screen_space: true,
                geometry_details: Default::default(),
                render_priority: i32::MAX,
            }
        });
    }
}

#[derive(Clone)]
struct PipelineIdDescriptor {
    vertex_shader_path: LitStr,
    spirv_vertex_shader: Option<LitBool>,
    fragment_shader_path: LitStr,
    spirv_fragment_shader: Option<LitBool>,
    vertex_layouts: Vec<ExprCall>,
    uses_camera: LitBool,
    geometry_details: Option<GeometryDetailsDescriptor>,
    immediate_size: Option<Expr>,
    render_priority: Option<Expr>,
    ident: Option<Lit>,
}

impl Parse for PipelineIdDescriptor {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let content;
        braced!(content in input);
        let fields = content.parse_terminated(SimpleField::parse, Token![,])?;
        let mut vertex_shader_path: Option<LitStr> = None;
        let mut spirv_vertex_shader: Option<LitBool> = None;
        let mut fragment_shader_path: Option<LitStr> = None;
        let mut spirv_fragment_shader: Option<LitBool> = None;
        let mut vertex_layouts: Vec<ExprCall> = Vec::new();
        let mut uses_camera: Option<LitBool> = None;
        let mut geometry_details: Option<GeometryDetailsDescriptor> = None;
        let mut immediate_size: Option<Expr> = None;
        let mut render_priority: Option<Expr> = None;
        let mut ident: Option<Lit> = None;

        for field in fields {
            match field.ident.to_string().as_str() {
                "vertex_shader_path" => {
                    if let Some(value) = field.value {
                        match value {
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
                            rest => {
                                return Err(syn::Error::new_spanned(
                                    rest,
                                    "Only string literals are valid paths",
                                ));
                            }
                        }
                    }
                }
                "spirv_vertex_shader" => {
                    if let Some(value) = field.value {
                        match value {
                            SimpleFieldValue::Literal(lit) => {
                                if let Lit::Bool(bool) = lit {
                                    spirv_vertex_shader = Some(bool);
                                } else {
                                    return Err(syn::Error::new(
                                        lit.span(),
                                        "Only boolean literals are valid here",
                                    ));
                                }
                            }
                            rest => {
                                return Err(syn::Error::new_spanned(
                                    rest,
                                    "Only boolean literals are valid here",
                                ));
                            }
                        }
                    }
                }
                "fragment_shader_path" => {
                    if let Some(value) = field.value {
                        match value {
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
                            rest => {
                                return Err(syn::Error::new_spanned(
                                    rest,
                                    "Only string literals are valid paths",
                                ));
                            }
                        }
                    }
                }
                "spirv_fragment_shader" => {
                    if let Some(value) = field.value {
                        match value {
                            SimpleFieldValue::Literal(lit) => {
                                if let Lit::Bool(bool) = lit {
                                    spirv_fragment_shader = Some(bool);
                                } else {
                                    return Err(syn::Error::new(
                                        lit.span(),
                                        "Only boolean literals are valid here",
                                    ));
                                }
                            }
                            rest => {
                                return Err(syn::Error::new_spanned(
                                    rest,
                                    "Only boolean literals are valid here",
                                ));
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
                            rest => {
                                return Err(syn::Error::new_spanned(
                                    rest,
                                    "Invalid value for vertex layout",
                                ));
                            }
                        }
                    }
                }
                "uses_camera" => {
                    if let Some(SimpleFieldValue::Literal(Lit::Bool(bool))) = field.value {
                        uses_camera = Some(bool);
                    }
                }
                "geometry_details" => {
                    if let Some(value) = field.value {
                        match value {
                            SimpleFieldValue::Group(expr) => {
                                let stream = quote! {#expr};
                                geometry_details =
                                    Some(parse2::<GeometryDetailsDescriptor>(stream)?);
                            }
                            SimpleFieldValue::Literal(lit) => {
                                return Err(syn::Error::new(
                                    lit.span(),
                                    "Invalid value for geometry details",
                                ));
                            }
                            SimpleFieldValue::Expression(expr) => {
                                return Err(syn::Error::new(
                                    expr.span(),
                                    "Invalid value for geometry details",
                                ));
                            }
                        }
                    }
                }
                "immediate_size" => {
                    if let Some(value) = field.value {
                        let expr = quote! {#value};
                        immediate_size = Some(parse2::<Expr>(expr)?);
                    }
                }
                "render_priority" => {
                    if let Some(SimpleFieldValue::Expression(priority)) = field.value {
                        render_priority = Some(priority);

                    }
                }
                "ident" => {
                    if let Some(SimpleFieldValue::Literal(lit)) = field.value {
                        ident = Some(lit);
                    }
                }
                _ => {
                    return Err(syn::Error::new_spanned(
                        field.ident,
                        "Invalid argument passed into the pipeline descriptor",
                    ));
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
            spirv_vertex_shader,
            fragment_shader_path,
            spirv_fragment_shader,
            vertex_layouts,
            uses_camera,
            geometry_details,
            immediate_size,
            render_priority,
            ident,
        })
    }
}

impl quote::ToTokens for PipelineIdDescriptor {
    fn to_tokens(&self, tokens: &mut TokenStream2) {
        let PipelineIdDescriptor {
            vertex_shader_path,
            spirv_vertex_shader,
            fragment_shader_path,
            spirv_fragment_shader,
            vertex_layouts,
            uses_camera,
            geometry_details,
            immediate_size,
            render_priority,
            ..
        } = self;
        let geometry_details = match geometry_details {
            Some(geo) => quote! {#geo},
            None => quote! {v4::engine_management::pipeline::GeometryDetails::default()},
        };
        let spirv_vertex_shader = if let Some(is_spirv) = spirv_vertex_shader {
            quote! {#is_spirv}
        } else {
            quote! {false}
        };
        let spirv_fragment_shader = if let Some(is_spirv) = spirv_fragment_shader {
            quote! {#is_spirv}
        } else {
            quote! {false}
        };

        let immediate_size = if let Some(expr) = immediate_size.as_ref() {
            quote! {#expr}
        } else {
            quote! {0}
        };

        let render_priority = if let Some(lit) = render_priority.as_ref() {
            quote! {#lit}
        } else {
            quote! {0}
        };

        tokens.extend(quote! {
            v4::engine_management::pipeline::PipelineId {
                vertex_shader: v4::engine_management::pipeline::PipelineShader::Path(#vertex_shader_path),
                spirv_vertex_shader: #spirv_vertex_shader,
                fragment_shader: v4::engine_management::pipeline::PipelineShader::Path(#fragment_shader_path),
                spirv_fragment_shader: #spirv_fragment_shader,
                vertex_layouts: vec![#(#vertex_layouts),*],
                uses_camera: #uses_camera,
                is_screen_space: false,
                geometry_details: #geometry_details,
                immediate_size: #immediate_size,
                render_priority: #render_priority,
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
        let fields = input.parse_terminated(SimpleField::parse, Token![,])?;

        let mut details = GeometryDetailsDescriptor::default();

        for field in fields {
            match field.ident.to_string().as_str() {
                "topology" => match field.value.unwrap() {
                    SimpleFieldValue::Expression(expr) => {
                        if let Expr::Path(path) = expr {
                            details.topology = Some(path);
                        }
                    }
                    rest => {
                        return Err(syn::Error::new_spanned(
                            rest,
                            "Invalid argument passed into geometry details topology field",
                        ));
                    }
                },
                "strip_index_format" => match field.value.unwrap() {
                    SimpleFieldValue::Expression(expr) => {
                        if let Expr::Path(path) = expr {
                            details.strip_index_format = Some(path);
                        }
                    }
                    rest => {
                        return Err(syn::Error::new_spanned(
                            rest,
                            "Invalid argument passed into geometry details strip index format field",
                        ));
                    }
                },
                "front_face" => match field.value.unwrap() {
                    SimpleFieldValue::Expression(expr) => {
                        if let Expr::Path(path) = expr {
                            details.front_face = Some(path);
                        }
                    }
                    rest => {
                        return Err(syn::Error::new_spanned(
                            rest,
                            "Invalid argument passed into geometry details front face field",
                        ));
                    }
                },
                "cull_mode" => match field.value.unwrap() {
                    SimpleFieldValue::Expression(expr) => {
                        if let Expr::Path(path) = expr {
                            details.cull_mode = Some(path);
                        }
                    }
                    rest => {
                        return Err(syn::Error::new_spanned(
                            rest,
                            "Invalid argument passed into geometry details cull mode field",
                        ));
                    }
                },
                "polygon_mode" => match field.value.unwrap() {
                    SimpleFieldValue::Expression(expr) => {
                        if let Expr::Path(path) = expr {
                            details.polygon_mode = Some(path);
                        }
                    }
                    rest => {
                        return Err(syn::Error::new_spanned(
                            rest,
                            "Invalid argument passed into geometry details polygon mode field",
                        ));
                    }
                },
                _ => {
                    return Err(syn::Error::new_spanned(
                        field.ident,
                        "Invalid argument passed into the pipeline geometry details descriptor",
                    ));
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

        match ident.to_string().as_str() {
            "Texture" => Ok(ShaderAttachmentDescriptor::Texture(input.parse()?)),
            "Buffer" => Ok(ShaderAttachmentDescriptor::Buffer(input.parse()?)),
            _ => Err(syn::Error::new(ident.span(), "Invalid material attachment")),
        }
    }
}

impl quote::ToTokens for ShaderAttachmentDescriptor {
    fn to_tokens(&self, tokens: &mut TokenStream2) {
        match self {
            ShaderAttachmentDescriptor::Texture(ShaderTextureAttachmentDescriptor {
                texture_bundle,
                visibility,
            }) => tokens.extend(quote! {
                v4::ecs::material::ShaderAttachment::Texture(
                    v4::ecs::material::ShaderTextureAttachment {
                        texture_bundle: #texture_bundle,
                        visibility: #visibility,
                    }
                )
            }),
            ShaderAttachmentDescriptor::Buffer(ShaderBufferAttachmentDescriptor {
                buffer,
                visibility,
            }) => tokens.extend(quote! {
                v4::ecs::material::ShaderBufferAttachment {
                    buffer: #buffer,
                    visibility: #visibility,
                }
            }),
        };
    }
}

struct ShaderTextureAttachmentDescriptor {
    texture_bundle: Expr,
    visibility: ExprPath,
}

impl Parse for ShaderTextureAttachmentDescriptor {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let content;
        parenthesized!(content in input);
        let fields = content.parse_terminated(SimpleField::parse, Token![,])?;
        let mut texture_bundle: Option<Expr> = None;
        let mut visibility: Option<ExprPath> = None;

        for field in fields {
            match field.ident.to_string().as_str() {
                "texture_bundle" => {
                    texture_bundle = match field.value {
                        Some(value) => Some(match value {
                            SimpleFieldValue::Expression(expr) => Ok(expr),
                            rest => Err(syn::Error::new_spanned(rest, "Invalid texture value")),
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
                            rest => Err(syn::Error::new_spanned(
                                rest,
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
            texture_bundle: texture_bundle.unwrap(),
            visibility: visibility.unwrap(),
        })
    }
}

struct ShaderBufferAttachmentDescriptor {
    buffer: Expr,
    visibility: ExprPath,
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
                            rest => Err(syn::Error::new_spanned(rest, "Invalid buffer value")),
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
                            rest => Err(syn::Error::new_spanned(
                                rest,
                                "Invalid buffer visibility value",
                            )),
                        }?),
                        None => None,
                    }
                }
                _ => {}
            }
        }
        Ok(ShaderBufferAttachmentDescriptor {
            buffer: buffer.unwrap(),
            visibility: visibility.unwrap(),
        })
    }
}
