//! `cargo xtask check-versions [tag]`: the versions that must agree, and don't build anything.
//!
//! * the app's, in the root `Cargo.toml` (`workspace.package.version`): also the built-in
//!   plugins', in `plugins/Cargo.toml`, and this task's own (it is compiled with the app's);
//! * the GitHub tag, `v` and that version;
//! * the plugin API's own, in `crates/manifest/Cargo.toml`: the protocol version, which is
//!   not the app's and is only reported.

use std::path::Path;

use anyhow::{Context as _, Result, ensure};

pub struct Versions {
    pub app: String,
    pub plugin_api: String,
    pub tag: String,
}

/// Read the versions and check they agree, and that `requested_tag` (if given) is this
/// release's.
pub fn check(root: &Path, requested_tag: Option<&str>) -> Result<Versions> {
    let app = version(&root.join("Cargo.toml"), &["workspace", "package", "version"])?;
    let plugins = version(&root.join("plugins/Cargo.toml"), &["workspace", "package", "version"])?;
    let plugin_api = version(&root.join("crates/manifest/Cargo.toml"), &["package", "version"])?;
    let versions = agree(&app, &plugins, &plugin_api, requested_tag)?;
    ensure!(
        versions.app == env!("CARGO_PKG_VERSION"),
        "xtask was built as {}, but the app is {}: run it through `cargo xtask`",
        env!("CARGO_PKG_VERSION"),
        versions.app
    );
    println!("Versions: app and built-in plugins {}, plugin API {}, tag {}", versions.app, versions.plugin_api, versions.tag);
    Ok(versions)
}

fn agree(app: &str, plugins: &str, plugin_api: &str, requested_tag: Option<&str>) -> Result<Versions> {
    ensure!(plugins == app, "the built-in plugins are {plugins} but the app is {app}: set plugins/Cargo.toml to {app}");
    let tag = format!("v{app}");
    if let Some(requested) = requested_tag {
        ensure!(requested == tag, "the tag {requested:?} isn't this release's: the app is {app}, so the tag is {tag:?}");
    }
    Ok(Versions { app: app.into(), plugin_api: plugin_api.into(), tag })
}

/// The string at `keys` in a `Cargo.toml`.
fn version(manifest: &Path, keys: &[&str]) -> Result<String> {
    let text = std::fs::read_to_string(manifest).with_context(|| format!("reading {}", manifest.display()))?;
    let table: toml::Table = text.parse().with_context(|| format!("parsing {}", manifest.display()))?;
    let mut value = toml::Value::Table(table);
    for key in keys {
        value = value
            .get(key)
            .with_context(|| format!("{} has no `{}`", manifest.display(), keys.join(".")))?
            .clone();
    }
    value.as_str().map(str::to_owned).with_context(|| format!("`{}` in {} isn't a string", keys.join("."), manifest.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matching_versions_give_the_tag() {
        let versions = agree("2.1.0", "2.1.0", "2.0.0", Some("v2.1.0")).unwrap();
        assert_eq!((versions.app.as_str(), versions.plugin_api.as_str(), versions.tag.as_str()), ("2.1.0", "2.0.0", "v2.1.0"));
    }

    #[test]
    fn the_plugin_api_may_differ_from_the_app() {
        assert!(agree("2.1.0", "2.1.0", "2.0.0", None).is_ok());
    }

    #[test]
    fn plugins_of_another_version_are_refused() {
        let error = agree("2.1.0", "2.0.0", "2.0.0", None).err().expect("refused");
        assert!(error.to_string().contains("set plugins/Cargo.toml to 2.1.0"), "{error}");
    }

    #[test]
    fn another_tag_is_refused() {
        let error = agree("2.1.0", "2.1.0", "2.0.0", Some("v2.0.9")).err().expect("refused");
        assert!(error.to_string().contains("\"v2.1.0\""), "{error}");
    }

    #[test]
    fn this_repositorys_versions_agree() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let versions = check(root, None).unwrap();
        assert_eq!(versions.tag, format!("v{}", env!("CARGO_PKG_VERSION")));
    }
}
