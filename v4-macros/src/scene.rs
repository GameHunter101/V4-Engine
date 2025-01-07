#![allow(unused)]
use darling::FromMeta;
use proc_macro::TokenStream;
use proc_macro2::Literal;
use quote::quote;
use syn::{
    parenthesized,
    parse::{discouraged::AnyDelimiter, Parse, ParseStream},
    parse_macro_input,
    punctuated::Punctuated,
    Expr, FieldValue, Ident, Lit, LitStr, Member, Token,
};
use v4_core::ecs::scene::Scene;

#[derive(Debug)]
struct SceneDescriptor<'a> {
    entities: Vec<EntityDescriptor<'a>>,
}

#[derive(Debug)]
struct EntityDescriptor<'a> {
    components: ComponentDescriptor,
    material: Option<MaterialDescriptor>,
    children: Option<Vec<&'a EntityDescriptor<'a>>>,
    parent: Option<&'a EntityDescriptor<'a>>,
    ident: Option<Lit>,
}

#[derive(Debug)]
struct MaterialDescriptor {
    vertex_shader_path: LitStr,
    fragment_shader_path: LitStr,
    // TODO: textures: TextureDescriptor,
    ident: Option<Lit>,
}

#[derive(Debug)]
struct ComponentDescriptor {
    component_type: Ident,
    params: Vec<SimpleField>,
    ident: Option<Lit>,
}

#[derive(Debug)]
struct SimpleField {
    member: Member,
    colon: Option<Token![:]>,
    expr: Expr,
}

fn parse_arguments_and_ident(input: ParseStream) -> syn::Result<(Vec<SimpleField>, Option<Lit>)> {
    let mut fields = Vec::new();
    let mut ident = None;
    loop {
        let member: Member = input.parse()?;
        let colon = if input.peek(Token![:]) {
            Some(input.parse::<Token![:]>()?)
        } else {
            None
        };

        if input.peek(Lit) {
            ident = Some(input.parse::<Lit>()?);
        } else {
            fields.push(SimpleField {
                member,
                colon,
                expr: input.parse()?,
            })
        }
    }

    Ok((fields, ident))
}

impl Parse for ComponentDescriptor {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let component_type = input.parse()?;
        let (_, _, args_contents) = input.parse_any_delimiter()?;
        let (params, ident) = parse_arguments_and_ident(&args_contents)?;

        let mut component = ComponentDescriptor {
            component_type,
            params,
            ident,
        };
        Ok(component)
    }
}

impl Parse for SceneDescriptor<'_> {
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

pub fn scene_impl(item: TokenStream) -> TokenStream {
    /* let scene_descriptor = parse_macro_input!(item as SceneDescriptor);
    let scene = Scene::new(scene_index, device, queue, format) */
    // quote! { #scene_descriptor}.into()
    // item
    let temp = parse_macro_input!(item as Expr);
    quote! {
        #temp.thing
    }.into()
}
