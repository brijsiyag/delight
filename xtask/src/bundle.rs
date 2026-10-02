//! `cargo xtask bundle-macos [--native]`: build Delight and assemble `dist/Delight.app`,
//! unsigned.
//!
//! * the app, built for both Mac architectures and joined (`--native`: for this Mac's only,
//!   to try a bundle without waiting for two builds);
//! * the built-in plugins, built once for WebAssembly, into `Contents/Resources/plugins`,
//!   where the app looks for them;
//! * Sparkle, the updater, into `Contents/Frameworks`;
//! * `Info.plist` from `packaging/macos/Info.plist` with this release's version and build;
//! * the icon.

use std::ffi::OsString;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail, ensure};

use crate::config::{APP_NAME, PLUGINS_TARGET, TARGETS};
use crate::shell::{capture, run, tool};
use crate::{icon, sparkle, versions, wasi_sdk};

pub struct Bundle {
    pub app: PathBuf,
}

pub fn run_task(root: &Path, arguments: Vec<String>) -> Result<Bundle> {
    let native = match arguments.as_slice() {
        [] => false,
        [flag] if flag == "--native" => true,
        _ => bail!("bundle-macos takes only --native"),
    };
    let versions = versions::check(root, None)?;
    let dist = root.join("dist");
    std::fs::create_dir_all(&dist)?;
    let sdk = wasi_sdk_path(root)?;

    build_plugins(root, &sdk)?;
    let executable = build_app(root, native)?;

    let app = dist.join(format!("{APP_NAME}.app"));
    if app.exists() {
        std::fs::remove_dir_all(&app)?;
    }
    let contents = app.join("Contents");
    let (macos, frameworks, resources) = (contents.join("MacOS"), contents.join("Frameworks"), contents.join("Resources"));
    for folder in [&macos, &frameworks, &resources.join("plugins")] {
        std::fs::create_dir_all(folder)?;
    }

    // `ditto` keeps the framework's symbolic links and extended attributes.
    let sparkle = sparkle::ensure(&dist)?;
    run(tool("ditto", [sparkle.join("Sparkle.framework"), frameworks.join("Sparkle.framework")]))?;
    copy_executable(&executable, &macos.join(APP_NAME))?;
    copy_plugins(root, &resources.join("plugins"))?;
    write_info_plist(root, &contents.join("Info.plist"), &versions.app)?;
    let work = dist.join("icon-work");
    if work.exists() {
        std::fs::remove_dir_all(&work)?;
    }
    icon::build(&root.join("packaging/macos/AppIcon.svg"), &work, &resources.join(format!("{APP_NAME}.icns")))?;
    std::fs::remove_dir_all(&work).ok();

    run(tool("plutil", [OsString::from("-lint"), contents.join("Info.plist").into_os_string()]))?;
    println!("Built {}", app.display());
    Ok(Bundle { app })
}

/// The WASI SDK, fetched if it isn't there (the built-in JSON and YAML plugins have C in them).
fn wasi_sdk_path(root: &Path) -> Result<PathBuf> {
    wasi_sdk::run(Vec::new(), root)?;
    Ok(root.join("target/wasi-sdk"))
}

/// The built-in plugins, in release, for WebAssembly.
fn build_plugins(root: &Path, sdk: &Path) -> Result<()> {
    let mut build = tool("cargo", ["build", "--release", "--locked", "--target", PLUGINS_TARGET]);
    build.current_dir(root.join("plugins")).env("WASI_SDK_PATH", sdk);
    run(build)
}

/// Build the app, and return the executable: for both architectures joined into one, or for
/// this Mac's only.
fn build_app(root: &Path, native: bool) -> Result<PathBuf> {
    if native {
        let mut build = tool("cargo", ["build", "--release", "--locked", "-p", "delight-app"]);
        build.current_dir(root);
        run(build)?;
        return Ok(root.join("target/release/delight"));
    }
    let installed = capture(tool("rustup", ["target", "list", "--installed"]))?;
    let mut builds = Vec::new();
    for target in TARGETS {
        ensure!(
            installed.lines().any(|line| line == target),
            "the Rust target {target} isn't installed: `rustup target add {target}`"
        );
        let mut build = tool("cargo", ["build", "--release", "--locked", "-p", "delight-app", "--target", target]);
        build.current_dir(root);
        run(build)?;
        builds.push(root.join("target").join(target).join("release/delight"));
    }
    let joined = root.join("target/universal/delight");
    std::fs::create_dir_all(joined.parent().expect("has a parent"))?;
    let mut lipo = tool("lipo", ["-create"]);
    lipo.args(&builds).arg("-output").arg(&joined);
    run(lipo)?;
    Ok(joined)
}

fn copy_executable(from: &Path, to: &Path) -> Result<()> {
    std::fs::copy(from, to).with_context(|| format!("copying {}", from.display()))?;
    std::fs::set_permissions(to, std::fs::Permissions::from_mode(0o755))?;
    Ok(())
}

/// The built-ins' `.wasm` files: one for each member of the plugins workspace, whose crate (and so
/// file) is named as its folder and its plugin id, as Delight loads a plugin from `<id>.wasm`.
fn copy_plugins(root: &Path, into: &Path) -> Result<()> {
    let members = plugin_members(&root.join("plugins/Cargo.toml"))?;
    ensure!(!members.is_empty(), "plugins/Cargo.toml lists no plugins");
    for member in members {
        let file = format!("{member}.wasm");
        let from = root.join("plugins/target").join(PLUGINS_TARGET).join("release").join(&file);
        std::fs::copy(&from, into.join(&file)).with_context(|| format!("copying the built plugin {}", from.display()))?;
    }
    Ok(())
}

/// The members of the plugins workspace (`formats`, `network`, …).
fn plugin_members(manifest: &Path) -> Result<Vec<String>> {
    let table: toml::Table = std::fs::read_to_string(manifest)?.parse()?;
    let members = table
        .get("workspace")
        .and_then(|workspace| workspace.get("members"))
        .and_then(|members| members.as_array())
        .context("plugins/Cargo.toml has no workspace members")?;
    Ok(members.iter().filter_map(|member| member.as_str().map(str::to_owned)).collect())
}

/// `packaging/macos/Info.plist` with this release's version and build number (the number of
/// commits, so each build is newer than the last).
fn write_info_plist(root: &Path, to: &Path, version: &str) -> Result<()> {
    let build = capture(tool("git", ["-C", root.to_str().context("the repository's path isn't UTF-8")?, "rev-list", "--count", "HEAD"]))?;
    let mut info = plist::Value::from_file(root.join("packaging/macos/Info.plist")).context("reading Info.plist")?;
    fill_info(&mut info, version, build.trim())?;
    info.to_file_xml(to).context("writing Info.plist")
}

fn fill_info(info: &mut plist::Value, version: &str, build: &str) -> Result<()> {
    let dictionary = info.as_dictionary_mut().context("Info.plist isn't a dictionary")?;
    dictionary.insert("CFBundleShortVersionString".into(), version.into());
    dictionary.insert("CFBundleVersion".into(), build.into());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn packaged_info() -> plist::Value {
        plist::Value::from_file(Path::new(env!("CARGO_MANIFEST_DIR")).join("../packaging/macos/Info.plist")).unwrap()
    }

    #[test]
    fn the_release_version_and_build_are_written() {
        let mut info = packaged_info();
        fill_info(&mut info, "2.1.0", "417").unwrap();
        let info = info.as_dictionary().unwrap();
        assert_eq!(info["CFBundleShortVersionString"].as_string(), Some("2.1.0"));
        assert_eq!(info["CFBundleVersion"].as_string(), Some("417"));
    }

    #[test]
    fn the_plist_carries_what_the_updater_and_the_launcher_need() {
        let info = packaged_info();
        let info = info.as_dictionary().unwrap();
        assert_eq!(info["CFBundleExecutable"].as_string(), Some(APP_NAME));
        assert_eq!(info["CFBundleIdentifier"].as_string(), Some("dev.delight.app"));
        assert_eq!(info["LSUIElement"].as_boolean(), Some(true), "no Dock icon");
        assert!(info["SUFeedURL"].as_string().is_some_and(|url| url.starts_with(crate::config::REPOSITORY)));
        assert!(info["SUPublicEDKey"].as_string().is_some_and(|key| key.len() > 40), "the update key");
        assert_eq!(info["SUEnableAutomaticChecks"].as_boolean(), Some(true));
        assert!(info["NSLocalNetworkUsageDescription"].as_string().is_some(), "plugins with Network can reach the local network");
    }

    #[test]
    fn the_entitlements_allow_wasmtime_to_run_and_nothing_else() {
        let entitlements = plist::Value::from_file(Path::new(env!("CARGO_MANIFEST_DIR")).join("../packaging/macos/Delight.entitlements")).unwrap();
        let entitlements = entitlements.as_dictionary().unwrap();
        assert_eq!(entitlements.len(), 1);
        assert_eq!(entitlements["com.apple.security.cs.allow-unsigned-executable-memory"].as_boolean(), Some(true));
    }

    #[test]
    fn the_built_in_plugins_are_the_plugin_workspaces_members() {
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR")).join("../plugins/Cargo.toml");
        let mut members = plugin_members(&manifest).unwrap();
        members.sort();
        assert_eq!(members, ["formats", "graphics", "network"]);
    }
}
