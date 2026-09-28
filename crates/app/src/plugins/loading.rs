//! Finding the plugin files, reading their manifests without running them, and
//! starting the plugins: at launch, and again after an install or a delete.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use delight_protocol::Manifest;
use delight_runtime::{Plugin, plugin_options, read_manifest};
use gpui::{App, AppContext as _, PlatformTextSystem};

use super::host_root::HostRoot;
use super::{Broken, Plugins, Problem, Source, builtins_dir, data_dir, plugins_dir};
use crate::launcher;

/// Start the plugins at launch, shaping their text with the app's text system.
pub fn load(text_system: Arc<dyn PlatformTextSystem>, cx: &mut App) {
    cx.set_global(Plugins { text_system: Some(text_system), ..Plugins::default() });
    reload(cx);
}

/// Start the built-ins and every plugin in the plugins folder, in the background, and
/// tell the launcher once they have. An installed plugin with a built-in's id replaces
/// it (and comes last).
pub fn reload(cx: &mut App) {
    let Some(text_system) = cx.global::<Plugins>().text_system.clone() else { return };
    cx.global_mut::<Plugins>().loading = true;
    cx.refresh_windows();
    let (builtins, installed) = (builtins_dir(), plugins_dir());
    cx.spawn(async move |cx| {
        let read = cx
            .background_spawn(async move { in_order(manifests(&builtins, true), manifests(&installed, false)) })
            .await;
        let (starting, mut broken) = cx.update(|cx| {
            let mut ids = HashSet::new();
            let mut starting = Vec::new();
            let mut broken = Vec::new();
            for (source, manifest) in read {
                let manifest = match manifest {
                    Ok(manifest) => manifest,
                    Err((problem, detail)) => {
                        log::warn!("{}: {detail}", source.file.display());
                        broken.push(Broken { source, problem, detail });
                        continue;
                    }
                };
                let id = manifest.plugin.id.clone();
                if !ids.insert(id.clone()) {
                    let detail = format!("another plugin is already {id}");
                    log::warn!("{}: {detail}", source.file.display());
                    broken.push(Broken { source, problem: Problem::SameId, detail });
                    continue;
                }
                let options = plugin_options(&manifest, data_dir(&id), text_system.clone());
                let root = cx.new(|cx| HostRoot::new(id.clone(), cx));
                let start = Plugin::start(source.file.clone(), manifest, options, root, cx);
                starting.push((source, start));
            }
            (starting, broken)
        });
        let (mut started, mut sources) = (Vec::new(), Vec::new());
        for (source, start) in starting {
            match start.await {
                Ok(plugin) => {
                    started.push(plugin);
                    sources.push(source);
                }
                Err(error) => {
                    log::warn!("{}: {error:#}", source.file.display());
                    broken.push(Broken { source, problem: Problem::DoesntStart, detail: format!("{error:#}") });
                }
            }
        }
        cx.update(|cx| {
            let plugins = cx.global_mut::<Plugins>();
            plugins.started = started.into();
            plugins.sources = sources.into();
            plugins.broken = broken.into();
            plugins.loading = false;
            launcher::plugins_loaded(cx);
            cx.refresh_windows();
        });
    })
    .detach();
}

type Read = Vec<(Source, Result<Manifest, (Problem, String)>)>;

/// The built-ins, then the installed plugins, leaving out a built-in that an installed
/// plugin replaces (has its id).
fn in_order(builtins: Read, installed: Read) -> Read {
    let installed_ids: HashSet<String> =
        installed.iter().filter_map(|(_, manifest)| Some(manifest.as_ref().ok()?.plugin.id.clone())).collect();
    let replaced = |manifest: &Result<Manifest, (Problem, String)>| {
        manifest.as_ref().is_ok_and(|manifest| installed_ids.contains(&manifest.plugin.id))
    };
    builtins.into_iter().filter(|(_, manifest)| !replaced(manifest)).chain(installed).collect()
}

/// Each `.wasm` file in `dir`, by path, with its manifest or why it has none. The
/// plugins folder is made if it's missing; a missing built-ins folder has none.
fn manifests(dir: &Path, built_in: bool) -> Read {
    if !built_in && let Err(error) = std::fs::create_dir_all(dir) {
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
            let manifest = match std::fs::read(&file) {
                Ok(wasm) => read_manifest(&wasm).map_err(|error| (Problem::NotAPlugin, format!("{error:#}"))),
                Err(error) => Err((Problem::Unreadable, error.to_string())),
            };
            (Source { file, built_in }, manifest)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use delight_protocol::{Operation, PluginProperties};

    use super::*;

    fn plugin(file: &str, id: &str, built_in: bool) -> (Source, Result<Manifest, (Problem, String)>) {
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
                tips: Vec::new(),
            },
            operations: vec![Operation {
                id: "run".into(),
                title: "Run".into(),
                description: String::new(),
                icon: None,
                tags: Vec::new(),
            }],
        };
        (Source { file: PathBuf::from(file), built_in }, Ok(manifest))
    }

    fn files(read: &Read) -> Vec<&str> {
        read.iter().map(|(source, _)| source.file.to_str().unwrap()).collect()
    }

    #[test]
    fn built_ins_first_and_an_installed_plugin_replaces_one() {
        let builtins = vec![plugin("json.wasm", "delight.json", true), plugin("svg.wasm", "delight.svg", true)];
        let installed = vec![plugin("my-svg.wasm", "delight.svg", false), plugin("logs.wasm", "acme.logs", false)];
        assert_eq!(files(&in_order(builtins, installed)), ["json.wasm", "my-svg.wasm", "logs.wasm"]);
    }
}
