#![allow(unused)]
use std::collections::HashMap;

use darling::FromMeta;
use proc_macro::TokenStream;
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

struct SceneDescriptor {
    entities: Vec<FlattenedEntityDescriptor>,
    materials: HashMap<MaterialId, MaterialDescriptor>,
    idents: HashMap<Lit, Id>,
}

struct EntityDescriptor {
    ident: Option<Lit>,
    components: Vec<ComponentDescriptor>,
    material: Option<MaterialDescriptor>,
    children: Vec<EntityDescriptor>,
    parent: Option<Box<EntityDescriptor>>,
}

enum EntityParameters {
    Components(Vec<ComponentDescriptor>),
    Material(MaterialDescriptor),
    Children(Vec<EntityDescriptor>),
    Parent(EntityDescriptor),
}

#[derive(Default)]
struct FlattenedEntityDescriptor {
    ident: Option<Lit>,
    components: Vec<ComponentDescriptor>,
    material: Option<MaterialId>,
    children: Vec<EntityId>,
    parent: Option<EntityId>,
    id: EntityId,
}

impl Parse for SceneDescriptor {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let recursed_entities = input.parse_terminated(EntityDescriptor::parse, Token![,])?;
        let mut items_count = 0;
        let (idents_map, flattened_entities, materials): (
            HashMap<Lit, Id>,
            Vec<FlattenedEntityDescriptor>,
            HashMap<MaterialId, MaterialDescriptor>,
        ) = recursed_entities
            .into_iter()
            .map(|entity| parse_idents(entity, &mut items_count))
            .reduce(|(mut acc_id, mut acc_ent, mut acc_mat), (id, ent, mat)| {
                acc_id.extend(id);
                acc_ent.extend(ent);
                acc_mat.extend(mat);
                (acc_id, acc_ent, acc_mat)
            })
            .unwrap();

        Ok(Self {
            entities: flattened_entities,
            materials,
            idents: idents_map,
        })
    }
}

impl quote::ToTokens for SceneDescriptor {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        let entities_construction = self.entities.iter().map(|entity| {
            let parent = entity.parent;
            let components = entity.components.iter().map(|comp| {
                let comp_type = &comp.component_type;
                let component_params = comp.params.iter().map(|param| {
                    let name = &param.ident;
                    if let Some(value) = &param.value {
                        if let Some(ident) = value.get_ident() {
                            let id = self.idents.get(&ident).unwrap();
                            return quote! {.#name(#id)};
                        }
                    }
                    match &param.value {
                        Some(value) => quote! {.#name(#value)},
                        None => quote! {.#name(#name)},
                    }
                });

                let set_id = match &comp.ident {
                    Some(ident) => {
                        let id = self.idents.get(ident).unwrap();
                        quote! {.id(#id)}
                    }
                    None => quote! {},
                };

                quote! {
                    Box::new(#comp_type::builder()#(#component_params)*#set_id.build())
                }
            });

            /* let mat = if let Some(mat) = entity.material {
                quote! {
                    scene.create_material()
                }
            } else {
            }; */

            quote! {
                scene.create_entity(
                    #parent,
                    vec![#(#components),*],

                )
            }
        });

        tokens.extend(quote! {
            let mut scene = v4::ecs::scene::Scene::new();
        });
    }
}

fn flatten_entities(
    entity: EntityDescriptor,
    id_offset: EntityId,
) -> Vec<FlattenedEntityDescriptor> {
    let mut flattened = Vec::new();

    let mut num_children_flattened = 0;
    for child in entity.children {
        let mut flattened_children = flatten_entities(child, id_offset);
        for flattened_child in &mut flattened_children {
            flattened_child.id += num_children_flattened;
            num_children_flattened += 1;
        }
        flattened.extend(flattened_children);
    }

    let children_indices = (id_offset..=flattened.len() as EntityId).collect();

    let mut parent_id_offset = flattened.len() as EntityId;
    let parent = if let Some(parent) = entity.parent {
        let mut flattened_parent = flatten_entities(*parent, id_offset);
        for entity in &mut flattened_parent {
            entity.id += parent_id_offset;
            parent_id_offset += 1;
        }
        let parent_id = flattened_parent.last().unwrap().id;
        flattened_parent
            .last_mut()
            .unwrap()
            .children
            .push(parent_id + 1);
        flattened.extend(flattened_parent);
        Some(parent_id)
    } else {
        None
    };

    flattened.push(FlattenedEntityDescriptor {
        ident: entity.ident,
        components: entity.components,
        material: None,
        children: children_indices,
        parent,
        id: if let Some(last) = flattened.last() {
            last.id + 1
        } else {
            id_offset
        },
    });

    flattened
}

fn parse_idents(
    entity_descriptor: EntityDescriptor,
    id_count: &mut u32,
) -> (
    HashMap<Lit, Id>,
    Vec<FlattenedEntityDescriptor>,
    HashMap<MaterialId, MaterialDescriptor>,
) {
    todo!()
    /* let mut id_map = HashMap::new();
    let mut flattened_entities = Vec::new();
    let mut materials = HashMap::new();

    let mut this_flattened_entity = FlattenedEntityDescriptor::default();

    if let Some(this_ident) = entity_descriptor.ident {
        *id_count += 1;
        id_map.insert(this_ident.clone(), Id::Entity(*id_count as EntityId));
        this_flattened_entity.ident = Some(this_ident);
    }

    this_flattened_entity.components = entity_descriptor.components;

    for component in &this_flattened_entity.components {
        if let Some(ident) = &component.ident {
            *id_count += 1;
            id_map.insert(ident.clone(), Id::Component(*id_count as ComponentId));
        }
    }

    if let Some(material_descriptor) = entity_descriptor.material {
        if let Some(material_ident) = &material_descriptor.ident {
            *id_count += 1;
            id_map.insert(
                material_ident.clone(),
                Id::Material(*id_count as MaterialId),
            );

            materials.insert(*id_count as MaterialId, material_descriptor);
            this_flattened_entity.material = Some(*id_count as MaterialId);
        }
    }

    if let Some(parent_descriptor) = entity_descriptor.parent {
        let (parent_ids, parent_entities, parent_materials) =
            parse_idents(*parent_descriptor, id_count);
        id_map.extend(parent_ids);
        flattened_entities.extend(parent_entities);
        materials.extend(parent_materials);
    }

    for child_descriptor in entity_descriptor.children {
        let (child_ids, child_entities, child_materials) = parse_idents(child_descriptor, id_count);
        id_map.extend(child_ids);
        flattened_entities.extend(child_entities);
        materials.extend(child_materials);
    }

    (id_map, flattened_entities, materials) */
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
            children: Vec::new(),
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
                    entity_descriptor.children = children;
                }
                EntityParameters::Parent(parent) => {
                    entity_descriptor.parent = Some(Box::new(parent))
                }
            }
        }

        Ok(entity_descriptor)
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
