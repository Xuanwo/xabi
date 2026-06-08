use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::parse::{Parse, ParseStream};
use syn::{
    parse_quote, Attribute, Error, Expr, FnArg, GenericArgument, Ident, Item, ItemImpl, ItemMod,
    ItemTrait, MetaNameValue, Pat, Path, PathArguments, ReturnType, Token, TraitItem, TraitItemFn,
    Type,
};

#[proc_macro_attribute]
pub fn xabi(attr: TokenStream, item: TokenStream) -> TokenStream {
    match expand_xabi(attr.into(), item.into()) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

#[proc_macro_attribute]
pub fn module(attr: TokenStream, item: TokenStream) -> TokenStream {
    match expand_module(attr.into(), item.into()) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

struct TraitArgs {
    id: Expr,
    version: Expr,
    error: Type,
}

impl Parse for TraitArgs {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let values =
            syn::punctuated::Punctuated::<MetaNameValue, Token![,]>::parse_terminated(input)?;
        let mut id = None;
        let mut version = None;
        let mut error = None;

        for value in values {
            let Some(ident) = value.path.get_ident() else {
                return Err(Error::new_spanned(value.path, "expected identifier"));
            };
            match ident.to_string().as_str() {
                "id" => id = Some(value.value),
                "version" => version = Some(value.value),
                "error" => error = Some(expr_to_type(value.value)?),
                other => {
                    return Err(Error::new_spanned(
                        ident,
                        format!("unsupported xabi option `{other}` for trait ABI"),
                    ));
                }
            }
        }

        Ok(Self {
            id: id.ok_or_else(|| input.error("missing `id = ...`"))?,
            version: version.ok_or_else(|| input.error("missing `version = ...`"))?,
            error: error.ok_or_else(|| input.error("missing `error = ...`"))?,
        })
    }
}

struct ImplArgs {
    name: Expr,
    version: Expr,
    constructor: Option<Expr>,
}

impl Parse for ImplArgs {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let values =
            syn::punctuated::Punctuated::<MetaNameValue, Token![,]>::parse_terminated(input)?;
        let mut name = None;
        let mut version = None;
        let mut constructor = None;

        for value in values {
            let Some(ident) = value.path.get_ident() else {
                return Err(Error::new_spanned(value.path, "expected identifier"));
            };
            match ident.to_string().as_str() {
                "name" => name = Some(value.value),
                "version" => version = Some(value.value),
                "constructor" => constructor = Some(value.value),
                other => {
                    return Err(Error::new_spanned(
                        ident,
                        format!("unsupported xabi option `{other}` for implementation export"),
                    ));
                }
            }
        }

        Ok(Self {
            name: name.ok_or_else(|| input.error("missing `name = ...`"))?,
            version: version.ok_or_else(|| input.error("missing `version = ...`"))?,
            constructor,
        })
    }
}

fn expr_to_type(expr: Expr) -> syn::Result<Type> {
    match expr {
        Expr::Path(path) => Ok(Type::Path(syn::TypePath {
            qself: None,
            path: path.path,
        })),
        other => Err(Error::new_spanned(other, "`error` must be a type path")),
    }
}

fn expand_xabi(attr: TokenStream2, item: TokenStream2) -> syn::Result<TokenStream2> {
    let item = syn::parse2::<Item>(item)?;
    match item {
        Item::Trait(item_trait) => expand_xabi_trait(attr, item_trait),
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

fn expand_xabi_trait(attr: TokenStream2, mut item_trait: ItemTrait) -> syn::Result<TokenStream2> {
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
        let foreign = spec.handle_method(&args.error)?;

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
    let error_ty = args.error;
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
            ) -> std::result::Result<Self, #error_ty> {
                let vtable = std::ptr::NonNull::new(vtable)
                    .ok_or_else(|| <#error_ty as From<::xabi::Error>>::from(
                        ::xabi::Error::NullPointer(concat!(stringify!(#vtable_ident), " pointer")),
                    ))?;
                unsafe { vtable.as_ref() }
                    .validate()
                    .map_err(<#error_ty as From<::xabi::Error>>::from)?;
                Ok(Self {
                    vtable,
                    _module: module,
                })
            }

            pub unsafe fn xabi_from_export(
                export: &::xabi::XabiExport,
                module: std::sync::Arc<::xabi::ModuleHandle>,
            ) -> std::result::Result<Self, #error_ty> {
                let abi_id = unsafe { export.abi_id.as_str() }
                    .map_err(<#error_ty as From<::xabi::Error>>::from)?;
                if abi_id != #id {
                    return Err(<#error_ty as From<::xabi::Error>>::from(
                        ::xabi::Error::Export(format!(
                            "module export has abi_id {abi_id}, expected {}",
                            #id,
                        )),
                    ));
                }
                let raw = unsafe { (export.make)() } as *mut #vtable_ident;
                unsafe { Self::xabi_from_vtable(raw, module) }
            }

            pub unsafe fn xabi_load(module: &::xabi::Module) -> std::result::Result<Self, #error_ty> {
                let handle = module.handle();
                for export in module.exports().map_err(<#error_ty as From<::xabi::Error>>::from)? {
                    let abi_id = unsafe { export.abi_id.as_str() }
                        .map_err(<#error_ty as From<::xabi::Error>>::from)?;
                    if abi_id == #id {
                        return unsafe { Self::xabi_from_export(export, handle) };
                    }
                }
                Err(<#error_ty as From<::xabi::Error>>::from(
                    ::xabi::Error::Export(format!(
                        "module does not contain xabi export {}",
                        #id,
                    )),
                ))
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

fn expand_module(attr: TokenStream2, item: TokenStream2) -> syn::Result<TokenStream2> {
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
            ::xabi::XabiExport {
                abi_id: ::xabi::XabiStr::from_static(
                    <#impl_ty as #trait_path>::__XABI_ID,
                ),
                name: ::xabi::XabiStr::from_static(#name),
                version: #version,
                make: #make_fn_ident,
            }
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

#[derive(Clone)]
struct MethodSpec {
    name: Ident,
    asyncness: bool,
    arg: Option<MethodArg>,
    ret: MethodRet,
}

#[derive(Clone)]
struct MethodArg {
    name: Ident,
    ty: Type,
    kind: ArgKind,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ArgKind {
    Bytes,
    Value,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum MethodRet {
    String,
    U32,
    Bool,
    ResultUnit,
    ResultBytes,
    ResultOptionalBytes,
}

impl MethodSpec {
    fn parse(method: &TraitItemFn) -> syn::Result<Self> {
        if !method.sig.generics.params.is_empty() {
            return Err(Error::new_spanned(
                &method.sig.generics,
                "xabi does not support generic methods",
            ));
        }

        let mut inputs = method.sig.inputs.iter();
        match inputs.next() {
            Some(FnArg::Receiver(receiver)) if receiver.reference.is_some() => {}
            _ => {
                return Err(Error::new_spanned(
                    &method.sig.inputs,
                    "xabi methods must take &self",
                ));
            }
        }

        let arg = match inputs.next() {
            Some(FnArg::Typed(arg)) => Some(parse_arg(arg)?),
            Some(FnArg::Receiver(_)) => {
                return Err(Error::new_spanned(
                    &method.sig.inputs,
                    "xabi supports at most one non-self argument",
                ));
            }
            None => None,
        };
        if inputs.next().is_some() {
            return Err(Error::new_spanned(
                &method.sig.inputs,
                "xabi supports at most one non-self argument",
            ));
        }

        let ret = parse_ret(&method.sig.output)?;
        let asyncness = method.sig.asyncness.is_some();
        validate_shape(method, arg.as_ref(), ret, asyncness)?;

        Ok(Self {
            name: method.sig.ident.clone(),
            asyncness,
            arg,
            ret,
        })
    }

    fn ffi_type(&self) -> syn::Result<TokenStream2> {
        if self.asyncness {
            let arg = self.ffi_arg_type();
            return Ok(quote! {
                unsafe extern "C" fn(
                    *mut std::ffi::c_void,
                    #arg
                    *mut ::xabi::XabiFuture,
                ) -> i32
            });
        }

        Ok(match (self.arg.as_ref().map(|arg| arg.kind), self.ret) {
            (None, MethodRet::String) => {
                quote!(unsafe extern "C" fn(*mut std::ffi::c_void) -> ::xabi::XabiOwnedBytes)
            }
            (None, MethodRet::U32) => {
                quote!(unsafe extern "C" fn(*mut std::ffi::c_void) -> u32)
            }
            (None, MethodRet::Bool) => {
                quote!(unsafe extern "C" fn(*mut std::ffi::c_void) -> u8)
            }
            (Some(ArgKind::Bytes), MethodRet::ResultUnit) => {
                quote!(unsafe extern "C" fn(*mut std::ffi::c_void, ::xabi::XabiBytes) -> i32)
            }
            (Some(ArgKind::Bytes), MethodRet::ResultOptionalBytes) => {
                quote!(
                    unsafe extern "C" fn(
                        *mut std::ffi::c_void,
                        ::xabi::XabiBytes,
                        *mut ::xabi::XabiOwnedBytes,
                    ) -> i32
                )
            }
            (Some(ArgKind::Value), MethodRet::ResultBytes) => {
                let ty = &self.arg.as_ref().expect("arg exists").ty;
                quote!(unsafe extern "C" fn(
                    *mut std::ffi::c_void,
                    *const #ty,
                    *mut ::xabi::XabiOwnedBytes,
                ) -> i32)
            }
            _ => {
                return Err(Error::new_spanned(
                    &self.name,
                    "unsupported xabi method shape",
                ));
            }
        })
    }

    fn ffi_arg_type(&self) -> TokenStream2 {
        match self.arg.as_ref().map(|arg| arg.kind) {
            Some(ArgKind::Bytes) => quote!(::xabi::XabiBytes,),
            Some(ArgKind::Value) => {
                let ty = &self.arg.as_ref().expect("arg exists").ty;
                quote!(*const #ty,)
            }
            None => quote!(),
        }
    }

    fn export_thunk(&self, trait_ident: &Ident) -> syn::Result<TokenStream2> {
        let name = &self.name;
        if self.asyncness {
            return self.async_export_thunk(trait_ident);
        }

        Ok(match (self.arg.as_ref().map(|arg| arg.kind), self.ret) {
            (None, MethodRet::String) => quote! {
                unsafe extern "C" fn #name<P: #trait_ident>(
                    instance: *mut std::ffi::c_void,
                ) -> ::xabi::XabiOwnedBytes {
                    ::xabi::catch_unwind_owned(|| {
                        let Some(plugin) = Self::__xabi_impl_ref::<P>(instance) else {
                            return ::xabi::XabiOwnedBytes::empty();
                        };
                        ::xabi::XabiOwnedBytes::from_string(plugin.#name())
                    })
                }
            },
            (None, MethodRet::U32) => quote! {
                unsafe extern "C" fn #name<P: #trait_ident>(
                    instance: *mut std::ffi::c_void,
                ) -> u32 {
                    ::xabi::catch_unwind_or(0, || {
                        let Some(plugin) = Self::__xabi_impl_ref::<P>(instance) else {
                            return 0;
                        };
                        plugin.#name()
                    })
                }
            },
            (None, MethodRet::Bool) => quote! {
                unsafe extern "C" fn #name<P: #trait_ident>(
                    instance: *mut std::ffi::c_void,
                ) -> u8 {
                    ::xabi::catch_unwind_or(0, || {
                        let Some(plugin) = Self::__xabi_impl_ref::<P>(instance) else {
                            return 0;
                        };
                        plugin.#name() as u8
                    })
                }
            },
            (Some(ArgKind::Bytes), MethodRet::ResultUnit) => {
                let arg = self.arg.as_ref().expect("arg exists");
                let arg_name = &arg.name;
                quote! {
                    unsafe extern "C" fn #name<P: #trait_ident>(
                        instance: *mut std::ffi::c_void,
                        #arg_name: ::xabi::XabiBytes,
                    ) -> i32 {
                        ::xabi::catch_unwind_code(|| {
                            let Some(plugin) = Self::__xabi_impl_ref::<P>(instance) else {
                                return ::xabi::ERR_INVALID_ARGUMENT;
                            };
                            let Ok(#arg_name) = (unsafe { #arg_name.as_slice() }) else {
                                return ::xabi::ERR_INVALID_ARGUMENT;
                            };
                            match plugin.#name(#arg_name) {
                                Ok(()) => ::xabi::OK,
                                Err(_) => ::xabi::ERR_EXPORT,
                            }
                        })
                    }
                }
            }
            (Some(ArgKind::Bytes), MethodRet::ResultOptionalBytes) => {
                let arg = self.arg.as_ref().expect("arg exists");
                let arg_name = &arg.name;
                quote! {
                    unsafe extern "C" fn #name<P: #trait_ident>(
                        instance: *mut std::ffi::c_void,
                        #arg_name: ::xabi::XabiBytes,
                        out: *mut ::xabi::XabiOwnedBytes,
                    ) -> i32 {
                        ::xabi::catch_unwind_code(|| {
                            let Some(plugin) = Self::__xabi_impl_ref::<P>(instance) else {
                                return ::xabi::ERR_INVALID_ARGUMENT;
                            };
                            let Some(out) = (unsafe { out.as_mut() }) else {
                                return ::xabi::ERR_INVALID_ARGUMENT;
                            };
                            let Ok(#arg_name) = (unsafe { #arg_name.as_slice() }) else {
                                return ::xabi::ERR_INVALID_ARGUMENT;
                            };
                            match plugin.#name(#arg_name) {
                                Ok(Some(bytes)) => {
                                    *out = ::xabi::XabiOwnedBytes::from_vec(bytes);
                                    ::xabi::OK
                                }
                                Ok(None) => {
                                    *out = ::xabi::XabiOwnedBytes::empty();
                                    ::xabi::OK
                                }
                                Err(_) => ::xabi::ERR_EXPORT,
                            }
                        })
                    }
                }
            }
            (Some(ArgKind::Value), MethodRet::ResultBytes) => {
                let arg = self.arg.as_ref().expect("arg exists");
                let arg_name = &arg.name;
                let arg_ty = &arg.ty;
                quote! {
                    unsafe extern "C" fn #name<P: #trait_ident>(
                        instance: *mut std::ffi::c_void,
                        #arg_name: *const #arg_ty,
                        out: *mut ::xabi::XabiOwnedBytes,
                    ) -> i32 {
                        ::xabi::catch_unwind_code(|| {
                            let Some(plugin) = Self::__xabi_impl_ref::<P>(instance) else {
                                return ::xabi::ERR_INVALID_ARGUMENT;
                            };
                            let Some(out) = (unsafe { out.as_mut() }) else {
                                return ::xabi::ERR_INVALID_ARGUMENT;
                            };
                            let Ok(#arg_name) = (unsafe { <#arg_ty>::from_ptr(#arg_name) }) else {
                                return ::xabi::ERR_INVALID_ARGUMENT;
                            };
                            let #arg_name = *#arg_name;
                            match plugin.#name(#arg_name) {
                                Ok(bytes) => {
                                    *out = ::xabi::XabiOwnedBytes::from_vec(bytes);
                                    ::xabi::OK
                                }
                                Err(_) => ::xabi::ERR_EXPORT,
                            }
                        })
                    }
                }
            }
            _ => {
                return Err(Error::new_spanned(name, "unsupported xabi method shape"));
            }
        })
    }

    fn async_export_thunk(&self, trait_ident: &Ident) -> syn::Result<TokenStream2> {
        let name = &self.name;
        let out_init = quote! {
            let Some(out) = (unsafe { out.as_mut() }) else {
                return ::xabi::ERR_INVALID_ARGUMENT;
            };
        };

        Ok(match (self.arg.as_ref().map(|arg| arg.kind), self.ret) {
            (Some(ArgKind::Value), MethodRet::ResultBytes) => {
                let arg = self.arg.as_ref().expect("arg exists");
                let arg_name = &arg.name;
                let arg_ty = &arg.ty;
                quote! {
                    unsafe extern "C" fn #name<P: #trait_ident>(
                        instance: *mut std::ffi::c_void,
                        #arg_name: *const #arg_ty,
                        out: *mut ::xabi::XabiFuture,
                    ) -> i32 {
                        ::xabi::catch_unwind_code(|| {
                            let Some(plugin) = Self::__xabi_impl_ref::<P>(instance) else {
                                return ::xabi::ERR_INVALID_ARGUMENT;
                            };
                            #out_init
                            let Ok(#arg_name) = (unsafe { <#arg_ty>::from_ptr(#arg_name) }) else {
                                return ::xabi::ERR_INVALID_ARGUMENT;
                            };
                            let #arg_name = *#arg_name;
                            let future = async move {
                                plugin.#name(#arg_name).await.map_err(|err| err.to_string())
                            };
                            *out = ::xabi::XabiFuture::from_result_bytes(future);
                            ::xabi::OK
                        })
                    }
                }
            }
            (Some(ArgKind::Bytes), MethodRet::ResultUnit) => {
                let arg = self.arg.as_ref().expect("arg exists");
                let arg_name = &arg.name;
                quote! {
                    unsafe extern "C" fn #name<P: #trait_ident>(
                        instance: *mut std::ffi::c_void,
                        #arg_name: ::xabi::XabiBytes,
                        out: *mut ::xabi::XabiFuture,
                    ) -> i32 {
                        ::xabi::catch_unwind_code(|| {
                            let Some(plugin) = Self::__xabi_impl_ref::<P>(instance) else {
                                return ::xabi::ERR_INVALID_ARGUMENT;
                            };
                            #out_init
                            let Ok(#arg_name) = (unsafe { #arg_name.as_slice() }) else {
                                return ::xabi::ERR_INVALID_ARGUMENT;
                            };
                            let #arg_name = #arg_name.to_vec();
                            let future = async move {
                                plugin.#name(&#arg_name)
                                    .await
                                    .map(|()| Vec::new())
                                    .map_err(|err| err.to_string())
                            };
                            *out = ::xabi::XabiFuture::from_result_bytes(future);
                            ::xabi::OK
                        })
                    }
                }
            }
            _ => {
                return Err(Error::new_spanned(
                    name,
                    "unsupported async xabi method shape",
                ));
            }
        })
    }

    fn handle_method(&self, error_ty: &Type) -> syn::Result<TokenStream2> {
        let name = &self.name;
        if self.asyncness {
            return self.async_handle_method(error_ty);
        }

        Ok(match (self.arg.as_ref().map(|arg| arg.kind), self.ret) {
            (None, MethodRet::String) => quote! {
                pub fn #name(&self) -> std::result::Result<String, #error_ty> {
                    let out = unsafe { (self.vtable().#name)(self.vtable().instance) };
                    unsafe {
                        out.to_string_and_free()
                            .map_err(<#error_ty as From<::xabi::Error>>::from)
                    }
                }
            },
            (None, MethodRet::U32) => quote! {
                pub fn #name(&self) -> u32 {
                    unsafe { (self.vtable().#name)(self.vtable().instance) }
                }
            },
            (None, MethodRet::Bool) => quote! {
                pub fn #name(&self) -> bool {
                    unsafe { (self.vtable().#name)(self.vtable().instance) != 0 }
                }
            },
            (Some(ArgKind::Bytes), MethodRet::ResultUnit) => {
                let arg = self.arg.as_ref().expect("arg exists");
                let arg_name = &arg.name;
                quote! {
                    pub fn #name(&self, #arg_name: &[u8]) -> std::result::Result<(), #error_ty> {
                        let code = unsafe {
                            (self.vtable().#name)(
                                self.vtable().instance,
                                ::xabi::XabiBytes::from_slice(#arg_name),
                            )
                        };
                        ::xabi::status_to_result(code, concat!("Xabi.", stringify!(#name)))
                            .map_err(<#error_ty as From<::xabi::Error>>::from)?;
                        Ok(())
                    }
                }
            }
            (Some(ArgKind::Bytes), MethodRet::ResultOptionalBytes) => {
                let arg = self.arg.as_ref().expect("arg exists");
                let arg_name = &arg.name;
                quote! {
                    pub fn #name(
                        &self,
                        #arg_name: &[u8],
                    ) -> std::result::Result<Option<Vec<u8>>, #error_ty> {
                        if !self.vtable().field_available(stringify!(#name)) {
                            return Ok(None);
                        }

                        let mut out = ::xabi::XabiOwnedBytes::empty();
                        let code = unsafe {
                            (self.vtable().#name)(
                                self.vtable().instance,
                                ::xabi::XabiBytes::from_slice(#arg_name),
                                &mut out,
                            )
                        };
                        ::xabi::status_to_result(code, concat!("Xabi.", stringify!(#name)))
                            .map_err(<#error_ty as From<::xabi::Error>>::from)?;
                        let bytes = unsafe {
                            out.to_vec_and_free()
                                .map_err(<#error_ty as From<::xabi::Error>>::from)?
                        };
                        if bytes.is_empty() {
                            Ok(None)
                        } else {
                            Ok(Some(bytes))
                        }
                    }
                }
            }
            (Some(ArgKind::Value), MethodRet::ResultBytes) => {
                let arg = self.arg.as_ref().expect("arg exists");
                let arg_name = &arg.name;
                let arg_ty = &arg.ty;
                quote! {
                    pub fn #name(
                        &self,
                        #arg_name: #arg_ty,
                    ) -> std::result::Result<Vec<u8>, #error_ty> {
                        let mut out = ::xabi::XabiOwnedBytes::empty();
                        let code = unsafe {
                            (self.vtable().#name)(self.vtable().instance, &#arg_name, &mut out)
                        };
                        ::xabi::status_to_result(code, concat!("Xabi.", stringify!(#name)))
                            .map_err(<#error_ty as From<::xabi::Error>>::from)?;
                        unsafe {
                            out.to_vec_and_free()
                                .map_err(<#error_ty as From<::xabi::Error>>::from)
                        }
                    }
                }
            }
            _ => {
                return Err(Error::new_spanned(name, "unsupported xabi method shape"));
            }
        })
    }

    fn async_handle_method(&self, error_ty: &Type) -> syn::Result<TokenStream2> {
        let name = &self.name;
        Ok(match (self.arg.as_ref().map(|arg| arg.kind), self.ret) {
            (Some(ArgKind::Value), MethodRet::ResultBytes) => {
                let arg = self.arg.as_ref().expect("arg exists");
                let arg_name = &arg.name;
                let arg_ty = &arg.ty;
                quote! {
                    pub async fn #name(
                        &self,
                        #arg_name: #arg_ty,
                    ) -> std::result::Result<Vec<u8>, #error_ty> {
                        let mut future = ::xabi::XabiFuture::empty();
                        let code = unsafe {
                            (self.vtable().#name)(self.vtable().instance, &#arg_name, &mut future)
                        };
                        ::xabi::status_to_result(code, concat!("Xabi.", stringify!(#name)))
                            .map_err(<#error_ty as From<::xabi::Error>>::from)?;
                        ::xabi::XabiFutureHandle::new(future)
                            .map_err(<#error_ty as From<::xabi::Error>>::from)?
                            .await
                            .map_err(<#error_ty as From<::xabi::Error>>::from)
                    }
                }
            }
            (Some(ArgKind::Bytes), MethodRet::ResultUnit) => {
                let arg = self.arg.as_ref().expect("arg exists");
                let arg_name = &arg.name;
                quote! {
                    pub async fn #name(
                        &self,
                        #arg_name: &[u8],
                    ) -> std::result::Result<(), #error_ty> {
                        let mut future = ::xabi::XabiFuture::empty();
                        let code = unsafe {
                            (self.vtable().#name)(
                                self.vtable().instance,
                                ::xabi::XabiBytes::from_slice(#arg_name),
                                &mut future,
                            )
                        };
                        ::xabi::status_to_result(code, concat!("Xabi.", stringify!(#name)))
                            .map_err(<#error_ty as From<::xabi::Error>>::from)?;
                        let bytes = ::xabi::XabiFutureHandle::new(future)
                            .map_err(<#error_ty as From<::xabi::Error>>::from)?
                            .await
                            .map_err(<#error_ty as From<::xabi::Error>>::from)?;
                        if bytes.is_empty() {
                            Ok(())
                        } else {
                            Err(<#error_ty as From<::xabi::Error>>::from(::xabi::Error::Export(
                                concat!("Xabi.", stringify!(#name), " returned a non-empty unit payload")
                                    .to_string(),
                            )))
                        }
                    }
                }
            }
            _ => {
                return Err(Error::new_spanned(
                    name,
                    "unsupported async xabi method shape",
                ));
            }
        })
    }
}

fn parse_arg(arg: &syn::PatType) -> syn::Result<MethodArg> {
    let Pat::Ident(name) = arg.pat.as_ref() else {
        return Err(Error::new_spanned(
            &arg.pat,
            "argument must be an identifier",
        ));
    };
    let ty = (*arg.ty).clone();
    let kind = if is_bytes_ref(&ty) {
        ArgKind::Bytes
    } else {
        ArgKind::Value
    };
    Ok(MethodArg {
        name: name.ident.clone(),
        ty,
        kind,
    })
}

fn parse_ret(output: &ReturnType) -> syn::Result<MethodRet> {
    let ReturnType::Type(_, ty) = output else {
        return Err(Error::new_spanned(
            output,
            "xabi methods must return a value",
        ));
    };
    if is_ident_type(ty, "String") {
        return Ok(MethodRet::String);
    }
    if is_ident_type(ty, "u32") {
        return Ok(MethodRet::U32);
    }
    if is_ident_type(ty, "bool") {
        return Ok(MethodRet::Bool);
    }
    parse_result_ret(ty)
}

fn validate_shape(
    method: &TraitItemFn,
    arg: Option<&MethodArg>,
    ret: MethodRet,
    asyncness: bool,
) -> syn::Result<()> {
    if asyncness {
        match (arg.map(|arg| arg.kind), ret) {
            (Some(ArgKind::Value), MethodRet::ResultBytes)
            | (Some(ArgKind::Bytes), MethodRet::ResultUnit) => Ok(()),
            _ => Err(Error::new_spanned(
                method,
                "async xabi methods currently support `async fn method(&self, input: ReprC) -> Result<Vec<u8>>` and `async fn method(&self, bytes: &[u8]) -> Result<()>`",
            )),
        }
    } else {
        match (arg.map(|arg| arg.kind), ret) {
            (None, MethodRet::String | MethodRet::U32 | MethodRet::Bool)
            | (Some(ArgKind::Bytes), MethodRet::ResultUnit | MethodRet::ResultOptionalBytes)
            | (Some(ArgKind::Value), MethodRet::ResultBytes) => Ok(()),
            _ => Err(Error::new_spanned(method, "unsupported xabi method shape")),
        }
    }
}

fn parse_result_ret(ty: &Type) -> syn::Result<MethodRet> {
    let Type::Path(path) = ty else {
        return Err(Error::new_spanned(ty, "unsupported return type"));
    };
    let Some(segment) = path.path.segments.last() else {
        return Err(Error::new_spanned(ty, "unsupported return type"));
    };
    if segment.ident != "Result" {
        return Err(Error::new_spanned(ty, "unsupported return type"));
    }
    let PathArguments::AngleBracketed(args) = &segment.arguments else {
        return Err(Error::new_spanned(ty, "Result must have one payload type"));
    };
    let Some(GenericArgument::Type(payload)) = args.args.first() else {
        return Err(Error::new_spanned(ty, "Result must have one payload type"));
    };

    if is_unit_type(payload) {
        return Ok(MethodRet::ResultUnit);
    }
    if is_vec_u8(payload) {
        return Ok(MethodRet::ResultBytes);
    }
    if is_option_vec_u8(payload) {
        return Ok(MethodRet::ResultOptionalBytes);
    }
    Err(Error::new_spanned(
        payload,
        "unsupported Result payload type",
    ))
}

fn is_ident_type(ty: &Type, expected: &str) -> bool {
    let Type::Path(path) = ty else {
        return false;
    };
    path.path
        .segments
        .last()
        .map(|segment| segment.ident == expected)
        .unwrap_or(false)
}

fn is_unit_type(ty: &Type) -> bool {
    matches!(ty, Type::Tuple(tuple) if tuple.elems.is_empty())
}

fn is_bytes_ref(ty: &Type) -> bool {
    let Type::Reference(reference) = ty else {
        return false;
    };
    let Type::Slice(slice) = reference.elem.as_ref() else {
        return false;
    };
    is_ident_type(&slice.elem, "u8")
}

fn is_vec_u8(ty: &Type) -> bool {
    let Type::Path(path) = ty else {
        return false;
    };
    let Some(segment) = path.path.segments.last() else {
        return false;
    };
    if segment.ident != "Vec" {
        return false;
    }
    let PathArguments::AngleBracketed(args) = &segment.arguments else {
        return false;
    };
    matches!(args.args.first(), Some(GenericArgument::Type(ty)) if is_ident_type(ty, "u8"))
}

fn is_option_vec_u8(ty: &Type) -> bool {
    let Type::Path(path) = ty else {
        return false;
    };
    let Some(segment) = path.path.segments.last() else {
        return false;
    };
    if segment.ident != "Option" {
        return false;
    }
    let PathArguments::AngleBracketed(args) = &segment.arguments else {
        return false;
    };
    matches!(args.args.first(), Some(GenericArgument::Type(ty)) if is_vec_u8(ty))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_export_async_trait() {
        let attr = quote! {
            id = TRAIT_ID,
            version = ABI_VERSION,
            error = Error
        };
        let item = quote! {
            pub trait DemoPlugin {
                fn name(&self) -> String;
                async fn build(&self, input: BuildInput) -> Result<Vec<u8>>;
                async fn load(&self, details: &[u8]) -> Result<()>;
            }
        };
        let expanded = expand_xabi_trait(attr, syn::parse2(item).expect("trait parses"))
            .expect("macro expands");
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
}
