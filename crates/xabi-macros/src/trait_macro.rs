use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{parse_quote, Error, ItemTrait, ReturnType, TraitItem};

use crate::args::TraitArgs;
use crate::method::MethodSpec;

pub(crate) fn expand_xabi_trait(
    attr: TokenStream2,
    mut item_trait: ItemTrait,
) -> syn::Result<TokenStream2> {
    let args = syn::parse2::<TraitArgs>(attr)?;
    item_trait
        .attrs
        .push(parse_quote!(#[allow(async_fn_in_trait)]));
    push_supertrait(&mut item_trait, parse_quote!(Send));
    push_supertrait(&mut item_trait, parse_quote!(Sync));
    push_supertrait(&mut item_trait, parse_quote!('static));

    let trait_ident = item_trait.ident.clone();
    let abi_ident = format_ident!("XabiV1AbiTrait{}", trait_ident);
    let vtable_ident = format_ident!("XabiV1VtableTrait{}", trait_ident);
    let handle_ident = format_ident!("XabiV1HandleTrait{}", trait_ident);

    let mut vtable_fields = Vec::new();
    let mut field_available_arms = Vec::new();
    let mut thunks = Vec::new();
    let mut init_fields = Vec::new();
    let mut handle_methods = Vec::new();

    for item in &item_trait.items {
        let TraitItem::Fn(method) = item else {
            continue;
        };
        let spec = MethodSpec::parse(method)?;
        let method_ident = &spec.name;
        let ffi_ty = spec.ffi_type()?;
        let thunk = spec.export_thunk(&trait_ident)?;
        let foreign = spec.handle_method()?;

        vtable_fields.push(quote!(pub #method_ident: #ffi_ty,));
        field_available_arms.push(quote! {
            stringify!(#method_ident) => {
                let field_end = std::mem::offset_of!(#vtable_ident, #method_ident)
                    + std::mem::size_of_val(&self.#method_ident);
                self.size >= field_end
            }
        });
        thunks.push(thunk);
        init_fields.push(quote!(#method_ident: #abi_ident::#method_ident::<P>,));
        handle_methods.push(foreign);
    }
    rewrite_async_trait_methods(&mut item_trait)?;

    let id = args.id;
    let version = args.version;
    item_trait.items.push(parse_quote! {
        #[doc(hidden)]
        const __XABI_ID: &'static str = #id;
    });
    item_trait.items.push(parse_quote! {
        #[doc(hidden)]
        fn __xabi_export(value: Self) -> *mut std::ffi::c_void
        where
            Self: Sized,
        {
            <#abi_ident as ::xabi::XabiContract<Self>>::export(value)
        }
    });

    Ok(quote! {
        #item_trait

        pub struct #abi_ident;

        impl #abi_ident {
            pub const ID: &'static str = #id;
            pub const VERSION: u32 = #version;

            #(#thunks)*

            unsafe extern "C" fn __xabi_destroy<P: #trait_ident>(instance: *mut std::ffi::c_void) {
                if !instance.is_null() {
                    drop(unsafe { Box::from_raw(instance as *mut P) });
                }
            }

            unsafe extern "C" fn __xabi_release(vtable: *mut #vtable_ident) {
                let Some(vtable) = (unsafe { vtable.as_mut() }) else {
                    return;
                };
                unsafe { (vtable.destroy)(vtable.instance) };
                drop(unsafe { Box::from_raw(vtable) });
            }

            fn __xabi_impl_ref<P: #trait_ident>(
                instance: *mut std::ffi::c_void,
            ) -> Option<&'static P> {
                unsafe { (instance as *const P).as_ref() }
            }
        }

        #[repr(C)]
        pub struct #vtable_ident {
            pub size: usize,
            pub abi_version: u32,
            pub capabilities: u64,
            pub instance: *mut std::ffi::c_void,
            #(#vtable_fields)*
            pub destroy: unsafe extern "C" fn(*mut std::ffi::c_void),
            pub release: unsafe extern "C" fn(*mut #vtable_ident),
        }

        impl #vtable_ident {
            pub const ABI_VERSION: u32 = #version;
            pub const MIN_SIZE: usize = std::mem::size_of::<Self>();

            pub fn validate(&self) -> ::xabi::Result<()> {
                ::xabi::validate_size(self.size, Self::MIN_SIZE, stringify!(#vtable_ident))?;
                ::xabi::validate_abi_version(
                    self.abi_version,
                    Self::ABI_VERSION,
                    stringify!(#vtable_ident),
                )?;
                Ok(())
            }

            pub fn field_available(&self, field: &str) -> bool {
                match field {
                    #(#field_available_arms,)*
                    "destroy" => {
                        let field_end = std::mem::offset_of!(#vtable_ident, destroy)
                            + std::mem::size_of_val(&self.destroy);
                        self.size >= field_end
                    }
                    "release" => {
                        let field_end = std::mem::offset_of!(#vtable_ident, release)
                            + std::mem::size_of_val(&self.release);
                        self.size >= field_end
                    }
                    _ => false,
                }
            }
        }

        pub struct #handle_ident {
            vtable: std::ptr::NonNull<#vtable_ident>,
            _module: std::sync::Arc<::xabi::ModuleHandle>,
        }

        unsafe impl Send for #handle_ident {}
        unsafe impl Sync for #handle_ident {}

        impl #handle_ident {
            pub unsafe fn xabi_from_vtable(
                vtable: *mut #vtable_ident,
                module: std::sync::Arc<::xabi::ModuleHandle>,
            ) -> ::xabi::Result<Self> {
                let vtable = std::ptr::NonNull::new(vtable)
                    .ok_or(::xabi::Error::NullPointer(concat!(stringify!(#vtable_ident), " pointer")))?;
                unsafe { vtable.as_ref() }
                    .validate()?;
                Ok(Self {
                    vtable,
                    _module: module,
                })
            }

            pub unsafe fn xabi_from_export(
                export: &::xabi::XabiExport,
                module: std::sync::Arc<::xabi::ModuleHandle>,
            ) -> ::xabi::Result<Self> {
                let abi_id = unsafe { export.abi_id.as_str() }?;
                if abi_id != #id {
                    return Err(::xabi::Error::Export(format!(
                        "module export has abi_id {abi_id}, expected {}",
                        #id,
                    )));
                }
                let raw = unsafe { (export.make)() } as *mut #vtable_ident;
                unsafe { Self::xabi_from_vtable(raw, module) }
            }

            pub unsafe fn xabi_load(module: &::xabi::Module) -> ::xabi::Result<Self> {
                let handle = module.handle();
                for export in module.exports()? {
                    let abi_id = unsafe { export.abi_id.as_str() }?;
                    if abi_id == #id {
                        return unsafe { Self::xabi_from_export(export, handle) };
                    }
                }
                Err(::xabi::Error::Export(format!(
                    "module does not contain xabi export {}",
                    #id,
                )))
            }

            fn vtable(&self) -> &#vtable_ident {
                unsafe { self.vtable.as_ref() }
            }

            #(#handle_methods)*
        }

        impl std::fmt::Debug for #handle_ident {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.debug_struct(stringify!(#handle_ident))
                    .field("abi_id", &#id)
                    .finish_non_exhaustive()
            }
        }

        impl Drop for #handle_ident {
            fn drop(&mut self) {
                unsafe {
                    (self.vtable().release)(self.vtable.as_ptr());
                }
            }
        }

        impl<P> ::xabi::XabiContract<P> for #abi_ident
        where
            P: #trait_ident,
        {
            const ID: &'static str = #id;

            fn export(plugin: P) -> *mut std::ffi::c_void {
                let instance = Box::into_raw(Box::new(plugin)) as *mut std::ffi::c_void;
                let vtable = #vtable_ident {
                    size: std::mem::size_of::<#vtable_ident>(),
                    abi_version: #version,
                    capabilities: 0,
                    instance,
                    #(#init_fields)*
                    destroy: #abi_ident::__xabi_destroy::<P>,
                    release: #abi_ident::__xabi_release,
                };
                Box::into_raw(Box::new(vtable)) as *mut std::ffi::c_void
            }
        }
    })
}

fn push_supertrait(item_trait: &mut ItemTrait, bound: syn::TypeParamBound) {
    if !item_trait
        .supertraits
        .iter()
        .any(|existing| quote!(#existing).to_string() == quote!(#bound).to_string())
    {
        item_trait.supertraits.push(bound);
    }
}

fn rewrite_async_trait_methods(item_trait: &mut ItemTrait) -> syn::Result<()> {
    for item in &mut item_trait.items {
        let TraitItem::Fn(method) = item else {
            continue;
        };
        if method.sig.asyncness.take().is_none() {
            continue;
        }

        let ReturnType::Type(_, output) = &method.sig.output else {
            return Err(Error::new_spanned(
                &method.sig.output,
                "async xabi methods must return a value",
            ));
        };
        method.sig.output = parse_quote!(-> impl std::future::Future<Output = #output> + Send);
    }
    Ok(())
}
