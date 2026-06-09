use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{Attribute, Error, Expr, Ident, Item, ItemImpl, ItemMod, Path, Type, parse_quote};

use crate::args::ImplArgs;

pub(crate) fn expand_module(attr: TokenStream2, item: TokenStream2) -> syn::Result<TokenStream2> {
    if !attr.is_empty() {
        return Err(Error::new_spanned(
            attr,
            "`#[xabi::module]` does not accept options",
        ));
    }

    let mut item_mod = syn::parse2::<ItemMod>(item)?;
    let Some((_, items)) = item_mod.content.as_mut() else {
        return Err(Error::new_spanned(
            item_mod,
            "`#[xabi::module]` requires an inline module",
        ));
    };

    let mut exports = Vec::new();
    for item in items.iter_mut() {
        let Item::Impl(item_impl) = item else {
            continue;
        };
        let Some(args) = take_xabi_impl_attr(&mut item_impl.attrs)? else {
            continue;
        };
        exports.push(ModuleExport::new(exports.len(), item_impl, args)?);
    }

    let export_count = exports.len();
    let make_fns = exports.iter().map(|export| export.make_fn());
    let export_entries = exports.iter().map(|export| export.entry());
    let layout_entries = exports.iter().map(|export| export.layout_entry());
    let layout_collectors = exports.iter().map(|export| export.layout_collector());

    items.push(parse_quote! {
        #[unsafe(no_mangle)]
        pub extern "C" fn xabi_manifest() -> *const ::xabi::XabiManifest {
            &XABI_MANIFEST
        }
    });
    items.push(parse_quote! {
        static XABI_EXPORTS: [::xabi::XabiExport; #export_count] = [
            #(#export_entries,)*
        ];
    });
    items.push(parse_quote! {
        static XABI_MANIFEST: ::xabi::XabiManifest = ::xabi::XabiManifest::new(&XABI_EXPORTS);
    });
    items.push(parse_quote! {
        #[doc(hidden)]
        pub static XABI_LAYOUT: ::xabi::XabiLayout = ::xabi::XabiLayout {
            package: env!("CARGO_PKG_NAME"),
            module: module_path!(),
            collect: __xabi_collect_layout,
        };
    });
    items.push(parse_quote! {
        #[doc(hidden)]
        fn __xabi_collect_layout(collector: &mut dyn ::xabi::XabiLayoutCollector) {
            ::xabi::__private::collect_runtime_layout(collector);
            #(#layout_entries)*
            #(#layout_collectors)*
        }
    });
    for make_fn in make_fns {
        items.push(syn::parse2(make_fn)?);
    }

    Ok(quote!(#item_mod))
}

struct ModuleExport {
    make_fn_ident: Ident,
    trait_path: Path,
    impl_ty: Type,
    name: Expr,
    version: Expr,
    constructor: Option<Expr>,
}

impl ModuleExport {
    fn new(index: usize, item_impl: &ItemImpl, args: ImplArgs) -> syn::Result<Self> {
        let trait_path = impl_trait_path(item_impl)?;
        let impl_ty = (*item_impl.self_ty).clone();
        let make_fn_ident = make_export_ident(index, &trait_path, &impl_ty);
        Ok(Self {
            make_fn_ident,
            trait_path,
            impl_ty,
            name: args.name,
            version: args.version,
            constructor: args.constructor,
        })
    }

    fn make_fn(&self) -> TokenStream2 {
        let make_fn_ident = &self.make_fn_ident;
        let trait_path = &self.trait_path;
        let impl_ty = &self.impl_ty;
        let constructor = self
            .constructor
            .as_ref()
            .map(|constructor| quote!((#constructor)()))
            .unwrap_or_else(|| quote!(<#impl_ty as Default>::default()));
        quote! {
            #[allow(non_snake_case)]
            unsafe extern "C" fn #make_fn_ident() -> *mut std::ffi::c_void {
                <#impl_ty as #trait_path>::__xabi_export(#constructor)
            }
        }
    }

    fn entry(&self) -> TokenStream2 {
        let make_fn_ident = &self.make_fn_ident;
        let trait_path = &self.trait_path;
        let impl_ty = &self.impl_ty;
        let name = &self.name;
        let version = &self.version;
        quote! {
            ::xabi::XabiExport::new(
                ::xabi::XabiStr::from_static(
                    <#impl_ty as #trait_path>::__XABI_ID,
                ),
                ::xabi::XabiStr::from_static(#name),
                <#impl_ty as #trait_path>::__XABI_VERSION,
                #version,
                ::xabi::CAP_NONE,
                #make_fn_ident,
            )
        }
    }

    fn layout_entry(&self) -> TokenStream2 {
        let trait_path = &self.trait_path;
        let impl_ty = &self.impl_ty;
        let name = &self.name;
        quote! {
            collector.push(::xabi::XabiLayoutItem::Export(::xabi::XabiExportLayout::new(
                <#impl_ty as #trait_path>::__XABI_ID,
                #name,
                <#impl_ty as #trait_path>::__XABI_VERSION,
            )));
        }
    }

    fn layout_collector(&self) -> TokenStream2 {
        let trait_path = &self.trait_path;
        let impl_ty = &self.impl_ty;
        quote! {
            <#impl_ty as #trait_path>::__xabi_collect_layout(collector);
        }
    }
}

fn make_export_ident(index: usize, trait_path: &Path, impl_ty: &Type) -> Ident {
    let Some(trait_ident) = trait_path.segments.last().map(|segment| &segment.ident) else {
        return format_ident!("__xabi_make_export_{index}");
    };
    let Some(impl_ident) = type_last_ident(impl_ty) else {
        return format_ident!("__xabi_make_export_{index}");
    };
    format_ident!("XabiV1Trait{}Impl{}", trait_ident, impl_ident)
}

fn type_last_ident(ty: &Type) -> Option<&Ident> {
    let Type::Path(path) = ty else {
        return None;
    };
    path.path.segments.last().map(|segment| &segment.ident)
}

fn take_xabi_impl_attr(attrs: &mut Vec<Attribute>) -> syn::Result<Option<ImplArgs>> {
    let Some(index) = attrs.iter().position(is_xabi_attr) else {
        return Ok(None);
    };
    let attr = attrs.remove(index);
    attr.parse_args::<ImplArgs>().map(Some)
}

fn is_xabi_attr(attr: &Attribute) -> bool {
    let path = attr.path();
    path.segments
        .last()
        .map(|segment| segment.ident == "xabi")
        .unwrap_or(false)
}

fn impl_trait_path(item_impl: &ItemImpl) -> syn::Result<Path> {
    let Some((_, path, _)) = &item_impl.trait_ else {
        return Err(Error::new_spanned(
            item_impl,
            "xabi implementation exports must implement a trait",
        ));
    };
    Ok(path.clone())
}
