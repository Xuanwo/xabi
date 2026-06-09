use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{parse_quote, Error, ItemTrait, ReturnType, TraitItem};

use crate::args::TraitArgs;
use crate::method::{HandleDecode, MethodSpec};

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
    let borrowed_ident = format_ident!("XabiV1BorrowedTrait{}", trait_ident);
    let owned_ident = format_ident!("XabiV1OwnedTrait{}", trait_ident);
    let ref_ident = format_ident!("XabiV1RefTrait{}", trait_ident);
    let owned_ref_ident = format_ident!("XabiV1OwnedRefTrait{}", trait_ident);

    let mut vtable_fields = Vec::new();
    let mut field_available_arms = Vec::new();
    let mut thunks = Vec::new();
    let mut init_fields = Vec::new();
    let mut handle_methods = Vec::new();
    let mut borrowed_methods = Vec::new();

    for item in &item_trait.items {
        let TraitItem::Fn(method) = item else {
            continue;
        };
        let spec = MethodSpec::parse(method)?;
        let method_ident = &spec.name;
        let ffi_ty = spec.ffi_type()?;
        let thunk = spec.export_thunk(&trait_ident)?;
        let foreign = spec.handle_method(HandleDecode::Module)?;
        let borrowed = spec.handle_method(HandleDecode::Local)?;

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
        borrowed_methods.push(borrowed);
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
        const __XABI_VERSION: u32 = #version;
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

            pub fn xabi_export<P: #trait_ident>(value: P) -> *mut #vtable_ident {
                <Self as ::xabi::XabiContract<P>>::export(value) as *mut #vtable_ident
            }

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

            fn __xabi_impl_mut<P: #trait_ident>(
                instance: *mut std::ffi::c_void,
            ) -> Option<&'static mut P> {
                unsafe { (instance as *mut P).as_mut() }
            }
        }

        #[repr(C)]
        pub struct #vtable_ident {
            pub size: usize,
            pub abi_version: u32,
            pub capabilities: u64,
            pub instance: *mut std::ffi::c_void,
            pub destroy: unsafe extern "C" fn(*mut std::ffi::c_void),
            pub release: unsafe extern "C" fn(*mut #vtable_ident),
            #(#vtable_fields)*
        }

        impl #vtable_ident {
            pub const ABI_VERSION: u32 = #version;
            pub const MIN_SIZE: usize = std::mem::offset_of!(#vtable_ident, release)
                + std::mem::size_of::<unsafe extern "C" fn(*mut #vtable_ident)>();
            pub const FULL_SIZE: usize = std::mem::size_of::<Self>();

            pub fn validate(&self) -> ::xabi::Result<()> {
                ::xabi::validate_size(self.size, Self::MIN_SIZE, stringify!(#vtable_ident))?;
                ::xabi::validate_abi_version(
                    self.abi_version,
                    Self::ABI_VERSION,
                    stringify!(#vtable_ident),
                )?;
                if self.instance.is_null() {
                    return Err(::xabi::Error::NullPointer(concat!(stringify!(#vtable_ident), "::instance")));
                }
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
            #[doc(hidden)]
            pub(crate) unsafe fn xabi_from_vtable(
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

            #[doc(hidden)]
            pub(crate) unsafe fn xabi_from_export(
                export: &::xabi::XabiExport,
                module: std::sync::Arc<::xabi::ModuleHandle>,
            ) -> ::xabi::Result<Self> {
                export.validate()?;
                let abi_id = unsafe { export.abi_id.as_str() }?;
                if abi_id != #id {
                    return Err(::xabi::Error::Export(format!(
                        "module export has abi_id {abi_id}, expected {}",
                        #id,
                    )));
                }
                if export.contract_version != #version {
                    return Err(::xabi::Error::AbiMismatch(format!(
                        "module export {} has contract version {}, expected {}",
                        #id,
                        export.contract_version,
                        #version,
                    )));
                }
                let raw = unsafe { (export.make)() } as *mut #vtable_ident;
                unsafe { Self::xabi_from_vtable(raw, module) }
            }

            #[doc(hidden)]
            pub(crate) unsafe fn xabi_from_owned_ref(
                owned_ref: #owned_ref_ident,
                module: std::sync::Arc<::xabi::ModuleHandle>,
            ) -> ::xabi::Result<Self> {
                owned_ref.validate()?;
                unsafe { Self::xabi_from_vtable(owned_ref.vtable, module) }
            }

            pub fn xabi_load(module: &::xabi::Module) -> ::xabi::Result<Self> {
                let handle = module.handle();
                let mut version_mismatch = None;
                for export in module.exports()? {
                    let abi_id = unsafe { export.abi_id.as_str() }?;
                    if abi_id == #id {
                        if export.contract_version == #version {
                            return unsafe { Self::xabi_from_export(export, handle) };
                        }
                        version_mismatch = Some(export.contract_version);
                    }
                }
                if let Some(actual) = version_mismatch {
                    return Err(::xabi::Error::AbiMismatch(format!(
                        "module contains xabi export {} with contract version {}, expected {}",
                        #id,
                        actual,
                        #version,
                    )));
                }
                Err(::xabi::Error::Export(format!(
                    "module does not contain xabi export {}",
                    #id,
                )))
            }

            pub fn xabi_load_named(module: &::xabi::Module, name: &str) -> ::xabi::Result<Self> {
                let handle = module.handle();
                let mut version_mismatch = None;
                for export in module.exports()? {
                    let abi_id = unsafe { export.abi_id.as_str() }?;
                    if abi_id != #id {
                        continue;
                    }
                    let export_name = unsafe { export.name.as_str() }?;
                    if export_name != name {
                        continue;
                    }
                    if export.contract_version == #version {
                        return unsafe { Self::xabi_from_export(export, handle) };
                    }
                    version_mismatch = Some(export.contract_version);
                }
                if let Some(actual) = version_mismatch {
                    return Err(::xabi::Error::AbiMismatch(format!(
                        "module contains xabi export {} named {} with contract version {}, expected {}",
                        #id,
                        name,
                        actual,
                        #version,
                    )));
                }
                Err(::xabi::Error::Export(format!(
                    "module does not contain xabi export {} named {}",
                    #id,
                    name,
                )))
            }

            pub fn xabi_load_all(module: &::xabi::Module) -> ::xabi::Result<Vec<(String, Self)>> {
                let handle = module.handle();
                let mut version_mismatch = None;
                let mut loaded = Vec::new();
                for export in module.exports()? {
                    let abi_id = unsafe { export.abi_id.as_str() }?;
                    if abi_id != #id {
                        continue;
                    }
                    if export.contract_version != #version {
                        version_mismatch = Some(export.contract_version);
                        continue;
                    }
                    let name = unsafe { export.name.as_str() }?.to_string();
                    let value = unsafe { Self::xabi_from_export(export, std::sync::Arc::clone(&handle)) }?;
                    loaded.push((name, value));
                }
                if loaded.is_empty() {
                    if let Some(actual) = version_mismatch {
                        return Err(::xabi::Error::AbiMismatch(format!(
                            "module contains xabi export {} with contract version {}, expected {}",
                            #id,
                            actual,
                            #version,
                        )));
                    }
                }
                Ok(loaded)
            }

            pub fn xabi_module(&self) -> std::sync::Arc<::xabi::ModuleHandle> {
                std::sync::Arc::clone(&self._module)
            }

            fn vtable(&self) -> &#vtable_ident {
                unsafe { self.vtable.as_ref() }
            }

            #(#handle_methods)*
        }

        #[derive(Clone, Copy, Debug)]
        pub struct #borrowed_ident {
            vtable: std::ptr::NonNull<#vtable_ident>,
        }

        unsafe impl Send for #borrowed_ident {}
        unsafe impl Sync for #borrowed_ident {}

        impl #borrowed_ident {
            #[doc(hidden)]
            pub(crate) unsafe fn xabi_from_vtable(vtable: *const #vtable_ident) -> ::xabi::Result<Self> {
                let vtable = std::ptr::NonNull::new(vtable as *mut #vtable_ident)
                    .ok_or(::xabi::Error::NullPointer(concat!(stringify!(#vtable_ident), " pointer")))?;
                unsafe { vtable.as_ref() }
                    .validate()?;
                Ok(Self { vtable })
            }

            pub fn xabi_as_ptr(&self) -> *const #vtable_ident {
                self.vtable.as_ptr()
            }

            fn vtable(&self) -> &#vtable_ident {
                unsafe { self.vtable.as_ref() }
            }

            #(#borrowed_methods)*
        }

        #[repr(C)]
        #[derive(Clone, Copy, Debug)]
        pub struct #ref_ident {
            pub size: usize,
            pub abi_version: u32,
            pub vtable: *const #vtable_ident,
        }

        unsafe impl Send for #ref_ident {}
        unsafe impl Sync for #ref_ident {}

        impl #ref_ident {
            pub const ABI_VERSION: u32 = #version;
            pub const MIN_SIZE: usize = std::mem::size_of::<Self>();

            pub fn validate(&self) -> ::xabi::Result<()> {
                ::xabi::validate_size(self.size, Self::MIN_SIZE, stringify!(#ref_ident))?;
                ::xabi::validate_abi_version(
                    self.abi_version,
                    Self::ABI_VERSION,
                    stringify!(#ref_ident),
                )?;
                if self.vtable.is_null() {
                    return Err(::xabi::Error::NullPointer(concat!(stringify!(#ref_ident), "::vtable")));
                }
                Ok(())
            }
        }

        impl ::xabi::XabiType for #borrowed_ident {
            type Wire = #ref_ident;

            fn into_wire(self) -> Self::Wire {
                #ref_ident {
                    size: std::mem::size_of::<#ref_ident>(),
                    abi_version: #ref_ident::ABI_VERSION,
                    vtable: self.vtable.as_ptr(),
                }
            }

            unsafe fn from_wire(wire: *const Self::Wire) -> ::xabi::Result<Self> {
                let wire = unsafe {
                    wire.as_ref()
                        .ok_or(::xabi::Error::NullPointer(concat!(stringify!(#ref_ident), " pointer")))?
                };
                wire.validate()?;
                unsafe { Self::xabi_from_vtable(wire.vtable) }
            }
        }

        #[repr(C)]
        #[derive(Clone, Copy, Debug)]
        pub struct #owned_ref_ident {
            pub size: usize,
            pub abi_version: u32,
            pub vtable: *mut #vtable_ident,
        }

        unsafe impl Send for #owned_ref_ident {}
        unsafe impl Sync for #owned_ref_ident {}

        impl #owned_ref_ident {
            pub const ABI_VERSION: u32 = #version;
            pub const MIN_SIZE: usize = std::mem::size_of::<Self>();

            pub fn xabi_from_value<P: #trait_ident>(value: P) -> Self {
                Self {
                    size: std::mem::size_of::<Self>(),
                    abi_version: Self::ABI_VERSION,
                    vtable: #abi_ident::xabi_export(value),
                }
            }

            pub fn validate(&self) -> ::xabi::Result<()> {
                ::xabi::validate_size(self.size, Self::MIN_SIZE, stringify!(#owned_ref_ident))?;
                ::xabi::validate_abi_version(
                    self.abi_version,
                    Self::ABI_VERSION,
                    stringify!(#owned_ref_ident),
                )?;
                if self.vtable.is_null() {
                    return Err(::xabi::Error::NullPointer(concat!(stringify!(#owned_ref_ident), "::vtable")));
                }
                Ok(())
            }
        }

        impl ::xabi::XabiType for #owned_ref_ident {
            type Wire = #owned_ref_ident;

            fn into_wire(self) -> Self::Wire {
                self
            }

            unsafe fn from_wire(wire: *const Self::Wire) -> ::xabi::Result<Self> {
                let wire = unsafe {
                    wire.as_ref()
                        .ok_or(::xabi::Error::NullPointer(concat!(stringify!(#owned_ref_ident), " pointer")))?
                };
                wire.validate()?;
                Ok(*wire)
            }
        }

        pub struct #owned_ident {
            vtable: std::ptr::NonNull<#vtable_ident>,
        }

        unsafe impl Send for #owned_ident {}
        unsafe impl Sync for #owned_ident {}

        impl #owned_ident {
            pub fn new<P: #trait_ident>(value: P) -> Self {
                let vtable = #abi_ident::xabi_export(value);
                let vtable = std::ptr::NonNull::new(vtable)
                    .expect("generated xabi export returned a null vtable");
                Self { vtable }
            }

            #[doc(hidden)]
            pub(crate) unsafe fn xabi_from_vtable(vtable: *mut #vtable_ident) -> ::xabi::Result<Self> {
                let vtable = std::ptr::NonNull::new(vtable)
                    .ok_or(::xabi::Error::NullPointer(concat!(stringify!(#vtable_ident), " pointer")))?;
                unsafe { vtable.as_ref() }
                    .validate()?;
                Ok(Self { vtable })
            }

            #[doc(hidden)]
            pub(crate) unsafe fn xabi_from_owned_ref(owned_ref: #owned_ref_ident) -> ::xabi::Result<Self> {
                owned_ref.validate()?;
                unsafe { Self::xabi_from_vtable(owned_ref.vtable) }
            }

            pub fn xabi_as_ptr(&self) -> *const #vtable_ident {
                self.vtable.as_ptr()
            }

            pub fn xabi_borrow(&self) -> #borrowed_ident {
                #borrowed_ident {
                    vtable: self.vtable,
                }
            }
        }

        impl Drop for #owned_ident {
            fn drop(&mut self) {
                unsafe {
                    (self.vtable.as_ref().release)(self.vtable.as_ptr());
                }
            }
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
                    capabilities: ::xabi::CAP_NONE,
                    instance,
                    destroy: #abi_ident::__xabi_destroy::<P>,
                    release: #abi_ident::__xabi_release,
                    #(#init_fields)*
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
        if let Some(default) = method.default.take() {
            method.default = Some(parse_quote!({
                async move #default
            }));
        }
    }
    Ok(())
}
