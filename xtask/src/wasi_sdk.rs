//! `cargo xtask wasi-sdk`: the pinned WASI SDK (clang and wasi-libc for WebAssembly),
//! unpacked into `target/wasi-sdk`, where `plugins/.cargo/config.toml` points
//! `WASI_SDK_PATH`. Only building plugins with C in them needs it.

use std::fs::File;
use std::io::{Read as _, Write as _};
use std::path::{Component, Path, PathBuf};

use anyhow::{Context as _, Result, bail, ensure};
use flate2::read::GzDecoder;
use sha2::{Digest as _, Sha256};

/// The release to fetch: its tag, and the version its `VERSION` file starts with.
const TAG: &str = "wasi-sdk-34";
const VERSION: &str = "34.0";
/// The SHA-256 of each archive, from the release page. A different archive is refused.
const ARM64_SHA256: &str = "9c59398106b417f8f14913380fdf0097a8cc0ff4af9eb3ce0065a859e88d49e9";
const X86_64_SHA256: &str = "87d27fa8adc68dee59bfbf2e22a6d34ef717c34d6bf1d8af2a56fc929d9ce0eb";

pub fn run(args: Vec<String>, root: &Path) -> Result<()> {
    let mut force = false;
    let mut to = root.join("target/wasi-sdk");
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--force" => force = true,
            "--to" => to = PathBuf::from(args.next().context("--to needs a folder")?),
            other => bail!("unknown option {other:?}"),
        }
    }
    if !force && installed(&to) {
        println!("WASI SDK {VERSION} is already in {}", to.display());
        return Ok(());
    }
    let (arch, sha256) = match std::env::consts::ARCH {
        "aarch64" => ("arm64", ARM64_SHA256),
        "x86_64" => ("x86_64", X86_64_SHA256),
        other => bail!("no WASI SDK is pinned for {other}"),
    };
    let name = format!("wasi-sdk-{VERSION}-{arch}-macos");
    let url = format!("https://github.com/WebAssembly/wasi-sdk/releases/download/{TAG}/{name}.tar.gz");

    let parent = to.parent().context("the folder has a parent")?;
    std::fs::create_dir_all(parent)?;
    let archive = parent.join(format!("{name}.tar.gz.partial"));
    download(&url, &archive, sha256).with_context(|| format!("fetching {url}"))?;

    // Unpack beside the folder and rename, so an interrupted run never leaves half an SDK.
    let partial = parent.join(format!("{name}.partial"));
    if partial.exists() {
        std::fs::remove_dir_all(&partial)?;
    }
    println!("Unpacking into {}", to.display());
    unpack(&archive, &partial)?;
    std::fs::remove_file(&archive).ok();
    if to.exists() {
        std::fs::remove_dir_all(&to)?;
    }
    std::fs::rename(&partial, &to)?;
    ensure!(installed(&to), "the SDK in {} isn't version {VERSION}", to.display());
    println!("WASI SDK {VERSION} is in {}", to.display());
    Ok(())
}

/// Whether the SDK of this version is in `folder`: its `VERSION` file says so.
fn installed(folder: &Path) -> bool {
    std::fs::read_to_string(folder.join("VERSION")).is_ok_and(|text| text.lines().next() == Some(VERSION))
}

/// Download `url` to `file`, showing progress, and check its SHA-256 is `sha256`; the file is
/// removed if it isn't.
fn download(url: &str, file: &Path, sha256: &str) -> Result<()> {
    let tls = ureq::tls::TlsConfig::builder().root_certs(ureq::tls::RootCerts::PlatformVerifier).build();
    let agent: ureq::Agent = ureq::Agent::config_builder().tls_config(tls).build().into();
    let response = agent.get(url).call()?;
    let total: Option<u64> = response.headers().get("content-length").and_then(|value| value.to_str().ok()).and_then(|value| value.parse().ok());
    let mut body = response.into_body().into_reader();
    let mut out = File::create(file)?;
    let (mut hasher, mut buffer, mut done, mut shown) = (Sha256::new(), vec![0; 1 << 20], 0_u64, 0_u64);
    loop {
        let read = body.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        out.write_all(&buffer[..read])?;
        done += read as u64;
        if done - shown >= 20 << 20 {
            shown = done;
            match total {
                Some(total) => print!("\rDownloading {} of {} MB", done >> 20, total >> 20),
                None => print!("\rDownloading {} MB", done >> 20),
            }
            std::io::stdout().flush().ok();
        }
    }
    println!("\rDownloaded {} MB          ", done >> 20);
    let found = hex(&hasher.finalize());
    if found != sha256 {
        std::fs::remove_file(file).ok();
        bail!("the download's SHA-256 is {found}, not the pinned {sha256}: refusing it");
    }
    Ok(())
}

/// Unpack a `.tar.gz` into `folder`, dropping the archive's one top-level folder
/// (`tar --strip-components 1`), and refusing any path that leaves `folder`.
fn unpack(archive: &Path, folder: &Path) -> Result<()> {
    let mut tar = tar::Archive::new(GzDecoder::new(File::open(archive)?));
    tar.set_preserve_permissions(true);
    for entry in tar.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        let Some(inside) = stripped(&path)? else { continue };
        let destination = folder.join(inside);
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)?;
        }
        entry.unpack(&destination).with_context(|| format!("unpacking {}", path.display()))?;
    }
    Ok(())
}

/// `path` without its first component; `None` for the top-level folder itself. An error for a
/// path that isn't plain (absolute, or with `..`).
fn stripped(path: &Path) -> Result<Option<PathBuf>> {
    let mut components = path.components();
    ensure!(
        path.components().all(|component| matches!(component, Component::Normal(_) | Component::CurDir)),
        "{} isn't a plain relative path",
        path.display()
    );
    components.next();
    let rest: PathBuf = components.collect();
    Ok((!rest.as_os_str().is_empty()).then_some(rest))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_top_level_folder_is_dropped() {
        assert_eq!(stripped(Path::new("wasi-sdk-34.0-arm64-macos/")).unwrap(), None);
        assert_eq!(stripped(Path::new("wasi-sdk-34.0-arm64-macos/bin/clang")).unwrap(), Some(PathBuf::from("bin/clang")));
    }

    #[test]
    fn paths_that_leave_the_folder_are_refused() {
        assert!(stripped(Path::new("top/../../etc/passwd")).is_err());
        assert!(stripped(Path::new("/etc/passwd")).is_err());
    }

    #[test]
    fn digests_are_lowercase_hex() {
        assert_eq!(hex(&[0x00, 0xab, 0x0f]), "00ab0f");
        assert_eq!(hex(&Sha256::digest(b"abc")), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }

    #[test]
    fn the_pins_are_sha256_digests() {
        for pin in [ARM64_SHA256, X86_64_SHA256] {
            assert_eq!(pin.len(), 64);
            assert!(pin.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)));
        }
    }

    #[test]
    fn an_sdk_is_the_version_whose_file_says_so() {
        let folder = std::env::temp_dir().join(format!("delight-xtask-sdk-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        assert!(!installed(&folder));
        std::fs::write(folder.join("VERSION"), "34.0\nwasi-libc: x\n").unwrap();
        assert!(installed(&folder));
        std::fs::write(folder.join("VERSION"), "33.0\n").unwrap();
        assert!(!installed(&folder));
        std::fs::remove_dir_all(&folder).unwrap();
    }
}
