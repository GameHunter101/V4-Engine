extern crate proc_macro;
use proc_macro::TokenStream;

mod component;

#[proc_macro_attribute]
pub fn component(args: TokenStream, item: TokenStream) -> TokenStream {
    component::component_impl(args, item)
}
