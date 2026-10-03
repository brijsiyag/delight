//! TEMPORARY(save_file): the system's save panel for a plugin, with Delight writing the file,
//! until GPUI's own (`cx.prompt_for_new_path`) works in one. docs/development.md, "Temporary host
//! APIs".

use std::path::PathBuf;

use anyhow::{Result, anyhow};
use gpui::{App, Task};

use crate::dialogs;

/// Show the system's save panel in Downloads, `name` suggested with its extension kept, and write
/// `contents` to the file the user chose: where it went; none when they cancelled; an error without
/// showing it when an alert or a panel is up already, or when the file can't be written.
pub fn save(name: String, contents: Vec<u8>, cx: &mut App) -> Task<Result<Option<PathBuf>>> {
    let folder = dirs::download_dir().or_else(std::env::home_dir).unwrap_or_else(|| PathBuf::from("/"));
    let open = |cx: &mut App| cx.prompt_for_new_path(&folder, Some(&name));
    let keeping = name.clone();
    let chosen = dialogs::panel("the save panel", open, move |panel| panel.keep_extension(&keeping), cx);
    cx.spawn(async move |cx| {
        let Some(path) = chosen.await? else { return Ok(None) };
        let written = path.clone();
        cx.background_executor()
            .spawn(async move { std::fs::write(&written, contents) })
            .await
            .map_err(|error| anyhow!("writing {}: {error}", path.display()))?;
        Ok(Some(path))
    })
}
