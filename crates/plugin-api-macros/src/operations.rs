//! `#[derive(Operations)]`: a plugin's operations, from a fieldless enum with one
//! `#[operation(...)]` per variant.
//!
//! In three steps: [`parse_variant`] reads each variant into its checked `Operation`
//! (every mistake is reported, each at its variant), [`check_list`] checks what only
//! the whole list can tell, and [`generate`] writes the `Operations` impl.

use darling::FromVariant;
use delight_manifest::{Operation, encode_operations, first_duplicate, validate_operations};
use proc_macro2::{Literal, TokenStream};
use quote::quote;
use syn::punctuated::Punctuated;
use syn::token::Comma;
use syn::{Data, DeriveInput, Fields, Ident, LitStr, Variant};

use crate::{Icons, invalid, values};

pub fn expand(item: TokenStream) -> darling::Result<TokenStream> {
    let input: DeriveInput = syn::parse2(item)?;
    let variants = enum_variants(&input)?;
    let mut icons = Icons::default();
    let mut errors = darling::Error::accumulator();
    let operations: Vec<(OperationArgs, Operation)> = variants
        .iter()
        .filter_map(|variant| errors.handle(parse_variant(variant, &mut icons)))
        .collect();
    errors.finish()?;
    check_list(&operations)?;
    Ok(generate(&input.ident, &operations, &icons))
}

/// An empty `impl Operations`, emitted with the errors when `expand` fails. It is
/// never run (the build fails anyway); it only keeps the error from repeating.
pub fn stand_in(item: TokenStream) -> TokenStream {
    let Ok(input) = syn::parse2::<DeriveInput>(item) else {
        return TokenStream::new();
    };
    if !input.generics.params.is_empty() {
        return TokenStream::new();
    }
    let name = &input.ident;
    quote! {
        impl ::delight_plugin_api::Operations for #name {
            const ENCODED: &'static [u8] = b"";
            fn id(&self) -> &'static str {
                ::core::unreachable!()
            }
            fn from_id(_: &str) -> ::core::option::Option<Self> {
                ::core::option::Option::None
            }
        }
    }
}

/// What one variant's `#[operation(...)]` says, as its author wrote it. darling
/// reports a missing, repeated or unknown property, at its place.
#[derive(FromVariant)]
#[darling(attributes(operation))]
struct OperationArgs {
    ident: Ident,
    /// A literal rather than a string, so a duplicate is reported at it.
    id: LitStr,
    title: String,
    #[darling(default)]
    description: String,
    #[darling(default)]
    icon: Option<LitStr>,
    #[darling(default)]
    tags: Vec<LitStr>,
}

impl OperationArgs {
    /// The operation to store: the icon's SVG instead of its path, and
    /// `delight-manifest`'s checks for one operation passed.
    fn to_operation(&self, icons: &mut Icons) -> darling::Result<Operation> {
        let operation = Operation {
            id: self.id.value(),
            title: self.title.clone(),
            description: self.description.clone(),
            icon: self.icon.as_ref().map(|icon| icons.read(icon)).transpose()?,
            tags: values(&self.tags),
        };
        operation
            .validate()
            .map_err(|error| invalid(error).with_span(&self.ident))?;
        Ok(operation)
    }
}

/// The variants of the enum the derive is on; a struct or a generic enum is refused.
fn enum_variants(input: &DeriveInput) -> darling::Result<&Punctuated<Variant, Comma>> {
    let Data::Enum(data) = &input.data else {
        return Err(
            darling::Error::custom("Operations is derived for an enum, one variant per operation")
                .with_span(&input.ident),
        );
    };
    if !input.generics.params.is_empty() {
        return Err(darling::Error::custom("an operations enum can't be generic").with_span(&input.generics));
    }
    Ok(&data.variants)
}

/// One variant: no fields, its `#[operation(...)]`, and the operation it describes.
fn parse_variant(variant: &Variant, icons: &mut Icons) -> darling::Result<(OperationArgs, Operation)> {
    if !matches!(variant.fields, Fields::Unit) {
        return Err(darling::Error::custom("an operation is a variant without fields")
            .with_span(&variant.fields));
    }
    if !variant.attrs.iter().any(|attr| attr.path().is_ident("operation")) {
        return Err(darling::Error::custom(
            "each operation needs #[operation(id = \"…\", title = \"…\")]",
        )
        .with_span(variant));
    }
    // darling's own errors here (a missing `title`) have no position; give them the
    // variant's.
    let args = OperationArgs::from_variant(variant).map_err(|error| error.with_span(variant))?;
    let operation = args.to_operation(icons)?;
    Ok((args, operation))
}

/// What only the whole list can tell: an id used twice (reported at the second use),
/// then all of `delight-manifest`'s rules for the list.
fn check_list(operations: &[(OperationArgs, Operation)]) -> darling::Result<()> {
    let list: Vec<Operation> = operations.iter().map(|(_, operation)| operation.clone()).collect();
    if let Some(index) = first_duplicate(&list) {
        let id = &operations[index].0.id;
        return Err(darling::Error::custom(format!("operation id {:?} is used twice", id.value()))
            .with_span(id));
    }
    validate_operations(&list).map_err(invalid)
}

/// `impl Operations` for the enum: the operations as the section stores them, and the
/// conversion between each variant and its id.
fn generate(name: &Ident, operations: &[(OperationArgs, Operation)], icons: &Icons) -> TokenStream {
    let list: Vec<Operation> = operations.iter().map(|(_, operation)| operation.clone()).collect();
    let encoded = Literal::byte_string(&encode_operations(&list));
    let variants: Vec<&Ident> = operations.iter().map(|(args, _)| &args.ident).collect();
    let ids: Vec<&str> = list.iter().map(|operation| operation.id.as_str()).collect();
    let tracked = icons.tracked();
    quote! {
        #tracked

        impl ::delight_plugin_api::Operations for #name {
            const ENCODED: &'static [u8] = #encoded;

            fn id(&self) -> &'static str {
                match self {
                    #(Self::#variants => #ids,)*
                }
            }

            fn from_id(id: &str) -> ::core::option::Option<Self> {
                match id {
                    #(#ids => ::core::option::Option::Some(Self::#variants),)*
                    _ => ::core::option::Option::None,
                }
            }
        }
    }
}
