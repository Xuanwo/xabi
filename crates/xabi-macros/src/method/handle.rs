use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::Type;

use super::{generated_trait_type_path, MethodRet, MethodSpec};

#[derive(Clone, Copy)]
pub(crate) enum HandleDecode {
    Module,
    Local,
}

impl MethodSpec {
    pub(crate) fn handle_method(&self, decode: HandleDecode) -> syn::Result<TokenStream2> {
        if self.asyncness {
            return self.async_handle_method(decode);
        }

        let name = &self.name;
        let receiver = self.handle_receiver();
        if self.args.is_empty() {
            match self.ret {
                MethodRet::String => {
                    return Ok(quote! {
                        pub fn #name(#receiver) -> ::xabi::Result<String> {
                            let vtable = self.vtable();
                            if !vtable.field_available(stringify!(#name)) {
                                return Err(::xabi::Error::AbiMismatch(format!(
                                    "Xabi.{} is not available in this vtable",
                                    stringify!(#name),
                                )));
                            }
                            let out = unsafe { (vtable.#name)(vtable.instance) };
                            unsafe { out.to_string_and_free() }
                        }
                    });
                }
                MethodRet::U32 => {
                    return Ok(quote! {
                        pub fn #name(#receiver) -> ::xabi::Result<u32> {
                            let vtable = self.vtable();
                            if !vtable.field_available(stringify!(#name)) {
                                return Err(::xabi::Error::AbiMismatch(format!(
                                    "Xabi.{} is not available in this vtable",
                                    stringify!(#name),
                                )));
                            }
                            Ok(unsafe { (vtable.#name)(vtable.instance) })
                        }
                    });
                }
                MethodRet::Bool => {
                    return Ok(quote! {
                        pub fn #name(#receiver) -> ::xabi::Result<bool> {
                            let vtable = self.vtable();
                            if !vtable.field_available(stringify!(#name)) {
                                return Err(::xabi::Error::AbiMismatch(format!(
                                    "Xabi.{} is not available in this vtable",
                                    stringify!(#name),
                                )));
                            }
                            Ok(unsafe { (vtable.#name)(vtable.instance) != 0 })
                        }
                    });
                }
                MethodRet::Value(ref ty) => {
                    return Ok(quote! {
                        pub fn #name(#receiver) -> ::xabi::Result<#ty> {
                            let vtable = self.vtable();
                            if !vtable.field_available(stringify!(#name)) {
                                return Err(::xabi::Error::AbiMismatch(format!(
                                    "Xabi.{} is not available in this vtable",
                                    stringify!(#name),
                                )));
                            }
                            let out = unsafe { (vtable.#name)(vtable.instance) };
                            unsafe { <#ty as ::xabi::XabiType>::from_payload(out) }
                        }
                    });
                }
                _ => {}
            }
        }

        let error_ty = self.error_ty().expect("Result return has error type");
        let ok_ty = self.ok_type(decode);
        let args = self.handle_arg_defs();
        let (locals, call_args) = self.handle_arg_lowering();
        let ok_decode = self.ok_decode_expr(quote!(out), quote!(stringify!(#name)), decode);

        Ok(quote! {
            pub fn #name(
                #receiver,
                #(#args)*
            ) -> std::result::Result<#ok_ty, ::xabi::XabiCallError<#error_ty>> {
                let vtable = self.vtable();
                if !vtable.field_available(stringify!(#name)) {
                    return Err(::xabi::XabiCallError::Runtime(::xabi::Error::AbiMismatch(format!(
                        "Xabi.{} is not available in this vtable",
                        stringify!(#name),
                    ))));
                }
                #(#locals)*
                let mut out = ::xabi::XabiOwnedBytes::empty();
                let code = unsafe {
                    (vtable.#name)(
                        vtable.instance,
                        #(#call_args)*
                        &mut out,
                    )
                };
                match code {
                    ::xabi::OK => {
                        #ok_decode
                    }
                    ::xabi::ERR_EXPORT => {
                        match unsafe { <#error_ty as ::xabi::XabiType>::from_payload(out) } {
                            Ok(err) => Err(::xabi::XabiCallError::Export(err)),
                            Err(err) => Err(::xabi::XabiCallError::Runtime(err)),
                        }
                    }
                    _ => {
                        match ::xabi::status_to_result(code, concat!("Xabi.", stringify!(#name))) {
                            Ok(()) => {
                                #ok_decode
                            }
                            Err(err) => Err(::xabi::XabiCallError::Runtime(err)),
                        }
                    }
                }
            }
        })
    }

    fn async_handle_method(&self, decode: HandleDecode) -> syn::Result<TokenStream2> {
        let name = &self.name;
        let receiver = self.handle_receiver();
        let error_ty = self.error_ty().expect("Result return has error type");
        let ok_ty = self.ok_type(decode);
        let args = self.handle_arg_defs();
        let (locals, call_args) = self.handle_arg_lowering();
        let ok_decode = self.ok_decode_expr(quote!(payload), quote!(stringify!(#name)), decode);

        Ok(quote! {
            pub async fn #name(
                #receiver,
                #(#args)*
            ) -> std::result::Result<#ok_ty, ::xabi::XabiCallError<#error_ty>> {
                let vtable = self.vtable();
                if !vtable.field_available(stringify!(#name)) {
                    return Err(::xabi::XabiCallError::Runtime(::xabi::Error::AbiMismatch(format!(
                        "Xabi.{} is not available in this vtable",
                        stringify!(#name),
                    ))));
                }
                #(#locals)*
                let mut future = ::xabi::XabiFuture::empty();
                let code = unsafe {
                    (vtable.#name)(
                        vtable.instance,
                        #(#call_args)*
                        &mut future,
                    )
                };
                ::xabi::status_to_result(code, concat!("Xabi.", stringify!(#name)))
                    .map_err(::xabi::XabiCallError::Runtime)?;
                let bytes = ::xabi::XabiTypedFuture::<#error_ty>::new(future)
                    .map_err(::xabi::XabiCallError::Runtime)?
                    .await?;
                let payload = ::xabi::XabiOwnedBytes::from_vec(bytes);
                #ok_decode
            }
        })
    }

    fn handle_receiver(&self) -> TokenStream2 {
        if self.receiver_mut {
            quote!(&mut self)
        } else {
            quote!(&self)
        }
    }

    fn ok_type(&self, decode: HandleDecode) -> TokenStream2 {
        match &self.ret {
            MethodRet::ResultUnit(_) => quote!(()),
            MethodRet::ResultBytes(_) => quote!(Vec<u8>),
            MethodRet::ResultString(_) => quote!(String),
            MethodRet::ResultValue { ok, .. } => quote!(#ok),
            MethodRet::ResultObject { trait_path, .. } => match decode {
                HandleDecode::Module => {
                    let handle_ident = generated_trait_type_path(trait_path, "XabiV1HandleTrait");
                    quote!(#handle_ident)
                }
                HandleDecode::Local => {
                    let owned_ident = generated_trait_type_path(trait_path, "XabiV1OwnedTrait");
                    quote!(#owned_ident)
                }
            },
            MethodRet::ResultObjectPair { ok, trait_path, .. } => match decode {
                HandleDecode::Module => {
                    let handle_ident = generated_trait_type_path(trait_path, "XabiV1HandleTrait");
                    quote!((#ok, #handle_ident))
                }
                HandleDecode::Local => {
                    let owned_ident = generated_trait_type_path(trait_path, "XabiV1OwnedTrait");
                    quote!((#ok, #owned_ident))
                }
            },
            _ => quote!(()),
        }
    }

    fn ok_decode_expr(
        &self,
        payload: TokenStream2,
        method: TokenStream2,
        decode: HandleDecode,
    ) -> TokenStream2 {
        match &self.ret {
            MethodRet::ResultUnit(_) => quote! {
                let bytes = unsafe {
                    #payload
                        .to_vec_and_free()
                        .map_err(::xabi::XabiCallError::Runtime)?
                };
                if bytes.is_empty() {
                    Ok(())
                } else {
                    Err(::xabi::XabiCallError::Runtime(::xabi::Error::Export(
                        format!("Xabi.{} returned a non-empty unit payload", #method),
                    )))
                }
            },
            MethodRet::ResultBytes(_) => quote! {
                unsafe {
                    #payload
                        .to_vec_and_free()
                        .map_err(::xabi::XabiCallError::Runtime)
                }
            },
            MethodRet::ResultString(_) => quote! {
                unsafe {
                    #payload
                        .to_string_and_free()
                        .map_err(::xabi::XabiCallError::Runtime)
                }
            },
            MethodRet::ResultValue { ok, .. } => quote! {
                unsafe {
                    <#ok as ::xabi::XabiType>::from_payload(#payload)
                        .map_err(::xabi::XabiCallError::Runtime)
                }
            },
            MethodRet::ResultObject { trait_path, .. } => {
                let ret_ident = generated_trait_type_path(trait_path, "XabiV1OwnedRefTrait");
                match decode {
                    HandleDecode::Module => {
                        let handle_ident =
                            generated_trait_type_path(trait_path, "XabiV1HandleTrait");
                        quote! {
                            let wire = unsafe {
                                <#ret_ident as ::xabi::XabiType>::from_payload(#payload)
                                    .map_err(::xabi::XabiCallError::Runtime)?
                            };
                            unsafe {
                                #handle_ident::xabi_from_vtable(wire.vtable, self.xabi_module())
                                    .map_err(::xabi::XabiCallError::Runtime)
                            }
                        }
                    }
                    HandleDecode::Local => {
                        let owned_ident = generated_trait_type_path(trait_path, "XabiV1OwnedTrait");
                        quote! {
                            let wire = unsafe {
                                <#ret_ident as ::xabi::XabiType>::from_payload(#payload)
                                    .map_err(::xabi::XabiCallError::Runtime)?
                            };
                            unsafe {
                                #owned_ident::xabi_from_vtable(wire.vtable)
                                    .map_err(::xabi::XabiCallError::Runtime)
                            }
                        }
                    }
                }
            }
            MethodRet::ResultObjectPair { ok, trait_path, .. } => {
                let ret_ident = generated_trait_type_path(trait_path, "XabiV1OwnedRefTrait");
                let object_decode = match decode {
                    HandleDecode::Module => {
                        let handle_ident =
                            generated_trait_type_path(trait_path, "XabiV1HandleTrait");
                        quote! {
                            unsafe {
                                #handle_ident::xabi_from_vtable(object_wire.vtable, self.xabi_module())
                                    .map_err(::xabi::XabiCallError::Runtime)?
                            }
                        }
                    }
                    HandleDecode::Local => {
                        let owned_ident = generated_trait_type_path(trait_path, "XabiV1OwnedTrait");
                        quote! {
                            unsafe {
                                #owned_ident::xabi_from_vtable(object_wire.vtable)
                                    .map_err(::xabi::XabiCallError::Runtime)?
                            }
                        }
                    }
                };
                object_pair_decode_expr(ok, ret_ident, payload, method, object_decode)
            }
            _ => quote!(Ok(())),
        }
    }
}

fn object_pair_decode_expr(
    ok: &Type,
    ret_ident: TokenStream2,
    payload: TokenStream2,
    method: TokenStream2,
    object_decode: TokenStream2,
) -> TokenStream2 {
    quote! {
        #[repr(C)]
        #[derive(Clone, Copy)]
        struct __XabiResultObjectPair<OkWire: Copy + 'static, ObjectWire: Copy + 'static> {
            size: usize,
            abi_version: u32,
            ok: OkWire,
            object: ObjectWire,
        }
        let expected_size = std::mem::size_of::<
            __XabiResultObjectPair<
                <#ok as ::xabi::XabiType>::Wire,
                #ret_ident,
            >
        >();
        let bytes = unsafe {
            #payload
                .to_vec_and_free()
                .map_err(::xabi::XabiCallError::Runtime)?
        };
        if bytes.len() != expected_size {
            return Err(::xabi::XabiCallError::Runtime(::xabi::Error::AbiMismatch(
                format!(
                    "Xabi.{} returned payload size {}, expected {}",
                    #method,
                    bytes.len(),
                    expected_size,
                ),
            )));
        }
        let mut wire = std::mem::MaybeUninit::<
            __XabiResultObjectPair<
                <#ok as ::xabi::XabiType>::Wire,
                #ret_ident,
            >
        >::uninit();
        unsafe {
            std::ptr::copy_nonoverlapping(
                bytes.as_ptr(),
                wire.as_mut_ptr().cast::<u8>(),
                bytes.len(),
            );
        }
        let wire = unsafe { wire.assume_init() };
        ::xabi::validate_size(wire.size, expected_size, "__XabiResultObjectPair")
            .map_err(::xabi::XabiCallError::Runtime)?;
        ::xabi::validate_abi_version(
            wire.abi_version,
            ::xabi::ABI_VERSION,
            "__XabiResultObjectPair",
        )
        .map_err(::xabi::XabiCallError::Runtime)?;
        let value = unsafe {
            <#ok as ::xabi::XabiType>::from_wire(
                std::ptr::addr_of!(wire.ok)
            )
        }
        .map_err(::xabi::XabiCallError::Runtime)?;
        let object_wire = unsafe {
            <#ret_ident as ::xabi::XabiType>::from_wire(
                std::ptr::addr_of!(wire.object)
            )
        }
        .map_err(::xabi::XabiCallError::Runtime)?;
        let object = {
            #object_decode
        };
        Ok((value, object))
    }
}
