//! Running the tools a release needs (Apple's `codesign`, `notarytool`, `hdiutil`, …, cargo),
//! and saying what is run.

use std::ffi::OsStr;
use std::process::Command;

use anyhow::{Context as _, Result, ensure};

/// A command with its arguments.
pub fn tool<I, S>(program: impl AsRef<OsStr>, arguments: I) -> Command
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut command = Command::new(program);
    command.args(arguments);
    command
}

/// Run it, showing the command first; an error if it fails.
pub fn run(mut command: Command) -> Result<()> {
    eprintln!("> {command:?}");
    let status = command.status().with_context(|| format!("running {command:?}"))?;
    ensure!(status.success(), "{command:?} failed with {status}");
    Ok(())
}

/// Run it and return what it printed, trimmed; an error, with its messages, if it fails.
pub fn capture(mut command: Command) -> Result<String> {
    let output = command.output().with_context(|| format!("running {command:?}"))?;
    ensure!(
        output.status.success(),
        "{command:?} failed with {}:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}
