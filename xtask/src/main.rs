//! Delight's development and release tasks, run as `cargo xtask <task>` (the alias is in
//! `.cargo/config.toml`). `README.md` ("Releasing") says how a release is made.
//!
//! * `wasi-sdk`: fetch the pinned WASI SDK into `target/wasi-sdk`.
//! * `sparkle`: fetch the pinned Sparkle into `dist/`.
//! * `check-versions [tag]`: the versions that must agree.
//! * `bundle-macos [--native]`, `sign-macos`, `package-macos`: the stages of a release.
//! * `release-macos [tag]`: all of them, ending at a disk image and an update feed.

mod bundle;
mod config;
mod download;
mod icon;
mod package;
mod release;
mod shell;
mod sign;
mod sparkle;
mod versions;
mod wasi_sdk;

use std::path::PathBuf;

use anyhow::{Result, bail, ensure};

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let root = root();
    let task = args.next();
    let rest: Vec<String> = args.collect();
    match task.as_deref() {
        Some("wasi-sdk") => wasi_sdk::run(rest, &root),
        Some("sparkle") => sparkle::ensure(&root.join("dist")).map(|folder| println!("Sparkle is in {}", folder.display())),
        Some("check-versions") => versions::check(&root, one_or_none(&rest, "a tag")?).map(drop),
        Some("bundle-macos") => on_a_mac()?.then(|| bundle::run_task(&root, rest)).transpose().map(drop),
        Some("sign-macos") => on_a_mac()?.then(|| sign::run_task(&root, one_or_none(&rest, "an app")?.map(PathBuf::from))).transpose().map(drop),
        Some("package-macos") => on_a_mac()?.then(|| package::run_task(&root, one_or_none(&rest, "an app")?.map(PathBuf::from))).transpose().map(drop),
        Some("release-macos") => on_a_mac()?.then(|| release::run_task(&root, one_or_none(&rest, "a tag")?.map(str::to_owned))).transpose().map(drop),
        Some("help" | "--help" | "-h") | None => {
            println!("{USAGE}");
            Ok(())
        }
        Some(other) => bail!("there is no task {other:?}\n\n{USAGE}"),
    }
}

const USAGE: &str = "cargo xtask <task>

Tasks:
  wasi-sdk [--force] [--to <folder>]
      Fetch the pinned WASI SDK into target/wasi-sdk (or <folder>), checking its SHA-256.
  sparkle
      Fetch the pinned Sparkle into dist/Sparkle-<version> (its tools: generate_keys, generate_appcast).
  check-versions [tag]
      Check the app, the built-in plugins and the tag agree. Builds nothing.
  bundle-macos [--native]
      Build Delight and assemble dist/Delight.app, unsigned. --native builds for this Mac only.
  sign-macos [Delight.app]
      Sign the bundle with the team's Developer ID Application certificate.
  package-macos [Delight.app]
      Notarise the app, make dist/Delight-X.Y.Z.dmg (signed, notarised) and dist/appcast.xml.
  release-macos [tag]
      check-versions, bundle-macos, sign-macos and package-macos in a row.";

/// The repository's root: this crate's folder is directly under it.
fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().expect("xtask is in the repository").to_path_buf()
}

/// The release tasks use Apple's tools: they run on a Mac.
fn on_a_mac() -> Result<bool> {
    ensure!(cfg!(target_os = "macos"), "the release tasks run on a Mac");
    Ok(true)
}

fn one_or_none<'a>(arguments: &'a [String], what: &str) -> Result<Option<&'a str>> {
    match arguments {
        [] => Ok(None),
        [one] => Ok(Some(one)),
        _ => bail!("this task takes at most {what}"),
    }
}
