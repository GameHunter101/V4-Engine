use darling::{ast::NestedMeta, FromMeta};
use proc_macro::TokenStream;
use quote::quote;
use syn::{
    ext::IdentExt, parse::Parser, parse_macro_input, spanned::Spanned, DeriveInput, Error, Expr,
    Item, ItemStruct, Lit, Meta, MetaNameValue,
};
use v4_core::ecs::entity::EntityId;

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

    if let syn::Fields::Named(ref mut fields) = component_struct.fields {
        // let test = syn::parse_quote! {}
        fields.named.push(
            syn::Field::parse_named
                .parse2(quote! {
                    parent_entity_id: v4_core::ecs::entity::EntityId
                })
                .unwrap(),
        );

        fields.named.push(
            syn::Field::parse_named
                .parse2(quote! {
                    component_id: v4_core::ecs::component::ComponentId
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
    }

    let ident = component_struct.ident.clone();
    let generics = component_struct.generics.clone();

    quote! {
        #component_struct

        impl #generics v4_core::ecs::component::ComponentDetails for #ident #generics {
            fn id(&self) -> v4_core::ecs::component::ComponentId {
                self.component_id
            }

            fn set_id(&mut self, new_id: v4_core::ecs::component::ComponentId) {
                self.component_id = new_id;
            }

            fn is_initialized(&self) -> bool {
                self.is_initialized
            }

            fn parent_entity_id(&self) -> v4_core::ecs::entity::EntityId {
                self.parent_entity_id
            }

            fn set_parent_entity(&mut self, parent_id: v4_core::ecs::entity::EntityId) {
                self.parent_entity_id = parent_id;
            }

            fn is_enabled(&self) -> bool {
                self.is_enabled
            }

            fn set_enabled_state(&mut self, enabled_state: bool) {
                self.is_enabled = enabled_state;
            }
        }
    }
    .into()
}
