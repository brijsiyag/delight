//! The plugins: every `.wasm` in the plugins folder, started at launch, each with
//! its own [`HostRoot`], what it may ask of the app. Delight's own tools join them
//! in step 11.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use anyhow::Context as _;
use delight_protocol::{Color, HostApi, Manifest, Theme};
use delight_runtime::{Plugin, plugin_options, read_manifest};
use embedded_gpui::shared;
use delight_ui::ActiveTheme as _;
use gpui::{App, AppContext as _, ClipboardItem, Context, Global, PlatformTextSystem, Subscription};

use crate::{history, launcher};

/// The plugins that started, in the app's order (by file path). Empty until they
/// have all started or failed.
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

/// A plugin's own folder, mounted in its sandbox as `/data`.
fn data_dir(plugin_id: &str) -> PathBuf {
    crate::app_dir().join("plugin-data").join(plugin_id)
}

/// Start every plugin in the plugins folder, in the background, and tell the
/// launcher once they have. A file that isn't a plugin, or a second plugin with the
/// same id, is logged and left out.
pub fn load(text_system: Arc<dyn PlatformTextSystem>, cx: &mut App) {
    let dir = plugins_dir();
    cx.spawn(async move |cx| {
        let read = cx.background_spawn(async move { manifests(&dir) }).await;
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

/// Each `.wasm` file in `dir`, by path, with its manifest or why it has none.
fn manifests(dir: &Path) -> Vec<(PathBuf, anyhow::Result<Manifest>)> {
    if let Err(error) = std::fs::create_dir_all(dir) {
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
        theme_for_plugins(cx.theme())
    }
}

/// The app's theme, as plugins get it.
fn theme_for_plugins(theme: &delight_ui::Theme) -> Theme {
    let color = Color::from;
    Theme {
        dark: theme.dark,
        text: color(theme.text),
        text_muted: color(theme.text_muted),
        text_faint: color(theme.text_faint),
        surface: color(theme.surface),
        fill: color(theme.fill),
        border: color(theme.border),
        accent: color(theme.accent),
        accent_text: color(theme.accent_text),
        success: color(theme.success),
        warning: color(theme.warning),
        error: color(theme.error),
        font: theme.font.to_string(),
        mono_font: theme.mono_font.to_string(),
        text_size: theme.text_size.into(),
        radius: theme.radius.into(),
    }
}
