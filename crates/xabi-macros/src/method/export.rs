use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::Ident;

use super::{MethodRet, MethodSpec};

impl MethodSpec {
    pub(crate) fn export_thunk(&self, trait_ident: &Ident) -> syn::Result<TokenStream2> {
        if self.asyncness {
            return self.async_export_thunk(trait_ident);
        }

        let name = &self.name;
        if self.args.is_empty() {
            match self.ret {
                MethodRet::String => {
                    return Ok(quote! {
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
                    });
                }
                MethodRet::U32 => {
                    return Ok(quote! {
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
                    });
                }
                MethodRet::Bool => {
                    return Ok(quote! {
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
                    });
                }
                _ => {}
            }
        }

        let ffi_args = self.ffi_arg_defs();
        let (decoders, call_args) = self.export_arg_decoding(false);
        let ok = self.sync_ok_payload();
        Ok(quote! {
            unsafe extern "C" fn #name<P: #trait_ident>(
                instance: *mut std::ffi::c_void,
                #(#ffi_args)*
                out: *mut ::xabi::XabiOwnedBytes,
            ) -> i32 {
                ::xabi::catch_unwind_code(|| {
                    let Some(plugin) = Self::__xabi_impl_ref::<P>(instance) else {
                        return ::xabi::ERR_INVALID_ARGUMENT;
                    };
                    let Some(out) = (unsafe { out.as_mut() }) else {
                        return ::xabi::ERR_INVALID_ARGUMENT;
                    };
                    #(#decoders)*
                    match plugin.#name(#(#call_args)*) {
                        Ok(value) => {
                            #ok
                            ::xabi::OK
                        }
                        Err(err) => {
                            *out = ::xabi::XabiType::into_payload(err);
                            ::xabi::ERR_EXPORT
                        }
                    }
                })
            }
        })
    }

    fn async_export_thunk(&self, trait_ident: &Ident) -> syn::Result<TokenStream2> {
        let name = &self.name;
        let ffi_args = self.ffi_arg_defs();
        let (decoders, call_args) = self.export_arg_decoding(true);
        let future = self.async_future_assignment(name, &call_args);

        Ok(quote! {
            unsafe extern "C" fn #name<P: #trait_ident>(
                instance: *mut std::ffi::c_void,
                #(#ffi_args)*
                out: *mut ::xabi::XabiFuture,
            ) -> i32 {
                ::xabi::catch_unwind_code(|| {
                    let Some(plugin) = Self::__xabi_impl_ref::<P>(instance) else {
                        return ::xabi::ERR_INVALID_ARGUMENT;
                    };
                    let Some(out) = (unsafe { out.as_mut() }) else {
                        return ::xabi::ERR_INVALID_ARGUMENT;
                    };
                    #(#decoders)*
                    #future
                    ::xabi::OK
                })
            }
        })
    }

    fn sync_ok_payload(&self) -> TokenStream2 {
        match &self.ret {
            MethodRet::ResultUnit(_) => quote! {
                *out = ::xabi::XabiOwnedBytes::empty();
            },
            MethodRet::ResultBytes(_) => quote! {
                *out = ::xabi::XabiOwnedBytes::from_vec(value);
            },
            MethodRet::ResultString(_) => quote! {
                *out = ::xabi::XabiOwnedBytes::from_string(value);
            },
            MethodRet::ResultOptionalBytes(_) => quote! {
                *out = value
                    .map(::xabi::XabiOwnedBytes::from_vec)
                    .unwrap_or_else(::xabi::XabiOwnedBytes::empty);
            },
            MethodRet::ResultOptionalString(_) => quote! {
                *out = value
                    .map(::xabi::XabiOwnedBytes::from_string)
                    .unwrap_or_else(::xabi::XabiOwnedBytes::empty);
            },
            MethodRet::ResultValue { .. } => quote! {
                *out = ::xabi::XabiType::into_payload(value);
            },
            MethodRet::ResultObject { trait_ident, .. } => {
                let abi_ident = format_ident!("XabiV1AbiTrait{}", trait_ident);
                let ret_ident = format_ident!("XabiV1OwnedRefTrait{}", trait_ident);
                quote! {
                    let raw = #abi_ident::xabi_export(value);
                    let wire = #ret_ident {
                        size: std::mem::size_of::<#ret_ident>(),
                        abi_version: #ret_ident::ABI_VERSION,
                        vtable: raw,
                    };
                    let bytes = unsafe {
                        std::slice::from_raw_parts(
                            std::ptr::addr_of!(wire).cast::<u8>(),
                            std::mem::size_of::<#ret_ident>(),
                        )
                    };
                    *out = ::xabi::XabiOwnedBytes::from_vec(bytes.to_vec());
                }
            }
            _ => quote! {},
        }
    }

    fn async_future_assignment(&self, name: &Ident, call_args: &[TokenStream2]) -> TokenStream2 {
        match &self.ret {
            MethodRet::ResultObject { trait_ident, .. } => {
                let abi_ident = format_ident!("XabiV1AbiTrait{}", trait_ident);
                let ret_ident = format_ident!("XabiV1OwnedRefTrait{}", trait_ident);
                quote! {
                    *out = ::xabi::XabiFuture::from_result_bytes(async move {
                        plugin.#name(#(#call_args)*).await.map(|value| {
                            let raw = #abi_ident::xabi_export(value);
                            let wire = #ret_ident {
                                size: std::mem::size_of::<#ret_ident>(),
                                abi_version: #ret_ident::ABI_VERSION,
                                vtable: raw,
                            };
                            let bytes = unsafe {
                                std::slice::from_raw_parts(
                                    std::ptr::addr_of!(wire).cast::<u8>(),
                                    std::mem::size_of::<#ret_ident>(),
                                )
                            };
                            bytes.to_vec()
                        })
                    });
                }
            }
            _ => {
                let future = self.async_future_expr();
                quote! {
                    let future = async move {
                        plugin.#name(#(#call_args)*).await
                    };
                    *out = #future;
                }
            }
        }
    }

    fn async_future_expr(&self) -> TokenStream2 {
        match &self.ret {
            MethodRet::ResultUnit(_) => quote! {
                ::xabi::XabiFuture::from_result_bytes(async move {
                    future.await.map(|()| Vec::new())
                })
            },
            MethodRet::ResultBytes(_) => quote! {
                ::xabi::XabiFuture::from_result_bytes(future)
            },
            MethodRet::ResultString(_) => quote! {
                ::xabi::XabiFuture::from_result_bytes(async move {
                    future.await.map(String::into_bytes)
                })
            },
            MethodRet::ResultOptionalBytes(_) => quote! {
                ::xabi::XabiFuture::from_result_bytes(async move {
                    future.await.map(|value| value.unwrap_or_default())
                })
            },
            MethodRet::ResultOptionalString(_) => quote! {
                ::xabi::XabiFuture::from_result_bytes(async move {
                    future.await.map(|value| value.unwrap_or_default().into_bytes())
                })
            },
            MethodRet::ResultValue { .. } => quote! {
                ::xabi::XabiFuture::from_result_value(future)
            },
            _ => quote! {
                ::xabi::XabiFuture::empty()
            },
        }
    }
}
