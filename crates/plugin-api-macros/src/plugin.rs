//! `#[plugin(...)]`: the plugin's properties, its custom section and its entry point.
//!
//! In three steps: [`PluginArgs::parse`] reads the attribute, [`PluginArgs::to_properties`]
//! turns it into the checked `PluginProperties` stored in the `.wasm`, and [`generate`]
//! writes the code.

use darling::FromMeta;
use darling::ast::NestedMeta;
use delight_manifest::{
    MAX_TIPS, Permission, PermissionRequest, PluginProperties, SECTION, encode_properties, validate_id, validate_reason,
    validate_tip,
};
use proc_macro2::{Literal, TokenStream};
use quote::quote;
use syn::{DeriveInput, Expr, ExprLit, Ident, Lit, LitStr};

use crate::{Icons, invalid, values};

pub fn expand(attr: TokenStream, item: TokenStream) -> darling::Result<TokenStream> {
    let input: DeriveInput = syn::parse2(item.clone())?;
    if !input.generics.params.is_empty() {
        return Err(darling::Error::custom("a plugin type can't be generic").with_span(&input.generics));
    }
    let args = PluginArgs::parse(attr)?;
    let mut icons = Icons::default();
    let properties = args.to_properties(&mut icons)?;
    Ok(generate(&item, &input.ident, &properties, &icons))
}

/// What `#[plugin(...)]` says, as its author wrote it. darling reports a missing,
/// repeated or unknown property, at its place.
#[derive(FromMeta)]
struct PluginArgs {
    /// Literals rather than strings where a later check needs their position.
    id: LitStr,
    name: String,
    #[darling(default)]
    description: String,
    #[darling(default)]
    author: String,
    icon: LitStr,
    #[darling(default)]
    tags: Vec<LitStr>,
    #[darling(default)]
    permissions: Permissions,
    #[darling(default)]
    tips: Vec<LitStr>,
}

impl PluginArgs {
    fn parse(attr: TokenStream) -> darling::Result<Self> {
        Self::from_list(&NestedMeta::parse_meta_list(attr)?)
    }

    /// The properties to store: the icon's SVG instead of its path, the crate's own
    /// version, and `delight-manifest`'s checks passed.
    fn to_properties(&self, icons: &mut Icons) -> darling::Result<PluginProperties> {
        validate_id(&self.id.value()).map_err(|error| invalid(error).with_span(&self.id))?;
        self.check_tips()?;
        let properties = PluginProperties {
            id: self.id.value(),
            name: self.name.clone(),
            version: std::env::var("CARGO_PKG_VERSION").unwrap_or_default(),
            description: self.description.clone(),
            author: self.author.clone(),
            icon: icons.read(&self.icon)?,
            tags: values(&self.tags),
            permissions: self.permissions.0.clone(),
            tips: values(&self.tips),
        };
        properties.validate().map_err(invalid)?;
        Ok(properties)
    }

    /// Each tip's check, at that tip; one too many, at the first extra.
    fn check_tips(&self) -> darling::Result<()> {
        let mut errors = darling::Error::accumulator();
        for tip in &self.tips {
            errors.handle(validate_tip(&tip.value()).map_err(|error| invalid(error).with_span(tip)));
        }
        if let Some(extra) = self.tips.get(MAX_TIPS) {
            errors.push(darling::Error::custom(format!("at most {MAX_TIPS} tips")).with_span(extra));
        }
        errors.finish()
    }
}

/// `permissions = [Network("Fetches schemas from the web")]`: permission names, as
/// `delight-manifest` spells them, each with why the plugin needs it, and then the data
/// only that permission has, by field name: `Commands("Lists processes", programs =
/// ["/bin/ps", "/usr/sbin/lsof"])`. The macro knows no permission: the fields are read
/// as the permission's own type is, so a new permission needs nothing here.
#[derive(Default)]
struct Permissions(Vec<PermissionRequest>);

impl FromMeta for Permissions {
    fn from_expr(expr: &Expr) -> darling::Result<Self> {
        let Expr::Array(array) = expr else {
            return Err(darling::Error::unexpected_expr_type(expr));
        };
        let mut errors = darling::Error::accumulator();
        let permissions = array
            .elems
            .iter()
            .filter_map(|element| errors.handle(permission(element)))
            .collect();
        errors.finish_with(Permissions(permissions))
    }
}

/// One permission in the list, such as `Network("Fetches schemas from the web")`.
fn permission(element: &Expr) -> darling::Result<PermissionRequest> {
    let expected = || {
        darling::Error::custom("expected a permission and why the plugin needs it, such as `Network(\"Fetches schemas from the web\")`")
            .with_span(element)
    };
    let call = match element {
        Expr::Call(call) => call,
        Expr::Path(path) if path.path.get_ident().is_some() => {
            let message = "say why the plugin needs it, which people read when they install it: `Name(\"…\")`";
            return Err(darling::Error::custom(message).with_span(element));
        }
        _ => return Err(expected()),
    };
    let name = match &*call.func {
        Expr::Path(path) => path.path.get_ident(),
        _ => None,
    }
    .ok_or_else(expected)?;
    let mut arguments = call.args.iter();
    let Some(Expr::Lit(ExprLit { lit: Lit::Str(reason), .. })) = arguments.next() else {
        return Err(expected());
    };
    validate_reason(&reason.value()).map_err(|error| invalid(error).with_span(reason))?;
    // The rest, `field = value`, joined with the name into the object the manifest
    // holds, which the permission's own type reads and checks.
    let mut object = serde_json::Map::new();
    object.insert("permission".into(), name.to_string().into());
    for argument in arguments {
        let Expr::Assign(assign) = argument else {
            return Err(expected());
        };
        let field = match &*assign.left {
            Expr::Path(path) => path.path.get_ident(),
            _ => None,
        }
        .ok_or_else(expected)?;
        object.insert(field.to_string(), json(&assign.right)?);
    }
    let permission: Permission = serde_json::from_value(object.into())
        .map_err(|error| darling::Error::custom(format!("{name}: {error}")).with_span(call))?;
    permission.spec().validate().map_err(|error| invalid(error).with_span(call))?;
    Ok(PermissionRequest { permission, reason: reason.value() })
}

/// A field's value as JSON: a string, number, boolean, or an array of them.
fn json(expr: &Expr) -> darling::Result<serde_json::Value> {
    let unsupported = || darling::Error::custom("expected a string, number, boolean or array of them").with_span(expr);
    match expr {
        Expr::Lit(ExprLit { lit, .. }) => match lit {
            Lit::Str(string) => Ok(string.value().into()),
            Lit::Bool(boolean) => Ok(boolean.value.into()),
            Lit::Int(int) => Ok(int.base10_parse::<i64>().map_err(darling::Error::from)?.into()),
            _ => Err(unsupported()),
        },
        Expr::Array(array) => Ok(array.elems.iter().map(json).collect::<darling::Result<Vec<_>>>()?.into()),
        _ => Err(unsupported()),
    }
}

/// The plugin's type as written, and, for wasm, its custom section (the properties
/// joined with its `Operation`'s) and the component's entry point.
fn generate(item: &TokenStream, plugin: &Ident, properties: &PluginProperties, icons: &Icons) -> TokenStream {
    let properties = Literal::byte_string(&encode_properties(properties));
    let tracked = icons.tracked();
    let api = quote!(::delight_plugin_api);
    let operations = quote! {
        <<#plugin as #api::Plugin>::Operation as #api::Operations>::ENCODED
    };
    quote! {
        #item

        #tracked

        // The plugin's `Operation` must derive `Operations`; checked natively too.
        const _: &[u8] = #operations;

        #[cfg(target_arch = "wasm32")]
        const _: () = {
            const PROPERTIES: &[u8] = #properties;
            const OPERATIONS: &[u8] = #operations;
            const LEN: usize = PROPERTIES.len() + OPERATIONS.len();

            #[unsafe(link_section = #SECTION)]
            #[used]
            static DELIGHT_PLUGIN: [u8; LEN] =
                #api::__private::concat::<LEN>(PROPERTIES, OPERATIONS);

            struct Entry(#api::gpui::AnyEntity);

            impl #api::__private::embedded_gpui::Plugin for Entry {
                fn new(cx: &mut #api::gpui::App) -> Self {
                    Entry(#api::__private::start::<#plugin>(cx))
                }

                fn assets() -> Option<Box<dyn #api::gpui::AssetSource>> {
                    <#plugin as #api::Plugin>::assets()
                }
            }

            #api::__private::embedded_gpui::register_plugin!(Entry);
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use delight_manifest::Permission;

    fn read(element: Expr) -> Result<PermissionRequest, String> {
        permission(&element).map_err(|error| error.to_string())
    }

    #[test]
    fn a_permission_is_written_with_why_it_is_needed() {
        let request = read(syn::parse_quote!(Network("Fetches schemas from the web"))).unwrap();
        assert_eq!(request.permission, Permission::network());
        assert_eq!(request.reason, "Fetches schemas from the web");

        assert!(read(syn::parse_quote!(Network)).unwrap_err().contains("say why"));
        assert!(read(syn::parse_quote!(Network("  "))).unwrap_err().contains("blank"));
        assert!(read(syn::parse_quote!(Files("Reads files"))).unwrap_err().contains("unknown variant"));
        assert!(read(syn::parse_quote!(Network("a", "b"))).unwrap_err().contains("expected a permission"));
    }

    #[test]
    fn a_permissions_own_data_is_written_by_field() {
        let request = read(syn::parse_quote!(Commands("Lists processes", programs = ["/bin/ps", "/usr/sbin/lsof"]))).unwrap();
        assert_eq!(request.permission, Permission::commands(["/bin/ps", "/usr/sbin/lsof"]));

        assert!(read(syn::parse_quote!(Commands("Lists processes"))).unwrap_err().contains("missing field `programs`"));
        assert!(read(syn::parse_quote!(Commands("Lists", programs = []))).unwrap_err().contains("no programs"));
        assert!(read(syn::parse_quote!(Commands("Lists", programs = ["ps"]))).unwrap_err().contains("directly in"));
        assert!(read(syn::parse_quote!(Commands("Lists", programs = [1]))).is_err(), "a number is not a program");
        assert!(read(syn::parse_quote!(Commands("Lists", ["/bin/ps"]))).unwrap_err().contains("expected a permission"));
        assert!(read(syn::parse_quote!(Network("Fetches", programs = ["/bin/ps"]))).unwrap_err().contains("unknown field"));
    }
}
