//! The proc macros behind `#[delight_plugin_api::plugin]` and
//! `#[derive(delight_plugin_api::Operations)]`; use them through that crate.
//!
//! Both read their attributes (parsed by `darling`) and the icon files those name,
//! check them at compile time, and turn them into their part of the plugin's custom
//! section (see `delight_manifest::SECTION`).

mod operations;
mod plugin;

use std::path::PathBuf;

use proc_macro::TokenStream;
use quote::quote;
use syn::LitStr;

/// Make a type the plugin: its properties, and the component's entry point.
///
/// ```ignore
/// #[plugin(
///     id = "dev.delight.json",         // names its files: letters, digits, . _ -
///     name = "JSON",
///     description = "Format and minify JSON",
///     author = "Delight",
///     icon = "assets/icon.svg",        // a square, full-colour SVG
///     tags = ["json"],
///     permissions = [Network],         // leave out for none
///     tips = ["Paste JSON to format it"], // up to 5, each one line
/// )]
/// struct Json;
/// ```
///
/// `id`, `name` and `icon` are required; the version is the crate's own. `tips` are
/// hints on using the plugin, which the launcher's empty input shows now and then:
/// at most 5, each at most 80 characters. Icon paths
/// are relative to the crate's `Cargo.toml`. The type must implement `Plugin`, whose
/// `Operation` derives `Operations`: the two are joined into the plugin's custom
/// section at compile time. A mistake is a compile error.
#[proc_macro_attribute]
pub fn plugin(attr: TokenStream, item: TokenStream) -> TokenStream {
    let item = proc_macro2::TokenStream::from(item);
    plugin::expand(attr.into(), item.clone())
        .unwrap_or_else(|error| {
            // The type stays, so the error isn't followed by "cannot find type".
            let errors = error.write_errors();
            quote!(#item #errors)
        })
        .into()
}

/// A plugin's operations, from a fieldless enum with one `#[operation(...)]` per
/// variant.
///
/// ```ignore
/// #[derive(Operations)]
/// enum JsonOperation {
///     #[operation(id = "format", title = "Format JSON")]
///     Format,
///     #[operation(id = "minify", title = "Minify JSON", icon = "assets/minify.svg")]
///     Minify,
/// }
/// ```
///
/// `id` and `title` are required; `description`, `icon` and `tags` are optional. The
/// id is what the app stores (history, the picked tool), so it is written out rather
/// than taken from the variant's name, and a rename in the code changes nothing.
#[proc_macro_derive(Operations, attributes(operation))]
pub fn operations(item: TokenStream) -> TokenStream {
    let item = proc_macro2::TokenStream::from(item);
    operations::expand(item.clone())
        .unwrap_or_else(|error| {
            // A stand-in impl, so the error isn't followed by "`Operations` is not
            // implemented" wherever the enum is used.
            let stand_in = operations::stand_in(item);
            let errors = error.write_errors();
            quote!(#stand_in #errors)
        })
        .into()
}

/// The icons a macro reads: their SVGs go into the manifest, and their files are
/// tracked so the plugin is rebuilt when one changes.
#[derive(Default)]
struct Icons(Vec<PathBuf>);

impl Icons {
    /// The SVG at `path`, relative to the plugin crate's `Cargo.toml` (Cargo sets
    /// `CARGO_MANIFEST_DIR` to that folder while it compiles the plugin).
    fn read(&mut self, path: &LitStr) -> darling::Result<String> {
        let crate_dir = std::env::var("CARGO_MANIFEST_DIR")
            .map_err(|_| darling::Error::custom("CARGO_MANIFEST_DIR isn't set"))?;
        let file = PathBuf::from(crate_dir).join(path.value());
        let svg = std::fs::read_to_string(&file).map_err(|error| {
            darling::Error::custom(format!("reading {}: {error}", file.display())).with_span(path)
        })?;
        self.0.push(file);
        Ok(svg)
    }

    /// Items that include each file read, so the compiler tracks them.
    fn tracked(&self) -> proc_macro2::TokenStream {
        let files = self.0.iter().map(|file| file.to_string_lossy().into_owned());
        quote!(#(const _: &[u8] = include_bytes!(#files);)*)
    }
}

/// The text of each literal in a `tags = ["a", "b"]` list.
fn values(literals: &[LitStr]) -> Vec<String> {
    literals.iter().map(LitStr::value).collect()
}

/// A failed `delight-manifest` check as a compile error; give it a position with
/// `.with_span(…)`, or it is reported at the macro.
fn invalid(error: anyhow::Error) -> darling::Error {
    darling::Error::custom(format!("{error:#}"))
}

#[cfg(test)]
mod tests {
    #[test]
    fn released_with_the_plugin_api() {
        assert_eq!(env!("CARGO_PKG_VERSION"), delight_manifest::PLUGIN_API_VERSION);
    }
}
