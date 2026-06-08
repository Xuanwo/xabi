use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::Error;

use super::{ArgKind, MethodRet, MethodSpec};

impl MethodSpec {
    pub(crate) fn handle_method(&self) -> syn::Result<TokenStream2> {
        let name = &self.name;
        if self.asyncness {
            return self.async_handle_method();
        }

        Ok(
            match (self.arg.as_ref().map(|arg| arg.kind), self.ret.clone()) {
                (None, MethodRet::String) => quote! {
                    pub fn #name(&self) -> ::xabi::Result<String> {
                        let out = unsafe { (self.vtable().#name)(self.vtable().instance) };
                        unsafe { out.to_string_and_free() }
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
                (Some(ArgKind::Bytes), MethodRet::ResultUnit(error_ty)) => {
                    let arg = self.arg.as_ref().expect("arg exists");
                    let arg_name = &arg.name;
                    quote! {
                        pub fn #name(
                            &self,
                            #arg_name: &[u8],
                        ) -> std::result::Result<(), ::xabi::XabiCallError<#error_ty>> {
                            let mut out = ::xabi::XabiOwnedBytes::empty();
                            let code = unsafe {
                                (self.vtable().#name)(
                                    self.vtable().instance,
                                    ::xabi::XabiBytes::from_slice(#arg_name),
                                    &mut out,
                                )
                            };
                            match code {
                                ::xabi::OK => {
                                    let bytes = unsafe {
                                        out.to_vec_and_free()
                                            .map_err(::xabi::XabiCallError::Runtime)?
                                    };
                                    if bytes.is_empty() {
                                        Ok(())
                                    } else {
                                        Err(::xabi::XabiCallError::Runtime(::xabi::Error::Export(
                                            concat!("Xabi.", stringify!(#name), " returned a non-empty unit payload")
                                                .to_string(),
                                        )))
                                    }
                                }
                                ::xabi::ERR_EXPORT => {
                                    match unsafe { <#error_ty as ::xabi::XabiType>::from_payload(out) } {
                                        Ok(err) => Err(::xabi::XabiCallError::Export(err)),
                                        Err(err) => Err(::xabi::XabiCallError::Runtime(err)),
                                    }
                                }
                                _ => {
                                    match ::xabi::status_to_result(code, concat!("Xabi.", stringify!(#name))) {
                                        Ok(()) => Ok(()),
                                        Err(err) => Err(::xabi::XabiCallError::Runtime(err)),
                                    }
                                }
                            }
                        }
                    }
                }
                (Some(ArgKind::Bytes), MethodRet::ResultOptionalBytes(error_ty)) => {
                    let arg = self.arg.as_ref().expect("arg exists");
                    let arg_name = &arg.name;
                    quote! {
                        pub fn #name(
                            &self,
                            #arg_name: &[u8],
                        ) -> std::result::Result<Option<Vec<u8>>, ::xabi::XabiCallError<#error_ty>> {
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
                            match code {
                                ::xabi::OK => {
                                    let bytes = unsafe {
                                        out.to_vec_and_free()
                                            .map_err(::xabi::XabiCallError::Runtime)?
                                    };
                                    if bytes.is_empty() {
                                        Ok(None)
                                    } else {
                                        Ok(Some(bytes))
                                    }
                                }
                                ::xabi::ERR_EXPORT => {
                                    match unsafe { <#error_ty as ::xabi::XabiType>::from_payload(out) } {
                                        Ok(err) => Err(::xabi::XabiCallError::Export(err)),
                                        Err(err) => Err(::xabi::XabiCallError::Runtime(err)),
                                    }
                                }
                                _ => {
                                    match ::xabi::status_to_result(code, concat!("Xabi.", stringify!(#name))) {
                                        Ok(()) => Ok(None),
                                        Err(err) => Err(::xabi::XabiCallError::Runtime(err)),
                                    }
                                }
                            }
                        }
                    }
                }
                (Some(ArgKind::Value), MethodRet::ResultBytes(error_ty)) => {
                    let arg = self.arg.as_ref().expect("arg exists");
                    let arg_name = &arg.name;
                    let arg_ty = &arg.ty;
                    quote! {
                        pub fn #name(
                            &self,
                            #arg_name: #arg_ty,
                        ) -> std::result::Result<Vec<u8>, ::xabi::XabiCallError<#error_ty>> {
                            let wire = <#arg_ty as ::xabi::XabiType>::into_wire(#arg_name);
                            let mut out = ::xabi::XabiOwnedBytes::empty();
                            let code = unsafe {
                                (self.vtable().#name)(self.vtable().instance, &wire, &mut out)
                            };
                            match code {
                                ::xabi::OK => unsafe {
                                    out.to_vec_and_free()
                                        .map_err(::xabi::XabiCallError::Runtime)
                                },
                                ::xabi::ERR_EXPORT => {
                                    match unsafe { <#error_ty as ::xabi::XabiType>::from_payload(out) } {
                                        Ok(err) => Err(::xabi::XabiCallError::Export(err)),
                                        Err(err) => Err(::xabi::XabiCallError::Runtime(err)),
                                    }
                                }
                                _ => {
                                    match ::xabi::status_to_result(code, concat!("Xabi.", stringify!(#name))) {
                                        Ok(()) => Ok(Vec::new()),
                                        Err(err) => Err(::xabi::XabiCallError::Runtime(err)),
                                    }
                                }
                            }
                        }
                    }
                }
                _ => {
                    return Err(Error::new_spanned(name, "unsupported xabi method shape"));
                }
            },
        )
    }

    fn async_handle_method(&self) -> syn::Result<TokenStream2> {
        let name = &self.name;
        Ok(
            match (self.arg.as_ref().map(|arg| arg.kind), self.ret.clone()) {
                (Some(ArgKind::Value), MethodRet::ResultBytes(error_ty)) => {
                    let arg = self.arg.as_ref().expect("arg exists");
                    let arg_name = &arg.name;
                    let arg_ty = &arg.ty;
                    quote! {
                        pub async fn #name(
                            &self,
                            #arg_name: #arg_ty,
                        ) -> std::result::Result<Vec<u8>, ::xabi::XabiCallError<#error_ty>> {
                            let wire = <#arg_ty as ::xabi::XabiType>::into_wire(#arg_name);
                            let mut future = ::xabi::XabiFuture::empty();
                            let code = unsafe {
                                (self.vtable().#name)(self.vtable().instance, &wire, &mut future)
                            };
                            ::xabi::status_to_result(code, concat!("Xabi.", stringify!(#name)))
                                .map_err(::xabi::XabiCallError::Runtime)?;
                            ::xabi::XabiTypedFuture::<#error_ty>::new(future)
                                .map_err(::xabi::XabiCallError::Runtime)?
                                .await
                        }
                    }
                }
                (Some(ArgKind::Bytes), MethodRet::ResultUnit(error_ty)) => {
                    let arg = self.arg.as_ref().expect("arg exists");
                    let arg_name = &arg.name;
                    quote! {
                        pub async fn #name(
                            &self,
                            #arg_name: &[u8],
                        ) -> std::result::Result<(), ::xabi::XabiCallError<#error_ty>> {
                            let mut future = ::xabi::XabiFuture::empty();
                            let code = unsafe {
                                (self.vtable().#name)(
                                    self.vtable().instance,
                                    ::xabi::XabiBytes::from_slice(#arg_name),
                                    &mut future,
                                )
                            };
                            ::xabi::status_to_result(code, concat!("Xabi.", stringify!(#name)))
                                .map_err(::xabi::XabiCallError::Runtime)?;
                            let bytes = ::xabi::XabiTypedFuture::<#error_ty>::new(future)
                                .map_err(::xabi::XabiCallError::Runtime)?
                                .await?;
                            if bytes.is_empty() {
                                Ok(())
                            } else {
                                Err(::xabi::XabiCallError::Runtime(::xabi::Error::Export(
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
            },
        )
    }
}
