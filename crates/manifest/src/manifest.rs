//! The manifest: what a plugin is (its properties and operations), and the rules it
//! must follow.

use std::collections::HashSet;

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

use crate::permission::{PermissionData, PermissionRequest};

/// What a plugin is: its properties and the tools it offers.
#[derive(Clone, Debug, PartialEq)]
pub struct Manifest {
    pub plugin: PluginProperties,
    /// At least one.
    pub operations: Vec<Operation>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PluginProperties {
    /// Names the plugin's files and folders, so it is limited to `[A-Za-z0-9._-]`,
    /// 1 to 128 characters, not starting with `.`. Reverse-DNS by convention.
    pub id: String,
    pub name: String,
    /// The plugin crate's version.
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: String,
    /// A full-colour, square SVG with its own background, shown in both appearances.
    /// Where the plugin is shown (Settings, installing), and for its operations
    /// that have no icon of their own.
    pub icon: String,
    #[serde(default)]
    pub tags: Vec<String>,
    /// What it may do outside its sandbox, each with why it needs it.
    #[serde(default)]
    pub permissions: Vec<PermissionRequest>,
    /// Up to [`MAX_TIPS`] short tips, written by its author, on what to type and what
    /// it gives, such as "cal <email> shows someone's meetings": not keys or actions,
    /// which the footer shows. The launcher's empty input shows them now and then,
    /// while the plugin has a tool on. As few as are worth reading.
    #[serde(default)]
    pub tips: Vec<String>,
    /// Where the plugin is published: the URL (`http` or `https`) of a location holding its
    /// `<id>.wasm` and `<id>.xml`, a manifest with its version (see `Release`, with the `files`
    /// feature), such as `https://github.com/acme/plugins/releases/latest/download`. The app looks
    /// there once a day and offers a newer version. Without one, the plugin is updated only by
    /// installing a newer file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub update: Option<String>,
}

/// Most tips a plugin has.
pub const MAX_TIPS: usize = 5;
/// The longest a tip is, in characters, so it fits the launcher's input on one line:
/// the input is 562pt wide when a tip shows, which is 72 characters of 13pt Lilex, and
/// symbols such as ⌘ and ↵ come from wider fonts.
pub const MAX_TIP_CHARS: usize = 60;
/// The longest a permission's reason is, in characters: a sentence.
pub const MAX_REASON_CHARS: usize = 100;
/// The longest a location's URL is.
pub const MAX_URL_CHARS: usize = 2048;
/// One tool a plugin offers.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Operation {
    /// Unique within the plugin, and stored by the app (history, the picked tool), so
    /// it shouldn't change.
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    /// Its own icon, an SVG like [`PluginProperties::icon`], so a plugin's tools can
    /// look different in the tool list. Without one it shows the plugin's.
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

impl Manifest {
    /// The icon to show for `operation`: its own, or else the plugin's.
    pub fn icon_for<'a>(&'a self, operation: &'a Operation) -> &'a str {
        operation.icon.as_deref().unwrap_or(&self.plugin.icon)
    }

    /// Check what the types can't, for the properties and the operations.
    pub fn validate(&self) -> Result<()> {
        self.plugin.validate()?;
        validate_operations(&self.operations)
    }
}

impl PluginProperties {
    /// Its permission of type `P` (`permission::<CommandsPermission>()`), with its own
    /// data, if it asks for one.
    pub fn permission<P: PermissionData>(&self) -> Option<&P> {
        self.permissions.iter().find_map(|request| P::from_permission(&request.permission))
    }

    /// Check what the types can't: the id's form, a name, a version, its permissions'
    /// reasons, its tips and the URL of where it is published.
    pub fn validate(&self) -> Result<()> {
        if let Some(url) = &self.update {
            validate_url(url)?;
        }
        validate_id(&self.id)?;
        if self.name.trim().is_empty() {
            bail!("the plugin has no name");
        }
        if self.version.trim().is_empty() {
            bail!("the plugin has no version");
        }
        let mut asked = HashSet::new();
        for request in &self.permissions {
            let name = request.permission.spec().name();
            if !asked.insert(name) {
                bail!("the plugin asks for {name} twice");
            }
            validate_reason(&request.reason)?;
            request.permission.spec().validate()?;
        }
        if self.tips.len() > MAX_TIPS {
            bail!("the plugin has {} tips: at most {MAX_TIPS}", self.tips.len());
        }
        self.tips.iter().try_for_each(|tip| validate_tip(tip))
    }
}

/// Check a plugin id: it names files and folders, so 1 to 128 of `[A-Za-z0-9._-]`,
/// not starting with `.`.
pub fn validate_id(id: &str) -> Result<()> {
    let allowed = |c: char| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-');
    if id.is_empty() || id.len() > 128 || id.starts_with('.') || !id.chars().all(allowed) {
        bail!(
            "plugin id {id:?} must be 1 to 128 letters, digits, '.', '_' or '-', \
             not starting with '.'"
        );
    }
    Ok(())
}

impl Operation {
    /// Check one operation: it has an id and a title.
    pub fn validate(&self) -> Result<()> {
        if self.id.is_empty() {
            bail!("an operation has no id");
        }
        if self.title.trim().is_empty() {
            bail!("operation {:?} has no title", self.id);
        }
        Ok(())
    }
}

/// Check why a plugin asks for a permission: not blank, and at most
/// [`MAX_REASON_CHARS`] characters.
pub fn validate_reason(reason: &str) -> Result<()> {
    if reason.trim().is_empty() {
        bail!("a permission's reason is blank");
    }
    let chars = reason.chars().count();
    if chars > MAX_REASON_CHARS {
        bail!("a permission's reason is {chars} characters: at most {MAX_REASON_CHARS}");
    }
    Ok(())
}

/// Check a URL the app will fetch (where a plugin is published): `http://` or `https://`,
/// a host, no spaces or control characters, and at most [`MAX_URL_CHARS`] characters.
pub fn validate_url(url: &str) -> Result<()> {
    let Some(rest) = url.strip_prefix("https://").or_else(|| url.strip_prefix("http://")) else {
        bail!("{url:?} isn't an http:// or https:// URL");
    };
    let host = rest.split(['/', '?', '#']).next().unwrap_or_default();
    if host.is_empty() {
        bail!("{url:?} has no host");
    }
    if url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        bail!("{url:?} has spaces or control characters in it");
    }
    if url.chars().count() > MAX_URL_CHARS {
        bail!("the URL is longer than {MAX_URL_CHARS} characters");
    }
    Ok(())
}

/// Check one tip: not blank, and at most [`MAX_TIP_CHARS`] characters.
pub fn validate_tip(tip: &str) -> Result<()> {
    if tip.trim().is_empty() {
        bail!("a tip is blank");
    }
    let chars = tip.chars().count();
    if chars > MAX_TIP_CHARS {
        bail!("a tip is {chars} characters: at most {MAX_TIP_CHARS}, so it fits on the launcher's line");
    }
    Ok(())
}

/// Check a plugin's operations: at least one, each valid, and no id used twice.
pub fn validate_operations(operations: &[Operation]) -> Result<()> {
    if operations.is_empty() {
        bail!("the plugin offers no operations");
    }
    for operation in operations {
        operation.validate()?;
    }
    if let Some(index) = first_duplicate(operations) {
        bail!("operation id {:?} is used twice", operations[index].id);
    }
    Ok(())
}

/// The index of the first operation whose id an earlier one already uses.
pub fn first_duplicate(operations: &[Operation]) -> Option<usize> {
    let mut ids = HashSet::new();
    operations
        .iter()
        .position(|operation| !ids.insert(operation.id.as_str()))
}

/// A valid manifest for tests, here and in the section's.
#[cfg(test)]
pub(crate) fn sample() -> Manifest {
    Manifest {
        plugin: PluginProperties {
            id: "dev.delight.json".into(),
            name: "JSON".into(),
            version: "1.0.0".into(),
            description: "Format, minify and escape JSON".into(),
            author: "Delight".into(),
            icon: "<svg/>".into(),
            tags: vec!["json".into()],
            permissions: vec![PermissionRequest {
                permission: crate::Permission::network(),
                reason: "Fetches schemas from the web".into(),
            }],
            tips: vec!["Paste JSON to format it".into()],
            update: Some("https://example.com/plugins".into()),
        },
        operations: vec![Operation {
            id: "format".into(),
            title: "Format JSON".into(),
            description: String::new(),
            icon: None,
            tags: Vec::new(),
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::permission::{CommandsPermission, NetworkPermission, Permission};

    #[test]
    fn an_operation_shows_its_own_icon_or_else_the_plugins() {
        let mut manifest = sample();
        let plain = manifest.operations[0].clone();
        let own = Operation {
            id: "minify".into(),
            icon: Some("<svg id=\"minify\"/>".into()),
            ..plain.clone()
        };
        manifest.operations.push(own.clone());
        assert_eq!(manifest.icon_for(&plain), "<svg/>");
        assert_eq!(manifest.icon_for(&own), "<svg id=\"minify\"/>");
    }

    #[test]
    fn validate_accepts_a_good_manifest() {
        sample().validate().unwrap();
    }

    #[test]
    fn validate_checks_the_id() {
        for (id, valid) in [
            ("dev.delight.json", true),
            ("a", true),
            ("A-b_c.9", true),
            (&"a".repeat(128), true),
            ("", false),
            (&"a".repeat(129), false),
            (".hidden", false),
            ("with space", false),
            ("slash/id", false),
            ("dév", false),
        ] {
            let mut manifest = sample();
            manifest.plugin.id = id.to_string();
            assert_eq!(manifest.validate().is_ok(), valid, "id {id:?}");
        }
    }

    #[test]
    fn validate_checks_name_version_and_operations() {
        let mut no_name = sample();
        no_name.plugin.name = " ".into();
        assert!(no_name.validate().is_err());

        let mut no_version = sample();
        no_version.plugin.version = String::new();
        assert!(no_version.validate().is_err());

        let mut no_operations = sample();
        no_operations.operations.clear();
        assert!(no_operations.validate().is_err());

        let mut twice = sample();
        twice.operations.push(twice.operations[0].clone());
        assert!(twice.validate().is_err());

        let mut unnamed = sample();
        unnamed.operations[0].id = String::new();
        assert!(unnamed.validate().is_err());

        let mut untitled = sample();
        untitled.operations[0].title = String::new();
        assert!(untitled.validate().is_err());
    }

    #[test]
    fn validate_checks_the_permissions() {
        let with_reasons = |reasons: &[&str]| {
            let mut manifest = sample();
            manifest.plugin.permissions = reasons
                .iter()
                .map(|reason| PermissionRequest { permission: Permission::network(), reason: reason.to_string() })
                .collect();
            manifest.validate()
        };
        assert!(with_reasons(&["Fetches schemas"]).is_ok());
        assert!(with_reasons(&[""]).is_err(), "none");
        assert!(with_reasons(&["  "]).is_err(), "blank");
        assert!(with_reasons(&[&"é".repeat(MAX_REASON_CHARS)]).is_ok(), "the limit counts characters");
        assert!(with_reasons(&[&"é".repeat(MAX_REASON_CHARS + 1)]).is_err(), "too long");
        assert!(with_reasons(&["Fetches", "Syncs"]).is_err(), "asked for twice");
        assert!(sample().plugin.permission::<NetworkPermission>().is_some());
        assert!(sample().plugin.permission::<CommandsPermission>().is_none());
    }

    #[test]
    fn validate_checks_the_tips() {
        let with_tips = |tips: Vec<String>| {
            let mut manifest = sample();
            manifest.plugin.tips = tips;
            manifest.validate()
        };
        assert!(with_tips(Vec::new()).is_ok(), "tips are optional");
        assert!(with_tips(vec!["cal <email> shows someone's meetings".into(); MAX_TIPS]).is_ok());
        assert!(with_tips(vec!["a tip".into(); MAX_TIPS + 1]).is_err(), "too many");
        assert!(with_tips(vec!["  ".into()]).is_err(), "blank");
        assert!(with_tips(vec!["é".repeat(MAX_TIP_CHARS)]).is_ok(), "the limit counts characters");
        assert!(with_tips(vec!["é".repeat(MAX_TIP_CHARS + 1)]).is_err(), "too long");
    }

    #[test]
    fn validate_checks_the_update_location() {
        for (url, valid) in [
            ("https://example.com/plugins/json", true),
            ("http://plugins.example.com:8080/delight", true),
            ("http://localhost:8080", true),
            ("ftp://example.com/json", false),
            ("example.com/json", false),
            ("https:///json", false),
            ("https://example.com/a json", false),
            (&format!("https://example.com/{}", "a".repeat(MAX_URL_CHARS)), false),
        ] {
            let mut manifest = sample();
            manifest.plugin.update = Some(url.to_string());
            assert_eq!(manifest.validate().is_ok(), valid, "{url:?}");
        }
        let mut none = sample();
        none.plugin.update = None;
        none.validate().unwrap();
    }

    #[test]
    fn the_first_duplicate_is_the_later_operation() {
        let mut operations = sample().operations;
        assert_eq!(first_duplicate(&operations), None);
        operations.push(Operation {
            id: "minify".into(),
            ..operations[0].clone()
        });
        operations.push(operations[0].clone());
        assert_eq!(first_duplicate(&operations), Some(2));
    }

    #[test]
    fn validate_checks_what_a_permission_holds() {
        let mut manifest = sample();
        manifest.plugin.permissions.push(PermissionRequest {
            permission: Permission::commands(["/bin/ps"]),
            reason: "Lists".into(),
        });
        manifest.validate().unwrap();
        assert_eq!(manifest.plugin.permission::<CommandsPermission>().unwrap().programs, ["/bin/ps"]);
        manifest.plugin.permissions[1].permission = Permission::commands(["ps"]);
        assert!(manifest.validate().is_err());
    }
}
