//! `#[derive(Actions)]`: a tool's footer actions, from a fieldless enum with one
//! variant per action.
//!
//! Each variant's id is its name: the app only hands ids back to the tool that listed
//! them, and stores none, so nothing needs to outlive a rename.

use proc_macro2::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields};

pub fn expand(item: TokenStream) -> darling::Result<TokenStream> {
    let input: DeriveInput = syn::parse2(item)?;
    let Data::Enum(data) = &input.data else {
        return Err(darling::Error::custom("Actions is derived for an enum, one variant per action")
            .with_span(&input.ident));
    };
    if !input.generics.params.is_empty() {
        return Err(darling::Error::custom("an actions enum can't be generic").with_span(&input.generics));
    }
    let mut errors = darling::Error::accumulator();
    for variant in &data.variants {
        if !matches!(variant.fields, Fields::Unit) {
            errors.push(darling::Error::custom("an action is a variant without fields").with_span(&variant.fields));
        }
    }
    errors.finish()?;
    let name = &input.ident;
    let variants: Vec<_> = data.variants.iter().map(|variant| &variant.ident).collect();
    let ids: Vec<String> = variants.iter().map(|variant| variant.to_string()).collect();
    Ok(quote! {
        impl ::delight_plugin_api::Actions for #name {
            fn id(&self) -> &'static str {
                match *self {
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
    })
}

/// An empty `impl Actions`, emitted with the errors when `expand` fails, so the error
/// isn't followed by "`Actions` is not implemented" wherever the enum is used.
pub fn stand_in(item: TokenStream) -> TokenStream {
    let Ok(input) = syn::parse2::<DeriveInput>(item) else {
        return TokenStream::new();
    };
    if !input.generics.params.is_empty() {
        return TokenStream::new();
    }
    let name = &input.ident;
    quote! {
        impl ::delight_plugin_api::Actions for #name {
            fn id(&self) -> &'static str {
                ::core::unreachable!()
            }
            fn from_id(_: &str) -> ::core::option::Option<Self> {
                ::core::option::Option::None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_action_is_its_variants_name() {
        let expanded = expand(quote!(enum EchoAction { Copy, CopyDataUri })).unwrap().to_string();
        assert!(expanded.contains("Self :: CopyDataUri => \"CopyDataUri\""), "{expanded}");
        assert!(expanded.contains("\"Copy\" => :: core :: option :: Option :: Some (Self :: Copy)"), "{expanded}");
    }

    #[test]
    fn only_a_fieldless_enum_is_actions() {
        assert!(expand(quote!(struct Echo;)).is_err());
        assert!(expand(quote!(enum EchoAction { Copy(String) })).is_err());
        assert!(expand(quote!(enum EchoAction<T> { Copy(T) })).is_err());
    }
}
