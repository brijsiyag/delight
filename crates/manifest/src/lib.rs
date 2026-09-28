//! What a plugin's `.wasm` says about itself, read by the app before any of the
//! plugin's code runs: the protocol version it was built for, and its manifest (its
//! properties and operations). Installing shows what a plugin can do from this, and
//! its sandbox is set up from it.
//!
//! Plugins declare the manifest in code: `#[delight_plugin_api::plugin(...)]` on the
//! plugin type gives its [`PluginProperties`], and `#[derive(Operations)]` on an enum
//! its [`Operation`]s. The macros check both at compile time and store them in the
//! `.wasm`'s [`SECTION`].
//!
//! - `version`: [`PROTOCOL_VERSION`], and which plugins an app runs;
//! - `manifest`: what a plugin is, and the rules it must follow;
//! - `section`: how both are stored in the `.wasm`, and read back.
//!
//! This crate has no embedded_gpui in it: the plugin API's proc macros depend on it,
//! and whatever a proc macro depends on is built natively for every plugin.

mod manifest;
mod section;
mod version;

pub use manifest::{
    Manifest, Operation, Permission, PluginProperties, first_duplicate, validate_id,
    validate_operations,
};
pub use section::{SECTION, decode_section, encode_operations, encode_properties};
pub use version::{PROTOCOL_VERSION, ProtocolVersion};
