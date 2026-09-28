//! The plugins: Delight's own tools (the built-ins) and every `.wasm` in the plugins
//! folder, each with its own host object, and the files that don't load, with why.
//!
//! * this file: the plugins as they are now, and where they come from.
//! * `loading`: finding the files, reading their manifests, and starting them, at
//!   launch and again after an install or a delete.
//! * `install`: installing a plugin, and deleting one.
//! * `host_root`: what a plugin may ask of the app.

mod host_root;
mod install;
mod loading;

use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use delight_runtime::Plugin;
use gpui::{App, Global, PlatformTextSystem};

pub use install::{delete, delete_file, inspect, install};
pub use loading::{load, reload};

/// Where a plugin comes from.
#[derive(Clone, Debug, PartialEq)]
pub struct Source {
    pub file: PathBuf,
    pub built_in: bool,
}

/// Why a plugin file doesn't load.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Problem {
    Unreadable,
    /// No manifest this Delight reads: not a plugin, built for another protocol
    /// version, or its manifest is invalid.
    NotAPlugin,
    /// Another plugin, earlier in the order, already has its id.
    SameId,
    DoesntStart,
}

impl Problem {
    pub fn summary(self) -> &'static str {
        match self {
            Problem::Unreadable => "Couldn’t be read",
            Problem::NotAPlugin => "Not a plugin for this Delight",
            Problem::SameId => "Another plugin has the same id",
            Problem::DoesntStart => "Doesn’t start",
        }
    }

    /// What to do about it.
    pub fn hint(self) -> &'static str {
        match self {
            Problem::Unreadable => "The file couldn’t be opened.",
            Problem::NotAPlugin => {
                "It may be built for the previous Delight or another version. Rebuild it with the plugin \
                 API, then install it again."
            }
            Problem::SameId => "Two files are the same plugin: delete one of them.",
            Problem::DoesntStart => "It stopped while starting. Rebuild it, or ask its author.",
        }
    }
}

/// A plugin file that doesn't load, and why.
#[derive(Clone, Debug)]
pub struct Broken {
    pub source: Source,
    pub problem: Problem,
    /// The error, as it came.
    pub detail: String,
}

/// The plugins that started, in the app's order (the built-ins, then the installed
/// ones, each by file path), each with where it comes from; and the files that don't
/// load.
#[derive(Default)]
struct Plugins {
    started: Rc<[Plugin]>,
    /// Where each of `started` comes from, in the same order.
    sources: Rc<[Source]>,
    broken: Rc<[Broken]>,
    /// While they're being started (again).
    loading: bool,
    /// What they shape their text with: the app's.
    text_system: Option<Arc<dyn PlatformTextSystem>>,
}

impl Global for Plugins {}

pub fn all(cx: &App) -> Rc<[Plugin]> {
    cx.try_global::<Plugins>().map(|plugins| plugins.started.clone()).unwrap_or_default()
}

/// Where each of [`all`] comes from, in the same order.
pub fn sources(cx: &App) -> Rc<[Source]> {
    cx.try_global::<Plugins>().map(|plugins| plugins.sources.clone()).unwrap_or_default()
}

pub fn broken(cx: &App) -> Rc<[Broken]> {
    cx.try_global::<Plugins>().map(|plugins| plugins.broken.clone()).unwrap_or_default()
}

/// Whether the plugins are being started (again) now.
pub fn loading(cx: &App) -> bool {
    cx.try_global::<Plugins>().is_some_and(|plugins| plugins.loading)
}

/// Where installed plugins are: `.wasm` files.
fn plugins_dir() -> PathBuf {
    crate::app_dir().join("plugins")
}

/// Where Delight's own tools are: `Contents/Resources/plugins` in Delight.app, and
/// the built-ins' build output (`plugins/` in this repository) during development.
fn builtins_dir() -> PathBuf {
    let bundled = std::env::current_exe()
        .ok()
        .and_then(|exe| Some(exe.parent()?.parent()?.join("Resources/plugins")))
        .filter(|dir| dir.is_dir());
    bundled.unwrap_or_else(|| {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins/target/wasm32-wasip2/release")
    })
}

/// A plugin's own folder, mounted in its sandbox as `/data`.
fn data_dir(plugin_id: &str) -> PathBuf {
    crate::app_dir().join("plugin-data").join(plugin_id)
}
