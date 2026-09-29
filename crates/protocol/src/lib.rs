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
    COMMAND_DIRS, CommandsPermission, MAX_TIP_CHARS, Manifest, NetworkPermission, Operation, PLUGIN_API_VERSION,
    PROTOCOL_VERSION, Permission, PermissionRequest, PermissionSpec, PluginProperties, ProtocolVersion, validate_id, validate_tip,
};

/// The most a plugin's settings ([`HostApi::set_settings`]) are, as JSON: they are a small
/// value the app loads at start with every plugin's, not a place for data (`/data` is).
pub const MAX_SETTINGS_BYTES: usize = 256 * 1024;

/// Running programs, for plugins with `Commands`.
mod commands;

pub use commands::*;

/// The Mac's DNS setup, for plugins with `Network`.
mod dns;

pub use dns::*;

// TEMPORARY(network): the app's HTTP for plugins, until embedded_gpui links `wasi:http`. It's all
// in `network/`; README, "Temporary host APIs".
mod network;

pub use network::*;

use embedded_gpui::ClipboardApi;
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

    /// The sections of this plugin's settings, in order: each is drawn by the app as a
    /// titled card on the plugin's page in the settings window, as its permissions and
    /// tips are, with the plugin's own content inside. None for a plugin without
    /// settings. This object notifies (`cx.notify`) when they change (a section added
    /// or resized), so the app, observing it, asks again.
    fn settings_sections(&mut self, cx: &mut gpui::Context<Self>) -> Vec<SettingsSection>;

    /// Draw the content of the section with this id on `surface`, inside its card:
    /// `false` if there is no such section.
    fn open_settings_section(&mut self, id: String, surface: Ref<SurfaceApi>, cx: &mut gpui::Context<Self>) -> bool;
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

    /// Hide the launcher.
    fn hide(&mut self, cx: &mut gpui::Context<Self>);

    /// Remember `text` as an input worth coming back to, for this plugin's
    /// `operation`: the launcher offers it as a completion while typing (Tab takes
    /// it) and in its history search (⌃R), and brings this tool up when it's used.
    /// Only what plugins remember is kept.
    fn remember_input(&mut self, operation: String, text: String, cx: &mut gpui::Context<Self>);

    /// The app's theme now, for the plugin to draw with. This object notifies when
    /// it changes (light and dark), so a plugin observes it and asks again.
    fn current_theme(&mut self, cx: &mut gpui::Context<Self>) -> Theme;

    /// The clipboard, for GPUI's own `cx.write_to_clipboard` and
    /// `cx.read_from_clipboard` in the plugin (embedded_gpui's `use_clipboard`), so a
    /// plugin copies, and ⌘V pastes in its text fields. Every plugin has it: it
    /// needs no permission.
    fn clipboard(&mut self, cx: &mut gpui::Context<Self>) -> Ref<ClipboardApi>;

    /// HTTP (and gRPC, over it) and callback listeners, done by the app: `None` unless
    /// the plugin has the `Network` permission.
    // TEMPORARY(network)
    fn http(&mut self, cx: &mut gpui::Context<Self>) -> Option<Ref<HttpApi>>;

    /// Open `url` with the app macOS has for it: a web page in the browser (a
    /// sign-in's, say), `mailto:` in the mail app, another app's own link. Not
    /// `file:`, which the app refuses, with why. Needs no permission.
    // TEMPORARY(open_url): until embedded_gpui forwards GPUI's own `cx.open_url` from plugins.
    async fn open_url(&mut self, url: String, cx: &mut gpui::Context<Self>);

    /// The Mac's DNS setup (`dns/`): `None` unless the plugin has the `Network`
    /// permission.
    fn dns(&mut self, cx: &mut gpui::Context<Self>) -> Option<Ref<DnsApi>>;

    /// Running the programs the manifest's `Commands` permission lists (`commands/`):
    /// `None` unless the plugin has it.
    fn commands(&mut self, cx: &mut gpui::Context<Self>) -> Option<Ref<CommandsApi>>;

    /// A secret this plugin saved with [`HostApi::set_secret`] (an API key, a sign-in's
    /// tokens), decrypted: `None` if there is none by this name. They are kept
    /// encrypted, with the key in the Keychain, apart from the plugin's other data.
    /// Needs no permission.
    async fn secret(&mut self, key: String, cx: &mut gpui::Context<Self>) -> Option<String>;

    /// Save a secret under `key` (1 to 256 bytes), encrypted. An empty `value` deletes
    /// it.
    async fn set_secret(&mut self, key: String, value: String, cx: &mut gpui::Context<Self>);

    /// Make the launcher's input this text, for chaining tools: a JSON tool offering "open
    /// the inner value" puts it here and the launcher finds the tool for it. An undoable
    /// edit (⌘Z brings the old text back) that detects again. Applied only while one of
    /// this plugin's own tools is the selected one, and deferred (the launcher may be mid-
    /// update). Needs no permission.
    fn set_launcher_input(&mut self, text: String, cx: &mut gpui::Context<Self>);

    /// This plugin's settings: the JSON it saved with [`HostApi::set_settings`], or `null`
    /// if it saved none. A small value the app keeps for the plugin apart from its data
    /// folder, in a file of the app's; the plugin chooses its shape (the plugin API reads it
    /// into a type of the plugin's). Needs no permission.
    async fn settings(&mut self, cx: &mut gpui::Context<Self>) -> String;

    /// Save this plugin's settings: `json` is a JSON value of at most [`MAX_SETTINGS_BYTES`],
    /// and `null` removes them. An error if it isn't JSON, is too big, or can't be saved.
    async fn set_settings(&mut self, json: String, cx: &mut gpui::Context<Self>);

    /// This Mac's time zone, as its offset from UTC now, in seconds: a plugin's sandbox
    /// has no time zone of its own (its clock is UTC).
    fn utc_offset_seconds(&mut self, cx: &mut gpui::Context<Self>) -> i32;

    /// Open the settings window on this plugin's own page (its settings, if it has
    /// them). Deferred: the launcher may be mid-update.
    fn show_settings(&mut self, cx: &mut gpui::Context<Self>);
}

/// One section of a plugin's settings: a titled card in the settings window. The plugin
/// draws what is inside it; the app draws the title, the card and the note under it, so
/// a plugin's settings look like the app's own.
#[data]
#[derive(PartialEq)]
pub struct SettingsSection {
    /// Names the section for the plugin (which content is drawn where).
    pub id: String,
    /// Above the card.
    pub title: String,
    /// How tall the content is, in pixels: a surface can't say, so the plugin does.
    pub height: f32,
    /// A note in small text under the card; empty for none.
    pub footer: String,
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

/// The app's look, for plugins to draw with: a few colours and sizes, in light or
/// dark. The app's own theme (`delight-ui`'s) as data.
#[data]
#[derive(PartialEq)]
pub struct Theme {
    pub dark: bool,
    /// Body text.
    pub text: Color,
    /// Secondary text: captions, descriptions.
    pub text_muted: Color,
    /// The quietest text: placeholders, hints.
    pub text_faint: Color,
    /// Raised areas: tooltips, popovers.
    pub surface: Color,
    /// Controls: buttons, switches, keycaps.
    pub fill: Color,
    pub border: Color,
    /// Selection, focus and primary actions.
    pub accent: Color,
    /// Text on the accent.
    pub accent_text: Color,
    pub success: Color,
    pub warning: Color,
    pub error: Color,
    /// Font families: for interface text, and for code.
    pub font: String,
    pub mono_font: String,
    /// The body text size, in pixels.
    pub text_size: f32,
    /// The corner radius, in pixels.
    pub radius: f32,
}

/// A colour: its hue, saturation and lightness, and its opacity, each from 0 to 1
/// (GPUI's `Hsla`).
#[data]
#[derive(Copy, PartialEq)]
pub struct Color {
    pub h: f32,
    pub s: f32,
    pub l: f32,
    pub a: f32,
}

/// In a plugin, the plugin API keeps the app's theme as a GPUI global, so what draws
/// with it can follow it (`cx.observe_global::<Theme>()`).
impl embedded_gpui::gpui::Global for Theme {}

impl From<Color> for embedded_gpui::gpui::Hsla {
    fn from(color: Color) -> Self {
        embedded_gpui::gpui::hsla(color.h, color.s, color.l, color.a)
    }
}

impl From<embedded_gpui::gpui::Hsla> for Color {
    fn from(color: embedded_gpui::gpui::Hsla) -> Self {
        Color { h: color.h, s: color.s, l: color.l, a: color.a }
    }
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
    fn colours_cross_as_their_four_numbers() {
        assert_wire(&Color { h: 0.5, s: 1.0, l: 0.25, a: 0.75 }, r#"{"h":0.5,"s":1.0,"l":0.25,"a":0.75}"#);
    }

    #[test]
    fn released_with_the_plugin_api() {
        assert_eq!(env!("CARGO_PKG_VERSION"), PLUGIN_API_VERSION);
    }

    #[test]
    fn interfaces_have_the_methods_of_the_contract() {
        let methods = |schema: embedded_gpui::Schema| -> Vec<&str> {
            schema.methods.iter().map(|method| method.name).collect()
        };
        assert_eq!(
            methods(PluginApi::schema()),
            ["detect", "open_tool", "settings_sections", "open_settings_section"]
        );
        assert_eq!(
            methods(ToolApi::schema()),
            ["on_input_changed", "list_actions", "perform_action"]
        );
        assert_eq!(
            methods(HostApi::schema()),
            ["toast", "hide", "remember_input", "current_theme", "clipboard", "http", "open_url", "dns", "commands", "secret", "set_secret", "set_launcher_input", "settings", "set_settings", "utc_offset_seconds", "show_settings"]
        );
    }
}
