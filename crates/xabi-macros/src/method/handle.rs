use proc_macro2::TokenStream as TokenStream2;
use quote::quote;

use super::{MethodRet, MethodSpec};

impl MethodSpec {
    pub(crate) fn handle_method(&self) -> syn::Result<TokenStream2> {
        if self.asyncness {
            return self.async_handle_method();
        }

        let name = &self.name;
        if self.args.is_empty() {
            match self.ret {
                MethodRet::String => {
                    return Ok(quote! {
                        pub fn #name(&self) -> ::xabi::Result<String> {
                            let out = unsafe { (self.vtable().#name)(self.vtable().instance) };
                            unsafe { out.to_string_and_free() }
                        }
                    });
                }
                MethodRet::U32 => {
                    return Ok(quote! {
                        pub fn #name(&self) -> u32 {
                            unsafe { (self.vtable().#name)(self.vtable().instance) }
                        }
                    });
                }
                MethodRet::Bool => {
                    return Ok(quote! {
                        pub fn #name(&self) -> bool {
                            unsafe { (self.vtable().#name)(self.vtable().instance) != 0 }
                        }
                    });
                }
                _ => {}
            }
        }

        let error_ty = self.error_ty().expect("Result return has error type");
        let ok_ty = self.ok_type();
        let args = self.handle_arg_defs();
        let (locals, call_args) = self.handle_arg_lowering();
        let ok_decode = self.ok_decode_expr(quote!(out), quote!(stringify!(#name)));

        Ok(quote! {
            pub fn #name(
                &self,
                #(#args)*
            ) -> std::result::Result<#ok_ty, ::xabi::XabiCallError<#error_ty>> {
                #(#locals)*
                let mut out = ::xabi::XabiOwnedBytes::empty();
                let code = unsafe {
                    (self.vtable().#name)(
                        self.vtable().instance,
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

    fn async_handle_method(&self) -> syn::Result<TokenStream2> {
        let name = &self.name;
        let error_ty = self.error_ty().expect("Result return has error type");
        let ok_ty = self.ok_type();
        let args = self.handle_arg_defs();
        let (locals, call_args) = self.handle_arg_lowering();
        let ok_decode = self.ok_decode_expr(quote!(payload), quote!(stringify!(#name)));

        Ok(quote! {
            pub async fn #name(
                &self,
                #(#args)*
            ) -> std::result::Result<#ok_ty, ::xabi::XabiCallError<#error_ty>> {
                #(#locals)*
                let mut future = ::xabi::XabiFuture::empty();
                let code = unsafe {
                    (self.vtable().#name)(
                        self.vtable().instance,
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

    fn ok_type(&self) -> TokenStream2 {
        match &self.ret {
            MethodRet::ResultUnit(_) => quote!(()),
            MethodRet::ResultBytes(_) => quote!(Vec<u8>),
            MethodRet::ResultString(_) => quote!(String),
            MethodRet::ResultOptionalBytes(_) => quote!(Option<Vec<u8>>),
            MethodRet::ResultOptionalString(_) => quote!(Option<String>),
            MethodRet::ResultValue { ok, .. } => quote!(#ok),
            _ => quote!(()),
        }
    }

    fn ok_decode_expr(&self, payload: TokenStream2, method: TokenStream2) -> TokenStream2 {
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
            MethodRet::ResultOptionalBytes(_) => quote! {
                let bytes = unsafe {
                    #payload
                        .to_vec_and_free()
                        .map_err(::xabi::XabiCallError::Runtime)?
                };
                if bytes.is_empty() {
                    Ok(None)
                } else {
                    Ok(Some(bytes))
                }
            },
            MethodRet::ResultOptionalString(_) => quote! {
                let value = unsafe {
                    #payload
                        .to_string_and_free()
                        .map_err(::xabi::XabiCallError::Runtime)?
                };
                if value.is_empty() {
                    Ok(None)
                } else {
                    Ok(Some(value))
                }
            },
            MethodRet::ResultValue { ok, .. } => quote! {
                unsafe {
                    <#ok as ::xabi::XabiType>::from_payload(#payload)
                        .map_err(::xabi::XabiCallError::Runtime)
                }
            },
            _ => quote!(Ok(())),
        }
    }
}
