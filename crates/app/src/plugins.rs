//! The plugins: Delight's own tools (the built-ins) and every `.wasm` in the plugins
//! folder, started at launch, each with its own [`HostRoot`], what it may ask of the
//! app.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use anyhow::Context as _;
use delight_protocol::{HostApi, Manifest, Theme};
use delight_runtime::{Plugin, plugin_options, read_manifest};
use embedded_gpui::shared;
use delight_ui::ActiveTheme as _;
use gpui::{App, AppContext as _, ClipboardItem, Context, Global, PlatformTextSystem, Subscription};

use crate::{history, launcher};

/// The plugins that started, in the app's order: the built-ins, then the installed
/// ones, each by file path. Empty until they have all started or failed.
#[derive(Default)]
struct Plugins(Rc<[Plugin]>);

impl Global for Plugins {}

pub fn all(cx: &App) -> Rc<[Plugin]> {
    cx.try_global::<Plugins>().map(|plugins| plugins.0.clone()).unwrap_or_default()
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

/// Start the built-ins and every plugin in the plugins folder, in the background, and
/// tell the launcher once they have. An installed plugin with a built-in's id
/// replaces it (and comes last). A file that isn't a plugin, or a second installed
/// plugin with the same id, is logged and left out.
pub fn load(text_system: Arc<dyn PlatformTextSystem>, cx: &mut App) {
    let (builtins, installed) = (builtins_dir(), plugins_dir());
    cx.spawn(async move |cx| {
        let read = cx
            .background_spawn(async move { in_order(manifests(&builtins, false), manifests(&installed, true)) })
            .await;
        let starting = cx.update(|cx| {
            let mut ids = HashSet::new();
            let mut starting = Vec::new();
            for (file, manifest) in read {
                let manifest = match manifest {
                    Ok(manifest) => manifest,
                    Err(error) => {
                        log::warn!("{}: {error:#}", file.display());
                        continue;
                    }
                };
                let id = manifest.plugin.id.clone();
                if !ids.insert(id.clone()) {
                    log::warn!("{}: another plugin is already {id}", file.display());
                    continue;
                }
                let options = plugin_options(&manifest, data_dir(&id), text_system.clone());
                let root = cx.new(|cx| HostRoot {
                    plugin_id: id.clone(),
                    // The plugin observes this object: tell it when the theme changes.
                    _theme_changes: cx.observe_global::<delight_ui::Theme>(|_, cx| cx.notify()),
                });
                starting.push((file.clone(), Plugin::start(file, manifest, options, root, cx)));
            }
            starting
        });
        let mut started = Vec::new();
        for (file, start) in starting {
            match start.await {
                Ok(plugin) => started.push(plugin),
                Err(error) => log::warn!("{}: {error:#}", file.display()),
            }
        }
        cx.update(|cx| {
            cx.set_global(Plugins(started.into()));
            launcher::plugins_loaded(cx);
        });
    })
    .detach();
}

type Read = Vec<(PathBuf, anyhow::Result<Manifest>)>;

/// The built-ins, then the installed plugins, leaving out a built-in that an installed
/// plugin replaces (has its id).
fn in_order(builtins: Read, installed: Read) -> Read {
    let installed_ids: HashSet<String> =
        installed.iter().filter_map(|(_, manifest)| Some(manifest.as_ref().ok()?.plugin.id.clone())).collect();
    let replaced = |manifest: &anyhow::Result<Manifest>| {
        manifest.as_ref().is_ok_and(|manifest| installed_ids.contains(&manifest.plugin.id))
    };
    builtins.into_iter().filter(|(_, manifest)| !replaced(manifest)).chain(installed).collect()
}

/// Each `.wasm` file in `dir`, by path, with its manifest or why it has none. The
/// plugins folder is made if it's missing (`create`); a missing built-ins folder has
/// none.
fn manifests(dir: &Path, create: bool) -> Read {
    if create && let Err(error) = std::fs::create_dir_all(dir) {
        log::error!("creating the plugins folder {}: {error}", dir.display());
    }
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(|entry| Some(entry.ok()?.path()))
        .filter(|path| path.extension().is_some_and(|extension| extension == "wasm"))
        .collect();
    files.sort();
    files
        .into_iter()
        .map(|file| {
            let manifest = std::fs::read(&file).context("reading it").and_then(|wasm| read_manifest(&wasm));
            (file, manifest)
        })
        .collect()
}

/// What one plugin may ask of the app. Each plugin has its own, so every call is
/// that plugin's.
struct HostRoot {
    plugin_id: String,
    _theme_changes: Subscription,
}

#[shared]
impl HostApi for HostRoot {
    // Deferred: the launcher may be in the middle of an update.
    fn toast(&mut self, message: String, cx: &mut Context<Self>) {
        cx.defer(move |cx| launcher::toast(message.into(), cx));
    }

    fn copy_text(&mut self, text: String, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(text));
    }

    fn hide(&mut self, cx: &mut Context<Self>) {
        cx.defer(launcher::hide);
    }

    fn remember_input(&mut self, operation: String, text: String, cx: &mut Context<Self>) {
        let plugin_id = &self.plugin_id;
        if let Err(error) = history::get_mut(cx).remember(plugin_id, &operation, &text) {
            log::error!("remembering {plugin_id}'s input: {error:#}");
        }
    }

    fn current_theme(&mut self, cx: &mut Context<Self>) -> Theme {
        Theme::from(cx.theme())
    }
}

#[cfg(test)]
mod tests {
    use delight_protocol::{Operation, PluginProperties};

    use super::*;

    fn plugin(file: &str, id: &str) -> (PathBuf, anyhow::Result<Manifest>) {
        let manifest = Manifest {
            plugin: PluginProperties {
                id: id.into(),
                name: id.into(),
                version: "1".into(),
                description: String::new(),
                author: String::new(),
                icon: "<svg/>".into(),
                tags: Vec::new(),
                permissions: Vec::new(),
            },
            operations: vec![Operation {
                id: "run".into(),
                title: "Run".into(),
                description: String::new(),
                icon: None,
                tags: Vec::new(),
            }],
        };
        (PathBuf::from(file), Ok(manifest))
    }

    fn files(read: &Read) -> Vec<&str> {
        read.iter().map(|(file, _)| file.to_str().unwrap()).collect()
    }

    #[test]
    fn built_ins_first_and_an_installed_plugin_replaces_one() {
        let builtins = vec![plugin("json.wasm", "delight.json"), plugin("svg.wasm", "delight.svg")];
        let installed = vec![plugin("my-svg.wasm", "delight.svg"), plugin("logs.wasm", "acme.logs")];
        assert_eq!(files(&in_order(builtins, installed)), ["json.wasm", "my-svg.wasm", "logs.wasm"]);
    }
}
