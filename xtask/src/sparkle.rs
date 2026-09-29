//! The pinned Sparkle release: its framework goes into the app, and its `generate_appcast`
//! writes the update feed. Fetched once into `dist/`.

use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, ensure};

use crate::config::{SPARKLE_SHA256, SPARKLE_VERSION};

/// The folder Sparkle is unpacked in, fetching it first if it isn't there.
pub fn ensure(dist: &Path) -> Result<PathBuf> {
    let folder = dist.join(format!("Sparkle-{SPARKLE_VERSION}"));
    if folder.join("Sparkle.framework").is_dir() && folder.join("bin/generate_appcast").is_file() {
        return Ok(folder);
    }
    std::fs::create_dir_all(dist)?;
    let archive = dist.join(format!("Sparkle-{SPARKLE_VERSION}.tar.xz"));
    let url = format!("https://github.com/sparkle-project/Sparkle/releases/download/{SPARKLE_VERSION}/Sparkle-{SPARKLE_VERSION}.tar.xz");
    crate::download::fetch(&url, &archive, SPARKLE_SHA256).with_context(|| format!("fetching {url}"))?;

    // The archive is xz; unpack it beside the folder and rename, so an interrupted run
    // leaves nothing half done.
    let mut tar = Vec::new();
    lzma_rs::xz_decompress(&mut BufReader::new(File::open(&archive)?), &mut tar).context("decompressing Sparkle")?;
    let partial = dist.join(format!("Sparkle-{SPARKLE_VERSION}.partial"));
    if partial.exists() {
        std::fs::remove_dir_all(&partial)?;
    }
    std::fs::create_dir_all(&partial)?;
    let mut archive_tar = tar::Archive::new(tar.as_slice());
    archive_tar.set_preserve_permissions(true);
    archive_tar.unpack(&partial).context("unpacking Sparkle")?;
    ensure!(partial.join("Sparkle.framework").is_dir(), "Sparkle.framework is missing from the archive");
    ensure!(partial.join("bin/generate_appcast").is_file(), "generate_appcast is missing from the archive");
    if folder.exists() {
        std::fs::remove_dir_all(&folder)?;
    }
    std::fs::rename(&partial, &folder)?;
    std::fs::remove_file(&archive).ok();
    Ok(folder)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pin_is_a_sha256_digest_of_a_release() {
        assert_eq!(SPARKLE_SHA256.len(), 64);
        assert!(SPARKLE_SHA256.bytes().all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()));
        assert!(SPARKLE_VERSION.split('.').count() == 3);
    }
}
