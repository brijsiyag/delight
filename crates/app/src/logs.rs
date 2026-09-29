//! The app's log files, and the menu's "Open Logs".
//!
//! Everything the app logs goes to the console and to a file in `~/Library/Logs/Delight` (where
//! Console.app also finds it): one file for each run, `delight-YYYYMMDD-HHMMSS.log`, and a new one
//! when a run's file gets big. Only the last few are kept.
//!
//! Open Logs opens the file in the Mac's text editor when there is only one. With several it
//! zips them into the Downloads folder and shows the zip in Finder.

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{Context as _, Result};
use gpui::{App, AppContext as _};

/// A run's file is closed and a new one started at this size.
const MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;
/// How many files are kept, the current one included.
const KEEP_FILES: usize = 6;

/// Where the log files are.
pub fn dir() -> PathBuf {
    dirs::home_dir().map_or_else(std::env::temp_dir, |home| home.join("Library/Logs")).join("Delight")
}

/// The log files in `dir`, oldest first (their names order by time).
pub fn list(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = fs::read_dir(dir)
        .map(|entries| entries.filter_map(|entry| entry.ok().map(|entry| entry.path())).collect())
        .unwrap_or_default();
    files.retain(|file| file.extension().is_some_and(|extension| extension == "log"));
    files.sort();
    files
}

fn file_name(now: chrono::DateTime<chrono::Local>) -> String {
    format!("delight-{}.log", now.format("%Y%m%d-%H%M%S"))
}

/// Delete the oldest files in `dir` until `keep` are left.
fn prune(dir: &Path, keep: usize) {
    let files = list(dir);
    for old in files.iter().take(files.len().saturating_sub(keep)) {
        if let Err(error) = fs::remove_file(old) {
            log::warn!("removing the old log {}: {error}", old.display());
        }
    }
}

/// The file being written: what is logged goes here, and into a new file once this one is big.
pub struct SessionLog {
    dir: PathBuf,
    max_bytes: u64,
    keep: usize,
    state: Arc<Mutex<State>>,
}

struct State {
    file: File,
    written: u64,
    /// The name of the file, so a new one made in the same second doesn't reuse it.
    name: String,
}

impl SessionLog {
    /// Start a file in [`dir`], and prune the old ones.
    pub fn start() -> Result<Self> {
        Self::in_dir(dir(), MAX_FILE_BYTES, KEEP_FILES)
    }

    fn in_dir(dir: PathBuf, max_bytes: u64, keep: usize) -> Result<Self> {
        fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
        let (file, name) = Self::new_file(&dir, None)?;
        prune(&dir, keep);
        Ok(Self { dir, max_bytes, keep, state: Arc::new(Mutex::new(State { file, written: 0, name })) })
    }

    /// A new file named for now (or, in the same second as `after`, one second later).
    fn new_file(dir: &Path, after: Option<&str>) -> Result<(File, String)> {
        let mut now = chrono::Local::now();
        let mut name = file_name(now);
        while after.is_some_and(|after| name.as_str() <= after) || dir.join(&name).exists() {
            now += chrono::TimeDelta::seconds(1);
            name = file_name(now);
        }
        let file = File::create(dir.join(&name)).with_context(|| format!("creating {name}"))?;
        Ok((file, name))
    }

    /// A writer for the logger; clones share the file.
    pub fn writer(&self) -> impl Write + Send + 'static {
        SessionWriter { log: Arc::clone(&self.state), dir: self.dir.clone(), max_bytes: self.max_bytes, keep: self.keep }
    }
}

struct SessionWriter {
    log: Arc<Mutex<State>>,
    dir: PathBuf,
    max_bytes: u64,
    keep: usize,
}

impl Write for SessionWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let Ok(mut state) = self.log.lock() else { return Ok(buf.len()) };
        if state.written > 0 && state.written + buf.len() as u64 > self.max_bytes
            && let Ok((file, name)) = SessionLog::new_file(&self.dir, Some(&state.name))
        {
            *state = State { file, written: 0, name };
            prune(&self.dir, self.keep);
        }
        state.file.write_all(buf)?;
        state.written += buf.len() as u64;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.log.lock().map_or(Ok(()), |mut state| state.file.flush())
    }
}

/// Zip `files` into a new `Delight-logs-….zip` in `out_dir`; its path.
pub fn zip_files(files: &[PathBuf], out_dir: &Path) -> Result<PathBuf> {
    use zip::write::SimpleFileOptions;
    fs::create_dir_all(out_dir).with_context(|| format!("creating {}", out_dir.display()))?;
    let zip_path = out_dir.join(format!("Delight-logs-{}.zip", chrono::Local::now().format("%Y%m%d-%H%M%S")));
    let mut zip = zip::ZipWriter::new(File::create(&zip_path).with_context(|| format!("creating {}", zip_path.display()))?);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for file in files {
        let name = file.file_name().and_then(|name| name.to_str()).context("a log file without a name")?;
        zip.start_file(name, options)?;
        io::copy(&mut File::open(file).with_context(|| format!("reading {}", file.display()))?, &mut zip)?;
    }
    zip.finish()?;
    Ok(zip_path)
}

/// The menu's Open Logs. One log file opens in the text editor; several are zipped into
/// Downloads and shown in Finder; none opens the folder.
pub fn open(cx: &mut App) {
    let dir = dir();
    let files = list(&dir);
    match files.as_slice() {
        [] => crate::macos::reveal(&dir),
        [only] => crate::macos::open_in_text_editor(only),
        _ => {
            let out_dir = dirs::download_dir().unwrap_or_else(|| dirs::home_dir().unwrap_or_else(std::env::temp_dir));
            let zipped = cx.background_spawn(async move { zip_files(&files, &out_dir) });
            cx.spawn(async move |cx| match zipped.await {
                Ok(zip) => {
                    log::info!("the logs are zipped in {}", zip.display());
                    cx.update(|_| crate::macos::reveal(&zip));
                }
                Err(error) => log::error!("zipping the logs: {error:#}"),
            })
            .detach();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("delight-logs-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_run_writes_one_file_and_old_ones_are_pruned() {
        let dir = temp_dir("prune");
        for day in 1..=8 {
            fs::write(dir.join(format!("delight-202609{day:02}-120000.log")), "old").unwrap();
        }
        fs::write(dir.join("notes.txt"), "not a log").unwrap();
        let log = SessionLog::in_dir(dir.clone(), 1024, 4).unwrap();
        writeln!(log.writer(), "hello").unwrap();
        let files = list(&dir);
        assert_eq!(files.len(), 4, "{files:?}");
        assert!(files.last().unwrap().file_name().unwrap().to_str().unwrap().starts_with("delight-2026"), "the new one is last");
        assert_eq!(fs::read_to_string(files.last().unwrap()).unwrap(), "hello\n");
        assert!(dir.join("notes.txt").exists(), "only log files are pruned");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_big_file_is_closed_and_a_new_one_started() {
        let dir = temp_dir("rotate");
        let log = SessionLog::in_dir(dir.clone(), 20, 6).unwrap();
        let mut writer = log.writer();
        for _ in 0..3 {
            writer.write_all(b"0123456789012345\n").unwrap();
        }
        let files = list(&dir);
        assert_eq!(files.len(), 3, "{files:?}");
        for file in &files {
            assert_eq!(fs::read_to_string(file).unwrap(), "0123456789012345\n", "{file:?}");
        }
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn logs_are_zipped_with_their_own_names() {
        let dir = temp_dir("zip");
        let (a, b) = (dir.join("delight-20260929-100000.log"), dir.join("delight-20260930-100000.log"));
        fs::write(&a, "first run\n").unwrap();
        fs::write(&b, "second run\n").unwrap();
        let out = dir.join("out");
        let zip_path = zip_files(&[a, b], &out).unwrap();
        assert!(zip_path.file_name().unwrap().to_str().unwrap().starts_with("Delight-logs-"));
        let mut archive = zip::ZipArchive::new(File::open(&zip_path).unwrap()).unwrap();
        assert_eq!(archive.len(), 2);
        let mut text = String::new();
        io::Read::read_to_string(&mut archive.by_name("delight-20260930-100000.log").unwrap(), &mut text).unwrap();
        assert_eq!(text, "second run\n");
        fs::remove_dir_all(dir).unwrap();
    }
}
