//! Delight's plugins, without windows.
//!
//! - [`read_manifest`]: what a plugin's `.wasm` says it is, read without running any
//!   of it;
//! - [`rank`]: which tools fit an input, from what every plugin detected.
//!
//! Running plugins (their sandbox, starting them, talking to them) comes next.

mod plugin_file;
mod ranking;

pub use plugin_file::read_manifest;
pub use ranking::{Candidate, RECOMMENDED, rank};
