//! Installing a plugin (copying its file into the plugins folder), and deleting one.

use std::path::{Path, PathBuf};

use anyhow::Context as _;
use delight_protocol::Manifest;
use delight_runtime::read_manifest;
use gpui::{App, AppContext as _, Task};

use super::{all, data_dir, plugins_dir, reload, sources};
use crate::{history, settings};

/// What installing `file` would add: its manifest, read without running any of it.
pub fn inspect(file: PathBuf, cx: &App) -> Task<anyhow::Result<Manifest>> {
    cx.background_spawn(async move {
        let wasm = std::fs::read(&file).with_context(|| format!("reading {}", file.display()))?;
        read_manifest(&wasm)
    })
}

/// Copy the plugin in `file` into the plugins folder as `<id>.wasm`, replacing the
/// installed plugin with its id, and start the plugins again.
pub fn install(file: &Path, manifest: &Manifest, cx: &mut App) -> anyhow::Result<()> {
    let dir = plugins_dir();
    std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    let id = &manifest.plugin.id;
    let target = dir.join(format!("{id}.wasm"));
    // The installed plugin it replaces may be in a file of another name.
    let replaced = installed_files(id, cx);
    if file != target {
        std::fs::copy(file, &target).with_context(|| format!("copying it to {}", target.display()))?;
    }
    for old in replaced.iter().filter(|old| **old != target) {
        if let Err(error) = std::fs::remove_file(old) {
            log::error!("removing the replaced {}: {error}", old.display());
        }
    }
    reload(cx);
    Ok(())
}

/// The installed files (not the built-ins) of the plugin with this id.
fn installed_files(plugin_id: &str, cx: &App) -> Vec<PathBuf> {
    all(cx)
        .iter()
        .zip(sources(cx).iter())
        .filter(|(plugin, source)| !source.built_in && plugin.manifest().plugin.id == plugin_id)
        .map(|(_, source)| source.file.clone())
        .collect()
}

/// Delete an installed plugin: its file, its data folder, what it remembered and its
/// switches; then start the plugins again.
pub fn delete(plugin_id: &str, file: &Path, cx: &mut App) -> anyhow::Result<()> {
    std::fs::remove_file(file).with_context(|| format!("removing {}", file.display()))?;
    let data = data_dir(plugin_id);
    match std::fs::remove_dir_all(&data) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
            log::error!("removing {}: {error}", data.display());
        }
        _ => {}
    }
    history::forget_plugin(plugin_id, cx);
    crate::secrets::forget_plugin(plugin_id, cx);
    settings::update(cx, |settings| settings.forget_plugin(plugin_id));
    reload(cx);
    Ok(())
}

/// Delete a plugin file that doesn't load, then start the plugins again.
pub fn delete_file(file: &Path, cx: &mut App) -> anyhow::Result<()> {
    std::fs::remove_file(file).with_context(|| format!("removing {}", file.display()))?;
    reload(cx);
    Ok(())
}
