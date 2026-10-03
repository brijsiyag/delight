//! TEMPORARY(save_file): the system's save panel, through the app, until GPUI's own
//! `cx.prompt_for_new_path` works in a plugin. docs/development.md, "Temporary host APIs".

use std::path::PathBuf;

use anyhow::{Result, anyhow};
use delight_protocol::HostApiCaller as _;

use crate::Host;
use crate::gpui::{App, Task};

impl Host {
    /// Show the system's save panel with `name` suggested (in Downloads), and write `contents`
    /// to the file the user chooses: where it went, or `None` when they cancelled; an error when
    /// Delight can't show the panel (another alert or picker is open) or write the file. Delight
    /// writes it, so this needs no folder and no permission.
    pub fn save_file(&self, name: impl Into<String>, contents: impl Into<Vec<u8>>, cx: &mut App) -> Task<Result<Option<PathBuf>>> {
        let Some(remote) = &self.remote else {
            return Task::ready(Err(anyhow!("the save panel is Delight's: a plugin has it only in Delight")));
        };
        // As base64 text: a third larger than the file, where a list of numbers would be 3.5 times.
        let asked = remote.save_file(name.into(), delight_protocol::Bytes(contents.into()), cx);
        cx.spawn(async move |_| Ok(asked.await?.map(PathBuf::from)))
    }
}
