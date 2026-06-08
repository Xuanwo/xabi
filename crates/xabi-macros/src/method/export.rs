use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{Error, Ident};

use super::{ArgKind, MethodRet, MethodSpec};

impl MethodSpec {
    pub(crate) fn export_thunk(&self, trait_ident: &Ident) -> syn::Result<TokenStream2> {
        let name = &self.name;
        if self.asyncness {
            return self.async_export_thunk(trait_ident);
        }

        Ok(
            match (self.arg.as_ref().map(|arg| arg.kind), self.ret.clone()) {
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
                (Some(ArgKind::Bytes), MethodRet::ResultUnit(_)) => {
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
                                    Ok(()) => ::xabi::OK,
                                    Err(err) => {
                                        *out = ::xabi::XabiType::into_payload(err);
                                        ::xabi::ERR_EXPORT
                                    }
                                }
                            })
                        }
                    }
                }
                (Some(ArgKind::Bytes), MethodRet::ResultOptionalBytes(_)) => {
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
                                    Err(err) => {
                                        *out = ::xabi::XabiType::into_payload(err);
                                        ::xabi::ERR_EXPORT
                                    }
                                }
                            })
                        }
                    }
                }
                (Some(ArgKind::Value), MethodRet::ResultBytes(_)) => {
                    let arg = self.arg.as_ref().expect("arg exists");
                    let arg_name = &arg.name;
                    let arg_ty = &arg.ty;
                    quote! {
                        unsafe extern "C" fn #name<P: #trait_ident>(
                            instance: *mut std::ffi::c_void,
                            #arg_name: *const <#arg_ty as ::xabi::XabiType>::Wire,
                            out: *mut ::xabi::XabiOwnedBytes,
                        ) -> i32 {
                            ::xabi::catch_unwind_code(|| {
                                let Some(plugin) = Self::__xabi_impl_ref::<P>(instance) else {
                                    return ::xabi::ERR_INVALID_ARGUMENT;
                                };
                                let Some(out) = (unsafe { out.as_mut() }) else {
                                    return ::xabi::ERR_INVALID_ARGUMENT;
                                };
                                let Ok(#arg_name) = (unsafe { <#arg_ty as ::xabi::XabiType>::from_wire(#arg_name) }) else {
                                    return ::xabi::ERR_INVALID_ARGUMENT;
                                };
                                match plugin.#name(#arg_name) {
                                    Ok(bytes) => {
                                        *out = ::xabi::XabiOwnedBytes::from_vec(bytes);
                                        ::xabi::OK
                                    }
                                    Err(err) => {
                                        *out = ::xabi::XabiType::into_payload(err);
                                        ::xabi::ERR_EXPORT
                                    }
                                }
                            })
                        }
                    }
                }
                _ => {
                    return Err(Error::new_spanned(name, "unsupported xabi method shape"));
                }
            },
        )
    }

    fn async_export_thunk(&self, trait_ident: &Ident) -> syn::Result<TokenStream2> {
        let name = &self.name;
        let out_init = quote! {
            let Some(out) = (unsafe { out.as_mut() }) else {
                return ::xabi::ERR_INVALID_ARGUMENT;
            };
        };

        Ok(
            match (self.arg.as_ref().map(|arg| arg.kind), self.ret.clone()) {
                (Some(ArgKind::Value), MethodRet::ResultBytes(_)) => {
                    let arg = self.arg.as_ref().expect("arg exists");
                    let arg_name = &arg.name;
                    let arg_ty = &arg.ty;
                    quote! {
                        unsafe extern "C" fn #name<P: #trait_ident>(
                            instance: *mut std::ffi::c_void,
                            #arg_name: *const <#arg_ty as ::xabi::XabiType>::Wire,
                            out: *mut ::xabi::XabiFuture,
                        ) -> i32 {
                            ::xabi::catch_unwind_code(|| {
                                let Some(plugin) = Self::__xabi_impl_ref::<P>(instance) else {
                                    return ::xabi::ERR_INVALID_ARGUMENT;
                                };
                                #out_init
                                let Ok(#arg_name) = (unsafe { <#arg_ty as ::xabi::XabiType>::from_wire(#arg_name) }) else {
                                    return ::xabi::ERR_INVALID_ARGUMENT;
                                };
                                let future = async move {
                                    plugin.#name(#arg_name).await
                                };
                                *out = ::xabi::XabiFuture::from_result_bytes(future);
                                ::xabi::OK
                            })
                        }
                    }
                }
                (Some(ArgKind::Bytes), MethodRet::ResultUnit(_)) => {
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
            },
        )
    }
}
