//! What a plugin is: read by the app from the `.wasm` before any of its code runs,
//! so installing shows what a plugin can do, and its sandbox is set up from it.

use std::collections::HashSet;

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    /// Names the plugin's files and folders, so it is limited to `[A-Za-z0-9._-]`,
    /// 1 to 128 characters, not starting with `.`. Reverse-DNS by convention.
    pub id: String,
    pub name: String,
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
    /// The tools the plugin offers; at least one.
    pub operations: Vec<Operation>,
    #[serde(default)]
    pub permissions: Vec<Permission>,
}

/// One tool a plugin offers.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Operation {
    /// Unique within the plugin.
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    /// Its own icon, an SVG like [`Manifest::icon`], so a plugin's tools can look
    /// different in the tool list. Without one it shows the plugin's.
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

/// What a plugin may do outside its sandbox. Each is granted when the plugin is
/// installed, and gates the objects the app hands it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Permission {
    /// Reach the network.
    Network,
}

impl Manifest {
    /// The icon to show for `operation`: its own, or else the plugin's.
    pub fn icon_for<'a>(&'a self, operation: &'a Operation) -> &'a str {
        operation.icon.as_deref().unwrap_or(&self.icon)
    }

    /// Check what the types can't: the id's form, and the operations.
    pub fn validate(&self) -> Result<()> {
        validate_id(&self.id)?;
        if self.name.trim().is_empty() {
            bail!("the plugin has no name");
        }
        if self.version.trim().is_empty() {
            bail!("the plugin has no version");
        }
        if self.operations.is_empty() {
            bail!("the plugin offers no operations");
        }
        let mut ids = HashSet::new();
        for operation in &self.operations {
            if operation.id.is_empty() {
                bail!("an operation has no id");
            }
            if !ids.insert(operation.id.as_str()) {
                bail!("operation id {:?} is used twice", operation.id);
            }
        }
        Ok(())
    }
}

fn validate_id(id: &str) -> Result<()> {
    let allowed = |c: char| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-');
    if id.is_empty() || id.len() > 128 || id.starts_with('.') || !id.chars().all(allowed) {
        bail!(
            "plugin id {id:?} must be 1 to 128 letters, digits, '.', '_' or '-', \
             not starting with '.'"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest() -> Manifest {
        Manifest {
            id: "dev.delight.json".into(),
            name: "JSON".into(),
            version: "1.0.0".into(),
            description: "Format, minify and escape JSON".into(),
            author: "Delight".into(),
            icon: "<svg/>".into(),
            tags: vec!["json".into()],
            operations: vec![Operation {
                id: "format".into(),
                title: "Format JSON".into(),
                description: String::new(),
                icon: None,
                tags: Vec::new(),
            }],
            permissions: vec![Permission::Network],
        }
    }

    #[test]
    fn round_trips_through_json() {
        let manifest = manifest();
        let json = serde_json::to_string(&manifest).unwrap();
        assert_eq!(serde_json::from_str::<Manifest>(&json).unwrap(), manifest);
    }

    #[test]
    fn optional_fields_default() {
        let manifest: Manifest = serde_json::from_str(
            r#"{
                "id": "a", "name": "A", "version": "1", "icon": "<svg/>",
                "operations": [{"id": "x", "title": "X"}]
            }"#,
        )
        .unwrap();
        assert_eq!(manifest.description, "");
        assert!(manifest.permissions.is_empty());
        assert!(manifest.operations[0].tags.is_empty());
        assert_eq!(manifest.operations[0].icon, None);
        manifest.validate().unwrap();
    }

    #[test]
    fn an_operation_shows_its_own_icon_or_else_the_plugins() {
        let mut manifest = manifest();
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
    fn permissions_are_spelled_as_in_the_manifest() {
        assert_eq!(
            serde_json::to_string(&[Permission::Network]).unwrap(),
            r#"["Network"]"#
        );
        assert!(serde_json::from_str::<Permission>(r#""Files""#).is_err());
    }

    #[test]
    fn validate_accepts_a_good_manifest() {
        manifest().validate().unwrap();
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
            let manifest = Manifest {
                id: id.to_string(),
                ..manifest()
            };
            assert_eq!(manifest.validate().is_ok(), valid, "id {id:?}");
        }
    }

    #[test]
    fn validate_checks_name_version_and_operations() {
        let no_name = Manifest {
            name: " ".into(),
            ..manifest()
        };
        assert!(no_name.validate().is_err());

        let no_version = Manifest {
            version: String::new(),
            ..manifest()
        };
        assert!(no_version.validate().is_err());

        let no_operations = Manifest {
            operations: Vec::new(),
            ..manifest()
        };
        assert!(no_operations.validate().is_err());

        let mut twice = manifest();
        twice.operations.push(twice.operations[0].clone());
        assert!(twice.validate().is_err());

        let mut unnamed = manifest();
        unnamed.operations[0].id = String::new();
        assert!(unnamed.validate().is_err());
    }
}
