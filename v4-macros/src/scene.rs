#![allow(unused)]
use std::collections::HashMap;

use darling::FromMeta;
use proc_macro::TokenStream;
use quote::{format_ident, quote, ToTokens};
use syn::{
    braced, bracketed, parenthesized,
    parse::{discouraged::AnyDelimiter, Parse, ParseStream},
    parse_macro_input, parse_quote,
    punctuated::Punctuated,
    Expr, ExprCall, ExprLit, ExprPath, FieldValue, Ident, Lit, LitStr, Member, PatLit, PatPath,
    Token,
};
use v4_core::ecs::{component::ComponentId, entity::EntityId, material::MaterialId, scene::Scene};

pub struct SceneDescriptor {
    entities: Vec<TransformedEntityDescriptor>,
    idents: HashMap<Lit, Id>,
    relationships: HashMap<EntityId, Vec<EntityId>>,
    materials: Vec<MaterialDescriptor>,
}

struct TransformedEntityDescriptor {
    components: Vec<ComponentDescriptor>,
    material: Option<MaterialId>,
    parent: Option<EntityId>,
    id: EntityId,
    is_enabled: bool,
    ident: Option<Lit>,
}

struct EntityDescriptor {
    ident: Option<Lit>,
    components: Vec<ComponentDescriptor>,
    material: Option<MaterialDescriptor>,
    parent: Option<Lit>,
    is_enabled: bool,
}

enum EntityParameters {
    Components(Vec<ComponentDescriptor>),
    Material(MaterialDescriptor),
    Parent(Lit),
    Enabled(bool),
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

        let mut current_ident = 1;
        let transformed_entities = entities.into_iter().map(|entity| {

            let material_id = if let Some(material) = entity.material {
                if let Some(ident) = &material.ident {
                    idents.insert(ident.clone(), Id::Material(materials.len()));
                }
                materials.push(material);
                Some(materials.len() - 1)
            } else {
                None
            };

            let mut transformed_entity = TransformedEntityDescriptor {
                components: entity.components,
                material: material_id,
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
        })
    }
}

impl quote::ToTokens for SceneDescriptor {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        let entity_initializations = self.entities.iter().map(|entity| {
            let parent_id = match entity.parent {
                Some(id) => quote! {Some(#id)},
                None => quote! {None},
            };

            let component_initializations = entity.components.iter().map(|component| {
                let component_type = &component.component_type;
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
                    Box::new(#component_type::builder()#(#params)*#id_set.build())
                }
            });

            let material = if let Some(material_id) = entity.material {
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
                    _ => todo!(),
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

            #(#entity_initializations)*
        });
    }
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
    params: Vec<SimpleField>,
    ident: Option<Lit>,
}

impl Parse for ComponentDescriptor {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let component_type: Ident = input.parse()?;

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
                    .position(|param| param.value == Some(SimpleFieldValue::Literal(ident.clone())))
                    .unwrap(),
            );
        }

        let mut component = ComponentDescriptor {
            component_type,
            params,
            ident,
        };
        Ok(component)
    }
}

struct SimpleField {
    ident: Ident,
    value: Option<SimpleFieldValue>,
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

impl quote::ToTokens for SimpleFieldValue {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        tokens.extend(match self {
            SimpleFieldValue::Expression(expr) => quote! {#expr},
            SimpleFieldValue::Literal(lit) => quote! {#lit},
        });
    }
}

struct MaterialDescriptor {
    vertex_shader_path: LitStr,
    fragment_shader_path: LitStr,
    // TODO: textures: TextureDescriptor,
    ident: Option<Lit>,
}

impl Parse for MaterialDescriptor {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let params = input.parse_terminated(SimpleField::parse, Token![,])?;

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

        let mut material_descriptor = MaterialDescriptor {
            vertex_shader_path: LitStr::from_string("")?,
            fragment_shader_path: LitStr::from_string("")?,
            ident,
        };

        for param in params {
            match param.ident.to_string().as_str() {
                "vertex_shader_path" => {
                    if let Some(SimpleFieldValue::Literal(Lit::Str(str))) = param.value {
                        material_descriptor.vertex_shader_path = str;
                    } else {
                        return Err(syn::Error::new_spanned(
                            param.ident,
                            "Vertex shader path requires a string literal",
                        ));
                    }
                }
                "fragment_shader_path" => {
                    if let Some(SimpleFieldValue::Literal(Lit::Str(str))) = param.value {
                        material_descriptor.fragment_shader_path = str;
                    } else {
                        return Err(syn::Error::new_spanned(
                            param.ident,
                            "Fragment shader path requires a string literal",
                        ));
                    }
                }
                "ident" => material_descriptor.ident = input.parse()?,
                _ => {
                    return Err(syn::Error::new_spanned(
                        param.ident,
                        "Invalid argument passed into the material descriptor",
                    ));
                }
            }
        }
        Ok(material_descriptor)
    }
}

pub enum Id {
    Entity(EntityId),
    Component(ComponentId),
    Material(MaterialId),
}

impl quote::ToTokens for Id {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        tokens.extend(match self {
            Id::Entity(id) => quote! {#id},
            Id::Component(id) => quote! {#id},
            Id::Material(id) => quote! {#id},
        });
    }
}
