#![allow(unused)]
use std::collections::HashMap;

use darling::FromMeta;
use proc_macro::TokenStream;
use proc_macro2::Literal;
use quote::{quote, ToTokens};
use syn::{
    braced, bracketed, parenthesized,
    parse::{discouraged::AnyDelimiter, Parse, ParseStream},
    parse_macro_input, parse_quote,
    punctuated::Punctuated,
    Expr, ExprCall, ExprLit, ExprPath, FieldValue, Ident, Lit, LitStr, Member, PatLit, PatPath,
    Token,
};
use v4_core::ecs::{component::ComponentId, entity::EntityId, material::MaterialId, scene::Scene};

#[derive(Debug)]
struct SceneDescriptor {
    entities: Vec<EntityDescriptor>,
}

impl Parse for SceneDescriptor {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut entities = Vec::new();
        while let Ok(entity_ident) = input.parse::<Lit>() {
            let entity = EntityDescriptor {
                components: todo!(),
                material: todo!(),
                children: todo!(),
                parent: todo!(),
                ident: if entity_ident == Lit::new(Literal::string("_")) {
                    None
                } else {
                    Some(entity_ident)
                },
            };
            entities.push(entity_ident);
        }

        Ok(SceneDescriptor {
            entities: Vec::new(),
        })
    }
}

#[derive(Debug)]
struct EntityDescriptor {
    ident: Option<Lit>,
    components: Vec<ComponentDescriptor>,
    material: Option<MaterialDescriptor>,
    children: Option<Vec<EntityDescriptor>>,
    parent: Option<Box<EntityDescriptor>>,
}

enum EntityParameters {
    Components(Vec<ComponentDescriptor>),
    Material(MaterialDescriptor),
    Children(Vec<EntityDescriptor>),
    Parent(EntityDescriptor),
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
            "children" => {
                let content;
                bracketed!(content in input);
                let entities = content.parse_terminated(EntityDescriptor::parse, Token![,])?;
                Ok(EntityParameters::Children(entities.into_iter().collect()))
            }
            "parent" => Ok(EntityParameters::Parent(input.parse()?)),
            _ => Err(syn::Error::new_spanned(
                param_type,
                "Invalid argument passed into the entity descriptor.",
            )),
        }
    }
}

impl Parse for EntityDescriptor {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let raw_ident: Lit = input.parse()?;
        let mut ident = match raw_ident {
            Lit::Str(lit_str) => {
                if lit_str.value() == *"_" {
                    None
                } else {
                    Some(Lit::Str(lit_str))
                }
            }
            lit => Some(lit),
        };

        let mut entity_descriptor = EntityDescriptor {
            ident,
            components: Vec::new(),
            material: None,
            children: None,
            parent: None,
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
                EntityParameters::Children(children) => {
                    entity_descriptor.children = Some(children);
                }
                EntityParameters::Parent(parent) => {
                    entity_descriptor.parent = Some(Box::new(parent))
                }
            }
        }

        Ok(entity_descriptor)
    }
}

#[derive(Debug)]
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

        let params = content
            .parse_terminated(SimpleField::parse, Token![,])?
            .into_iter()
            .collect();

        let mut component = ComponentDescriptor {
            component_type,
            params,
            ident: None,
        };
        Ok(component)
    }
}

#[derive(Debug)]
enum SimpleFieldValue {
    Expression(Expr),
    Literal(Lit),
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

#[derive(Debug)]
struct MaterialDescriptor {
    vertex_shader_path: LitStr,
    fragment_shader_path: LitStr,
    // TODO: textures: TextureDescriptor,
    ident: Option<Lit>,
}

impl Parse for MaterialDescriptor {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let params = input.parse_terminated(SimpleField::parse, Token![,])?;
        let mut material_descriptor = MaterialDescriptor {
            vertex_shader_path: LitStr::from_string("")?,
            fragment_shader_path: LitStr::from_string("")?,
            ident: None,
        };
        for param in params {
            match param.ident.to_string().as_str() {
                "vertex_shader_path" => {
                    if let Some(SimpleFieldValue::Literal(Lit::Str(str))) = param.value {
                        material_descriptor.vertex_shader_path = str;
                    } else {
                        return Err(syn::Error::new_spanned(
                            param.ident,
                            "Vertex shader path requires a string literal.",
                        ));
                    }
                }
                "fragment_shader_path" => {
                    if let Some(SimpleFieldValue::Literal(Lit::Str(str))) = param.value {
                        material_descriptor.fragment_shader_path = str;
                    } else {
                        return Err(syn::Error::new_spanned(
                            param.ident,
                            "Fragment shader path requires a string literal.",
                        ));
                    }
                }
                "ident" => material_descriptor.ident = input.parse()?,
                _ => {
                    return Err(syn::Error::new_spanned(
                        param.ident,
                        "Invalid argument passed into the material descriptor.",
                    ));
                }
            }
        }
        Ok(material_descriptor)
    }
}

pub fn scene_impl(item: TokenStream) -> TokenStream {
    // Component creation
    let ComponentDescriptor {
        component_type,
        mut params,
        ident,
    } = parse_macro_input!(item as ComponentDescriptor);

    let mut identifier: Option<Lit> = None;

    let mut removal_index = None;

    for (i, param) in params.iter().enumerate() {
        if let Some(SimpleFieldValue::Expression(Expr::Call(ExprCall { func, args, .. }))) =
            &param.value
        {
            if let Expr::Path(ExprPath { path, .. }) = *func.clone() {
                if let Some(possible_ident) = path.get_ident() {
                    if &possible_ident.to_string() == "ident" {
                        if let Some(Expr::Lit(lit)) = args.first() {
                            identifier = Some(lit.lit.clone());
                            removal_index = Some(i);
                        }
                    }
                }
            }
        }
    }

    if let Some(index) = removal_index {
        params.remove(index);
    }

    let builder_function_calls = params.into_iter().map(|param| {
        let SimpleField { ident, value } = param;
        if let Some(value) = value {
            match value {
                SimpleFieldValue::Expression(expr) => quote! {.#ident(#expr)},
                SimpleFieldValue::Literal(lit) => quote! {.#ident(#lit)},
            }
        } else {
            quote! {.#ident(#ident)}
        }
    });

    if let Some(ident) = identifier {
        quote! {#ident}.into()
    } else {
        quote! {
            5
        }
        .into()
    }

    /* quote! {
        #component_type::builder()
        #(#builder_function_calls)*.build()
    }
    .into() */

    /* let entity = parse_macro_input!(item as EntityDescriptor);

    let idents_map = parse_idents(&entity, &mut 0);

    let EntityDescriptor { ident, components, material, children, parent } = entity;

    let components_construction: Vec<proc_macro2::TokenStream> = components
        .into_iter()
        .map(|component| {
            let ComponentDescriptor {
                component_type,
                params,
                ident,
            } = component;

            let builder_function_calls = params.into_iter().map(|param| {
                let SimpleField { ident, value } = param;
                if let Some(value) = value {
                    match value {
                        SimpleFieldValue::Expression(expr) => quote! {.#ident(#expr)},
                        SimpleFieldValue::Literal(lit) => quote! {.#ident(#lit)},
                    }
                } else {
                    quote! {.#ident(#ident)}
                }
            });

            quote! {
                #component_type::builder()
                #(#builder_function_calls)*.build()
            }
        })
        .collect();

    quote! {
        scene.create_entity()
        (#(#components_construction,)*)
    }
    .into() */
}

#[derive(Debug)]
pub enum Id {
    Entity(EntityId),
    Component(ComponentId),
    Material(MaterialId),
}

fn parse_idents(entity_descriptor: &EntityDescriptor, id_count: &mut u32) -> HashMap<Lit, Id> {
    let mut map = HashMap::new();

    if let Some(this_ident) = &entity_descriptor.ident {
        *id_count += 1;
        map.insert(this_ident.clone(), Id::Entity(*id_count as EntityId));
    }

    for component in &entity_descriptor.components {
        if let Some(ident) = &component.ident {
            *id_count += 1;
            map.insert(ident.clone(), Id::Component(*id_count as ComponentId));
        }
    }

    if let Some(parent_descriptor) = &entity_descriptor.parent {
        map.extend(parse_idents(parent_descriptor, id_count));
    }

    if let Some(children) = &entity_descriptor.children {
        for child_descriptor in children {
            map.extend(parse_idents(child_descriptor, id_count));
        }
    }

    if let Some(material_descriptor) = &entity_descriptor.material {
        if let Some(material_ident) = &material_descriptor.ident {
            *id_count += 1;
            map.insert(
                material_ident.clone(),
                Id::Material(*id_count as MaterialId),
            );
        }
    }

    map
}
