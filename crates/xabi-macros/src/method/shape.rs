use syn::{
    parse_quote, Error, GenericArgument, Pat, PathArguments, ReturnType, TraitItemFn, Type,
    TypeParamBound,
};

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
    } else if is_str_ref(&ty) {
        ArgKind::Str
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
    if is_result_type(ty) {
        parse_result_ret(ty)
    } else {
        Ok(MethodRet::Value((**ty).clone()))
    }
}

pub(super) fn validate_shape(
    method: &TraitItemFn,
    args: &[MethodArg],
    ret: &MethodRet,
    asyncness: bool,
) -> syn::Result<()> {
    if matches!(
        ret,
        MethodRet::String | MethodRet::U32 | MethodRet::Bool | MethodRet::Value(_)
    ) && !args.is_empty()
    {
        return Err(Error::new_spanned(
            method,
            "non-Result xabi methods cannot take arguments",
        ));
    }
    if asyncness
        && matches!(
            ret,
            MethodRet::String | MethodRet::U32 | MethodRet::Bool | MethodRet::Value(_)
        )
    {
        return Err(Error::new_spanned(
            method,
            "async xabi methods must return Result",
        ));
    }
    Ok(())
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
    if is_ident_type(payload, "String") {
        return Ok(MethodRet::ResultString(error_ty));
    }
    if is_vec_u8(payload) {
        return Ok(MethodRet::ResultBytes(error_ty));
    }
    if let Some((trait_path, trait_ident)) = xabi_object_trait(payload)? {
        return Ok(MethodRet::ResultObject {
            trait_path,
            trait_ident,
            error: error_ty,
        });
    }
    Ok(MethodRet::ResultValue {
        ok: (*payload).clone(),
        error: error_ty,
    })
}

fn is_result_type(ty: &Type) -> bool {
    let Type::Path(path) = ty else {
        return false;
    };
    path.path
        .segments
        .last()
        .map(|segment| segment.ident == "Result")
        .unwrap_or(false)
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

fn is_str_ref(ty: &Type) -> bool {
    let Type::Reference(reference) = ty else {
        return false;
    };
    is_ident_type(&reference.elem, "str")
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

fn xabi_object_trait(ty: &Type) -> syn::Result<Option<(syn::Path, syn::Ident)>> {
    let Type::ImplTrait(impl_trait) = ty else {
        return Ok(None);
    };
    let Some(TypeParamBound::Trait(bound)) =
        impl_trait.bounds.iter().find_map(|bound| match bound {
            TypeParamBound::Trait(bound) => Some(TypeParamBound::Trait(bound.clone())),
            _ => None,
        })
    else {
        return Err(Error::new_spanned(
            impl_trait,
            "xabi object returns must use `impl SomeXabiTrait`",
        ));
    };
    if bound.path.segments.len() != 1 {
        return Err(Error::new_spanned(
            bound.path,
            "xabi object returns currently require an in-scope trait identifier",
        ));
    }
    let trait_ident = bound
        .path
        .segments
        .last()
        .expect("one segment")
        .ident
        .clone();
    Ok(Some((bound.path, trait_ident)))
}
