//! `cargo xtask sign-macos [Delight.app]`: sign the bundle with the team's Developer ID
//! Application certificate, with the Hardened Runtime (which notarisation requires).

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, ensure};

use crate::config::{APP_NAME, TEAM_ID};
use crate::shell::{capture, run, tool};

pub fn run_task(root: &Path, app: Option<PathBuf>) -> Result<PathBuf> {
    let app = app.unwrap_or_else(|| root.join(format!("dist/{APP_NAME}.app")));
    ensure!(app.is_dir(), "{} isn't there: run `cargo xtask bundle-macos` first", app.display());
    let identity = identity()?;
    let entitlements = root.join("packaging/macos/Delight.entitlements");

    // From the inside out: a signature covers what is inside it, so what is inside is signed first.
    sparkle(&app.join("Contents/Frameworks/Sparkle.framework"), &identity)?;
    codesign(&app, &identity, Some(&entitlements), false)?;
    verify(&app)?;
    println!("Signed {} as {identity}", app.display());
    Ok(app)
}

/// Sparkle comes signed by its authors: sign its parts again as our team's, in the order its
/// documentation gives. Its downloader keeps the entitlements it shipped with.
fn sparkle(framework: &Path, identity: &str) -> Result<()> {
    ensure!(framework.is_dir(), "{} is missing", framework.display());
    let version = framework.join("Versions/B");
    codesign(&version.join("XPCServices/Installer.xpc"), identity, None, false)?;
    codesign(&version.join("XPCServices/Downloader.xpc"), identity, None, true)?;
    codesign(&version.join("Autoupdate"), identity, None, false)?;
    codesign(&version.join("Updater.app"), identity, None, false)?;
    codesign(framework, identity, None, false)
}

/// Sign `path`; with `keep`, the entitlements it already has stay.
pub fn codesign(path: &Path, identity: &str, entitlements: Option<&Path>, keep: bool) -> Result<()> {
    let mut command = tool("codesign", ["--force", "--options", "runtime", "--timestamp"]);
    if keep {
        command.arg("--preserve-metadata=entitlements");
    }
    if let Some(entitlements) = entitlements {
        command.arg("--entitlements").arg(entitlements);
    }
    command.arg("--sign").arg(identity).arg(path);
    run(command)
}

pub fn verify(app: &Path) -> Result<()> {
    run(tool("codesign", [OsString::from("--verify"), "--deep".into(), "--strict".into(), "--verbose=2".into(), app.into()]))
}

/// The Developer ID Application certificate of the team: `DEVELOPER_ID_APPLICATION` if it
/// names one, else the one in the keychain. Another team's is refused.
pub fn identity() -> Result<String> {
    let team = std::env::var("DEVELOPER_TEAM_ID").unwrap_or_else(|_| TEAM_ID.into());
    let identity = match std::env::var("DEVELOPER_ID_APPLICATION") {
        Ok(identity) => identity,
        Err(_) => {
            let listing = capture(tool("security", ["find-identity", "-v", "-p", "codesigning"]))?;
            select(&listing, &team).with_context(|| {
                format!("no \"Developer ID Application\" certificate for team {team} is in the keychain (`security find-identity -v -p codesigning`)")
            })?
        }
    };
    ensure!(identity.contains(&format!("({team})")), "{identity} isn't team {team}'s");
    Ok(identity)
}

/// The Developer ID Application identity of `team` in the output of `security find-identity`.
fn select(listing: &str, team: &str) -> Option<String> {
    listing.lines().find_map(|line| {
        let identity = line.split('"').nth(1)?;
        (identity.starts_with("Developer ID Application:") && identity.ends_with(&format!("({team})"))).then(|| identity.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const LISTING: &str = r#"  1) AAAA "Apple Development: someone (ABCDE12345)"
  2) BBBB "Developer ID Application: Other Person (OTHERTEAM1)"
  3) CCCC "Developer ID Application: Brijmohan Siyag (S3L4RJ57GY)"
     3 valid identities found"#;

    #[test]
    fn only_the_teams_developer_id_application_certificate_is_chosen() {
        assert_eq!(select(LISTING, "S3L4RJ57GY").as_deref(), Some("Developer ID Application: Brijmohan Siyag (S3L4RJ57GY)"));
        assert_eq!(select(LISTING, "NOSUCHTEAM").as_deref(), None);
        assert_eq!(select(r#"1) X "Apple Development: a (S3L4RJ57GY)""#, "S3L4RJ57GY"), None, "not a Developer ID one");
    }
}
