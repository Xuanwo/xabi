use syn::{
    Error, GenericArgument, Pat, PathArguments, ReturnType, TraitItemFn, Type, TypeParamBound,
    parse_quote,
};

use crate::type_shape::{XabiValueContext, validate_return_type, validate_xabi_value_type};

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
        validate_xabi_value_type(&ty, XabiValueContext::MethodArgument)?;
        ArgKind::Value
    };
    Ok(MethodArg {
        name: name.ident.clone(),
        ty,
        kind,
    })
}

pub(super) fn parse_ret(output: &ReturnType) -> syn::Result<MethodRet> {
    let ty = validate_return_type(output)?;
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
        validate_xabi_value_type(ty, XabiValueContext::MethodReturn)?;
        Ok(MethodRet::Value(ty.clone()))
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
    if type_args.len() > 2 {
        return Err(Error::new_spanned(
            ty,
            "Result must have one payload type and at most one error type",
        ));
    }
    let Some(payload) = type_args.first() else {
        return Err(Error::new_spanned(ty, "Result must have a payload type"));
    };
    let error_ty = type_args
        .get(1)
        .map(|ty| (*ty).clone())
        .unwrap_or_else(|| parse_quote!(::xabi::Error));
    validate_xabi_value_type(&error_ty, XabiValueContext::ResultError)?;

    if is_unit_type(payload) {
        return Ok(MethodRet::ResultUnit(error_ty));
    }
    if is_ident_type(payload, "String") {
        return Ok(MethodRet::ResultString(error_ty));
    }
    if is_vec_u8(payload) {
        return Ok(MethodRet::ResultBytes(error_ty));
    }
    if let Some(trait_path) = xabi_object_trait(payload)? {
        return Ok(MethodRet::ResultObject {
            trait_path,
            error: error_ty,
        });
    }
    if let Some((ok, trait_path)) = xabi_object_pair(payload)? {
        validate_xabi_value_type(&ok, XabiValueContext::ResultPayload)?;
        return Ok(MethodRet::ResultObjectPair {
            ok,
            trait_path,
            error: error_ty,
        });
    }
    if matches!(payload, Type::Tuple(_)) {
        return Err(Error::new_spanned(
            payload,
            "xabi Result tuple payloads are only supported for object pairs `(T, impl SomeXabiTrait + 'static)`; use `#[xabi::data]` for structured values",
        ));
    }
    validate_xabi_value_type(payload, XabiValueContext::ResultPayload)?;
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
    if reference.mutability.is_some() {
        return false;
    }
    let Type::Slice(slice) = reference.elem.as_ref() else {
        return false;
    };
    is_ident_type(&slice.elem, "u8")
}

fn is_str_ref(ty: &Type) -> bool {
    let Type::Reference(reference) = ty else {
        return false;
    };
    if reference.mutability.is_some() {
        return false;
    }
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

fn xabi_object_trait(ty: &Type) -> syn::Result<Option<syn::Path>> {
    let Type::ImplTrait(impl_trait) = ty else {
        return Ok(None);
    };
    let has_static = impl_trait.bounds.iter().any(
        |bound| matches!(bound, TypeParamBound::Lifetime(lifetime) if lifetime.ident == "static"),
    );
    if !has_static {
        return Err(Error::new_spanned(
            impl_trait,
            "xabi object returns must use `impl SomeXabiTrait + 'static`",
        ));
    }
    let object_bounds = impl_trait
        .bounds
        .iter()
        .filter_map(|bound| match bound {
            TypeParamBound::Trait(bound) if !is_marker_trait_bound(bound) => Some(bound),
            _ => None,
        })
        .collect::<Vec<_>>();
    let [bound] = object_bounds.as_slice() else {
        return Err(Error::new_spanned(
            impl_trait,
            "xabi object returns must name exactly one xabi trait plus optional auto trait bounds",
        ));
    };
    if bound
        .path
        .segments
        .iter()
        .any(|segment| !matches!(segment.arguments, PathArguments::None))
    {
        return Err(Error::new_spanned(
            &bound.path,
            "xabi object return trait bounds must be plain xabi trait paths without generic arguments or associated type constraints",
        ));
    }
    Ok(Some(bound.path.clone()))
}

fn xabi_object_pair(ty: &Type) -> syn::Result<Option<(Type, syn::Path)>> {
    let Type::Tuple(tuple) = ty else {
        return Ok(None);
    };
    if tuple.elems.len() != 2 {
        return Ok(None);
    }
    let mut elems = tuple.elems.iter();
    let ok = elems.next().expect("tuple len checked").clone();
    let object = elems.next().expect("tuple len checked");
    let Some(trait_path) = xabi_object_trait(object)? else {
        return Ok(None);
    };
    Ok(Some((ok, trait_path)))
}

fn is_marker_trait_bound(bound: &syn::TraitBound) -> bool {
    bound
        .path
        .segments
        .last()
        .map(|segment| {
            segment.ident == "Send"
                || segment.ident == "Sync"
                || segment.ident == "Unpin"
                || segment.ident == "Sized"
        })
        .unwrap_or(false)
}
