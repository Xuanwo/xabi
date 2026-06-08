use syn::parse::{Parse, ParseStream};
use syn::{Error, Expr, MetaNameValue, Token};

pub(crate) struct TraitArgs {
    pub(crate) id: Expr,
    pub(crate) version: Expr,
}

impl Parse for TraitArgs {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let values =
            syn::punctuated::Punctuated::<MetaNameValue, Token![,]>::parse_terminated(input)?;
        let mut id = None;
        let mut version = None;

        for value in values {
            let Some(ident) = value.path.get_ident() else {
                return Err(Error::new_spanned(value.path, "expected identifier"));
            };
            match ident.to_string().as_str() {
                "id" => id = Some(value.value),
                "version" => version = Some(value.value),
                other => {
                    return Err(Error::new_spanned(
                        ident,
                        format!("unsupported xabi option `{other}` for trait ABI"),
                    ));
                }
            }
        }

        Ok(Self {
            id: id.ok_or_else(|| input.error("missing `id = ...`"))?,
            version: version.ok_or_else(|| input.error("missing `version = ...`"))?,
        })
    }
}

pub(crate) struct ImplArgs {
    pub(crate) name: Expr,
    pub(crate) version: Expr,
    pub(crate) constructor: Option<Expr>,
}

impl Parse for ImplArgs {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let values =
            syn::punctuated::Punctuated::<MetaNameValue, Token![,]>::parse_terminated(input)?;
        let mut name = None;
        let mut version = None;
        let mut constructor = None;

        for value in values {
            let Some(ident) = value.path.get_ident() else {
                return Err(Error::new_spanned(value.path, "expected identifier"));
            };
            match ident.to_string().as_str() {
                "name" => name = Some(value.value),
                "version" => version = Some(value.value),
                "constructor" => constructor = Some(value.value),
                other => {
                    return Err(Error::new_spanned(
                        ident,
                        format!("unsupported xabi option `{other}` for implementation export"),
                    ));
                }
            }
        }

        Ok(Self {
            name: name.ok_or_else(|| input.error("missing `name = ...`"))?,
            version: version.ok_or_else(|| input.error("missing `version = ...`"))?,
            constructor,
        })
    }
}
