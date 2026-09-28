//! `#[plugin(...)]`: the plugin's properties, its custom section and its entry point.
//!
//! In three steps: [`PluginArgs::parse`] reads the attribute, [`PluginArgs::to_properties`]
//! turns it into the checked `PluginProperties` stored in the `.wasm`, and [`generate`]
//! writes the code.

use darling::FromMeta;
use darling::ast::NestedMeta;
use delight_manifest::{Permission, PluginProperties, SECTION, encode_properties, validate_id};
use proc_macro2::{Literal, TokenStream};
use quote::quote;
use syn::{DeriveInput, Expr, Ident, LitStr};

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
}

impl PluginArgs {
    fn parse(attr: TokenStream) -> darling::Result<Self> {
        Self::from_list(&NestedMeta::parse_meta_list(attr)?)
    }

    /// The properties to store: the icon's SVG instead of its path, the crate's own
    /// version, and `delight-manifest`'s checks passed.
    fn to_properties(&self, icons: &mut Icons) -> darling::Result<PluginProperties> {
        validate_id(&self.id.value()).map_err(|error| invalid(error).with_span(&self.id))?;
        let properties = PluginProperties {
            id: self.id.value(),
            name: self.name.clone(),
            version: std::env::var("CARGO_PKG_VERSION").unwrap_or_default(),
            description: self.description.clone(),
            author: self.author.clone(),
            icon: icons.read(&self.icon)?,
            tags: values(&self.tags),
            permissions: self.permissions.0.clone(),
        };
        properties.validate().map_err(invalid)?;
        Ok(properties)
    }
}

/// `permissions = [Network]`: permission names, as `delight-manifest` spells them.
/// darling's own `PathList` reads only the `permissions(Network)` form.
#[derive(Default)]
struct Permissions(Vec<Permission>);

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

/// One name in the list, such as `Network`.
fn permission(element: &Expr) -> darling::Result<Permission> {
    let name = match element {
        Expr::Path(path) => path.path.get_ident(),
        _ => None,
    }
    .ok_or_else(|| {
        darling::Error::custom("expected a permission, such as `Network`").with_span(element)
    })?;
    name.to_string()
        .parse()
        .map_err(|error| darling::Error::custom(error).with_span(name))
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
