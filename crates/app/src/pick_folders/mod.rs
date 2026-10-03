//! TEMPORARY(pick_folders): the system's folder picker for a plugin, until GPUI's own
//! (`cx.prompt_for_paths`) works in one. docs/development.md, "Temporary host APIs".

use std::path::PathBuf;

use anyhow::Result;
use gpui::{App, PathPromptOptions, Task};

use crate::dialogs;

/// Show the system's folder picker: one folder, or several with `multiple`, its button saying
/// `prompt` (the system's own when none). The folders picked; none when the user cancelled; an
/// error without showing it when an alert or a panel is up already.
pub fn pick(multiple: bool, prompt: Option<String>, cx: &mut App) -> Task<Result<Vec<PathBuf>>> {
    let options = PathPromptOptions { files: false, directories: true, multiple, prompt: prompt.map(Into::into) };
    let picked = dialogs::panel("the folder picker", |cx| cx.prompt_for_paths(options), |_| {}, cx);
    cx.spawn(async move |_| Ok(picked.await?.unwrap_or_default()))
}
