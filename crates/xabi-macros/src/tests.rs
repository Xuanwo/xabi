use proc_macro2::TokenStream as TokenStream2;
use quote::quote;

use crate::data_macro::expand_data;
use crate::opaque_macro::expand_opaque;
use crate::trait_macro::expand_xabi_trait;

#[test]
fn snapshot_export_async_trait() {
    let attr = quote! {
        id = TRAIT_ID,
        version = ABI_VERSION
    };
    let item = quote! {
        pub trait DemoPlugin {
            fn name(&self) -> String;
            async fn build(&self, input: BuildInput) -> Result<Vec<u8>>;
            async fn load(&self, details: &[u8]) -> Result<()>;
        }
    };
    let expanded =
        expand_xabi_trait(attr, syn::parse2(item).expect("trait parses")).expect("macro expands");
    let file = syn::parse2::<syn::File>(expanded).expect("expanded code parses");
    let rendered = prettyplease::unparse(&file);
    let snapshot = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/snapshots/export_async_trait.rs");
    if std::env::var_os("UPDATE_XABI_SNAPSHOTS").is_some() {
        std::fs::write(&snapshot, &rendered).expect("write snapshot");
    } else {
        let expected = std::fs::read_to_string(&snapshot).expect("read snapshot");
        assert_eq!(rendered, expected);
    }
}

#[test]
fn snapshot_data_type() {
    let item = quote! {
        pub struct BuildInput {
            pub value: u64,
        }
    };
    let expanded = expand_data(TokenStream2::new(), item).expect("macro expands");
    let file = syn::parse2::<syn::File>(expanded).expect("expanded code parses");
    let rendered = prettyplease::unparse(&file);
    let snapshot =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/snapshots/data_type.rs");
    if std::env::var_os("UPDATE_XABI_SNAPSHOTS").is_some() {
        std::fs::write(&snapshot, &rendered).expect("write snapshot");
    } else {
        let expected = std::fs::read_to_string(&snapshot).expect("read snapshot");
        assert_eq!(rendered, expected);
    }
}

#[test]
fn snapshot_opaque_handle() {
    let item = quote! {
        pub struct StreamHandle {
            raw: *mut ArrowArrayStream,
        }
    };
    let expanded = expand_opaque(TokenStream2::new(), item).expect("macro expands");
    let file = syn::parse2::<syn::File>(expanded).expect("expanded code parses");
    let rendered = prettyplease::unparse(&file);
    let snapshot =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/snapshots/opaque_handle.rs");
    if std::env::var_os("UPDATE_XABI_SNAPSHOTS").is_some() {
        std::fs::write(&snapshot, &rendered).expect("write snapshot");
    } else {
        let expected = std::fs::read_to_string(&snapshot).expect("read snapshot");
        assert_eq!(rendered, expected);
    }
}

#[test]
fn snapshot_trait_object_return() {
    let attr = quote! {
        id = FACTORY_TRAIT_ID,
        version = ABI_VERSION
    };
    let item = quote! {
        pub trait Factory {
            async fn make(&self, name: &str) -> Result<impl Child + 'static, Error>;
        }
    };
    let expanded =
        expand_xabi_trait(attr, syn::parse2(item).expect("trait parses")).expect("macro expands");
    let file = syn::parse2::<syn::File>(expanded).expect("expanded code parses");
    let rendered = prettyplease::unparse(&file);
    let snapshot = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/snapshots/trait_object_return.rs");
    if std::env::var_os("UPDATE_XABI_SNAPSHOTS").is_some() {
        std::fs::write(&snapshot, &rendered).expect("write snapshot");
    } else {
        let expected = std::fs::read_to_string(&snapshot).expect("read snapshot");
        assert_eq!(rendered, expected);
    }
}
