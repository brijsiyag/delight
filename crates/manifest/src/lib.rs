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
//! - `section`: how both are stored in the `.wasm`, and read back;
//! - with the `files` feature, `wasm`: reading the manifest out of a `.wasm`
//!   (`read_manifest`), and `release`: what the location a plugin is published at holds, which it
//!   is installed and updated from (`Release`).
//!
//! This crate has no embedded_gpui in it: the plugin API's proc macros depend on it,
//! and whatever a proc macro depends on is built natively for every plugin, which is why
//! reading files is a feature they leave off.

mod manifest;
mod permission;
#[cfg(feature = "files")]
mod release;
mod section;
mod version;
#[cfg(feature = "files")]
mod wasm;

#[cfg(feature = "files")]
pub use release::{Link, MAX_MANIFEST_BYTES, MAX_PLUGIN_BYTES, PluginList, Release, is_newer, manifest_url, read_published, wasm_url};
pub use manifest::{
    MAX_REASON_CHARS, MAX_TIP_CHARS, MAX_TIPS, MAX_URL_CHARS, Manifest, Operation, PluginProperties, first_duplicate,
    validate_id, validate_operations, validate_reason, validate_tip, validate_url,
};
#[cfg(feature = "files")]
pub use wasm::read_manifest;
pub use permission::{
    COMMAND_DIRS, CommandsPermission, FilesPermission, MAX_PROGRAMS, NetworkPermission, Permission, PermissionData,
    PermissionRequest, PermissionSpec, Permissions, expand_home, home_spelled, validate_program,
};
pub use section::{SECTION, decode_section, encode_operations, encode_properties};
pub use version::{PLUGIN_API_VERSION, PROTOCOL_VERSION, ProtocolVersion};
