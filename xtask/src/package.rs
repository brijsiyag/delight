//! `cargo xtask package-macos [Delight.app]`: from the signed bundle to the one thing that is
//! published, `dist/Delight-X.Y.Z.dmg`, and the update feed `dist/appcast.xml`.
//!
//! The disk image serves first installs and updates alike: Sparkle updates from it. The app
//! is notarised before it goes in, so it also passes Gatekeeper once dragged out; the disk
//! image is signed, notarised and stapled too.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, ensure};

use crate::config::{APP_NAME, NOTARY_PROFILE, REPOSITORY, SPARKLE_ACCOUNT};
use crate::shell::{capture, run, tool};
use crate::{sign, sparkle};

pub struct Package {
    pub dmg: PathBuf,
    pub appcast: PathBuf,
    pub version: String,
}

pub fn run_task(root: &Path, app: Option<PathBuf>) -> Result<Package> {
    let dist = root.join("dist");
    let app = app.unwrap_or_else(|| dist.join(format!("{APP_NAME}.app")));
    ensure!(app.is_dir(), "{} isn't there: run `cargo xtask bundle-macos` and `sign-macos` first", app.display());
    sign::verify(&app).context("the app isn't signed: run `cargo xtask sign-macos`")?;
    let identity = sign::identity()?;
    let profile = std::env::var("NOTARY_PROFILE").unwrap_or_else(|_| NOTARY_PROFILE.into());
    let version = bundle_version(&app)?;

    // Apple looks at the app first, and the ticket is stapled to it, so it opens offline.
    let for_apple = dist.join("notarize-app.zip");
    std::fs::remove_file(&for_apple).ok();
    run(tool("ditto", [OsString::from("-c"), "-k".into(), "--keepParent".into(), app.clone().into(), for_apple.clone().into()]))?;
    notarize(&for_apple, &profile)?;
    std::fs::remove_file(&for_apple).ok();
    staple(&app)?;

    let dmg = dist.join(dmg_name(&version));
    make_dmg(&app, &dist, &dmg)?;
    sign::codesign(&dmg, &identity, None, false)?;
    notarize(&dmg, &profile)?;
    staple(&dmg)?;
    run(tool("spctl", [OsString::from("--assess"), "--type".into(), "open".into(), "--context".into(), "context:primary-signature".into(), "--verbose=2".into(), dmg.clone().into()]))?;

    let appcast = appcast(&dist, &dmg, &version)?;
    println!("Disk image: {}", dmg.display());
    println!("Update feed: {}", appcast.display());
    Ok(Package { dmg, appcast, version })
}

/// The disk image's name.
pub fn dmg_name(version: &str) -> String {
    format!("{APP_NAME}-{version}.dmg")
}

/// Where an update for `version` is downloaded from: the release of its tag.
pub fn download_prefix(version: &str) -> String {
    format!("{REPOSITORY}/releases/download/v{version}/")
}

fn bundle_version(app: &Path) -> Result<String> {
    let info = plist::Value::from_file(app.join("Contents/Info.plist")).context("reading the app's Info.plist")?;
    info.as_dictionary()
        .and_then(|info| info.get("CFBundleShortVersionString"))
        .and_then(|version| version.as_string())
        .map(str::to_owned)
        .context("the app's Info.plist has no CFBundleShortVersionString")
}

/// A disk image with the app and a shortcut to Applications to drag it to.
fn make_dmg(app: &Path, dist: &Path, dmg: &Path) -> Result<()> {
    let stage = dist.join("dmg-stage");
    if stage.exists() {
        std::fs::remove_dir_all(&stage)?;
    }
    std::fs::create_dir_all(&stage)?;
    run(tool("ditto", [app.as_os_str(), stage.join(format!("{APP_NAME}.app")).as_os_str()]))?;
    std::os::unix::fs::symlink("/Applications", stage.join("Applications"))?;
    std::fs::remove_file(dmg).ok();
    run(tool(
        "hdiutil",
        [
            OsString::from("create"),
            "-volname".into(),
            APP_NAME.into(),
            "-srcfolder".into(),
            stage.clone().into(),
            "-format".into(),
            "UDZO".into(),
            "-ov".into(),
            dmg.into(),
        ],
    ))?;
    std::fs::remove_dir_all(&stage).ok();
    Ok(())
}

/// Send `file` to Apple to notarise and wait for the answer; anything but "Accepted" is an error.
fn notarize(file: &Path, profile: &str) -> Result<()> {
    let output = capture(tool(
        "xcrun",
        [OsString::from("notarytool"), "submit".into(), file.into(), "--keychain-profile".into(), profile.into(), "--wait".into()],
    ))?;
    println!("{output}");
    ensure!(accepted(&output), "Apple didn't accept {}: `xcrun notarytool log <id> --keychain-profile {profile}` says why", file.display());
    Ok(())
}

fn accepted(output: &str) -> bool {
    output.lines().any(|line| line.trim() == "status: Accepted")
}

fn staple(path: &Path) -> Result<()> {
    run(tool("xcrun", [OsString::from("stapler"), "staple".into(), path.into()]))?;
    run(tool("xcrun", [OsString::from("stapler"), "validate".into(), path.into()]))
}

/// The update feed for the disk image, signed with the update key in the login keychain.
fn appcast(dist: &Path, dmg: &Path, version: &str) -> Result<PathBuf> {
    let sparkle = sparkle::ensure(dist)?;
    let work = dist.join("appcast-work");
    if work.exists() {
        std::fs::remove_dir_all(&work)?;
    }
    std::fs::create_dir_all(&work)?;
    std::fs::copy(dmg, work.join(dmg.file_name().context("the disk image has no name")?))?;
    run(tool(
        sparkle.join("bin/generate_appcast"),
        [
            OsString::from("--account"),
            SPARKLE_ACCOUNT.into(),
            "--download-url-prefix".into(),
            download_prefix(version).into(),
            "--link".into(),
            REPOSITORY.into(),
            work.clone().into(),
        ],
    ))?;
    let generated = work.join("appcast.xml");
    ensure!(generated.is_file(), "Sparkle wrote no appcast.xml");
    let feed = dist.join("appcast.xml");
    std::fs::copy(&generated, &feed)?;
    std::fs::remove_dir_all(&work).ok();
    Ok(feed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_disk_image_is_named_for_the_version() {
        assert_eq!(dmg_name("2.1.0"), "Delight-2.1.0.dmg");
    }

    #[test]
    fn updates_are_downloaded_from_the_releases_tag() {
        assert_eq!(download_prefix("2.1.0"), "https://github.com/brijsiyag/delight/releases/download/v2.1.0/");
    }

    #[test]
    fn only_an_accepted_submission_counts() {
        assert!(accepted("Successfully uploaded file\n  id: abc\n  status: Accepted\n"));
        assert!(!accepted("  status: Invalid"));
        assert!(!accepted("  status: In Progress"));
    }
}
