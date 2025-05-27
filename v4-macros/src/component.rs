use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{
    parse::Parser, parse_macro_input, punctuated::Punctuated, Expr, Field, GenericParam, Generics,
    Ident, ItemStruct, Meta, MetaNameValue, Token, Type, TypeParam,
};

pub fn component_impl(args: TokenStream, item: TokenStream) -> TokenStream {
    let rendering_order = get_rendering_order(
        parse_macro_input!(args with Punctuated::<Meta, Token![,]>::parse_terminated),
    );
    let mut component_struct = parse_macro_input!(item as ItemStruct);

    let ident = component_struct.ident.clone();
    let generics = component_struct.generics.clone();

    let builder_ident = format_ident!("{}Builder", ident.to_string());

    let builder = builder_construction(builder_ident, &component_struct);

    component_struct
        .fields
        .iter_mut()
        .for_each(|field| field.attrs = Vec::new());

    let added_component_fields = [
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
                added_component_fields.clone().into_iter(),
            ));
    }

    let component_generics = remove_generics_bounds(&generics);

    quote! {
        #[derive(Debug)]
        #component_struct

        #builder

        impl #generics v4::ecs::component::ComponentDetails for #ident #component_generics {
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

fn get_rendering_order(args: Punctuated<Meta, Token![,]>) -> TokenStream2 {
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

    if let Some(expr) = rendering_order_expr {
        quote! {#expr}
    } else {
        quote! {0}
    }
}

fn builder_struct_construction(
    builder_ident: Ident,
    builder_required_field_generics: &[TypeParam],
    builder_generics: Generics,
    component_struct: ItemStruct,
) -> ItemStruct {
    let syn::Fields::Named(comp_fields) = &component_struct.fields else {
        panic!("Unnamed struct fields are invalid");
    };

    let builder_fields_partial: Vec<Field> = component_struct
        .fields
        .iter()
        .map(|field| {
            if field.attrs.is_empty() {
                let field_ty = &field.ty;
                Field {
                    ty: Type::Verbatim(quote! {Option<#field_ty>}),
                    ..field.clone()
                }
            } else {
                Field {
                    attrs: Vec::new(),
                    ..field.clone()
                }
            }
        })
        .collect();

    let added_builder_fields = [
        Field::parse_named
            .parse2(quote! {
                id: v4::ecs::component::ComponentId
            })
            .unwrap(),
        Field::parse_named
            .parse2(quote! {
                is_enabled: bool
            })
            .unwrap(),
        Field::parse_named
            .parse2(quote! {
                _marker: std::marker::PhantomData<(#(#builder_required_field_generics),*)>
            })
            .unwrap(),
    ];

    let builder_fields = builder_fields_partial
        .iter()
        .map(|field| Field {
            attrs: Vec::new(),
            ..field.clone()
        })
        .chain(added_builder_fields.into_iter());

    ItemStruct {
        ident: builder_ident,
        generics: builder_generics,
        fields: syn::Fields::Named(syn::FieldsNamed {
            brace_token: comp_fields.brace_token,
            named: Punctuated::from_iter(builder_fields),
        }),
        vis: syn::Visibility::Public(syn::token::Pub::default()),
        ..component_struct.clone()
    }
}

struct TypedBuilderTypes {
    set_type_ident: Ident,
    set_type: TypeParam,
    unset_type_ident: Ident,
    unset_type: TypeParam,
}

fn create_typed_builder_types(builder_name: String) -> TypedBuilderTypes {
    let set_type_ident = format_ident!("{builder_name}Set");
    let set_type = TypeParam {
        ident: set_type_ident.clone(),
        attrs: Vec::new(),
        colon_token: None,
        bounds: Punctuated::new(),
        eq_token: None,
        default: None,
    };

    let unset_type_ident = format_ident!("{builder_name}Unset");
    let unset_type = TypeParam {
        ident: unset_type_ident.clone(),
        attrs: Vec::new(),
        colon_token: None,
        bounds: Punctuated::new(),
        eq_token: None,
        default: None,
    };

    TypedBuilderTypes {
        set_type_ident,
        set_type,
        unset_type_ident,
        unset_type,
    }
}

/// (Required, Optional)
fn separate_required_and_optional_fields<'a>(
    all_fields: &[&'a Field],
) -> (Vec<&'a Field>, Vec<&'a Field>) {
    let basic_separation: (Vec<Option<&Field>>, Vec<Option<&Field>>) = all_fields
        .iter()
        .map(|field| {
            if field.attrs.is_empty() {
                (Some(*field), None)
            } else {
                (None, Some(*field))
            }
        })
        .unzip();

    let flattened_separation: Vec<Vec<&Field>> = [basic_separation.0, basic_separation.1]
        .into_iter()
        .map(|vec| vec.into_iter().flatten().collect::<Vec<&Field>>())
        .collect();
    (
        flattened_separation[0].clone(),
        flattened_separation[1].clone(),
    )
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

fn generics_helper(generic_params: impl IntoIterator<Item = GenericParam>) -> Generics {
    Generics {
        lt_token: None,
        params: Punctuated::from_iter(generic_params),
        gt_token: None,
        where_clause: None,
    }
}

fn remove_generics_bounds(generics: &Generics) -> Generics {
    generics_helper(generics.params.iter().map(|generic| {
        if let GenericParam::Type(type_param) = generic {
            GenericParam::Type(TypeParam {
                bounds: Punctuated::new(),
                ..type_param.clone()
            })
        } else if let GenericParam::Const(const_param) = generic {
            GenericParam::Type(TypeParam {
                attrs: Vec::new(),
                ident: const_param.ident.clone(),
                colon_token: None,
                bounds: Punctuated::new(),
                eq_token: None,
                default: None,
            })
        } else {
            generic.clone()
        }
    }))
}

/// Required, Required + Component, Unset + Component
fn generics_constructor(
    required_fields_idents: &[Ident],
    component_struct: &ItemStruct,
    unset_type: TypeParam,
) -> (Vec<TypeParam>, [Generics; 2]) {
    let required_fields_generics: Vec<TypeParam> = required_fields_idents
        .iter()
        .map(|ident| TypeParam {
            attrs: Vec::new(),
            ident: ident.clone(),
            colon_token: None,
            bounds: Punctuated::new(),
            eq_token: None,
            default: None,
        })
        .collect();

    let component_generics = &component_struct.generics;

    let required_and_component_generics = generics_helper(
        required_fields_generics
            .iter()
            .map(|generic_type| GenericParam::Type(generic_type.clone()))
            .chain(component_generics.params.iter().cloned()),
    );

    let unset_and_component_generics = generics_helper(
        required_fields_generics
            .iter()
            .map(|_| GenericParam::Type(unset_type.clone()))
            .chain(component_generics.params.iter().cloned()),
    );

    (
        required_fields_generics,
        [
            required_and_component_generics,
            remove_generics_bounds(&unset_and_component_generics),
        ],
    )
}

fn builder_required_methods_constructor(
    required_fields: &[&Field],
    optional_fields_idents: &[&Ident],
    full_builder_generics: &Generics,
    unset_type: &TypeParam,
    set_type: &TypeParam,
    builder_ident: Ident,
) -> Vec<TokenStream2> {
    required_fields.iter().enumerate().map(|(i, field)| {
        let field_ident = field.ident.as_ref().unwrap();
        let field_type = &field.ty;
        let current_excluded_generics = generics_helper(full_builder_generics.params.iter().enumerate().flat_map(|(index, generic)| {
            if index == i {
                None
            } else {
                Some(generic.clone())
            }
        }));

        let current_unset_generics = remove_generics_bounds(
            &generics_helper(full_builder_generics.params.iter().enumerate().map(|(index, generic)| {
            if index == i {
                GenericParam::Type(unset_type.clone())
            } else {
                generic.clone()
            }}))
        );

        let current_set_generics = remove_generics_bounds(&generics_helper(full_builder_generics.params.iter().enumerate().map(|(index, generic)| {
            if index == i {
                GenericParam::Type(set_type.clone())
            } else {
                generic.clone()
            }
        })));

    let required_fields_idents: Vec<&Ident> = required_fields
        .iter().enumerate()
        .flat_map(|(index, field)| if index == i {None} else { Some(field.ident.as_ref().unwrap())})
        .collect();

        quote! {
            impl #current_excluded_generics #builder_ident #current_unset_generics {
                pub fn #field_ident(self, #field_ident: #field_type) -> #builder_ident #current_set_generics {
                    #builder_ident {
                        #field_ident: Some(#field_ident),
                        #(#required_fields_idents: self.#required_fields_idents,)*
                        #(#optional_fields_idents: self.#optional_fields_idents,)*
                        id: self.id,
                        is_enabled: self.is_enabled,
                        _marker: std::marker::PhantomData,
                    }
                }
            }
        }
    }).collect()
}

fn builder_optional_methods_constructor(optional_fields: &[&Field]) -> Vec<TokenStream2> {
    optional_fields
        .iter()
        .map(|field| {
            let field_ident = field.ident.as_ref().unwrap();
            let field_type = &field.ty;

            quote! {
                pub fn #field_ident(self, #field_ident: #field_type) -> Self {
                Self { #field_ident, ..self }
                }
            }
        })
        .collect()
}

fn fields_to_idents<'a>(fields: &[&'a Field]) -> Vec<&'a Ident> {
    fields
        .iter()
        .map(|field| field.ident.as_ref().unwrap())
        .collect()
}

/// Required, Optional
fn builder_methods_constructor(
    required_fields: &[&Field],
    optional_fields: &[&Field],
    full_builder_generics: &Generics,
    unset_type: &TypeParam,
    set_type: &TypeParam,
    builder_ident: Ident,
) -> TokenStream2 {
    let optional_fields_idents: Vec<&Ident> = fields_to_idents(optional_fields);

    let required_fields_methods = builder_required_methods_constructor(
        required_fields,
        &optional_fields_idents,
        full_builder_generics,
        unset_type,
        set_type,
        builder_ident.clone(),
    );

    let optional_fields_methods = builder_optional_methods_constructor(optional_fields);

    let full_builder_generics_no_bounds = remove_generics_bounds(full_builder_generics);

    quote! {
        #(#required_fields_methods)*

        impl #full_builder_generics #builder_ident #full_builder_generics_no_bounds {
            #(#optional_fields_methods)*

            pub fn is_enabled(self, is_enabled: bool) -> Self {
                Self {is_enabled, ..self}
            }

            pub fn id(self, id: v4::ecs::component::ComponentId) -> Self {
                Self {id, ..self}
            }
        }
    }
}

fn build_method_constructor(
    full_builder_generics: &Generics,
    builder_required_fields_generics_arr: &[TypeParam],
    required_fields_trait_idents: &[Ident],
    builder_ident: &Ident,
    component_struct: &ItemStruct,
    required_fields: &[&Field],
    optional_fields: &[&Field],
) -> TokenStream2 {
    let full_builder_generics_no_bounds = remove_generics_bounds(full_builder_generics);

    let component_ident = &component_struct.ident;

    let component_generics_no_bounds = remove_generics_bounds(&component_struct.generics);

    let required_fields_idents = fields_to_idents(required_fields);
    let optional_fields_idents = fields_to_idents(optional_fields);

    quote! {
        impl #full_builder_generics #builder_ident #full_builder_generics_no_bounds
        where #(#builder_required_fields_generics_arr: #required_fields_trait_idents),* {
            pub fn build(self) -> #component_ident #component_generics_no_bounds {
                use std::hash::{DefaultHasher, Hash, Hasher};

                let mut hasher = DefaultHasher::new();

                std::time::Instant::now().hash(&mut hasher);

                let id = hasher.finish();

                #component_ident {
                    #(#required_fields_idents: self.#required_fields_idents.unwrap(),)*
                    #(#optional_fields_idents: self.#optional_fields_idents,)*
                    id: if self.id == 0 {
                            id
                        } else {
                            self.id
                        },
                    parent_entity_id: 0,
                    is_initialized: false,
                    is_enabled: self.is_enabled,
                }
            }
        }
    }
}

fn get_all_defaults(all_fields: &[&Field]) -> TokenStream2 {
    let field_defaults: Vec<TokenStream2> = all_fields
        .iter()
        .map(|field| {
            let field_ident = field.ident.as_ref().unwrap();
            if let Some(attr) = field.attrs.first() {
                if attr.path().is_ident("default") {
                    if let Ok(expr) = attr.parse_args::<Expr>() {
                        return quote! {#field_ident: #expr};
                    } else {
                        return quote! {#field_ident: Default::default()};
                    };
                } else {
                    panic!(
                        "Invalid field attribute '{}'",
                        attr.path().get_ident().unwrap()
                    )
                }
            } else {
                return quote! {#field_ident: None};
            }
        })
        .collect();

    quote! {#(#field_defaults,)*}
}

fn builder_construction(builder_ident: Ident, component_struct: &ItemStruct) -> TokenStream2 {
    let all_fields: Vec<&Field> = component_struct.fields.iter().collect();
    let (required_fields, optional_fields) = separate_required_and_optional_fields(&all_fields);

    let required_fields_idents: Vec<Ident> = required_fields
        .iter()
        .map(|field| {
            format_ident!(
                "{}",
                to_pascal_case(&field.ident.as_ref().unwrap().to_string())
            )
        })
        .collect();

    let TypedBuilderTypes {
        set_type_ident,
        set_type,
        unset_type_ident,
        unset_type,
    } = create_typed_builder_types(builder_ident.to_string());

    let (required_fields_generics_arr, [full_builder_generics, unset_builder_generics_no_bounds]) =
        generics_constructor(
            &required_fields_idents,
            &component_struct,
            unset_type.clone(),
        );

    let required_fields_trait_idents: Vec<Ident> = required_fields_idents
        .iter()
        .map(|ident| format_ident!("Has{ident}"))
        .collect();

    let builder_methods = builder_methods_constructor(
        &required_fields,
        &optional_fields,
        &full_builder_generics,
        &unset_type,
        &set_type,
        builder_ident.clone(),
    );

    let component_ident = &component_struct.ident;
    let component_generics = &component_struct.generics;
    let component_generics_no_bounds = remove_generics_bounds(&component_generics);

    let build_method = build_method_constructor(
        &full_builder_generics,
        &required_fields_generics_arr,
        &required_fields_trait_idents,
        &builder_ident,
        &component_struct,
        &required_fields,
        &optional_fields,
    );

    let field_defaults = get_all_defaults(&all_fields);

    let builder_struct = builder_struct_construction(
        builder_ident.clone(),
        &required_fields_generics_arr,
        full_builder_generics,
        component_struct.clone(),
    );

    quote! {
        #builder_struct

        pub struct #set_type_ident;
        pub struct #unset_type_ident;

        #(pub trait #required_fields_trait_idents {})*

        #(impl #required_fields_trait_idents for #set_type_ident {})*

        #builder_methods

        #build_method

        impl #component_generics #component_ident #component_generics_no_bounds {
            pub fn builder() -> #builder_ident #unset_builder_generics_no_bounds {
                #builder_ident::default()
            }
        }

        impl #component_generics Default for #builder_ident #unset_builder_generics_no_bounds{
            fn default() -> Self {
                Self {
                    #field_defaults
                    is_enabled: true,
                    id: 0,
                    _marker: std::marker::PhantomData,
                }
            }
        }
    }
}
