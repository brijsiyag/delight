//! Permissions: what a plugin may do outside its sandbox.
//!
//! Each permission is a type of its own that holds whatever data only it needs (the
//! programs `Commands` may run; `Network` needs none) and implements
//! [`PermissionSpec`]: how it is checked and how people are told about it. [`Permission`]
//! is the enum over them, so the manifest, the macro that writes it, and the app that
//! lists and enforces it treat every permission the same way. A new permission is a
//! new type, its `impl PermissionSpec`, and a variant.

use std::collections::HashSet;

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

/// The folders a program a plugin may run is in: the system's own, which macOS protects.
pub const COMMAND_DIRS: [&str; 4] = ["/bin", "/sbin", "/usr/bin", "/usr/sbin"];
/// Most programs [`CommandsPermission`] lists, so people can read them all when they
/// install.
pub const MAX_PROGRAMS: usize = 20;

/// What every permission is: checked when the manifest is, and described to people who
/// install the plugin and look at it in Settings.
pub trait PermissionSpec {
    /// The name the manifest spells it with, such as `Network`.
    fn name(&self) -> &'static str;

    /// Check its data, which the types can't.
    fn validate(&self) -> Result<()>;

    /// What people call it: "Runs commands".
    fn title(&self) -> &'static str;

    /// What it allows, with its data: shown under the title, before the plugin's own
    /// reason for asking.
    fn describe(&self) -> String;

    /// What its data lists, shown under [`describe`](Self::describe) as separate items
    /// (the programs `Commands` may run). None by default.
    fn items(&self) -> Vec<String> {
        Vec::new()
    }

    /// A [Lucide](https://lucide.dev) icon's file name, without `.svg`.
    fn icon(&self) -> &'static str;
}

/// A permission's own type, found in a [`Permission`]: what
/// `PluginProperties::permission::<P>()` looks for.
pub trait PermissionData: PermissionSpec + Sized {
    fn from_permission(permission: &Permission) -> Option<&Self>;
}

/// What a plugin may do outside its sandbox. Each is granted when the plugin is
/// installed, and gates the objects the app hands it. In the manifest it is an object
/// named by its `permission` field, next to its own data: `{"permission": "Commands",
/// "programs": ["/bin/ps"]}`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "permission")]
pub enum Permission {
    Network(NetworkPermission),
    Commands(CommandsPermission),
}

impl Permission {
    pub fn network() -> Self {
        Permission::Network(NetworkPermission {})
    }

    pub fn commands(programs: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Permission::Commands(CommandsPermission { programs: programs.into_iter().map(Into::into).collect() })
    }

    /// The permission as [`PermissionSpec`], whichever it is.
    pub fn spec(&self) -> &dyn PermissionSpec {
        match self {
            Permission::Network(network) => network,
            Permission::Commands(commands) => commands,
        }
    }
}

/// Reach the network.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NetworkPermission {}

impl PermissionSpec for NetworkPermission {
    fn name(&self) -> &'static str {
        "Network"
    }

    fn validate(&self) -> Result<()> {
        Ok(())
    }

    fn title(&self) -> &'static str {
        "Network"
    }

    fn describe(&self) -> String {
        "Can reach the internet and your local network, and listen on this Mac".into()
    }

    fn icon(&self) -> &'static str {
        "globe"
    }
}

impl PermissionData for NetworkPermission {
    fn from_permission(permission: &Permission) -> Option<&Self> {
        match permission {
            Permission::Network(network) => Some(network),
            _ => None,
        }
    }
}

/// Run programs, with any arguments.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandsPermission {
    /// The programs it may run, each an absolute path directly in one of the
    /// [`COMMAND_DIRS`], such as `/bin/ps`. At least one, at most [`MAX_PROGRAMS`].
    pub programs: Vec<String>,
}

impl PermissionSpec for CommandsPermission {
    fn name(&self) -> &'static str {
        "Commands"
    }

    /// At least one program, at most [`MAX_PROGRAMS`], each a [`validate_program`]
    /// path, none twice.
    fn validate(&self) -> Result<()> {
        if self.programs.is_empty() {
            bail!("Commands lists no programs");
        }
        if self.programs.len() > MAX_PROGRAMS {
            bail!("Commands lists {} programs: at most {MAX_PROGRAMS}", self.programs.len());
        }
        let mut listed = HashSet::new();
        for program in &self.programs {
            validate_program(program)?;
            if !listed.insert(program) {
                bail!("Commands lists {program:?} twice");
            }
        }
        Ok(())
    }

    fn title(&self) -> &'static str {
        "Runs commands"
    }

    fn describe(&self) -> String {
        "Can run these programs, with any arguments".into()
    }

    fn items(&self) -> Vec<String> {
        self.programs.clone()
    }

    fn icon(&self) -> &'static str {
        "terminal"
    }
}

impl PermissionData for CommandsPermission {
    fn from_permission(permission: &Permission) -> Option<&Self> {
        match permission {
            Permission::Commands(commands) => Some(commands),
            _ => None,
        }
    }
}

/// Check a program a plugin may run: an absolute path directly in one of the
/// [`COMMAND_DIRS`] (so no `..`, no subfolder, no relative name looked up in `PATH`).
pub fn validate_program(program: &str) -> Result<()> {
    let listed = program.rsplit_once('/').is_some_and(|(dir, name)| {
        COMMAND_DIRS.contains(&dir) && !matches!(name, "" | "." | "..") && !name.chars().any(char::is_control)
    });
    if !listed {
        bail!("{program:?} isn't a program directly in {}", COMMAND_DIRS.join(", "));
    }
    Ok(())
}

/// A permission a plugin asks for, and why. People see the reason, in the plugin's
/// words, next to what the permission allows, when they install it and in Settings,
/// and decide. In the manifest they are one object: `{"permission": "Commands",
/// "programs": ["/bin/ps"], "reason": "…"}`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "Requested")]
pub struct PermissionRequest {
    #[serde(flatten)]
    pub permission: Permission,
    /// At most [`MAX_REASON_CHARS`](crate::MAX_REASON_CHARS) characters. Empty for a
    /// plugin built before reasons (plugin API 1.5), which doesn't say.
    pub reason: String,
}

/// A permission as manifests spell it: with its reason, or, before 1.5, only its name
/// (then only `Network` existed).
#[derive(Deserialize)]
#[serde(untagged)]
enum Requested {
    WithReason {
        #[serde(flatten)]
        permission: Permission,
        reason: String,
    },
    Bare(BareName),
}

#[derive(Deserialize)]
enum BareName {
    Network,
}

impl From<Requested> for PermissionRequest {
    fn from(requested: Requested) -> Self {
        match requested {
            Requested::WithReason { permission, reason } => PermissionRequest { permission, reason },
            Requested::Bare(BareName::Network) => {
                PermissionRequest { permission: Permission::network(), reason: String::new() }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permissions_are_spelled_as_in_the_manifest() {
        assert_eq!(serde_json::to_string(&Permission::network()).unwrap(), r#"{"permission":"Network"}"#);
        assert_eq!(
            serde_json::to_string(&Permission::commands(["/bin/ps"])).unwrap(),
            r#"{"permission":"Commands","programs":["/bin/ps"]}"#
        );
        assert!(serde_json::from_str::<Permission>(r#"{"permission":"Files"}"#).is_err());
        assert_eq!(Permission::network().spec().name(), "Network");
        assert_eq!(Permission::commands(["/bin/ps"]).spec().name(), "Commands");
    }

    #[test]
    fn a_permission_comes_with_its_reason_or_before_1_5_without() {
        let request = PermissionRequest { permission: Permission::network(), reason: "Syncs".into() };
        let json = r#"{"permission":"Network","reason":"Syncs"}"#;
        assert_eq!(serde_json::to_string(&request).unwrap(), json);
        assert_eq!(serde_json::from_str::<PermissionRequest>(json).unwrap(), request);
        let bare = serde_json::from_str::<PermissionRequest>(r#""Network""#).unwrap();
        assert_eq!(bare, PermissionRequest { permission: Permission::network(), reason: String::new() });
        assert!(serde_json::from_str::<PermissionRequest>(r#""Commands""#).is_err(), "bare names were only Network");
    }

    #[test]
    fn a_permission_holds_only_its_own_data() {
        let json = r#"{"permission":"Commands","programs":["/bin/ps","/usr/sbin/lsof"],"reason":"Lists processes"}"#;
        let request = PermissionRequest {
            permission: Permission::commands(["/bin/ps", "/usr/sbin/lsof"]),
            reason: "Lists processes".into(),
        };
        assert_eq!(serde_json::to_string(&request).unwrap(), json);
        assert_eq!(serde_json::from_str::<PermissionRequest>(json).unwrap(), request);
        // Network has nowhere to put programs; Commands has to have them.
        assert!(serde_json::from_str::<PermissionRequest>(r#"{"permission":"Network","programs":["/bin/ps"],"reason":"x"}"#).is_err());
        assert!(serde_json::from_str::<PermissionRequest>(r#"{"permission":"Commands","reason":"x"}"#).is_err());
    }

    #[test]
    fn commands_check_their_programs() {
        for good in [&["/bin/ps"][..], &["/sbin/ping", "/usr/bin/curl", "/usr/sbin/lsof"]] {
            Permission::commands(good.iter().copied()).spec().validate().unwrap();
        }
        for (bad, why) in [
            (&[][..], "none"),
            (&["ps"], "not absolute"),
            (&["/bin/../usr/bin/ps"], "up a folder"),
            (&["/usr/local/bin/brew"], "not a system folder"),
            (&["/usr/bin/sub/tool"], "a subfolder"),
            (&["/bin/"], "no name"),
            (&["/bin/.."], "no name"),
            (&["/bin/ps", "/bin/ps"], "twice"),
        ] {
            assert!(Permission::commands(bad.iter().copied()).spec().validate().is_err(), "{why}");
        }
        let many = (0..=MAX_PROGRAMS).map(|n| format!("/bin/p{n}"));
        assert!(Permission::commands(many).spec().validate().is_err(), "too many");
    }

    #[test]
    fn people_are_told_what_each_allows() {
        let commands = Permission::commands(["/bin/ps", "/bin/kill"]);
        assert_eq!(commands.spec().title(), "Runs commands");
        assert_eq!(commands.spec().describe(), "Can run these programs, with any arguments");
        assert_eq!(commands.spec().items(), ["/bin/ps", "/bin/kill"]);
        assert!(Permission::network().spec().items().is_empty());
        assert_eq!(commands.spec().icon(), "terminal");
        assert_eq!(Permission::network().spec().icon(), "globe");
    }
}
