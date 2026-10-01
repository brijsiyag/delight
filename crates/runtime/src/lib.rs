//! Delight's plugins, without windows.
//!
//! - [`read_manifest`]: what a plugin's `.wasm` says it is, read without running any
//!   of it;
//! - [`rank`]: which tools fit an input, from what every plugin detected;
//! - [`updates`]: a plugin from where it is published, to install it or update it,
//!   downloaded and checked without running it;
//! - [`Plugin`]: a started plugin, in the sandbox [`plugin_options`] sets up from its
//!   manifest, with the objects the app hands it ([`Granted`]), and
//!   [`detect_all`], which asks every plugin at once and ranks the answers.

/// Running programs for plugins with `Commands`.
mod commands;
/// The Mac's DNS setup for plugins with `Network`.
mod dns;
mod granted;
// TEMPORARY(open_url): which URLs a plugin may open; README, "Temporary host APIs".
pub mod open_url;
// TEMPORARY(network): the app's HTTP for plugins, until embedded_gpui links `wasi:http`; README,
// "Temporary host APIs".
mod network;
mod plugin;
mod plugin_log;
mod ranking;
pub mod updates;

pub use granted::Granted;
pub use plugin::{CALL_TIMEOUT, Plugin, detect_all, plugin_options};
pub use delight_manifest::read_manifest;
pub use ranking::{Candidate, RECOMMENDED, rank};
