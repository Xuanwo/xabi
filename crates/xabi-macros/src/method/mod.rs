mod export;
mod handle;
mod shape;

use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{Error, FnArg, Ident, TraitItemFn, Type};

use shape::{parse_arg, parse_ret, validate_shape};

#[derive(Clone)]
pub(crate) struct MethodSpec {
    pub(crate) name: Ident,
    pub(super) asyncness: bool,
    pub(super) arg: Option<MethodArg>,
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
    Value,
}

#[derive(Clone, PartialEq, Eq)]
pub(super) enum MethodRet {
    String,
    U32,
    Bool,
    ResultUnit(Type),
    ResultBytes(Type),
    ResultOptionalBytes(Type),
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
        validate_shape(method, arg.as_ref(), &ret, asyncness)?;

        Ok(Self {
            name: method.sig.ident.clone(),
            asyncness,
            arg,
            ret,
        })
    }

    pub(crate) fn ffi_type(&self) -> syn::Result<TokenStream2> {
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

        Ok(
            match (self.arg.as_ref().map(|arg| arg.kind), self.ret.clone()) {
                (None, MethodRet::String) => {
                    quote!(unsafe extern "C" fn(*mut std::ffi::c_void) -> ::xabi::XabiOwnedBytes)
                }
                (None, MethodRet::U32) => {
                    quote!(unsafe extern "C" fn(*mut std::ffi::c_void) -> u32)
                }
                (None, MethodRet::Bool) => {
                    quote!(unsafe extern "C" fn(*mut std::ffi::c_void) -> u8)
                }
                (Some(ArgKind::Bytes), MethodRet::ResultUnit(_)) => {
                    quote!(
                        unsafe extern "C" fn(
                            *mut std::ffi::c_void,
                            ::xabi::XabiBytes,
                            *mut ::xabi::XabiOwnedBytes,
                        ) -> i32
                    )
                }
                (Some(ArgKind::Bytes), MethodRet::ResultOptionalBytes(_)) => {
                    quote!(
                        unsafe extern "C" fn(
                            *mut std::ffi::c_void,
                            ::xabi::XabiBytes,
                            *mut ::xabi::XabiOwnedBytes,
                        ) -> i32
                    )
                }
                (Some(ArgKind::Value), MethodRet::ResultBytes(_)) => {
                    let ty = &self.arg.as_ref().expect("arg exists").ty;
                    quote!(unsafe extern "C" fn(
                    *mut std::ffi::c_void,
                    *const <#ty as ::xabi::XabiType>::Wire,
                    *mut ::xabi::XabiOwnedBytes,
                ) -> i32)
                }
                _ => {
                    return Err(Error::new_spanned(
                        &self.name,
                        "unsupported xabi method shape",
                    ));
                }
            },
        )
    }

    fn ffi_arg_type(&self) -> TokenStream2 {
        match self.arg.as_ref().map(|arg| arg.kind) {
            Some(ArgKind::Bytes) => quote!(::xabi::XabiBytes,),
            Some(ArgKind::Value) => {
                let ty = &self.arg.as_ref().expect("arg exists").ty;
                quote!(*const <#ty as ::xabi::XabiType>::Wire,)
            }
            None => quote!(),
        }
    }
}
