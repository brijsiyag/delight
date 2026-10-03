//! TEMPORARY(pick_folders): the system's folder picker, through the app, until GPUI's own
//! `cx.prompt_for_paths` works in a plugin. docs/development.md, "Temporary host APIs".

use std::path::PathBuf;

use anyhow::{Result, anyhow};
use delight_protocol::HostApiCaller as _;

use crate::gpui::{App, Task};
use crate::Host;

impl Host {
    /// Show the system's folder picker. The folders picked are the plugin's, with `options`'
    /// access. Needs the `Files` permission.
    ///
    /// **When any is new, the plugin starts again** with them in its sandbox, at the paths picked,
    /// and this never returns, as with [`Host::request_permission`]. Otherwise the folders picked (all
    /// of which it had), or none when the user cancelled; an error when Delight can't show the
    /// picker (another alert or picker is open).
    pub fn pick_folders(&self, options: PickFolders, cx: &mut App) -> Task<Result<Vec<PathBuf>>> {
        let Some(remote) = &self.remote else {
            return Task::ready(Err(anyhow!("folders are Delight's to give: a plugin has them only in Delight")));
        };
        let PickFolders { multiple, access, prompt } = options;
        let asked = remote.pick_folders(multiple, access == Access::Write, prompt, cx);
        cx.spawn(async move |_| Ok(asked.await?.into_iter().map(PathBuf::from).collect()))
    }
}

/// What the folder picker lets the user pick ([`Host::pick_folders`]).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PickFolders {
    /// Several folders, not one.
    pub multiple: bool,
    pub access: Access,
    /// The picker's button: "Open" by default.
    pub prompt: Option<String>,
}

impl PickFolders {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn multiple(mut self) -> Self {
        self.multiple = true;
        self
    }

    pub fn access(mut self, access: Access) -> Self {
        self.access = access;
        self
    }

    pub fn prompt(mut self, prompt: impl Into<String>) -> Self {
        self.prompt = Some(prompt.into());
        self
    }
}

/// What a plugin may do in the folders picked ([`PickFolders::access`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Access {
    /// Read its files.
    #[default]
    Read,
    /// Write them (and add and remove them), and read them.
    Write,
}
