mod export;
mod handle;
mod shape;

use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{Error, FnArg, Ident, Path, PathArguments, TraitItemFn, Type};

pub(crate) use handle::HandleDecode;
use shape::{parse_arg, parse_ret, validate_shape};

#[derive(Clone)]
pub(crate) struct MethodSpec {
    pub(crate) name: Ident,
    pub(super) receiver_mut: bool,
    pub(super) asyncness: bool,
    pub(super) args: Vec<MethodArg>,
    pub(super) ret: MethodRet,
}

#[derive(Clone)]
pub(super) struct MethodArg {
    pub(super) name: Ident,
    pub(super) ty: Type,
    pub(super) kind: ArgKind,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ArgKind {
    Bytes,
    Str,
    Value,
}

#[derive(Clone, PartialEq, Eq)]
pub(super) enum MethodRet {
    String,
    U32,
    Bool,
    Value(Type),
    ResultUnit(Type),
    ResultBytes(Type),
    ResultString(Type),
    ResultValue {
        ok: Type,
        error: Type,
    },
    ResultObject {
        trait_path: Path,
        error: Type,
    },
    ResultObjectPair {
        ok: Type,
        trait_path: Path,
        error: Type,
    },
}

impl MethodSpec {
    pub(crate) fn parse(method: &TraitItemFn) -> syn::Result<Self> {
        if !method.sig.generics.params.is_empty() {
            return Err(Error::new_spanned(
                &method.sig.generics,
                "xabi does not support generic methods",
            ));
        }

        let mut inputs = method.sig.inputs.iter();
        let receiver_mut = match inputs.next() {
            Some(FnArg::Receiver(receiver)) if receiver.reference.is_some() => {
                receiver.mutability.is_some()
            }
            _ => {
                return Err(Error::new_spanned(
                    &method.sig.inputs,
                    "xabi methods must take &self or &mut self",
                ));
            }
        };

        let mut args = Vec::new();
        for input in inputs {
            match input {
                FnArg::Typed(arg) => args.push(parse_arg(arg)?),
                FnArg::Receiver(_) => {
                    return Err(Error::new_spanned(
                        &method.sig.inputs,
                        "xabi methods must take exactly one self receiver",
                    ));
                }
            }
        }
        if args.len() > 16 {
            return Err(Error::new_spanned(
                &method.sig.inputs,
                "xabi methods support at most 16 non-self arguments",
            ));
        }

        let ret = parse_ret(&method.sig.output)?;
        let asyncness = method.sig.asyncness.is_some();
        validate_shape(method, &args, &ret, asyncness)?;

        Ok(Self {
            name: method.sig.ident.clone(),
            receiver_mut,
            asyncness,
            args,
            ret,
        })
    }

    pub(crate) fn ffi_type(&self) -> syn::Result<TokenStream2> {
        let args = self.ffi_arg_types();
        if self.asyncness {
            return Ok(quote! {
                unsafe extern "C" fn(
                    *mut std::ffi::c_void,
                    #(#args)*
                    *mut ::xabi::XabiFuture,
                ) -> i32
            });
        }

        Ok(match self.ret.clone() {
            MethodRet::String if self.args.is_empty() => {
                quote!(unsafe extern "C" fn(*mut std::ffi::c_void) -> ::xabi::XabiOwnedBytes)
            }
            MethodRet::U32 if self.args.is_empty() => {
                quote!(unsafe extern "C" fn(*mut std::ffi::c_void) -> u32)
            }
            MethodRet::Bool if self.args.is_empty() => {
                quote!(unsafe extern "C" fn(*mut std::ffi::c_void) -> u8)
            }
            MethodRet::Value(_) if self.args.is_empty() => {
                quote!(unsafe extern "C" fn(*mut std::ffi::c_void) -> ::xabi::XabiOwnedBytes)
            }
            MethodRet::ResultUnit(_)
            | MethodRet::ResultBytes(_)
            | MethodRet::ResultString(_)
            | MethodRet::ResultValue { .. }
            | MethodRet::ResultObject { .. }
            | MethodRet::ResultObjectPair { .. } => {
                quote!(
                    unsafe extern "C" fn(
                        *mut std::ffi::c_void,
                        #(#args)*
                        *mut ::xabi::XabiOwnedBytes,
                    ) -> i32
                )
            }
            _ => {
                return Err(Error::new_spanned(
                    &self.name,
                    "unsupported xabi method shape",
                ));
            }
        })
    }

    pub(crate) fn layout_dependencies(&self) -> Vec<TokenStream2> {
        let mut deps = Vec::new();
        for arg in &self.args {
            if arg.kind == ArgKind::Value {
                let ty = &arg.ty;
                deps.push(quote! {
                    <#ty as ::xabi::XabiType>::collect_xabi_layout(collector);
                });
            }
        }

        match &self.ret {
            MethodRet::String | MethodRet::U32 | MethodRet::Bool => {}
            MethodRet::Value(ok) => {
                deps.push(quote! {
                    <#ok as ::xabi::XabiType>::collect_xabi_layout(collector);
                });
            }
            MethodRet::ResultUnit(error)
            | MethodRet::ResultBytes(error)
            | MethodRet::ResultString(error) => {
                deps.push(quote! {
                    <#error as ::xabi::XabiType>::collect_xabi_layout(collector);
                });
            }
            MethodRet::ResultValue { ok, error } => {
                deps.push(quote! {
                    <#ok as ::xabi::XabiType>::collect_xabi_layout(collector);
                    <#error as ::xabi::XabiType>::collect_xabi_layout(collector);
                });
            }
            MethodRet::ResultObject { trait_path, error } => {
                let abi_ident = generated_trait_type_path(trait_path, "XabiV1AbiTrait");
                deps.push(quote! {
                    <#abi_ident as ::xabi::XabiLayoutSource>::collect_xabi_layout(collector);
                    <#error as ::xabi::XabiType>::collect_xabi_layout(collector);
                });
            }
            MethodRet::ResultObjectPair {
                ok,
                trait_path,
                error,
            } => {
                let abi_ident = generated_trait_type_path(trait_path, "XabiV1AbiTrait");
                deps.push(quote! {
                    <#ok as ::xabi::XabiType>::collect_xabi_layout(collector);
                    <#abi_ident as ::xabi::XabiLayoutSource>::collect_xabi_layout(collector);
                    <#error as ::xabi::XabiType>::collect_xabi_layout(collector);
                });
            }
        }

        deps
    }

    pub(crate) fn ffi_type_name(&self) -> String {
        let mut args = vec!["*mut c_void".to_string()];
        args.extend(self.args.iter().map(|arg| match arg.kind {
            ArgKind::Bytes => "XabiBytes".to_string(),
            ArgKind::Str => "XabiStr".to_string(),
            ArgKind::Value => format!(
                "*const <{} as XabiType>::Wire",
                normalized_type_name(&arg.ty)
            ),
        }));

        if self.asyncness {
            args.push("*mut XabiFuture".to_string());
            return format!("unsafe extern \"C\" fn({}) -> i32", args.join(", "));
        }

        match &self.ret {
            MethodRet::String => {
                "unsafe extern \"C\" fn(*mut c_void) -> XabiOwnedBytes".to_string()
            }
            MethodRet::U32 => "unsafe extern \"C\" fn(*mut c_void) -> u32".to_string(),
            MethodRet::Bool => "unsafe extern \"C\" fn(*mut c_void) -> u8".to_string(),
            MethodRet::Value(_) => {
                "unsafe extern \"C\" fn(*mut c_void) -> XabiOwnedBytes".to_string()
            }
            MethodRet::ResultUnit(_)
            | MethodRet::ResultBytes(_)
            | MethodRet::ResultString(_)
            | MethodRet::ResultValue { .. }
            | MethodRet::ResultObject { .. }
            | MethodRet::ResultObjectPair { .. } => {
                args.push("*mut XabiOwnedBytes".to_string());
                format!("unsafe extern \"C\" fn({}) -> i32", args.join(", "))
            }
        }
    }

    fn ffi_arg_types(&self) -> Vec<TokenStream2> {
        self.args
            .iter()
            .map(|arg| {
                let ty = &arg.ty;
                match arg.kind {
                    ArgKind::Bytes => quote!(::xabi::XabiBytes,),
                    ArgKind::Str => quote!(::xabi::XabiStr,),
                    ArgKind::Value => quote!(*const <#ty as ::xabi::XabiType>::Wire,),
                }
            })
            .collect()
    }

    pub(super) fn ffi_arg_defs(&self) -> Vec<TokenStream2> {
        self.args
            .iter()
            .map(|arg| {
                let name = &arg.name;
                let ty = &arg.ty;
                match arg.kind {
                    ArgKind::Bytes => quote!(#name: ::xabi::XabiBytes,),
                    ArgKind::Str => quote!(#name: ::xabi::XabiStr,),
                    ArgKind::Value => quote!(#name: *const <#ty as ::xabi::XabiType>::Wire,),
                }
            })
            .collect()
    }

    pub(super) fn handle_arg_defs(&self) -> Vec<TokenStream2> {
        self.args
            .iter()
            .map(|arg| {
                let name = &arg.name;
                let ty = &arg.ty;
                quote!(#name: #ty,)
            })
            .collect()
    }

    pub(super) fn handle_arg_lowering(&self) -> (Vec<TokenStream2>, Vec<TokenStream2>) {
        let mut locals = Vec::new();
        let mut calls = Vec::new();
        for arg in &self.args {
            let name = &arg.name;
            match arg.kind {
                ArgKind::Bytes => calls.push(quote!(::xabi::XabiBytes::from_slice(#name),)),
                ArgKind::Str => calls.push(quote!(::xabi::XabiStr::from_borrowed(#name),)),
                ArgKind::Value => {
                    let wire = Ident::new(&format!("__xabi_wire_{name}"), name.span());
                    locals.push(quote! {
                        let #wire = ::xabi::XabiType::into_wire(#name);
                    });
                    calls.push(quote!(&#wire,));
                }
            }
        }
        (locals, calls)
    }

    pub(super) fn export_arg_decoding(
        &self,
        asyncness: bool,
    ) -> (Vec<TokenStream2>, Vec<TokenStream2>) {
        let mut decoders = Vec::new();
        let mut calls = Vec::new();
        for arg in &self.args {
            let name = &arg.name;
            let ty = &arg.ty;
            match (arg.kind, asyncness) {
                (ArgKind::Bytes, false) => {
                    decoders.push(quote! {
                        let Ok(#name) = (unsafe { #name.as_slice() }) else {
                            return ::xabi::ERR_INVALID_ARGUMENT;
                        };
                    });
                    calls.push(quote!(#name,));
                }
                (ArgKind::Bytes, true) => {
                    decoders.push(quote! {
                        let Ok(#name) = (unsafe { #name.as_slice() }) else {
                            return ::xabi::ERR_INVALID_ARGUMENT;
                        };
                        let #name = #name.to_vec();
                    });
                    calls.push(quote!(&#name,));
                }
                (ArgKind::Str, false) => {
                    decoders.push(quote! {
                        let Ok(#name) = (unsafe { #name.as_str() }) else {
                            return ::xabi::ERR_INVALID_ARGUMENT;
                        };
                    });
                    calls.push(quote!(#name,));
                }
                (ArgKind::Str, true) => {
                    decoders.push(quote! {
                        let Ok(#name) = (unsafe { #name.as_str() }) else {
                            return ::xabi::ERR_INVALID_ARGUMENT;
                        };
                        let #name = #name.to_string();
                    });
                    calls.push(quote!(&#name,));
                }
                (ArgKind::Value, _) => {
                    decoders.push(quote! {
                        let Ok(#name) = (unsafe { <#ty as ::xabi::XabiType>::from_wire(#name) }) else {
                            return ::xabi::ERR_INVALID_ARGUMENT;
                        };
                    });
                    calls.push(quote!(#name,));
                }
            }
        }
        (decoders, calls)
    }

    pub(super) fn error_ty(&self) -> Option<&Type> {
        match &self.ret {
            MethodRet::ResultUnit(error)
            | MethodRet::ResultBytes(error)
            | MethodRet::ResultString(error) => Some(error),
            MethodRet::ResultValue { error, .. } => Some(error),
            MethodRet::ResultObject { error, .. } => Some(error),
            MethodRet::ResultObjectPair { error, .. } => Some(error),
            _ => None,
        }
    }
}

pub(super) fn generated_trait_type_path(trait_path: &Path, prefix: &str) -> TokenStream2 {
    let mut generated_path = trait_path.clone();
    let segment = generated_path
        .segments
        .last_mut()
        .expect("xabi object trait path has a final segment");
    let trait_ident = segment.ident.clone();
    segment.ident = format_ident!("{}{}", prefix, trait_ident);
    segment.arguments = PathArguments::None;
    quote!(#generated_path)
}

fn normalized_type_name(ty: &Type) -> String {
    let mut value = quote!(#ty)
        .to_string()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    for (from, to) in [
        (" :: ", "::"),
        (":: ", "::"),
        (" ::", "::"),
        (" < ", "<"),
        ("< ", "<"),
        (" >", ">"),
        (" ,", ","),
    ] {
        value = value.replace(from, to);
    }
    value
}
