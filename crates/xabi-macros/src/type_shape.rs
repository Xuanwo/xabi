use syn::visit_mut::VisitMut;
use syn::{
    AngleBracketedGenericArguments, Error, GenericArgument, PathArguments, ReturnType, Type,
    parse_quote,
};

#[derive(Clone, Copy)]
pub(crate) enum XabiValueContext {
    DataField,
    MethodArgument,
    MethodReturn,
    ResultPayload,
    ResultError,
}

pub(crate) fn validate_xabi_value_type(ty: &Type, context: XabiValueContext) -> syn::Result<()> {
    match ty {
        Type::Path(path) => validate_path_arguments(&path.path.segments, context),
        Type::Paren(paren) => validate_xabi_value_type(&paren.elem, context),
        Type::Group(group) => validate_xabi_value_type(&group.elem, context),
        Type::Reference(_) => Err(Error::new_spanned(ty, borrowed_message(context))),
        Type::Ptr(_) => Err(Error::new_spanned(
            ty,
            "raw pointers are not xabi boundary values; wrap non-null external handles with `#[xabi::opaque]`",
        )),
        Type::Tuple(_) => Err(Error::new_spanned(
            ty,
            "tuple values are not xabi boundary values; use `#[xabi::data]` for structured payloads",
        )),
        Type::ImplTrait(_) | Type::TraitObject(_) => Err(Error::new_spanned(
            ty,
            "trait objects are not xabi boundary values; use a generated xabi trait handle or return `Result<impl SomeXabiTrait + 'static, E>`",
        )),
        Type::BareFn(_) => Err(Error::new_spanned(
            ty,
            "function pointers are not xabi boundary values; define a xabi callback trait instead",
        )),
        Type::Array(_) | Type::Slice(_) => Err(Error::new_spanned(
            ty,
            "array and slice values are not xabi boundary values; use `Vec<u8>` for owned bytes or `&[u8]` for method arguments",
        )),
        _ => Err(Error::new_spanned(
            ty,
            format!("unsupported xabi {}", context_name(context)),
        )),
    }
}

pub(crate) fn validate_return_type(output: &ReturnType) -> syn::Result<&Type> {
    let ReturnType::Type(_, ty) = output else {
        return Err(Error::new_spanned(
            output,
            "xabi methods must return a value",
        ));
    };
    Ok(ty)
}

fn validate_path_arguments<'a>(
    segments: impl IntoIterator<Item = &'a syn::PathSegment>,
    context: XabiValueContext,
) -> syn::Result<()> {
    for segment in segments {
        match &segment.arguments {
            PathArguments::None => {}
            PathArguments::Parenthesized(args) => {
                return Err(Error::new_spanned(
                    args,
                    "xabi boundary values cannot use parenthesized generic arguments",
                ));
            }
            PathArguments::AngleBracketed(args) => validate_angle_arguments(args, context)?,
        }
    }
    Ok(())
}

fn validate_angle_arguments(
    args: &AngleBracketedGenericArguments,
    context: XabiValueContext,
) -> syn::Result<()> {
    for arg in &args.args {
        match arg {
            GenericArgument::Type(ty) => validate_xabi_value_type(ty, context)?,
            GenericArgument::Lifetime(_)
                if matches!(
                    context,
                    XabiValueContext::DataField | XabiValueContext::MethodArgument
                ) => {}
            GenericArgument::Lifetime(_) => {
                return Err(Error::new_spanned(
                    arg,
                    "xabi output values cannot carry generic lifetime arguments",
                ));
            }
            GenericArgument::Const(_) => {
                return Err(Error::new_spanned(
                    arg,
                    "xabi boundary values cannot use const generic arguments",
                ));
            }
            GenericArgument::AssocType(_)
            | GenericArgument::AssocConst(_)
            | GenericArgument::Constraint(_) => {
                return Err(Error::new_spanned(
                    arg,
                    "xabi boundary values cannot use associated type constraints",
                ));
            }
            _ => {
                return Err(Error::new_spanned(
                    arg,
                    "unsupported xabi boundary value generic argument",
                ));
            }
        }
    }
    Ok(())
}

pub(crate) fn replace_lifetimes_with_static(ty: &Type) -> Type {
    struct ReplaceLifetimes;

    impl VisitMut for ReplaceLifetimes {
        fn visit_lifetime_mut(&mut self, lifetime: &mut syn::Lifetime) {
            *lifetime = parse_quote!('static);
        }
    }

    let mut ty = ty.clone();
    ReplaceLifetimes.visit_type_mut(&mut ty);
    ty
}

pub(crate) fn erase_lifetime_arguments(ty: &Type) -> Type {
    struct EraseLifetimes;

    impl VisitMut for EraseLifetimes {
        fn visit_path_segment_mut(&mut self, segment: &mut syn::PathSegment) {
            syn::visit_mut::visit_path_segment_mut(self, segment);
            let PathArguments::AngleBracketed(arguments) = &mut segment.arguments else {
                return;
            };
            arguments.args = std::mem::take(&mut arguments.args)
                .into_iter()
                .filter(|argument| !matches!(argument, GenericArgument::Lifetime(_)))
                .collect();
            if arguments.args.is_empty() {
                segment.arguments = PathArguments::None;
            }
        }
    }

    let mut ty = ty.clone();
    EraseLifetimes.visit_type_mut(&mut ty);
    ty
}

fn borrowed_message(context: XabiValueContext) -> &'static str {
    match context {
        XabiValueContext::MethodArgument => {
            "xabi method arguments can only borrow `&str` or `&[u8]`; use an owned xabi value type or generated borrowed xabi trait handle for other inputs"
        }
        XabiValueContext::DataField
        | XabiValueContext::MethodReturn
        | XabiValueContext::ResultPayload
        | XabiValueContext::ResultError => {
            "xabi boundary values cannot contain borrowed references; use owned strings, byte vectors, `#[xabi::data]`, `#[xabi::opaque]`, or generated xabi trait handles"
        }
    }
}

fn context_name(context: XabiValueContext) -> &'static str {
    match context {
        XabiValueContext::DataField => "data field type",
        XabiValueContext::MethodArgument => "method argument type",
        XabiValueContext::MethodReturn => "method return type",
        XabiValueContext::ResultPayload => "Result payload type",
        XabiValueContext::ResultError => "Result error type",
    }
}
