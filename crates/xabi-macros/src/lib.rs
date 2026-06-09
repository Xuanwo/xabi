use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use syn::{Error, Item};

mod args;
mod data_macro;
mod method;
mod module_macro;
mod opaque_macro;
mod trait_macro;
mod type_shape;

#[cfg(test)]
mod tests;

#[proc_macro_attribute]
pub fn xabi(attr: TokenStream, item: TokenStream) -> TokenStream {
    match expand_xabi(attr.into(), item.into()) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

#[proc_macro_attribute]
pub fn module(attr: TokenStream, item: TokenStream) -> TokenStream {
    match module_macro::expand_module(attr.into(), item.into()) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

#[proc_macro_attribute]
pub fn data(attr: TokenStream, item: TokenStream) -> TokenStream {
    match data_macro::expand_data(attr.into(), item.into()) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

#[proc_macro_attribute]
pub fn opaque(attr: TokenStream, item: TokenStream) -> TokenStream {
    match opaque_macro::expand_opaque(attr.into(), item.into()) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

fn expand_xabi(attr: TokenStream2, item: TokenStream2) -> syn::Result<TokenStream2> {
    let item = syn::parse2::<Item>(item)?;
    match item {
        Item::Trait(item_trait) => trait_macro::expand_xabi_trait(attr, item_trait),
        Item::Impl(item_impl) => Err(Error::new_spanned(
            item_impl.impl_token,
            "`#[xabi]` implementation exports must be placed inside a `#[xabi::module]` module",
        )),
        other => Err(Error::new_spanned(
            other,
            "`#[xabi]` can only be applied to traits or implementation exports",
        )),
    }
}
