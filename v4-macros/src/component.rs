use darling::{ast::NestedMeta, FromMeta};
use proc_macro::TokenStream;
use quote::quote;
use syn::{parse::Parser, parse_macro_input, ItemStruct, Token, Visibility};

#[allow(unused)]
#[derive(Debug, FromMeta)]
struct ComponentSpecs {
    rendering_order: Option<i32>,
}

pub fn component_impl(args: TokenStream, item: TokenStream) -> TokenStream {
    let mut component_struct = parse_macro_input!(item as ItemStruct);

    let attr_args = match NestedMeta::parse_meta_list(args.into()) {
        Ok(v) => v,
        Err(e) => {
            return TokenStream::from(darling::Error::from(e).write_errors());
        }
    };

    let public_fields = if let syn::Fields::Named(ref mut fields) = component_struct.fields {
        fields.named.push(
            syn::Field::parse_named
                .parse2(quote! {
                    id: std::sync::OnceLock<v4::ecs::component::ComponentId>
                })
                .unwrap(),
        );

        fields.named.push(
            syn::Field::parse_named
                .parse2(quote! {
                    parent_entity_id: v4::ecs::entity::EntityId
                })
                .unwrap(),
        );

        fields.named.push(
            syn::Field::parse_named
                .parse2(quote! {
                    is_initialized: bool
                })
                .unwrap(),
        );

        fields.named.push(
            syn::Field::parse_named
                .parse2(quote! {
                    is_enabled: bool
                })
                .unwrap(),
        );

        fields
            .named
            .iter()
            .filter(|field| field.vis == Visibility::Public(syn::token::Pub::default()))
            .collect()
    } else {
        Vec::new()
    };

    let ident = component_struct.ident.clone();
    let generics = component_struct.generics.clone();

    #[allow(clippy::collapsible_match)]
    let rendering_order = if attr_args.is_empty() {
        0
    } else {
        match &attr_args[0] {
            NestedMeta::Lit(lit) => {
                if let syn::Lit::Int(lit_int) = lit {
                    lit_int.base10_parse().unwrap_or(0)
                } else {
                    0
                }
            }
            _ => 0,
        }
    };

    quote! {
        #component_struct

        impl #generics v4::ecs::component::ComponentDetails for #ident #generics {
            fn id(&self) -> v4::ecs::component::ComponentId {
                *self.id.get_or_init(|| {
                    const PRIME: u64 = 2147483647;
                    let address = self as *const _ as u64;
                    let obfuscated = (address).wrapping_mul(PRIME).rotate_left(16);
                    let new_id = (obfuscated & 0xFFFF_FFFF) as v4::ecs::component::ComponentId;
                    new_id
                })
            }

            fn is_initialized(&self) -> bool {
                self.is_initialized
            }

            fn set_initialized(&mut self) {
                self.is_initialized = true;
            }

            fn parent_entity_id(&self) -> v4::ecs::entity::EntityId {
                self.parent_entity_id
            }

            fn set_parent_entity(&mut self, parent_id: v4::ecs::entity::EntityId) {
                self.parent_entity_id = parent_id;
            }

            fn is_enabled(&self) -> bool {
                self.is_enabled
            }

            fn set_enabled_state(&mut self, enabled_state: bool) {
                self.is_enabled = enabled_state;
            }

            fn rendering_order(&self) -> i32 {
                #rendering_order
            }
        }
    }
    .into()
}
