//! The contract between Delight and its plugins.
//!
//! A plugin is an embedded_gpui component. Each end installs one root object: the
//! plugin a [`PluginApi`], and the app, for that plugin alone, a [`HostApi`] (so no
//! plugin id ever travels). Everything else is reached through those roots: opening a
//! tool gives the app a [`ToolApi`] ref homed in the plugin.
//!
//! What a plugin is (its id, name, tools and permissions) is its [`Manifest`], which
//! the app reads from the `.wasm` without running it, together with the
//! [`PROTOCOL_VERSION`] the plugin was built for. Both live in `delight-manifest`,
//! re-exported here.
//!
//! `#[interface]` makes one message type per method at module level, so method names
//! are unique across the interfaces here and don't clash with type names.

pub use delight_manifest::{
    Manifest, Operation, PROTOCOL_VERSION, Permission, PluginProperties, ProtocolVersion,
};

use embedded_gpui::surface::SurfaceApi;
use embedded_gpui::{Ref, data, interface};

/// The plugin's root object: what the app reaches in a plugin.
#[interface]
pub trait PluginApi {
    /// Which of this plugin's operations fit `input`, and how well (0 to 1).
    fn detect(&mut self, input: Input, cx: &mut gpui::Context<Self>) -> Vec<Detection>;

    /// Open the tool for `operation`, drawing on `surface`. Async so that failing
    /// to open (an operation the plugin doesn't have) reaches the app as an error.
    async fn open_tool(
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
    /// The app telling the tool what the launcher's input now is: when the tool
    /// opens, and whenever the input changes.
    fn on_input_changed(&mut self, input: Input, cx: &mut gpui::Context<Self>);

    /// The footer actions, in order.
    fn list_actions(&mut self, cx: &mut gpui::Context<Self>) -> Vec<Action>;

    /// Run the action with this id. The tool does the work itself (copying,
    /// toasting) through its [`HostApi`].
    fn perform_action(&mut self, action: String, cx: &mut gpui::Context<Self>);
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
    use embedded_gpui::serde::{Serialize, de::DeserializeOwned};

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
    fn interfaces_have_the_methods_of_the_contract() {
        let methods = |schema: embedded_gpui::Schema| -> Vec<&str> {
            schema.methods.iter().map(|method| method.name).collect()
        };
        assert_eq!(methods(PluginApi::schema()), ["detect", "open_tool"]);
        assert_eq!(
            methods(ToolApi::schema()),
            ["on_input_changed", "list_actions", "perform_action"]
        );
        assert_eq!(methods(HostApi::schema()), ["toast", "copy_text", "hide"]);
    }
}
