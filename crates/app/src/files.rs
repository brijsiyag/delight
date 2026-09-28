//! Delight's own files: JSON, written so a crash mid-write never leaves half a file.

use std::path::Path;

use serde::Serialize;
use serde::de::DeserializeOwned;

/// Write `value` as pretty JSON through a temporary file and a rename, so a crash
/// mid-write never leaves half a file.
pub fn write_json(path: &Path, value: &impl Serialize) -> anyhow::Result<()> {
    if let Some(folder) = path.parent() {
        std::fs::create_dir_all(folder)?;
    }
    let temporary = path.with_extension("json.tmp");
    std::fs::write(&temporary, serde_json::to_vec_pretty(value)?)?;
    std::fs::rename(temporary, path)?;
    Ok(())
}

/// Read JSON from `path`: `None` if the file is missing, or invalid (logged).
pub fn read_json<T: DeserializeOwned>(path: &Path) -> Option<T> {
    let raw = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&raw)
        .map_err(|error| log::warn!("{} is invalid, so it's ignored: {error}", path.display()))
        .ok()
}
