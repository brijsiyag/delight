//! Open at login: a per-user LaunchAgent (`~/Library/LaunchAgents`) that starts this
//! Delight when the user logs in. It works whatever the app's signature (unlike
//! `SMAppService`), and launchd reads it only at login, so writing it doesn't start a
//! second Delight now.

use std::path::PathBuf;

use anyhow::Context as _;
use serde::{Deserialize, Serialize};

const LABEL: &str = "dev.delight.app";

/// The LaunchAgent file's contents (`man launchd.plist`).
#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct LaunchAgent {
    label: String,
    program_arguments: Vec<String>,
    run_at_load: bool,
    process_type: String,
    limit_load_to_session_type: String,
}

impl LaunchAgent {
    fn starting(exe: &str) -> Self {
        Self {
            label: LABEL.into(),
            program_arguments: vec![exe.into()],
            run_at_load: true,
            process_type: "Interactive".into(),
            limit_load_to_session_type: "Aqua".into(),
        }
    }
}

/// Write or remove the LaunchAgent. Done at every launch too, so the agent follows
/// the app when it's moved.
pub fn apply(enabled: bool) {
    let result = if enabled { install() } else { remove() };
    if let Err(error) = result {
        log::error!("updating the login item: {error:#}");
    }
}

fn plist_path() -> anyhow::Result<PathBuf> {
    let home = dirs::home_dir().context("no home folder")?;
    Ok(home.join("Library/LaunchAgents").join(format!("{LABEL}.plist")))
}

fn install() -> anyhow::Result<()> {
    let path = plist_path()?;
    let exe = std::env::current_exe()?;
    let mut xml = Vec::new();
    plist::to_writer_xml(&mut xml, &LaunchAgent::starting(&exe.to_string_lossy()))?;
    if std::fs::read(&path).is_ok_and(|current| current == xml) {
        return Ok(());
    }
    std::fs::create_dir_all(path.parent().context("no folder for the login item")?)?;
    std::fs::write(&path, xml)?;
    Ok(())
}

fn remove() -> anyhow::Result<()> {
    match std::fs::remove_file(plist_path()?) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        result => Ok(result?),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn odd_paths_round_trip() {
        let agent = LaunchAgent::starting("/Applications/R&D <tools>/Delight.app/Contents/MacOS/Delight");
        let mut xml = Vec::new();
        plist::to_writer_xml(&mut xml, &agent).unwrap();
        assert_eq!(plist::from_bytes::<LaunchAgent>(&xml).unwrap(), agent);
    }
}
