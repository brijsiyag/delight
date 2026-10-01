//! The plugins: Delight's own tools (the built-ins) and every `.wasm` in the plugins
//! folder, each with its own host object, and the files that don't load, with why.
//!
//! * this file: the plugins as they are now, and where they come from.
//! * `loading`: finding the files, reading their manifests, and starting them: all at
//!   launch, then each on its own after its install, update or delete.
//! * `install`: installing a plugin, and deleting one.
//! * `host_root`: what a plugin may ask of the app.
//! * `updates`: newer versions of installed plugins, from the locations they name.

mod host_root;
mod install;
mod loading;
pub mod updates;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::SystemTime;

use delight_runtime::Plugin;
use gpui::{App, Global, PlatformTextSystem, Task};

pub use install::{delete, delete_file, download, download_fraction, download_size, inspect, install, megabytes};
use install::save_download;
pub use loading::{forget_file, load, restart};

/// A file's size and modification time: read twice, an unchanged file gives equal stamps.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Stamp {
    len: u64,
    modified: Option<SystemTime>,
}

impl Stamp {
    fn of(file: &Path) -> Stamp {
        std::fs::metadata(file).map(|meta| Stamp { len: meta.len(), modified: meta.modified().ok() }).unwrap_or_default()
    }
}

/// Where a plugin comes from. Two sources are equal when they are the same unchanged file: a
/// plugin running from one is still the right one to run.
#[derive(Clone, Debug, PartialEq)]
pub struct Source {
    pub file: PathBuf,
    pub built_in: bool,
    stamp: Stamp,
}

impl Source {
    fn new(file: PathBuf, built_in: bool) -> Source {
        let stamp = Stamp::of(&file);
        Source { file, built_in, stamp }
    }
}

/// Why a plugin file doesn't load.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Problem {
    Unreadable,
    /// No manifest this Delight reads: not a plugin, built for another protocol
    /// version, or its manifest is invalid.
    NotAPlugin,
    /// Its file isn't named `<id>.wasm`, as every plugin file is.
    WrongName,
    DoesntStart,
}

impl Problem {
    pub fn summary(self) -> &'static str {
        match self {
            Problem::Unreadable => "Couldn’t be read",
            Problem::NotAPlugin => "Not a plugin for this Delight",
            Problem::WrongName => "Its file isn’t named by its id",
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
            Problem::WrongName => "A plugin’s file is named <id>.wasm. Rename it, or install it with Install Plugin…, which names it.",
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
/// load. Each plugin starts on its own (`loading::restart`): one starting again leaves the others
/// as they are.
#[derive(Default)]
struct Plugins {
    started: Rc<[Plugin]>,
    /// Where each of `started` comes from, in the same order.
    sources: Rc<[Source]>,
    broken: Rc<[Broken]>,
    /// The plugins starting now, by id: replacing one's task cancels its start.
    starting: HashMap<String, Task<()>>,
    /// What they shape their text with: the app's.
    text_system: Option<Arc<dyn PlatformTextSystem>>,
    /// Where compiled plugins are kept, shared by all of them; `None` if the folder can't be used.
    compile_cache: Option<embedded_gpui::CompileCache>,
}

impl Global for Plugins {}

pub fn all(cx: &App) -> Rc<[Plugin]> {
    cx.try_global::<Plugins>().map(|plugins| plugins.started.clone()).unwrap_or_default()
}

/// Let every plugin's copy of the clipboard catch up with the Mac's (see
/// [`Plugin::refresh_clipboard`]): a paste in a plugin's field then reads what was copied last.
///
/// TEMPORARY(clipboard): a workaround for embedded_gpui sending a clipboard change after the ⌘ key
/// that pastes it; remove it and its calls when embedded_gpui is fixed (see the README).
pub fn refresh_clipboards(cx: &mut App) {
    for plugin in all(cx).iter() {
        plugin.refresh_clipboard(cx);
    }
}

/// Where each of [`all`] comes from, in the same order.
pub fn sources(cx: &App) -> Rc<[Source]> {
    cx.try_global::<Plugins>().map(|plugins| plugins.sources.clone()).unwrap_or_default()
}

pub fn broken(cx: &App) -> Rc<[Broken]> {
    cx.try_global::<Plugins>().map(|plugins| plugins.broken.clone()).unwrap_or_default()
}

/// Whether a plugin is starting now.
pub fn loading(cx: &App) -> bool {
    cx.try_global::<Plugins>().is_some_and(|plugins| !plugins.starting.is_empty())
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
