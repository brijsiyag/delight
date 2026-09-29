//! What a plugin logs. The plugin API sets up `log` in the plugin so that each line goes to the
//! plugin's stderr as `[plugin LEVEL] text`. The app takes that stream and logs each line with
//! the app's own `log`, at that level and from `plugin::<name>`, so a plugin's lines are in the
//! console and in the log files with the rest, and say whose they are.

use std::io;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};

use tokio::io::AsyncWrite;
use wasmtime_wasi::cli::{IsTerminal, StdoutStream};

/// More than this without a newline is logged as a line of its own.
const MAX_LINE: usize = 64 * 1024;

/// A plugin's stderr, as log lines from the plugin named `name`.
pub(crate) struct PluginStderr {
    name: String,
    lines: Arc<Mutex<Lines>>,
}

impl PluginStderr {
    pub(crate) fn new(name: impl Into<String>) -> Self {
        Self { name: name.into(), lines: Arc::default() }
    }
}

impl IsTerminal for PluginStderr {
    fn is_terminal(&self) -> bool {
        false
    }
}

impl StdoutStream for PluginStderr {
    fn async_stream(&self) -> Box<dyn AsyncWrite + Send + Sync> {
        Box::new(Writer { target: format!("plugin::{}", self.name), lines: Arc::clone(&self.lines) })
    }
}

struct Writer {
    target: String,
    lines: Arc<Mutex<Lines>>,
}

impl AsyncWrite for Writer {
    fn poll_write(self: Pin<&mut Self>, _: &mut Context<'_>, bytes: &[u8]) -> Poll<io::Result<usize>> {
        if let Ok(mut lines) = self.lines.lock() {
            for line in lines.push(bytes) {
                let (level, text) = parse(&line);
                log::log!(target: self.target.as_str(), level, "{text}");
            }
        }
        Poll::Ready(Ok(bytes.len()))
    }

    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

/// Bytes cut into lines: what has not reached its newline waits for the next write.
#[derive(Default)]
struct Lines {
    pending: Vec<u8>,
}

impl Lines {
    /// The whole lines `bytes` completes (without their newlines).
    fn push(&mut self, bytes: &[u8]) -> Vec<String> {
        self.pending.extend_from_slice(bytes);
        let mut lines = Vec::new();
        while let Some(end) = self.pending.iter().position(|&byte| byte == b'\n') {
            let line: Vec<u8> = self.pending.drain(..=end).collect();
            lines.push(String::from_utf8_lossy(&line[..line.len() - 1]).trim_end_matches('\r').to_string());
        }
        if self.pending.len() > MAX_LINE {
            lines.push(String::from_utf8_lossy(&std::mem::take(&mut self.pending)).into_owned());
        }
        lines
    }
}

/// The level and the text of a line: `[plugin WARN] text` is a warning saying `text`. Anything
/// else (a panic's message, a stray print) is a warning too, as it is not something the plugin
/// chose to say.
fn parse(line: &str) -> (log::Level, &str) {
    let Some(rest) = line.strip_prefix("[plugin ") else { return (log::Level::Warn, line) };
    let Some((level, text)) = rest.split_once("] ") else { return (log::Level::Warn, line) };
    match level {
        "ERROR" => (log::Level::Error, text),
        "WARN" => (log::Level::Warn, text),
        "INFO" => (log::Level::Info, text),
        "DEBUG" => (log::Level::Debug, text),
        "TRACE" => (log::Level::Trace, text),
        _ => (log::Level::Warn, line),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_are_cut_at_newlines_across_writes() {
        let mut lines = Lines::default();
        assert!(lines.push(b"[plugin INFO] he").is_empty());
        assert_eq!(lines.push(b"llo\r\n[plugin WARN] second\nthird"), ["[plugin INFO] hello", "[plugin WARN] second"]);
        assert_eq!(lines.push(b" part\n"), ["third part"]);
    }

    #[test]
    fn a_very_long_line_is_not_kept_forever() {
        let mut lines = Lines::default();
        let long = vec![b'x'; MAX_LINE + 1];
        assert_eq!(lines.push(&long).len(), 1);
        assert!(lines.pending.is_empty());
    }

    #[test]
    fn the_plugins_level_is_kept_and_other_text_is_a_warning() {
        assert_eq!(parse("[plugin ERROR] opening a page: no"), (log::Level::Error, "opening a page: no"));
        assert_eq!(parse("[plugin INFO] started"), (log::Level::Info, "started"));
        assert_eq!(parse("thread 'main' panicked at src/x.rs:1"), (log::Level::Warn, "thread 'main' panicked at src/x.rs:1"));
        assert_eq!(parse("[plugin LOUD] x"), (log::Level::Warn, "[plugin LOUD] x"));
    }
}
