//! The protocol version: which plugins an app runs.

use std::fmt;

use serde::{Deserialize, Serialize};

/// The version of the contract between Delight and its plugins. Every plugin carries
/// the version it was built against; see [`ProtocolVersion::supports`] for which ones
/// the app runs.
///
/// Bump `minor` for changes that plugins built before them survive: a new host
/// method, a new field in data the app sends (older plugins ignore it), a new field
/// with a default in data plugins send, a new plugin or tool method the app copes
/// with older plugins lacking. Bump `major` (and reset `minor`) for anything else:
/// removing or renaming a method, changing a type, a new enum variant sent to plugins.
pub const PROTOCOL_VERSION: ProtocolVersion = ProtocolVersion { major: 1, minor: 0 };

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
