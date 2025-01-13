extern crate proc_macro;
use proc_macro::TokenStream;

mod component;
mod scene;

#[proc_macro_attribute]
pub fn component(args: TokenStream, item: TokenStream) -> TokenStream {
    component::component_impl(args, item)
}

#[proc_macro]
pub fn scene(item: TokenStream) -> TokenStream {
    // scene::
    item
}
