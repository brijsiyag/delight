//! What the release depends on that isn't logic: who signs it, where it is published, and the
//! pins of what it downloads. Taken from the release that worked on the build Mac
//! (`~/Desktop/delight`); a change here is a change to how Delight is released.

/// The Apple developer team that signs and notarises: only its certificates are used.
/// `DEVELOPER_TEAM_ID` names another (with `DEVELOPER_ID_APPLICATION`).
pub const TEAM_ID: &str = "S3L4RJ57GY";
/// The keychain profile `notarytool` was given the Apple ID's credentials under, on the
/// build Mac (`NOTARY_PROFILE` names another).
pub const NOTARY_PROFILE: &str = "delight-notary";

/// The app's name: its bundle, its executable in the bundle, its disk image.
pub const APP_NAME: &str = "Delight";
/// Where releases are published; the update feed is the latest release's `appcast.xml`.
pub const REPOSITORY: &str = "https://github.com/brijsiyag/delight";

/// A universal app: one build for each.
pub const TARGETS: [&str; 2] = ["aarch64-apple-darwin", "x86_64-apple-darwin"];
/// What the built-in plugins are built for; they are the same on every Mac.
pub const PLUGINS_TARGET: &str = "wasm32-wasip2";

/// Sparkle, the updater: the release it is taken from, its SHA-256, and the login-keychain
/// account its update-signing key is under. The key already exists (its public half is in
/// `Info.plist`); a new one would stop existing installs updating.
pub const SPARKLE_VERSION: &str = "2.9.6";
pub const SPARKLE_SHA256: &str = "52bf9e88cdd972fc0c81501377a880e90d47031bd8ca5462488f843e2609e192";
pub const SPARKLE_ACCOUNT: &str = "delight";
