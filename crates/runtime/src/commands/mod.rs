//! Running programs for plugins with the `Commands` permission ([`CommandsApi`]): the
//! ones the manifest lists, with any arguments.
//!
//! A program is started directly, never through a shell, so `|`, `;`, `&&` and `$(…)`
//! in an argument are only text. Its environment is empty (no `PATH`, no `DYLD_*`), its
//! working folder is the plugin's data folder, and it gets [`TIMEOUT`] to end.

use std::io::{Read, Write as _};
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use anyhow::{Context as _, Result, bail};
use delight_manifest::validate_program;
use delight_protocol::{Command, CommandOutput, CommandsApi};
use embedded_gpui::gpui::{AppContext as _, Context, Task};
use embedded_gpui::shared;
use wait_timeout::ChildExt as _;

/// Longest a program runs before it is killed.
pub const TIMEOUT: Duration = Duration::from_secs(60);
/// Most of a program's output kept, per stream: what it writes beyond that is dropped.
const MAX_OUTPUT: u64 = 16 * 1024 * 1024;

pub(crate) struct Commands {
    /// The programs the manifest lists.
    programs: Vec<String>,
    /// The plugin's data folder, where they run.
    folder: PathBuf,
}

impl Commands {
    pub(crate) fn new(programs: Vec<String>, folder: PathBuf) -> Self {
        Commands { programs, folder }
    }
}

#[shared]
impl CommandsApi for Commands {
    fn run_command(&mut self, command: Command, cx: &mut Context<Self>) -> Task<Result<CommandOutput>> {
        if let Err(error) = self.allowed(&command.program) {
            return Task::ready(Err(error));
        }
        let folder = self.folder.clone();
        cx.background_spawn(async move { run(&command, &folder, TIMEOUT) })
    }
}

impl Commands {
    /// Whether the manifest lists `program`: exactly, and in the standard folders (the
    /// manifest is checked for that too, when it is read).
    fn allowed(&self, program: &str) -> Result<()> {
        if validate_program(program).is_err() || !self.programs.iter().any(|listed| listed == program) {
            bail!("{program} isn't one of the programs this plugin's manifest lists");
        }
        Ok(())
    }
}

/// Run `command` in `folder`, for at most `limit`.
fn run(command: &Command, folder: &std::path::Path, limit: Duration) -> Result<CommandOutput> {
    let mut child = std::process::Command::new(&command.program)
        .args(&command.args)
        .env_clear()
        .current_dir(folder)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("starting {}", command.program))?;

    // Each on its own thread, so a program that fills a pipe while another is unread
    // can't stall the wait. The input's thread ends when the program stops reading.
    let mut stdin = child.stdin.take().context("no stdin")?;
    let input = command.stdin.clone().into_bytes();
    std::thread::spawn(move || drop(stdin.write_all(&input)));
    let stdout = collect(child.stdout.take().context("no stdout")?);
    let stderr = collect(child.stderr.take().context("no stderr")?);

    let Some(status) = child.wait_timeout(limit).context("waiting for the program")? else {
        child.kill().ok();
        child.wait().ok();
        bail!("{} ran longer than {} seconds", command.program, limit.as_secs());
    };
    let text = |output: std::thread::JoinHandle<Vec<u8>>| String::from_utf8_lossy(&output.join().unwrap_or_default()).into_owned();
    Ok(CommandOutput { code: status.code(), stdout: text(stdout), stderr: text(stderr) })
}

/// Read a stream to its end, keeping [`MAX_OUTPUT`] bytes.
fn collect(stream: impl Read + Send + 'static) -> std::thread::JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut kept = Vec::new();
        stream.take(MAX_OUTPUT).read_to_end(&mut kept).ok();
        kept
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn command(program: &str, args: &[&str], stdin: &str) -> Command {
        Command { program: program.into(), args: args.iter().map(|arg| arg.to_string()).collect(), stdin: stdin.into() }
    }

    fn in_tmp(command: &Command, limit: Duration) -> Result<CommandOutput> {
        run(command, &std::env::temp_dir(), limit)
    }

    #[test]
    fn a_program_runs_with_its_arguments_as_text() {
        // No shell: the pipe, the semicolon and the substitution are echoed, not run.
        let output = in_tmp(&command("/bin/echo", &["hi", "|", "cat", ";", "$(id)", "`id`"], ""), TIMEOUT).unwrap();
        assert!(output.success());
        assert_eq!(output.stdout, "hi | cat ; $(id) `id`\n");
        assert_eq!(output.stderr, "");
    }

    #[test]
    fn stdin_is_written_then_closed() {
        let output = in_tmp(&command("/bin/cat", &[], "from stdin"), TIMEOUT).unwrap();
        assert_eq!(output.stdout, "from stdin");
    }

    #[test]
    fn a_failing_status_is_a_result_not_an_error() {
        let output = in_tmp(&command("/bin/ls", &["/no/such/folder"], ""), TIMEOUT).unwrap();
        assert!(!output.success());
        assert!(output.code.is_some_and(|code| code != 0));
        assert!(output.stderr.contains("No such file"));
    }

    #[test]
    fn it_runs_in_the_folder_with_an_empty_environment() {
        let folder = std::env::temp_dir().canonicalize().unwrap();
        let output = run(&command("/bin/pwd", &[], ""), &folder, TIMEOUT).unwrap();
        assert_eq!(output.stdout.trim(), folder.to_str().unwrap());
        let output = in_tmp(&command("/usr/bin/env", &[], ""), TIMEOUT).unwrap();
        assert!(!output.stdout.contains("HOME="), "{:?}", output.stdout);
    }

    #[test]
    fn a_program_that_overruns_is_killed() {
        let error = in_tmp(&command("/bin/sleep", &["30"], ""), Duration::from_millis(200)).unwrap_err();
        assert!(error.to_string().contains("ran longer"), "{error:#}");
    }

    #[test]
    fn a_program_that_does_not_start_is_an_error() {
        assert!(in_tmp(&command("/bin/no-such-program", &[], ""), TIMEOUT).is_err());
    }

    #[test]
    fn only_listed_programs_in_the_standard_folders() {
        let commands = Commands::new(vec!["/bin/ps".into(), "/opt/tool".into()], std::env::temp_dir());
        assert!(commands.allowed("/bin/ps").is_ok());
        assert!(commands.allowed("/bin/ls").is_err(), "not listed");
        assert!(commands.allowed("ps").is_err(), "not a path");
        assert!(commands.allowed("/bin/../bin/ps").is_err(), "not the listed path");
        assert!(commands.allowed("/opt/tool").is_err(), "listed but not in a standard folder");
    }
}
