//! The protocol version: which plugins an app runs.

use std::fmt;

use serde::{Deserialize, Serialize};

/// The version of the plugin API: of the crates plugins build against
/// (delight-manifest, delight-protocol, delight-plugin-api and its macros), which
/// are released together with this one version, in their `Cargo.toml`s.
pub const PLUGIN_API_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The version of the contract between Delight and its plugins: the plugin API's
/// major.minor. Every plugin carries the version it was built against; see
/// [`ProtocolVersion::supports`] for which ones the app runs.
///
/// Bump the plugin API's minor for changes that plugins built before them survive: a
/// new host method, a new field in data the app sends (older plugins ignore it), a
/// new field with a default in data plugins send, a new plugin or tool method the app
/// copes with older plugins lacking. Bump its major (and reset the minor) for
/// anything else that reaches built plugins or their code: removing or renaming a
/// method, changing a type, a new enum variant sent to plugins, a change to the Rust
/// API that plugins can't build against unchanged. Bump the patch for the rest.
///
/// 1.1 added `HostApi::remember_input`; 1.2 `HostApi::current_theme`; 1.3 the
/// plugin's tips, in its manifest; 1.4 `PluginApi::open_settings` (the app treats a
/// plugin without it as having no settings page); 1.5 each permission with the
/// plugin's reason for it (the app shows a permission from an older plugin without one).
/// 2.0 moved to embedded_gpui's `surfaces-as-roots`, whose wire protocol changed:
/// plugins use GPUI's own clipboard (`HostApi::clipboard`: reading and writing)
/// instead of `HostApi::copy_text`, `Plugin::open_tool` and `settings_page` have no
/// window, and a tool's actions are an enum (`Tool::Action`, `#[derive(Actions)]`).
pub const PROTOCOL_VERSION: ProtocolVersion = ProtocolVersion {
    major: number(env!("CARGO_PKG_VERSION_MAJOR")),
    minor: number(env!("CARGO_PKG_VERSION_MINOR")),
};

/// A version number from Cargo, at compile time.
const fn number(digits: &str) -> u32 {
    match u32::from_str_radix(digits, 10) {
        Ok(number) => number,
        Err(_) => panic!("Cargo's version numbers are decimal"),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtocolVersion {
    pub major: u32,
    pub minor: u32,
}

impl ProtocolVersion {
    /// Whether an app on this version runs a plugin built for `plugin`: the same
    /// major, and a minor no newer than the app's. A plugin built for a newer minor
    /// may call what this app doesn't have, so it's refused rather than failing
    /// halfway.
    pub fn supports(self, plugin: ProtocolVersion) -> bool {
        plugin.major == self.major && plugin.minor <= self.minor
    }
}

impl fmt::Display for ProtocolVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.major, self.minor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn older_minors_are_supported_newer_ones_and_other_majors_are_not() {
        let version = |major, minor| ProtocolVersion { major, minor };
        let app = version(2, 3);
        assert!(app.supports(version(2, 3)));
        assert!(app.supports(version(2, 0)));
        assert!(!app.supports(version(2, 4)));
        assert!(!app.supports(version(1, 3)));
        assert!(!app.supports(version(3, 0)));
        assert_eq!(app.to_string(), "2.3");
    }
}
