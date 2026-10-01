//! Finding the plugin files, reading their manifests without running them, and starting the
//! plugins, each on its own ([`restart`]): every one at launch, then one when it is installed,
//! updated or deleted. Every plugin file, built-in or installed, is named `<id>.wasm`: a plugin is
//! found by its id. Plugins never wait for one another: each starts in a task of its own and
//! joins the running ones when it has; a newer change to a plugin cancels its start in progress.

use std::collections::{BTreeSet, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use delight_protocol::Manifest;
use delight_runtime::{Plugin, plugin_options, read_manifest};
use embedded_gpui::CompileCache;
use gpui::{App, AppContext as _, AsyncApp, PlatformTextSystem};

use super::host_root::HostRoot;
use super::{Broken, Plugins, Problem, Source, builtins_dir, data_dir, plugins_dir};
use crate::{launcher, plugin_windows};

/// Start the plugins at launch, shaping their text with the app's text system: every plugin file,
/// built-in or installed, is named `<id>.wasm`, and each plugin is started on its own, as
/// [`restart`] starts one. An installed plugin with a built-in's id replaces it.
pub fn load(text_system: Arc<dyn PlatformTextSystem>, cx: &mut App) {
    // One cache for every plugin, however often they start: its worker looks after the folder.
    let compile_cache = CompileCache::new(crate::cache_dir().join("compiled"))
        .map_err(|error| log::warn!("no cache of compiled plugins, they compile at every start: {error:#}"))
        .ok();
    cx.set_global(Plugins { text_system: Some(text_system), compile_cache, ..Plugins::default() });
    let installed = plugins_dir();
    if let Err(error) = std::fs::create_dir_all(&installed) {
        log::error!("creating the plugins folder {}: {error}", installed.display());
    }
    let ids: BTreeSet<String> = ids_in(&builtins_dir()).into_iter().chain(ids_in(&installed)).collect();
    for id in ids {
        restart(&id, cx);
    }
}

/// Plugin `id` was installed, updated or deleted: start it again, alone, from the file that is it
/// now (the installed `<id>.wasm`, else the built-in one), or take it away if there is none. The
/// other plugins go on as they are. A plugin already running from that very file goes on too.
/// Asked again before it has started, the newer ask cancels this one.
pub fn restart(id: &str, cx: &mut App) {
    let file = [(plugins_dir(), false), (builtins_dir(), true)]
        .into_iter()
        .map(|(dir, built_in)| (dir.join(format!("{id}.wasm")), built_in))
        .find(|(file, _)| file.is_file());
    let starting = cx.spawn({
        let id = id.to_string();
        async move |cx| {
            let Some((file, built_in)) = file else {
                cx.update(|cx| settle(&id, Outcome::Gone, cx));
                return;
            };
            let (source, manifest) = cx.background_spawn(async move { read_file(file, built_in) }).await;
            let manifest = match manifest {
                Ok(manifest) if manifest.plugin.id == id => manifest,
                Ok(manifest) => {
                    let detail = format!("it is named {id}.wasm, but it is the plugin {0}: name it {0}.wasm", manifest.plugin.id);
                    cx.update(|cx| settle(&id, Outcome::DoesntLoad(Broken { source, problem: Problem::WrongName, detail }), cx));
                    return;
                }
                Err((problem, detail)) => {
                    cx.update(|cx| settle(&id, Outcome::DoesntLoad(Broken { source, problem, detail }), cx));
                    return;
                }
            };
            // The very file it runs from, and still running: nothing changed.
            let unchanged = cx.update(|cx| {
                let plugins = cx.global::<Plugins>();
                plugins.sources.iter().zip(plugins.started.iter()).any(|(running, plugin)| *running == source && plugin.stopped().is_none())
            });
            if unchanged {
                cx.update(|cx| cx.global_mut::<Plugins>().starting.remove(&id));
                return;
            }
            start(id, source, manifest, cx).await;
        }
    });
    // Replacing a start in progress drops it, which cancels it.
    cx.global_mut::<Plugins>().starting.insert(id.to_string(), starting);
    cx.refresh_windows();
}

/// A plugin file that doesn't load was deleted: it is no longer listed.
pub fn forget_file(file: &Path, cx: &mut App) {
    let plugins = cx.global_mut::<Plugins>();
    plugins.broken = plugins.broken.iter().filter(|broken| broken.source.file != file).cloned().collect();
    cx.refresh_windows();
}

/// What became of one plugin.
enum Outcome {
    /// It started, from this file.
    Started(Source, Plugin),
    /// Its file doesn't load, or it didn't start.
    DoesntLoad(Broken),
    /// It has no file any more.
    Gone,
}

/// Start plugin `id` from `source`, then put it in its place among the running plugins.
async fn start(id: String, source: Source, manifest: Manifest, cx: &mut AsyncApp) {
    let started = cx.update(|cx| {
        let (text_system, compile_cache) = {
            let plugins = cx.global::<Plugins>();
            (plugins.text_system.clone()?, plugins.compile_cache.clone())
        };
        let data = data_dir(&id);
        // Compiled once, then loaded from the cache while the file is the same.
        let mut options = plugin_options(&manifest, data.clone(), text_system);
        if let Some(cache) = compile_cache {
            options = options.with_compile_cache(cache);
        }
        let host_id = id.clone();
        let root = move |granted, cx: &mut App| cx.new(|cx| HostRoot::new(host_id, granted, cx));
        Some(Plugin::start(source.file.clone(), manifest, options, data, root, cx))
    });
    let Some(started) = started else {
        cx.update(|cx| cx.global_mut::<Plugins>().starting.remove(&id));
        return;
    };
    let outcome = match started.await {
        Ok(plugin) => Outcome::Started(source, plugin),
        Err(error) => Outcome::DoesntLoad(Broken { source, problem: Problem::DoesntStart, detail: format!("{error:#}") }),
    };
    cx.update(|cx| settle(&id, outcome, cx));
}

/// Put what became of plugin `id` in place of what was running as it: its new instance, or the
/// reason its file doesn't load, or nothing. Its old instance's windows close, and the launcher lets
/// go of its tools.
fn settle(id: &str, outcome: Outcome, cx: &mut App) {
    let plugins = cx.global_mut::<Plugins>();
    plugins.starting.remove(id);
    let mut running: Vec<(Source, Plugin)> = plugins.sources.iter().cloned().zip(plugins.started.iter().cloned()).collect();
    running.retain(|(_, plugin)| plugin.manifest().plugin.id != id);
    let mut broken = plugins.broken.to_vec();
    match outcome {
        Outcome::Started(source, plugin) => {
            broken.retain(|broken| broken.source.file != source.file);
            running.push((source, plugin));
        }
        Outcome::DoesntLoad(file) => {
            log::warn!("{}: {}", file.source.file.display(), file.detail);
            broken.retain(|broken| broken.source.file != file.source.file);
            broken.push(file);
        }
        Outcome::Gone => {}
    }
    // The app's order: the built-ins, then the installed plugins, each by path.
    running.sort_by(|(a, _), (b, _)| (!a.built_in, &a.file).cmp(&(!b.built_in, &b.file)));
    plugins.sources = running.iter().map(|(source, _)| source.clone()).collect();
    plugins.started = running.into_iter().map(|(_, plugin)| plugin).collect();
    plugins.broken = broken.into();
    plugin_windows::close_of(&HashSet::from([id.to_string()]), cx);
    launcher::plugins_loaded(cx);
    cx.refresh_windows();
}

/// The ids of the plugins in `dir`: the names of its `.wasm` files. None if it's missing.
fn ids_in(dir: &Path) -> Vec<String> {
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            (path.extension()? == "wasm").then(|| path.file_stem()?.to_str().map(str::to_string)).flatten()
        })
        .collect()
}

/// One plugin file, with its manifest or why it has none.
fn read_file(file: PathBuf, built_in: bool) -> (Source, Result<Manifest, (Problem, String)>) {
    let manifest = match std::fs::read(&file) {
        Ok(wasm) => read_manifest(&wasm).map_err(|error| (Problem::NotAPlugin, format!("{error:#}"))),
        Err(error) => Err((Problem::Unreadable, error.to_string())),
    };
    (Source::new(file, built_in), manifest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plugin_goes_on_only_while_its_file_is_the_same() {
        let file = std::env::temp_dir().join(format!("delight-source-{}.wasm", std::process::id()));
        std::fs::write(&file, b"one").unwrap();
        let before = Source::new(file.clone(), false);
        assert_eq!(before, Source::new(file.clone(), false), "read twice: the same");
        assert_ne!(before, Source::new(file.clone(), true), "another kind of place");
        std::fs::write(&file, b"another plugin").unwrap();
        assert_ne!(before, Source::new(file.clone(), false), "the file changed: start it again");
        std::fs::remove_file(&file).unwrap();
        assert_ne!(before, Source::new(file, false), "the file went");
    }
}
