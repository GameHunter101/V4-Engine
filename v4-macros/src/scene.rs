#![allow(unused)]
use darling::FromMeta;
use proc_macro::TokenStream;
use proc_macro2::Literal;
use quote::{quote, ToTokens};
use syn::{
    parenthesized,
    parse::{discouraged::AnyDelimiter, Parse, ParseStream},
    parse_macro_input, parse_quote,
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
                value = Some(SimpleFieldValue::Expression(expr));
            }
            if let Ok(lit) = input.parse::<Lit>() {
                value = Some(SimpleFieldValue::Literal(lit));
            }
        }

        Ok(SimpleField {
            ident,
            value,
        })
    }
}

impl Parse for ComponentDescriptor {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let component_type: Ident = input.parse()?;

        let content;
        parenthesized!(content in input);

        let params = content.parse_terminated(SimpleField::parse, Token![,])?.into_iter().collect();

        let mut component = ComponentDescriptor {
            component_type,
            params,
            ident: None,
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
    // Component creation
    let ComponentDescriptor {
        component_type,
        params,
        ident,
    } = parse_macro_input!(item as ComponentDescriptor);
    let builder_function_calls = params.into_iter().map(|param| {
        let SimpleField {
            ident,
            value,
        } = param;
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
    .into()
}
