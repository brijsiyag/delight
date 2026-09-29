//! Running programs ([`Host::run`]), for plugins with the `Commands` permission.

use anyhow::{Context as _, Result, anyhow};
use delight_protocol::{Command, CommandOutput, CommandsApiCaller as _, HostApiCaller as _};

use crate::Host;
use crate::gpui::{App, Task};

impl Host {
    /// Run a program the manifest's `Commands` permission lists, with any arguments,
    /// and wait for it to end. It is started directly, not through a shell, with an
    /// empty environment, in this plugin's data folder, and killed if it runs too long.
    /// A program that ends with a failing status is `Ok` (see
    /// [`CommandOutput::success`]); one that isn't listed or doesn't start is an error.
    pub fn run(&self, command: Command, cx: &mut App) -> Task<Result<CommandOutput>> {
        let Some(remote) = self.remote.clone() else {
            return Task::ready(Err(anyhow!("programs are run by Delight: a plugin runs them only in Delight")));
        };
        let asked = remote.commands(cx);
        cx.spawn(async move |cx| {
            let commands = asked.await?.context("this plugin doesn't have the Commands permission")?.connect();
            let output = cx.update(|cx| commands.run_command(command, cx));
            output.await
        })
    }
}
