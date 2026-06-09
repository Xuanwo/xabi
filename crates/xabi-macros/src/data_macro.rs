use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{Error, ItemStruct};

use crate::type_shape::{XabiValueContext, validate_xabi_value_type};

pub(crate) fn expand_data(attr: TokenStream2, item: TokenStream2) -> syn::Result<TokenStream2> {
    if !attr.is_empty() {
        return Err(Error::new_spanned(
            attr,
            "`#[xabi::data]` does not accept options",
        ));
    }

    let item_struct = syn::parse2::<ItemStruct>(item)?;
    if !item_struct.generics.params.is_empty() {
        return Err(Error::new_spanned(
            &item_struct.generics,
            "xabi data types cannot be generic",
        ));
    }

    let syn::Fields::Named(fields) = &item_struct.fields else {
        return Err(Error::new_spanned(
            &item_struct.fields,
            "xabi data types must use named fields",
        ));
    };
    for field in &fields.named {
        validate_xabi_value_type(&field.ty, XabiValueContext::DataField)?;
    }

    let vis = &item_struct.vis;
    let ident = &item_struct.ident;
    let wire_ident = format_ident!("XabiV1Data{}", ident);
    let wire_struct_ident = &wire_ident;
    let field_idents = fields
        .named
        .iter()
        .map(|field| field.ident.as_ref().expect("named field"))
        .collect::<Vec<_>>();
    let wire_field_idents = field_idents
        .iter()
        .map(|ident| wire_field_ident(ident))
        .collect::<Vec<_>>();
    let field_tys = fields
        .named
        .iter()
        .map(|field| &field.ty)
        .collect::<Vec<_>>();
    let field_available_arms = fields
        .named
        .iter()
        .zip(wire_field_idents.iter())
        .map(|field| {
            let (field, wire_field_ident) = field;
            let ident = field.ident.as_ref().expect("named field");
            quote! {
                stringify!(#ident) => {
                    let field_end = std::mem::offset_of!(#wire_struct_ident, #wire_field_ident)
                        + std::mem::size_of_val(&self.#wire_field_ident);
                    self.size >= field_end
                }
            }
        })
        .collect::<Vec<_>>();
    let constructor_args = fields
        .named
        .iter()
        .map(|field| {
            let ident = field.ident.as_ref().expect("named field");
            let ty = &field.ty;
            if is_string_type(ty) {
                quote!(#ident: impl Into<#ty>)
            } else {
                quote!(#ident: #ty)
            }
        })
        .collect::<Vec<_>>();
    let constructor_fields = fields
        .named
        .iter()
        .map(|field| {
            let ident = field.ident.as_ref().expect("named field");
            let ty = &field.ty;
            if is_string_type(ty) {
                quote!(#ident: #ident.into())
            } else {
                quote!(#ident)
            }
        })
        .collect::<Vec<_>>();

    Ok(quote! {
        #item_struct

        #[repr(C)]
        #[derive(Clone, Copy)]
        #vis struct #wire_ident {
            pub size: usize,
            pub abi_version: u32,
            #(pub #wire_field_idents: <#field_tys as ::xabi::XabiType>::Wire,)*
        }

        impl #wire_ident {
            pub const ABI_VERSION: u32 = ::xabi::ABI_VERSION;
            pub const MIN_SIZE: usize = std::mem::offset_of!(#wire_ident, abi_version)
                + std::mem::size_of::<u32>();
            pub const FULL_SIZE: usize = std::mem::size_of::<Self>();

            pub fn validate(&self) -> ::xabi::Result<()> {
                ::xabi::validate_size(self.size, Self::MIN_SIZE, stringify!(#wire_ident))?;
                ::xabi::validate_abi_version(
                    self.abi_version,
                    Self::ABI_VERSION,
                    stringify!(#wire_ident),
                )?;
                Ok(())
            }

            pub fn field_available(&self, field: &str) -> bool {
                match field {
                    #(#field_available_arms,)*
                    _ => false,
                }
            }
        }

        impl #ident {
            #[allow(clippy::too_many_arguments)]
            pub fn new(#(#constructor_args),*) -> Self {
                Self {
                    #(#constructor_fields,)*
                }
            }
        }

        impl ::xabi::XabiType for #ident {
            type Wire = #wire_ident;
            const WIRE_TYPE_NAME: &'static str = stringify!(#wire_ident);

            fn into_wire(self) -> Self::Wire {
                let mut wire = std::mem::MaybeUninit::<#wire_ident>::zeroed();
                unsafe {
                    let wire_ptr = wire.as_mut_ptr();
                    std::ptr::addr_of_mut!((*wire_ptr).size)
                        .write(std::mem::size_of::<#wire_ident>());
                    std::ptr::addr_of_mut!((*wire_ptr).abi_version)
                        .write(#wire_ident::ABI_VERSION);
                    #(std::ptr::addr_of_mut!((*wire_ptr).#wire_field_idents)
                        .write(::xabi::XabiType::into_wire(self.#field_idents));)*
                    wire.assume_init()
                }
            }

            unsafe fn from_wire(wire: *const Self::Wire) -> ::xabi::Result<Self> {
                let wire = unsafe {
                    wire.as_ref()
                        .ok_or(::xabi::Error::NullPointer(concat!(stringify!(#wire_ident), " pointer")))?
                };
                wire.validate()?;
                #(
                    if !wire.field_available(stringify!(#field_idents)) {
                        return Err(::xabi::Error::AbiMismatch(format!(
                            "{} is missing required field {}",
                            stringify!(#wire_ident),
                            stringify!(#field_idents),
                        )));
                    }
                )*
                Ok(Self {
                    #(#field_idents: unsafe {
                        <#field_tys as ::xabi::XabiType>::from_wire(
                            std::ptr::addr_of!(wire.#wire_field_idents)
                        )
                    }?,)*
                })
            }

            fn collect_xabi_layout(collector: &mut dyn ::xabi::XabiLayoutCollector) {
                #(<#field_tys as ::xabi::XabiType>::collect_xabi_layout(collector);)*
                const __XABI_FIELDS: &[::xabi::XabiFieldLayout] = &[
                    ::xabi::XabiFieldLayout::new(
                        "size",
                        std::mem::offset_of!(#wire_ident, size),
                        "usize",
                    ),
                    ::xabi::XabiFieldLayout::new(
                        "abi_version",
                        std::mem::offset_of!(#wire_ident, abi_version),
                        "u32",
                    ),
                    #(
                        ::xabi::XabiFieldLayout::new(
                            stringify!(#field_idents),
                            std::mem::offset_of!(#wire_ident, #wire_field_idents),
                            <#field_tys as ::xabi::XabiType>::WIRE_TYPE_NAME,
                        ),
                    )*
                ];
                collector.push(::xabi::XabiLayoutItem::Type(::xabi::XabiTypeLayout::new(
                    concat!(module_path!(), "::", stringify!(#wire_ident)),
                    ::xabi::XabiLayoutStability::Prefix,
                    std::mem::size_of::<#wire_ident>(),
                    std::mem::align_of::<#wire_ident>(),
                    __XABI_FIELDS,
                )));
            }
        }
    })
}

fn wire_field_ident(ident: &syn::Ident) -> syn::Ident {
    match ident.to_string().as_str() {
        "size" | "abi_version" => format_ident!("__xabi_field_{}", ident),
        _ => ident.clone(),
    }
}

fn is_string_type(ty: &syn::Type) -> bool {
    let syn::Type::Path(path) = ty else {
        return false;
    };
    path.path
        .segments
        .last()
        .map(|segment| segment.ident == "String")
        .unwrap_or(false)
}
