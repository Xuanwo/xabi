use syn::{parse_quote, Error, GenericArgument, Pat, PathArguments, ReturnType, TraitItemFn, Type};

use super::{ArgKind, MethodArg, MethodRet};

pub(super) fn parse_arg(arg: &syn::PatType) -> syn::Result<MethodArg> {
    let Pat::Ident(name) = arg.pat.as_ref() else {
        return Err(Error::new_spanned(
            &arg.pat,
            "argument must be an identifier",
        ));
    };
    let ty = (*arg.ty).clone();
    let kind = if is_bytes_ref(&ty) {
        ArgKind::Bytes
    } else {
        ArgKind::Value
    };
    Ok(MethodArg {
        name: name.ident.clone(),
        ty,
        kind,
    })
}

pub(super) fn parse_ret(output: &ReturnType) -> syn::Result<MethodRet> {
    let ReturnType::Type(_, ty) = output else {
        return Err(Error::new_spanned(
            output,
            "xabi methods must return a value",
        ));
    };
    if is_ident_type(ty, "String") {
        return Ok(MethodRet::String);
    }
    if is_ident_type(ty, "u32") {
        return Ok(MethodRet::U32);
    }
    if is_ident_type(ty, "bool") {
        return Ok(MethodRet::Bool);
    }
    parse_result_ret(ty)
}

pub(super) fn validate_shape(
    method: &TraitItemFn,
    arg: Option<&MethodArg>,
    ret: &MethodRet,
    asyncness: bool,
) -> syn::Result<()> {
    if asyncness {
        match (arg.map(|arg| arg.kind), ret) {
            (Some(ArgKind::Value), MethodRet::ResultBytes(_))
            | (Some(ArgKind::Bytes), MethodRet::ResultUnit(_)) => Ok(()),
            _ => Err(Error::new_spanned(
                method,
                "async xabi methods currently support `async fn method(&self, input: ReprC) -> Result<Vec<u8>>` and `async fn method(&self, bytes: &[u8]) -> Result<()>`",
            )),
        }
    } else {
        match (arg.map(|arg| arg.kind), ret) {
            (None, MethodRet::String | MethodRet::U32 | MethodRet::Bool)
            | (
                Some(ArgKind::Bytes),
                MethodRet::ResultUnit(_) | MethodRet::ResultOptionalBytes(_),
            )
            | (Some(ArgKind::Value), MethodRet::ResultBytes(_)) => Ok(()),
            _ => Err(Error::new_spanned(method, "unsupported xabi method shape")),
        }
    }
}

fn parse_result_ret(ty: &Type) -> syn::Result<MethodRet> {
    let Type::Path(path) = ty else {
        return Err(Error::new_spanned(ty, "unsupported return type"));
    };
    let Some(segment) = path.path.segments.last() else {
        return Err(Error::new_spanned(ty, "unsupported return type"));
    };
    if segment.ident != "Result" {
        return Err(Error::new_spanned(ty, "unsupported return type"));
    }
    let PathArguments::AngleBracketed(args) = &segment.arguments else {
        return Err(Error::new_spanned(ty, "Result must have a payload type"));
    };
    let type_args = args
        .args
        .iter()
        .filter_map(|arg| match arg {
            GenericArgument::Type(ty) => Some(ty),
            _ => None,
        })
        .collect::<Vec<_>>();
    let Some(payload) = type_args.first() else {
        return Err(Error::new_spanned(ty, "Result must have a payload type"));
    };
    let error_ty = type_args
        .get(1)
        .map(|ty| (*ty).clone())
        .unwrap_or_else(|| parse_quote!(::xabi::Error));

    if is_unit_type(payload) {
        return Ok(MethodRet::ResultUnit(error_ty));
    }
    if is_vec_u8(payload) {
        return Ok(MethodRet::ResultBytes(error_ty));
    }
    if is_option_vec_u8(payload) {
        return Ok(MethodRet::ResultOptionalBytes(error_ty));
    }
    Err(Error::new_spanned(
        payload,
        "unsupported Result payload type",
    ))
}

fn is_ident_type(ty: &Type, expected: &str) -> bool {
    let Type::Path(path) = ty else {
        return false;
    };
    path.path
        .segments
        .last()
        .map(|segment| segment.ident == expected)
        .unwrap_or(false)
}

fn is_unit_type(ty: &Type) -> bool {
    matches!(ty, Type::Tuple(tuple) if tuple.elems.is_empty())
}

fn is_bytes_ref(ty: &Type) -> bool {
    let Type::Reference(reference) = ty else {
        return false;
    };
    let Type::Slice(slice) = reference.elem.as_ref() else {
        return false;
    };
    is_ident_type(&slice.elem, "u8")
}

fn is_vec_u8(ty: &Type) -> bool {
    let Type::Path(path) = ty else {
        return false;
    };
    let Some(segment) = path.path.segments.last() else {
        return false;
    };
    if segment.ident != "Vec" {
        return false;
    }
    let PathArguments::AngleBracketed(args) = &segment.arguments else {
        return false;
    };
    matches!(args.args.first(), Some(GenericArgument::Type(ty)) if is_ident_type(ty, "u8"))
}

fn is_option_vec_u8(ty: &Type) -> bool {
    let Type::Path(path) = ty else {
        return false;
    };
    let Some(segment) = path.path.segments.last() else {
        return false;
    };
    if segment.ident != "Option" {
        return false;
    }
    let PathArguments::AngleBracketed(args) = &segment.arguments else {
        return false;
    };
    matches!(args.args.first(), Some(GenericArgument::Type(ty)) if is_vec_u8(ty))
}
