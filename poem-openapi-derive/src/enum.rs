use darling::{
    FromDeriveInput, FromVariant,
    ast::{Data, Fields},
    util::Ignored,
};
use proc_macro2::{Ident, TokenStream};
use quote::quote;
use syn::{Attribute, DeriveInput, Error, Meta, Path, ext::IdentExt};

use crate::{
    common_args::{ExternalDocument, RenameRule, apply_rename_rule_variant},
    error::GeneratorResult,
    utils::{get_crate_name, get_description, optional_literal},
};

#[derive(FromVariant)]
#[darling(attributes(oai), forward_attrs(doc))]
struct EnumItem {
    ident: Ident,
    fields: Fields<Ignored>,

    #[darling(default)]
    rename: Option<String>,
}

#[derive(Copy, Clone)]
enum EnumRepr {
    I32,
    U32,
    I64,
    U64,
}

#[derive(FromDeriveInput)]
#[darling(attributes(oai), forward_attrs(doc))]
struct EnumArgs {
    ident: Ident,
    attrs: Vec<Attribute>,
    data: Data<EnumItem, Ignored>,

    #[darling(default)]
    internal: bool,
    #[darling(default)]
    rename_all: Option<RenameRule>,
    #[darling(default)]
    rename: Option<String>,
    #[darling(default)]
    remote: Option<Path>,
    #[darling(default)]
    deprecated: bool,
    #[darling(default)]
    external_docs: Option<ExternalDocument>,
    /// Optional override: #[oai(repr = "i32" | "i64" | "u32" | "u64")]
    #[darling(default)]
    repr: Option<String>,
}

pub(crate) fn generate(args: DeriveInput) -> GeneratorResult<TokenStream> {
    let input = args;
    let args: EnumArgs = EnumArgs::from_derive_input(&input)?;

    let crate_name = get_crate_name(args.internal);
    let ident = &args.ident;
    let oai_typename = args.rename.clone().unwrap_or_else(|| ident.to_string());
    let description = get_description(&args.attrs)?;
    let e = match &args.data {
        Data::Enum(e) => e,
        _ => return Err(Error::new_spanned(ident, "Enum can only be applied to an enum.").into()),
    };

    // Decide representation (numeric vs string) at macro time.
    // IMPORTANT: detect repr on the ORIGINAL input attrs, not the Darling-parsed ones.
    let repr = parse_oai_enum_repr(&args.repr).or_else(|| detect_rust_repr(&input.attrs));
    let is_numeric = repr.is_some();
    let (fmt_str, as_ty, parse_ty, number_getter) = match repr {
        Some(EnumRepr::I32) => ("int32", quote!(i32), quote!(i32), quote!(as_i64)),
        Some(EnumRepr::I64) => ("int64", quote!(i64), quote!(i64), quote!(as_i64)),
        Some(EnumRepr::U32) => ("int64", quote!(u32), quote!(u32), quote!(as_u64)), // int64 + bounds
        Some(EnumRepr::U64) => ("int64", quote!(u64), quote!(u64), quote!(as_u64)), // int64 + min=0
        None => ("int32", quote!(i32), quote!(i32), quote!(as_i64)), // unused in string mode
    };
    // Precompute numeric bounds setters (emit nothing when not needed)
    let min_setter_stmt: TokenStream = match repr {
        Some(EnumRepr::U32) | Some(EnumRepr::U64) => {
            quote!( s.minimum = ::std::option::Option::Some(0.0); )
        }
        _ => quote!(),
    };
    let max_setter_stmt: TokenStream = match repr {
        Some(EnumRepr::U32) => quote!( s.maximum = ::std::option::Option::Some(4294967295.0); ),
        _ => quote!(), // omit for i32/i64/u64
    };

    let mut enum_items = Vec::new();
    let mut ident_to_item = Vec::new();
    let mut item_to_ident = Vec::new();

    // Numeric-mode collections
    let mut enum_items_num = Vec::new();
    let mut ident_to_item_num = Vec::new();

    // For numeric parsing, we can't put expressions in match *patterns*.
    // Build a sequence of equality checks instead.
    let mut eq_checks_num: Vec<TokenStream> = Vec::new();
    let mut eq_checks_param_num: Vec<TokenStream> = Vec::new();

    for variant in e {
        if !variant.fields.is_empty() {
            return Err(Error::new_spanned(
                &variant.ident,
                format!(
                    "Invalid enum variant {}.\nOpenAPI enums may only contain unit variants.",
                    variant.ident
                ),
            )
            .into());
        }

        let item_ident = &variant.ident;
        let oai_item_name = variant.rename.clone().unwrap_or_else(|| {
            apply_rename_rule_variant(args.rename_all, variant.ident.unraw().to_string())
        });

        // String-mode data
        enum_items.push(quote!(#crate_name::types::ToJSON::to_json(&#ident::#item_ident).unwrap()));
        ident_to_item.push(quote!(#ident::#item_ident => #oai_item_name));
        item_to_ident
            .push(quote!(#oai_item_name => ::std::result::Result::Ok(#ident::#item_ident)));

        // Numeric-mode data
        enum_items_num
            .push(quote!(#crate_name::__private::serde_json::json!(#ident::#item_ident as #as_ty)));
        ident_to_item_num.push(quote!(#ident::#item_ident => (#ident::#item_ident as #as_ty)));
        // Build equality checks for parsing (JSON / parameters)
        eq_checks_num.push(quote! {
            if val == (#ident::#item_ident as #as_ty) { return ::std::result::Result::Ok(#ident::#item_ident); }
        });
        eq_checks_param_num.push(quote! {
            if raw == (#ident::#item_ident as #as_ty) { return ::std::result::Result::Ok(#ident::#item_ident); }
        });
    }

    let remote_conversion = if let Some(remote_ty) = &args.remote {
        let local_to_remote_items = e.iter().map(|item| {
            let item = &item.ident;
            quote! {
                #ident::#item => #remote_ty::#item,
            }
        });
        let remote_to_local_items = e.iter().map(|item| {
            let item = &item.ident;
            quote! {
                #remote_ty::#item => #ident::#item,
            }
        });

        Some(quote! {
            impl ::std::convert::From<#ident> for #remote_ty {
                fn from(value: #ident) -> Self {
                    match value {
                        #(#local_to_remote_items)*
                    }
                }
            }

            impl ::std::convert::From<#remote_ty> for #ident {
                fn from(value: #remote_ty) -> Self {
                    match value {
                        #(#remote_to_local_items)*
                    }
                }
            }
        })
    } else {
        None
    };
    let description = optional_literal(&description);
    let deprecated = args.deprecated;
    let external_docs = match &args.external_docs {
        Some(external_docs) => {
            let s = external_docs.to_token_stream(&crate_name);
            quote!(::std::option::Option::Some(#s))
        }
        None => quote!(::std::option::Option::None),
    };

    let expanded = quote! {
        impl #crate_name::types::Type for #ident {
            const IS_REQUIRED: bool = true;

            type RawValueType = Self;

            type RawElementValueType = Self;

            fn name() -> ::std::borrow::Cow<'static, str> {
                ::std::convert::Into::into(#oai_typename)
            }

            fn as_raw_value(&self) -> ::std::option::Option<&Self::RawValueType> {
                ::std::option::Option::Some(self)
            }

            fn schema_ref() -> #crate_name::registry::MetaSchemaRef {
                #crate_name::registry::MetaSchemaRef::Reference(<Self as #crate_name::types::Type>::name().into_owned())
            }

            fn register(registry: &mut #crate_name::registry::Registry) {
                registry.create_schema::<Self, _>(<Self as #crate_name::types::Type>::name().into_owned(), |registry| {
                    let mut s = if #is_numeric {
                        let mut s = #crate_name::registry::MetaSchema::new("integer");
                        s.format = ::std::option::Option::Some(::std::convert::Into::into(#fmt_str));
                        s.enum_items = ::std::vec![#(#enum_items_num),*];
                        // Unsigned bounds (OpenAPI 3.0 has no uint32/uint64 formats)
                        #min_setter_stmt
                        #max_setter_stmt
                        s
                    } else {
                        let mut s = #crate_name::registry::MetaSchema::new("string");
                        s.enum_items = ::std::vec![#(#enum_items),*];
                        s
                    };
                    s.description = #description;
                    s.external_docs = #external_docs;
                    s.deprecated = #deprecated;
                    s
                });
            }

            fn raw_element_iter<'a>(&'a self) -> ::std::boxed::Box<dyn ::std::iter::Iterator<Item = &'a Self::RawElementValueType> + 'a> {
                ::std::boxed::Box::new(::std::iter::IntoIterator::into_iter(self.as_raw_value()))
            }
        }

        impl #crate_name::types::ParseFromJSON for #ident {
            fn parse_from_json(value: ::std::option::Option<#crate_name::__private::serde_json::Value>) -> #crate_name::types::ParseResult<Self> {
                let value = value.unwrap_or_default();
                if #is_numeric {
                    match &value {
                        #crate_name::__private::serde_json::Value::Number(n) => {
                            if let ::std::option::Option::Some(raw) = n.#number_getter() {
                                let val: #as_ty = (raw as #parse_ty) as #as_ty;
                                #(#eq_checks_num)*
                                ::std::result::Result::Err(#crate_name::types::ParseError::custom("invalid enum value"))
                            } else {
                                ::std::result::Result::Err(#crate_name::types::ParseError::expected_type(value))
                            }
                        }
                        _ => ::std::result::Result::Err(#crate_name::types::ParseError::expected_type(value)),
                    }
                } else {
                    match &value {
                        #crate_name::__private::serde_json::Value::String(item) => match item.as_str() {
                            #(#item_to_ident,)*
                            _ => ::std::result::Result::Err(#crate_name::types::ParseError::expected_type(value)),
                        }
                        _ => ::std::result::Result::Err(#crate_name::types::ParseError::expected_type(value)),
                    }
                }
            }
        }

        impl #crate_name::types::ParseFromParameter for #ident {
            fn parse_from_parameter(value: &str) -> #crate_name::types::ParseResult<Self> {
                if #is_numeric {
                    match value.parse::<#parse_ty>() {
                        ::std::result::Result::Ok(parsed) => {
                            let raw: #as_ty = parsed as #as_ty;
                            #(#eq_checks_param_num)*
                            ::std::result::Result::Err(#crate_name::types::ParseError::custom("invalid enum value"))
                        }
                        ::std::result::Result::Err(_) => ::std::result::Result::Err(#crate_name::types::ParseError::custom("invalid integer")),
                    }
                } else {
                    match value {
                        #(#item_to_ident,)*
                        _ => ::std::result::Result::Err(#crate_name::types::ParseError::custom("Expect a valid enumeration value.")),
                    }
                }
            }
        }

        impl #crate_name::types::ToJSON for #ident {
            fn to_json(&self) -> ::std::option::Option<#crate_name::__private::serde_json::Value> {
                if #is_numeric {
                    let n = match self { #(#ident_to_item_num),* };
                    ::std::option::Option::Some(#crate_name::__private::serde_json::json!(n))
                } else {
                    let name = match self { #(#ident_to_item),* };
                    ::std::option::Option::Some(#crate_name::__private::serde_json::Value::String(::std::string::ToString::to_string(name)))
                }
            }
        }

        impl #crate_name::types::ParseFromMultipartField for #ident {
            async fn parse_from_multipart(field: ::std::option::Option<#crate_name::__private::poem::web::Field>) -> #crate_name::types::ParseResult<Self> {
                use poem_openapi::types::ParseFromParameter;
                match field {
                    ::std::option::Option::Some(field) => {
                        let s = field.text().await?;
                        Self::parse_from_parameter(&s)
                    },
                    ::std::option::Option::None => ::std::result::Result::Err(#crate_name::types::ParseError::expected_input()),
                }
            }
        }

        #remote_conversion
    };

    Ok(expanded)
}

fn parse_oai_enum_repr(string: &Option<String>) -> Option<EnumRepr> {
    match string.as_deref() {
        Some("i32") => Some(EnumRepr::I32),
        Some("i64") => Some(EnumRepr::I64),
        Some("u32") => Some(EnumRepr::U32),
        Some("u64") => Some(EnumRepr::U64),
        _ => None,
    }
}

fn detect_rust_repr(attrs: &[Attribute]) -> Option<EnumRepr> {
    for attr in attrs {
        if let Meta::List(list) = &attr.meta {
            if list.path.is_ident("repr") {
                let mut found: Option<EnumRepr> = None;
                let _ = list.parse_nested_meta(|meta| {
                    if meta.path.is_ident("i32") {
                        found = Some(EnumRepr::I32);
                    } else if meta.path.is_ident("i64") {
                        found = Some(EnumRepr::I64);
                    } else if meta.path.is_ident("u32") {
                        found = Some(EnumRepr::U32);
                    } else if meta.path.is_ident("u64") {
                        found = Some(EnumRepr::U64);
                    }
                    Ok(())
                });
                if found.is_some() {
                    return found;
                }
            }
        }
    }
    None
}
