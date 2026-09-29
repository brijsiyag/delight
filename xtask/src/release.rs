//! `cargo xtask release-macos [tag]`: check the versions, bundle, sign and package one release,
//! ending at `dist/Delight-X.Y.Z.dmg` and `dist/appcast.xml`, ready to publish.

use std::path::Path;

use anyhow::Result;

use crate::{bundle, package, sign, versions};

pub fn run_task(root: &Path, tag: Option<String>) -> Result<()> {
    // Before anything is built: a wrong tag or mismatched versions is found in a second.
    let versions = versions::check(root, tag.as_deref())?;
    let bundled = bundle::run_task(root, Vec::new())?;
    let app = sign::run_task(root, Some(bundled.app))?;
    let package = package::run_task(root, Some(app))?;
    println!(
        "\nDelight {} is built. Publish it with:\n\n  gh release create {} {} {} --generate-notes\n",
        package.version,
        versions.tag,
        package.dmg.display(),
        package.appcast.display()
    );
    Ok(())
}
