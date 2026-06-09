use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{Ident, Type};

use super::{MethodRet, MethodSpec, generated_trait_type_path};

impl MethodSpec {
    pub(crate) fn export_thunk(&self, trait_ident: &Ident) -> syn::Result<TokenStream2> {
        if self.asyncness {
            return self.async_export_thunk(trait_ident);
        }

        let name = &self.name;
        let impl_ref = self.impl_ref_expr();
        if self.args.is_empty() {
            match self.ret {
                MethodRet::String => {
                    return Ok(quote! {
                        unsafe extern "C" fn #name<P: #trait_ident>(
                            instance: *mut std::ffi::c_void,
                        ) -> ::xabi::XabiOwnedBytes {
                            ::xabi::catch_unwind_owned(|| {
                                let Some(plugin) = #impl_ref else {
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
                                let Some(plugin) = #impl_ref else {
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
                                let Some(plugin) = #impl_ref else {
                                    return 0;
                                };
                                plugin.#name() as u8
                            })
                        }
                    });
                }
                MethodRet::Value(_) => {
                    return Ok(quote! {
                        unsafe extern "C" fn #name<P: #trait_ident>(
                            instance: *mut std::ffi::c_void,
                        ) -> ::xabi::XabiOwnedBytes {
                            ::xabi::catch_unwind_owned(|| {
                                let Some(plugin) = #impl_ref else {
                                    return ::xabi::XabiOwnedBytes::empty();
                                };
                                ::xabi::XabiType::into_payload(plugin.#name())
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
                    let Some(plugin) = #impl_ref else {
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
        let impl_ref = self.impl_ref_expr();
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
                    let Some(plugin) = #impl_ref else {
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
            MethodRet::ResultValue { .. } => quote! {
                *out = ::xabi::XabiType::into_payload(value);
            },
            MethodRet::ResultObject { trait_path, .. } => {
                let payload = object_payload_expr(trait_path);
                quote! {
                    *out = ::xabi::XabiOwnedBytes::from_vec({
                        #payload
                    });
                }
            }
            MethodRet::ResultObjectPair { ok, trait_path, .. } => {
                let payload = object_pair_payload_expr(ok, trait_path);
                quote! {
                    let (value, object) = value;
                    *out = ::xabi::XabiOwnedBytes::from_vec({
                        #payload
                    });
                }
            }
            _ => quote! {},
        }
    }

    fn async_future_assignment(&self, name: &Ident, call_args: &[TokenStream2]) -> TokenStream2 {
        match &self.ret {
            MethodRet::ResultObject { trait_path, .. } => {
                let payload = object_payload_expr(trait_path);
                quote! {
                    *out = ::xabi::XabiFuture::from_result_bytes(async move {
                        plugin.#name(#(#call_args)*).await.map(|value| {
                            #payload
                        })
                    });
                }
            }
            MethodRet::ResultObjectPair { ok, trait_path, .. } => {
                let payload = object_pair_payload_expr(ok, trait_path);
                quote! {
                    *out = ::xabi::XabiFuture::from_result_bytes(async move {
                        plugin.#name(#(#call_args)*).await.map(|(value, object)| {
                            #payload
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

    fn impl_ref_expr(&self) -> TokenStream2 {
        if self.receiver_mut {
            quote!(Self::__xabi_impl_mut::<P>(instance))
        } else {
            quote!(Self::__xabi_impl_ref::<P>(instance))
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
            MethodRet::ResultValue { .. } => quote! {
                ::xabi::XabiFuture::from_result_value(future)
            },
            MethodRet::ResultObjectPair { .. } => quote! {
                ::xabi::XabiFuture::empty()
            },
            _ => quote! {
                ::xabi::XabiFuture::empty()
            },
        }
    }
}

fn object_payload_expr(trait_path: &syn::Path) -> TokenStream2 {
    let abi_ident = generated_trait_type_path(trait_path, "XabiV1AbiTrait");
    let ret_ident = generated_trait_type_path(trait_path, "XabiV1OwnedRefTrait");
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
        bytes.to_vec()
    }
}

fn object_pair_payload_expr(ok: &Type, trait_path: &syn::Path) -> TokenStream2 {
    let abi_ident = generated_trait_type_path(trait_path, "XabiV1AbiTrait");
    let ret_ident = generated_trait_type_path(trait_path, "XabiV1OwnedRefTrait");
    quote! {
        let raw = #abi_ident::xabi_export(object);
        let __xabi_object_wire = #ret_ident {
            size: std::mem::size_of::<#ret_ident>(),
            abi_version: #ret_ident::ABI_VERSION,
            vtable: raw,
        };
        let __xabi_ok_wire = <#ok as ::xabi::XabiType>::into_wire(value);
        #[repr(C)]
        #[derive(Clone, Copy)]
        struct __XabiResultObjectPair<OkWire: Copy + 'static, ObjectWire: Copy + 'static> {
            size: usize,
            abi_version: u32,
            ok: OkWire,
            object: ObjectWire,
        }
        let __xabi_pair_size = std::mem::size_of::<
            __XabiResultObjectPair<
                <#ok as ::xabi::XabiType>::Wire,
                #ret_ident,
            >
        >();
        let mut __xabi_wire = std::mem::MaybeUninit::<
            __XabiResultObjectPair<
                <#ok as ::xabi::XabiType>::Wire,
                #ret_ident,
            >
        >::zeroed();
        unsafe {
            let __xabi_wire_ptr = __xabi_wire.as_mut_ptr();
            std::ptr::addr_of_mut!((*__xabi_wire_ptr).size).write(__xabi_pair_size);
            std::ptr::addr_of_mut!((*__xabi_wire_ptr).abi_version).write(::xabi::ABI_VERSION);
            std::ptr::addr_of_mut!((*__xabi_wire_ptr).ok)
                .write(__xabi_ok_wire);
            std::ptr::addr_of_mut!((*__xabi_wire_ptr).object)
                .write(__xabi_object_wire);
            let __xabi_wire = __xabi_wire.assume_init();
            let bytes = std::slice::from_raw_parts(
                std::ptr::addr_of!(__xabi_wire).cast::<u8>(),
                std::mem::size_of_val(&__xabi_wire),
            );
            bytes.to_vec()
        }
    }
}
