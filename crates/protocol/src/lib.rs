//! The contract between Delight and its plugins.
//!
//! A plugin is an embedded_gpui component. Each end installs one root object: the
//! plugin a [`PluginApi`], and the app, for that plugin alone, a [`HostApi`] (so no
//! plugin id ever travels). Everything else is reached through those roots: opening a
//! tool gives the app a [`ToolApi`] ref homed in the plugin.
//!
//! What a plugin is (its id, name, tools and permissions) is its [`Manifest`], which
//! the app reads from the `.wasm` without running it.
//!
//! `#[interface]` makes one message type per method at module level, so method names
//! are unique across the interfaces here and don't clash with type names.

mod manifest;

pub use manifest::{Manifest, Operation, Permission};

use std::fmt;

use embedded_gpui::surface::SurfaceApi;
use embedded_gpui::{Ref, data, interface};
use serde::{Deserialize, Serialize};

/// The version of this contract. Every plugin carries the version it was built
/// against; see [`ProtocolVersion::supports`] for which ones the app runs.
///
/// Bump `minor` for changes that plugins built before them survive: a new
/// [`HostApi`] method, a new field in data the app sends (older plugins ignore it),
/// a new field with a default in data plugins send, a new [`PluginApi`] or
/// [`ToolApi`] method the app copes with older plugins lacking. Bump `major` (and
/// reset `minor`) for anything else: removing or renaming a method, changing a
/// type, a new enum variant sent to plugins.
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

/// The plugin's root object: what the app reaches in a plugin.
#[interface]
pub trait PluginApi {
    /// Which of this plugin's operations fit `input`, and how well (0 to 1).
    fn detect(&mut self, input: Input, cx: &mut gpui::Context<Self>) -> Vec<Detection>;

    /// Open the tool for `operation`, drawing on `surface`.
    fn open_tool(
        &mut self,
        operation: String,
        surface: Ref<SurfaceApi>,
        cx: &mut gpui::Context<Self>,
    ) -> Ref<ToolApi>;
}

/// One open tool, homed in the plugin. Its home notifies (`cx.notify`) when its
/// actions change, so the app observes the tool rather than asking after every draw.
#[interface]
pub trait ToolApi {
    /// Set the tool's input to the launcher's: when the tool opens, and whenever
    /// the input changes.
    fn set_input(&mut self, input: Input, cx: &mut gpui::Context<Self>);

    /// The footer actions, in order.
    fn actions(&mut self, cx: &mut gpui::Context<Self>) -> Vec<Action>;

    /// Run the action with this id. The tool does the work itself (copying,
    /// toasting) through its [`HostApi`].
    fn perform(&mut self, action: String, cx: &mut gpui::Context<Self>);
}

/// The app's root object for one plugin: what a plugin reaches in the app.
#[interface]
pub trait HostApi {
    /// Show `message` in the launcher's footer for a moment.
    fn toast(&mut self, message: String, cx: &mut gpui::Context<Self>);

    /// Put `text` on the clipboard.
    // TODO: migrate plugins to GPUI's own `cx.write_to_clipboard` and remove this
    // method, once embedded_gpui's plugin platform forwards clipboard writes to
    // the host (today it drops them). An embedded_gpui change.
    fn copy_text(&mut self, text: String, cx: &mut gpui::Context<Self>);

    /// Hide the launcher.
    fn hide(&mut self, cx: &mut gpui::Context<Self>);
}

/// What is in the launcher: the typed or pasted text. A struct, so more (such as
/// pasted files) can join it later as a minor protocol change.
#[data]
#[derive(Default, PartialEq)]
pub struct Input {
    pub text: String,
}

/// One operation that fits an input, and how well: 0 (not at all) to 1.
#[data]
#[derive(PartialEq)]
pub struct Detection {
    pub operation: String,
    pub confidence: f32,
}

/// A footer action of a tool.
#[data]
#[derive(PartialEq)]
pub struct Action {
    pub id: String,
    pub label: String,
    pub shortcut: Shortcut,
}

/// An action's key. Every action states one, so going without a key is a choice
/// rather than an oversight.
#[data]
#[derive(PartialEq)]
pub enum Shortcut {
    /// A GPUI keystroke such as `cmd-enter`.
    Keystroke(String),
    /// No key: the action is only clicked.
    ClickOnly,
}

#[cfg(test)]
mod tests {
    use super::*;
    use embedded_gpui::{Interface, decode, encode};
    use serde::{Serialize, de::DeserializeOwned};

    /// Encode `value` as it crosses the boundary, check the JSON, and decode it back.
    fn assert_wire<T>(value: &T, json: &str)
    where
        T: Serialize + DeserializeOwned + PartialEq + std::fmt::Debug,
    {
        let payload = encode(value).unwrap();
        assert_eq!(String::from_utf8(payload.bytes.clone()).unwrap(), json);
        assert_eq!(&decode::<T>(&payload).unwrap(), value);
    }

    #[test]
    fn input_crosses_as_its_text() {
        assert_wire(
            &Input {
                text: "{\"a\": 1}".into(),
            },
            r#"{"text":"{\"a\": 1}"}"#,
        );
        assert_wire(&Input::default(), r#"{"text":""}"#);
    }

    #[test]
    fn detections_and_actions_cross_as_plain_objects() {
        assert_wire(
            &vec![Detection {
                operation: "format".into(),
                confidence: 0.75,
            }],
            r#"[{"operation":"format","confidence":0.75}]"#,
        );
        assert_wire(
            &Action {
                id: "copy".into(),
                label: "Copy".into(),
                shortcut: Shortcut::Keystroke("cmd-enter".into()),
            },
            r#"{"id":"copy","label":"Copy","shortcut":{"Keystroke":"cmd-enter"}}"#,
        );
        assert_wire(
            &Action {
                id: "copy".into(),
                label: "Copy".into(),
                shortcut: Shortcut::ClickOnly,
            },
            r#"{"id":"copy","label":"Copy","shortcut":"ClickOnly"}"#,
        );
    }

    #[test]
    fn an_action_without_a_shortcut_is_refused() {
        for json in [
            r#"{"id":"copy","label":"Copy"}"#,
            r#"{"id":"copy","label":"Copy","shortcut":null}"#,
        ] {
            let payload = embedded_gpui::Payload::from_parts(json.into(), Vec::new());
            assert!(decode::<Action>(&payload).is_err(), "{json}");
        }
    }

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

    #[test]
    fn interfaces_have_the_methods_of_the_contract() {
        let methods = |schema: embedded_gpui::Schema| -> Vec<&str> {
            schema.methods.iter().map(|method| method.name).collect()
        };
        assert_eq!(methods(PluginApi::schema()), ["detect", "open_tool"]);
        assert_eq!(methods(ToolApi::schema()), ["set_input", "actions", "perform"]);
        assert_eq!(methods(HostApi::schema()), ["toast", "copy_text", "hide"]);
    }
}
