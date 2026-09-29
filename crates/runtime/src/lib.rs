//! Delight's plugins, without windows.
//!
//! - [`read_manifest`]: what a plugin's `.wasm` says it is, read without running any
//!   of it;
//! - [`rank`]: which tools fit an input, from what every plugin detected;
//! - [`Plugin`]: a started plugin, in the sandbox [`plugin_options`] sets up from its
//!   manifest, with the objects its permissions grant ([`Granted`]), and
//!   [`detect_all`], which asks every plugin at once and ranks the answers.

mod granted;
mod plugin;
mod plugin_file;
mod ranking;

pub use granted::Granted;
pub use plugin::{CALL_TIMEOUT, Plugin, detect_all, plugin_options};
pub use plugin_file::read_manifest;
pub use ranking::{Candidate, RECOMMENDED, rank};
