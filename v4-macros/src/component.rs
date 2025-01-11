use darling::{ast::NestedMeta, FromMeta};
use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{parse::Parser, parse_macro_input, Ident, ItemStruct};

#[allow(unused)]
#[derive(Debug, FromMeta)]
struct ComponentSpecs {
    rendering_order: Option<i32>,
}

pub fn component_impl(args: TokenStream, item: TokenStream) -> TokenStream {
    let mut component_struct = parse_macro_input!(item as ItemStruct);

    let ident = component_struct.ident.clone();
    let generics = component_struct.generics.clone();

    let builder_ident = format_ident!("{}Builder", ident.to_string());

    let (builder_fields, builder_methods): (
        Vec<proc_macro2::TokenStream>,
        Vec<proc_macro2::TokenStream>,
    ) = component_struct
        .fields
        .iter()
        .map(|field| {
            let field_ident = &field.ident;
            let ty = &field.ty;
            (
                quote! {#field_ident: Option<#ty>},
                quote! {
                    pub fn #field_ident(mut self, #field_ident: #ty) -> Self {
                        self.#field_ident = Some(#field_ident);
                        self
                    }
                },
            )
        })
        .collect();

    let builder_field_idents: Vec<Option<Ident>> = component_struct
        .fields
        .iter()
        .map(|field| field.ident.clone())
        .collect();

    let attr_args = match NestedMeta::parse_meta_list(args.into()) {
        Ok(v) => v,
        Err(e) => {
            return TokenStream::from(darling::Error::from(e).write_errors());
        }
    };

    if let syn::Fields::Named(fields) = &mut component_struct.fields {
        fields.named.push(
            syn::Field::parse_named
                .parse2(quote! {
                    id: v4::ecs::component::ComponentId
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
    }

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

        pub struct #builder_ident #generics {
            #(#builder_fields,)*
            enabled: bool,
            id: v4::ecs::component::ComponentId,
        }

        impl #generics Default for #builder_ident #generics {
            fn default() -> Self {
                Self {
                    #(#builder_field_idents: None,)*
                    enabled: true,
                    id: 0,
                }
            }
        }

        impl #generics #builder_ident #generics {
            #(#builder_methods)*

            fn enabled(mut self, enabled: bool) -> Self {
                self.enabled = enabled;
                self
            }

            fn id(mut self, id: v4::ecs::component::ComponentId) -> Self {
                self.id = id;
                self
            }

            fn build(self) -> #ident #generics {
                use std::hash::{DefaultHasher, Hash, Hasher};

                let mut hasher = DefaultHasher::new();
                file!().hash(&mut hasher);
                let file = (hasher.finish() & v4::ecs::component::ComponentId::MAX as u64) as v4::ecs::component::ComponentId;
                let line = line!();

                #ident {
                    #(#builder_field_idents: self.#builder_field_idents.unwrap(),)*
                    id:
                        if self.id == 0 {
                            file + line
                        } else {
                            self.id
                        },
                    parent_entity_id: 0,
                    is_initialized: false,
                    is_enabled: self.enabled,
                }
            }
        }

        impl #generics #ident #generics {
            fn builder() -> #builder_ident #generics {
                #builder_ident::default()
            }
        }

        impl #generics v4::ecs::component::ComponentDetails for #ident #generics {
            fn id(&self) -> v4::ecs::component::ComponentId {
                self.id
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
