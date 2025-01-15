extern crate proc_macro;
use proc_macro::TokenStream;
use quote::quote;
use scene::SceneDescriptor;
use syn::parse_macro_input;

mod component;
mod scene;

#[proc_macro_attribute]
pub fn component(args: TokenStream, item: TokenStream) -> TokenStream {
    component::component_impl(args, item)
}

#[proc_macro]
pub fn scene(item: TokenStream) -> TokenStream {
    let scene = parse_macro_input!(item as SceneDescriptor);

    quote! {#scene}.into()
}
