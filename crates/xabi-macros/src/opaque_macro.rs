use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{Error, ItemStruct, Type};

pub(crate) fn expand_opaque(attr: TokenStream2, item: TokenStream2) -> syn::Result<TokenStream2> {
    if !attr.is_empty() {
        return Err(Error::new_spanned(
            attr,
            "`#[xabi::opaque]` does not accept options",
        ));
    }

    let item_struct = syn::parse2::<ItemStruct>(item)?;
    if !item_struct.generics.params.is_empty() {
        return Err(Error::new_spanned(
            &item_struct.generics,
            "xabi opaque handles cannot be generic",
        ));
    }

    let syn::Fields::Named(fields) = &item_struct.fields else {
        return Err(Error::new_spanned(
            &item_struct.fields,
            "xabi opaque handles must use a named pointer field",
        ));
    };
    if fields.named.len() != 1 {
        return Err(Error::new_spanned(
            &item_struct.fields,
            "xabi opaque handles must contain exactly one pointer field",
        ));
    }
    let field = fields.named.first().expect("one field");
    let field_ident = field.ident.as_ref().expect("named field");
    let field_ty = &field.ty;
    if !matches!(field_ty, Type::Ptr(_)) {
        return Err(Error::new_spanned(
            field_ty,
            "xabi opaque handle field must be a raw pointer",
        ));
    }

    let vis = &item_struct.vis;
    let ident = &item_struct.ident;
    let wire_ident = format_ident!("XabiV1Opaque{}", ident);
    let field_ty_name = quote!(#field_ty)
        .to_string()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace("* mut ", "*mut ")
        .replace("* const ", "*const ");

    Ok(quote! {
        #item_struct

        #[repr(C)]
        #[derive(Clone, Copy)]
        #vis struct #wire_ident {
            pub size: usize,
            pub abi_version: u32,
            pub #field_ident: #field_ty,
        }

        impl #wire_ident {
            pub const ABI_VERSION: u32 = ::xabi::ABI_VERSION;
            pub const MIN_SIZE: usize = std::mem::offset_of!(#wire_ident, #field_ident)
                + std::mem::size_of::<#field_ty>();
            pub const FULL_SIZE: usize = std::mem::size_of::<Self>();

            pub fn validate(&self) -> ::xabi::Result<()> {
                ::xabi::validate_size(self.size, Self::MIN_SIZE, stringify!(#wire_ident))?;
                ::xabi::validate_abi_version(
                    self.abi_version,
                    Self::ABI_VERSION,
                    stringify!(#wire_ident),
                )?;
                if self.#field_ident.is_null() {
                    return Err(::xabi::Error::NullPointer(concat!(
                        stringify!(#wire_ident),
                        "::",
                        stringify!(#field_ident),
                    )));
                }
                Ok(())
            }
        }

        unsafe impl Send for #wire_ident {}
        unsafe impl Sync for #wire_ident {}

        impl #ident {
            pub unsafe fn from_raw(#field_ident: #field_ty) -> ::xabi::Result<Self> {
                if #field_ident.is_null() {
                    return Err(::xabi::Error::NullPointer(concat!(
                        stringify!(#ident),
                        "::",
                        stringify!(#field_ident),
                    )));
                }
                Ok(Self { #field_ident })
            }

            pub fn as_raw(&self) -> #field_ty {
                self.#field_ident
            }
        }

        impl ::xabi::XabiType for #ident {
            type Wire = #wire_ident;
            const WIRE_TYPE_NAME: &'static str = stringify!(#wire_ident);

            fn into_wire(self) -> Self::Wire {
                #wire_ident {
                    size: std::mem::size_of::<#wire_ident>(),
                    abi_version: #wire_ident::ABI_VERSION,
                    #field_ident: self.#field_ident,
                }
            }

            unsafe fn from_wire(wire: *const Self::Wire) -> ::xabi::Result<Self> {
                let wire = unsafe {
                    wire.as_ref()
                        .ok_or(::xabi::Error::NullPointer(concat!(stringify!(#wire_ident), " pointer")))?
                };
                wire.validate()?;
                Ok(Self {
                    #field_ident: wire.#field_ident,
                })
            }

            fn collect_xabi_layout(collector: &mut dyn ::xabi::XabiLayoutCollector) {
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
                    ::xabi::XabiFieldLayout::new(
                        stringify!(#field_ident),
                        std::mem::offset_of!(#wire_ident, #field_ident),
                        #field_ty_name,
                    ),
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
