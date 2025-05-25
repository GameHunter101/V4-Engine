use darling::FromMeta;
use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote, ToTokens};
use syn::{
    parse::Parser, parse_macro_input, punctuated::Punctuated, Expr, Field, GenericParam, Ident,
    ItemStruct, Meta, MetaNameValue, Token, Type, TypeParam,
};

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

    let builder_fields_partial: Vec<Field> = component_struct
        .fields
        .iter_mut()
        .map(|field| {
            let ty = if let Some(attr) = field.attrs.first() {
                if attr.path().is_ident("default") {
                    field.ty.clone()
                } else {
                    panic!(
                        "Invalid attribute for field {}",
                        field.ident.as_ref().unwrap()
                    )
                }
            } else {
                let field_ty = &field.ty;
                syn::Type::Verbatim(quote! {Option<#field_ty>})
            };

            Field {
                attrs: field.attrs.clone(),
                vis: syn::Visibility::Inherited,
                mutability: syn::FieldMutability::None,
                ident: field.ident.clone(),
                colon_token: field.colon_token,
                ty,
            }
        })
        .collect();

    let required_fields: Vec<&Field> = builder_fields_partial
        .iter()
        .filter(|field| field.attrs.is_empty())
        .collect();

    let optional_fields: Vec<&Field> = builder_fields_partial
        .iter()
        .filter(|field| !field.attrs.is_empty())
        .collect();

    let required_field_names: Vec<&Ident> = required_fields
        .iter()
        .map(|field| field.ident.as_ref().unwrap())
        .collect();

    let optional_field_names: Vec<&Ident> = optional_fields
        .iter()
        .map(|field| field.ident.as_ref().unwrap())
        .collect();

    let required_field_trait_idents: Vec<Ident> = required_fields
        .iter()
        .map(|field| {
            let field_ident = field.ident.as_ref().unwrap();
            format_ident!("Has{}", to_pascal_case(&field_ident.to_string()))
        })
        .collect();

    let builder_required_field_generics: Vec<TypeParam> = required_fields
        .iter()
        .map(|field| {
            let ident = field.ident.as_ref().unwrap();
            TypeParam::from_string(&to_pascal_case(&ident.to_string())).unwrap()
        })
        .collect();

    let field_defaults: Vec<TokenStream2> = component_struct
        .fields
        .iter_mut()
        .map(|field| {
            let field_ident = field.ident.clone().unwrap();
            let attrs = field.attrs.clone();
            field.attrs = Vec::new();
            if let Some(attr) = attrs.first() {
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

    // let test = required_fields.iter().map(|field| field.ty.to_token_stream());
    // panic!("{}", quote!{#(#test),*});
    let builder_required_methods: Vec<TokenStream2> = required_fields.iter().enumerate().map(|(i, field)| {
        let field_ident = field.ident.as_ref().unwrap();
        let original_type_string = field.ty.to_token_stream().to_string();
        let field_type = Type::from_string(&original_type_string[8..original_type_string.len()-1]).unwrap();

        let mut current_generics = generics.clone();
        let mut counter = 0;
        for index in 0..builder_required_field_generics.len() {
            if index != i {
                current_generics.params.insert(counter, GenericParam::Type(builder_required_field_generics[index].clone()));
                counter += 1;
            }
        }

        let mut current_generics_with_unset = generics.clone();
        current_generics_with_unset.params.extend(
            builder_required_field_generics
                .iter()
                .enumerate()
                .map(|(index, field)| {
                    if index == i {
                        GenericParam::Type(TypeParam::from_string("Unset").unwrap())
                    } else {
                        GenericParam::Type(field.clone())
                    }
                }),
        );

        let mut current_generics_with_set = generics.clone();
        current_generics_with_set.params.extend(
            builder_required_field_generics
                .iter()
                .enumerate()
                .map(|(index, field)| {
                    if index == i {
                        GenericParam::Type(TypeParam::from_string("Set").unwrap())
                    } else {
                        GenericParam::Type(field.clone())
                    }
                }),
        );

        let required_field_names: Vec<&&Ident> = required_field_names
            .iter()
            .filter(|name| name.to_string() != field_ident.to_string())
            .collect();

        quote! {
            impl #current_generics #builder_ident #current_generics_with_unset {
                fn #field_ident(self, #field_ident: #field_type) -> #builder_ident #current_generics_with_set {
                    #builder_ident {
                        #field_ident: Some(#field_ident),
                        #(#required_field_names: self.#required_field_names,)*
                        #(#optional_field_names: self.#optional_field_names,)*
                        id: self.id,
                        parent_entity_id: self.parent_entity_id,
                        is_initialized: self.is_initialized,
                        is_enabled: self.is_enabled,
                        _marker: std::marker::PhantomData,
                    }
                }
            }
        }
    }).collect();

    let builder_optional_methods: Vec<TokenStream2> = optional_fields
        .iter()
        .map(|field| {
            let field_ident = field.ident.as_ref().unwrap().to_string();
            let field_type = &field.ty;

            quote! {
                fn #field_ident(self, #field_ident: #field_type) -> Self {
                    Self { #field_ident, ..self }
                }
            }
        })
        .collect();

    let builder_required_field_generics_stream = quote! {#(#builder_required_field_generics,)*};

    let required_generics: Vec<GenericParam> = builder_required_field_generics
        .iter()
        .map(|field| GenericParam::Type(field.clone()))
        .collect();

    let mut builder_generics_unset = generics.clone();
    builder_generics_unset
        .params
        .extend(required_generics.iter().flat_map(|generic| {
            if let GenericParam::Type(_) = generic {
                Some(GenericParam::Type(TypeParam::from_string("Unset").unwrap()))
            } else {
                None
            }
        }));

    let mut builder_generics = generics.clone();
    builder_generics.params.extend(required_generics);

    let added_fields = [
        Field::parse_named
            .parse2(quote! {
                id: v4::ecs::component::ComponentId
            })
            .unwrap(),
        Field::parse_named
            .parse2(quote! {
                parent_entity_id: v4::ecs::entity::EntityId
            })
            .unwrap(),
        Field::parse_named
            .parse2(quote! {
                is_initialized: bool
            })
            .unwrap(),
        Field::parse_named
            .parse2(quote! {
                is_enabled: bool
            })
            .unwrap(),
    ];

    if let syn::Fields::Named(fields) = &mut component_struct.fields {
        fields
            .named
            .extend(Punctuated::<Field, Token![,]>::from_iter(
                added_fields.clone().into_iter(),
            ));
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

    let marker = Field::parse_named
        .parse2(quote! {
            _marker: std::marker::PhantomData<(#builder_required_field_generics_stream)>
        })
        .unwrap();

    let builder_fields = builder_fields_partial
        .into_iter()
        .map(|field| Field {
            attrs: Vec::new(),
            ..field
        })
        .chain(added_fields.into_iter())
        .chain(Some(marker).into_iter());

    let syn::Fields::Named(comp_fields) = &component_struct.fields else {
        panic!("Unnamed component is invalid");
    };

    let builder_struct = ItemStruct {
        ident: builder_ident.clone(),
        generics: builder_generics.clone(),
        fields: syn::Fields::Named(syn::FieldsNamed {
            brace_token: comp_fields.brace_token,
            named: Punctuated::from_iter(builder_fields),
        }),
        ..component_struct.clone()
    };

    quote! {
        #[derive(Debug)]
        #component_struct

        #builder_struct

        struct Set;
        struct Unset;

        #(trait #required_field_trait_idents {})*

        #(impl #required_field_trait_idents for Set {})*

        #(#builder_required_methods)*

        /* impl<#builder_required_field_generics_stream> #builder_ident #builder_generics {
            #(#builder_optional_methods)*

            pub fn enabled(self, enabled: bool) -> Self {
                Self {enabled, ..self}
            }

            pub fn id(self, id: v4::ecs::component::ComponentId) -> Self {
                Self {id, ..self}
            }
        }

        impl<#builder_required_field_generics_stream> #builder_ident #builder_generics
        where
            #(#builder_required_field_generics: #required_field_trait_idents,)* {
            fn build(self) -> #ident #impl_post_params {
                use std::hash::{DefaultHasher, Hash, Hasher};

                let mut hasher = DefaultHasher::new();

                std::time::Instant::now().hash(&mut hasher);

                let id = hasher.finish();

                #ident {
                    #(#required_field_names: self.#required_field_names.unwrap(),)*
                    #(#optional_field_names: self.#optional_field_names,)*
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

        impl #generics Default for #builder_ident #impl_post_params {
            fn default() -> Self {
                Self {
                    #(#field_defaults,)*
                    enabled: true,
                    id: 0,
                    _marker: std::marker::PhantomData,
                }
            }
        }

        impl #generics #ident #impl_post_params {
            pub fn builder() -> #builder_ident #builder_generics_unset {
                #builder_ident::default()
            }
        } */

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

fn to_pascal_case(str: &str) -> String {
    let chars: Vec<char> = str.chars().collect();
    chars
        .iter()
        .enumerate()
        .flat_map(|(i, c)| {
            if i == 0 || chars[i - 1] == '_' {
                c.to_uppercase().next()
            } else if *c == '_' {
                None
            } else {
                Some(*c)
            }
        })
        .collect::<String>()
}
