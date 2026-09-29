//! Running programs, as the app offers it to plugins with the `Commands` permission:
//! only the programs the plugin's manifest lists, each with any arguments. The app runs
//! a program itself (no shell, so `|`, `;` and `$(…)` are just text in an argument),
//! with an empty environment, in the plugin's data folder, and for a limited time.

use embedded_gpui::{data, interface};

/// Running programs for one plugin, homed in the app.
#[interface]
pub trait CommandsApi {
    /// Run `command` and wait for it to end. A program the manifest doesn't list, one
    /// that doesn't start, and one that overruns its time are errors; one that ends
    /// with a failing status is not (see [`CommandOutput::code`]).
    async fn run_command(&mut self, command: Command, cx: &mut gpui::Context<Self>) -> CommandOutput;
}

/// A program to run.
#[data]
#[derive(Default, PartialEq)]
pub struct Command {
    /// Its absolute path, as the manifest lists it: `/bin/ps`.
    pub program: String,
    /// Its arguments, each passed as it is.
    pub args: Vec<String>,
    /// Written to its standard input, which is then closed. Empty for none.
    pub stdin: String,
}

/// What a program did.
#[data]
#[derive(Default, PartialEq)]
pub struct CommandOutput {
    /// Its exit status; `None` when a signal ended it.
    pub code: Option<i32>,
    /// What it wrote, as text (invalid UTF-8 is replaced).
    pub stdout: String,
    pub stderr: String,
}

impl CommandOutput {
    /// Whether it ended with status 0.
    pub fn success(&self) -> bool {
        self.code == Some(0)
    }
}
