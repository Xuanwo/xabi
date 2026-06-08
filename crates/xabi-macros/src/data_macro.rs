use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{Error, ItemStruct};

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

    let vis = &item_struct.vis;
    let ident = &item_struct.ident;
    let wire_ident = format_ident!("XabiV1Data{}", ident);
    let field_idents = fields
        .named
        .iter()
        .map(|field| field.ident.as_ref().expect("named field"))
        .collect::<Vec<_>>();
    let field_tys = fields
        .named
        .iter()
        .map(|field| &field.ty)
        .collect::<Vec<_>>();

    Ok(quote! {
        #item_struct

        #[repr(C)]
        #[derive(Clone, Copy)]
        #vis struct #wire_ident {
            pub size: usize,
            pub abi_version: u32,
            #(pub #field_idents: #field_tys,)*
        }

        impl #wire_ident {
            pub const ABI_VERSION: u32 = ::xabi::ABI_VERSION;
            pub const MIN_SIZE: usize = std::mem::size_of::<Self>();

            pub fn validate(&self) -> ::xabi::Result<()> {
                ::xabi::validate_size(self.size, Self::MIN_SIZE, stringify!(#wire_ident))?;
                ::xabi::validate_abi_version(
                    self.abi_version,
                    Self::ABI_VERSION,
                    stringify!(#wire_ident),
                )?;
                Ok(())
            }
        }

        impl #ident {
            pub fn new(#(#field_idents: #field_tys),*) -> Self {
                Self {
                    #(#field_idents,)*
                }
            }
        }

        impl ::xabi::XabiType for #ident {
            type Wire = #wire_ident;

            fn into_wire(self) -> Self::Wire {
                let mut wire = std::mem::MaybeUninit::<#wire_ident>::zeroed();
                unsafe {
                    let wire_ptr = wire.as_mut_ptr();
                    std::ptr::addr_of_mut!((*wire_ptr).size)
                        .write(std::mem::size_of::<#wire_ident>());
                    std::ptr::addr_of_mut!((*wire_ptr).abi_version)
                        .write(#wire_ident::ABI_VERSION);
                    #(std::ptr::addr_of_mut!((*wire_ptr).#field_idents)
                        .write(self.#field_idents);)*
                    wire.assume_init()
                }
            }

            unsafe fn from_wire(wire: *const Self::Wire) -> ::xabi::Result<Self> {
                let wire = unsafe {
                    wire.as_ref()
                        .ok_or(::xabi::Error::NullPointer(concat!(stringify!(#wire_ident), " pointer")))?
                };
                wire.validate()?;
                Ok(Self {
                    #(#field_idents: wire.#field_idents,)*
                })
            }
        }
    })
}
