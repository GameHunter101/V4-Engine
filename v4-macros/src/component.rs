use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{
    parse::Parser, parse_macro_input, punctuated::Punctuated, Expr, GenericParam, Ident,
    ItemStruct, Meta, MetaNameValue, Token, TypeParam,
};

// TODO: Default values specified through attributes should have const variables in the builder
// class created with said default values.
// const variable name: {member_name}_DEFAULT_VAL
// const must have type, can probably extract the type while iterating through component members to
// get the attribute. Store in a tuple array?
// Scene macro will then check to see if a value was specified for some member. If a value was not
// specified, then it will use the const default value. If no default value is specified for the
// member, then there will be a COMPILE TIME ERROR stating that {member_name}_DEFAULT_VAL does not
// exist

pub fn component_impl(args: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(args with Punctuated::<Meta, Token![,]>::parse_terminated);
    let rendering_order_expr: Option<Expr> = args
        .into_iter()
        .flat_map(|arg| {
            if let Meta::NameValue(MetaNameValue { path, value, .. }) = arg {
                if let Some(name) = path.get_ident() {
                    if &name.to_string() == "rendering_order" {
                        return Some(value);
                    }
                }
            }
            None
        })
        .next();

    let rendering_order = if let Some(expr) = rendering_order_expr {
        quote! {#expr}
    } else {
        quote! {0}
    };

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

    let field_defaults: Vec<TokenStream2> = component_struct
        .fields
        .iter_mut()
        .map(|field| {
            let field_ident = field.ident.clone().unwrap();
            if let Some(attr) = field.attrs.first() {
                if attr.path().is_ident("default") {
                    if let Ok(expr) = attr.parse_args::<Expr>() {
                        return quote! {#field_ident: Some(#expr)};
                    } else {
                        return quote! {#field_ident: Some(Default::default())};
                    };
                } else {
                    panic!("Invalid attribute for field {field_ident}");
                }
            }
            return quote! {#field_ident: None};
        })
        .collect();

    let builder_field_idents: Vec<Option<Ident>> = component_struct
        .fields
        .iter()
        .map(|field| field.ident.clone())
        .collect();

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

    match &mut component_struct.fields {
        syn::Fields::Named(fields_named) => fields_named
            .named
            .iter_mut()
            .for_each(|field| field.attrs = Vec::new()),
        syn::Fields::Unnamed(fields_unnamed) => fields_unnamed
            .unnamed
            .iter_mut()
            .for_each(|field| field.attrs = Vec::new()),
        _ => {}
    }

    let impl_post_params: Punctuated<GenericParam, Token![,]> = generics
        .params
        .clone()
        .into_iter()
        .map(|generic| match generic {
            GenericParam::Lifetime(lifetime_param) => GenericParam::Lifetime(lifetime_param),
            GenericParam::Type(mut type_param) => {
                type_param.colon_token = None;
                type_param.bounds.clear();
                type_param.eq_token = None;
                type_param.default = None;
                GenericParam::Type(type_param)
            }
            GenericParam::Const(const_param) => {
                let param = TypeParam {
                    attrs: Vec::new(),
                    ident: const_param.ident,
                    colon_token: None,
                    bounds: Punctuated::new(),
                    eq_token: None,
                    default: None,
                };
                GenericParam::Type(param)
            }
        })
        .collect();

    let impl_post_params = if impl_post_params.is_empty() {
        quote! {}
    } else {
        quote! {<#impl_post_params>}
    };

    quote! {
        #[derive(Debug)]
        #component_struct

        pub struct #builder_ident #generics {
            #(#builder_fields,)*
            enabled: bool,
            id: v4::ecs::component::ComponentId,
        }

        impl #generics Default for #builder_ident #impl_post_params {
            fn default() -> Self {
                Self {
                    #(#field_defaults,)*
                    enabled: true,
                    id: 0,
                }
            }
        }

        impl #generics #builder_ident #impl_post_params {
            // #(#const_defaults)*

            #(#builder_methods)*

            pub fn enabled(mut self, enabled: bool) -> Self {
                self.enabled = enabled;
                self
            }

            pub fn id(mut self, id: v4::ecs::component::ComponentId) -> Self {
                self.id = id;
                self
            }

            pub fn build(self) -> #ident #impl_post_params {
                use std::hash::{DefaultHasher, Hash, Hasher};

                let mut hasher = DefaultHasher::new();

                std::time::Instant::now().hash(&mut hasher);

                let id = hasher.finish();

                #ident {
                    #(#builder_field_idents: self.#builder_field_idents.unwrap(),)*
                    id:
                    if self.id == 0 {
                        id
                    } else {
                        self.id
                    },
                    parent_entity_id: 0,
                    is_initialized: false,
                    is_enabled: self.enabled,
                }
            }
        }

        impl #generics #ident #impl_post_params {
            pub fn builder() -> #builder_ident #impl_post_params {
                #builder_ident::default()
            }
        }

        impl #generics v4::ecs::component::ComponentDetails for #ident #impl_post_params {
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
