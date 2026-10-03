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
    CommandsPermission, FilesPermission, MAX_TIP_CHARS, Manifest, NetworkPermission, Operation, PLUGIN_API_VERSION,
    PROTOCOL_VERSION, Permission, PermissionRequest, PermissionSpec, Permissions, PluginProperties, ProtocolVersion,
    expand_home, home_spelled, validate_id, validate_tip,
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

/// Bytes that cross, as base64 text.
mod bytes;

pub use bytes::Bytes;

// TEMPORARY(network): the app's HTTP for plugins, until embedded_gpui links `wasi:http`. It's all
// in `network/`; docs/development.md, "Temporary host APIs".
mod network;

pub use network::*;

/// The light and dark themes the app and plugins draw with.
mod theme;

pub use theme::*;

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

    /// Fill the surface of a window the plugin asked the app for ([`HostApi::open_window`]) with the
    /// view it made for `key`: whether it did.
    fn open_window_view(&mut self, key: String, surface: Ref<SurfaceApi>, cx: &mut gpui::Context<Self>) -> bool;
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

    /// The user clicked somewhere else in the launcher (its input, the footer, the list): the
    /// tool closes what it holds open that only a click can close, such as a menu.
    fn focus_lost(&mut self, cx: &mut gpui::Context<Self>);

    /// The tool's view was shown (`true`: the tool was picked, or the launcher came up with it
    /// picked) or hidden (another tool was picked, or the launcher hid).
    fn visibility_changed(&mut self, shown: bool, cx: &mut gpui::Context<Self>);
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

    /// Open a window of the plugin's own: a normal window, resizable and closed by its user (✕, ⌘W).
    /// The app makes it and asks the plugin to draw in it ([`PluginApi::open_window_view`]). A
    /// window with the same `key` that is open comes to the front instead. With
    /// `hide_with_launcher` it goes off screen when the launcher hides, and comes back with it;
    /// without, it stays up until its user closes it. Whether it is open now; not when the plugin
    /// has too many open already.
    async fn open_window(
        &mut self,
        key: String,
        title: String,
        width: f32,
        height: f32,
        hide_with_launcher: bool,
        cx: &mut gpui::Context<Self>,
    ) -> bool;

    /// Bring the plugin's open window `key` back on screen after it went off with the launcher.
    /// Whether the plugin has a window `key` open.
    fn show_window(&mut self, key: String, cx: &mut gpui::Context<Self>) -> bool;

    /// Close the plugin's window `key`, as its user would (✕): its view is let go. Whether the
    /// plugin had a window `key` open.
    fn close_window(&mut self, key: String, cx: &mut gpui::Context<Self>) -> bool;

    /// Ask the user to confirm something with the system's own alert (macOS's `NSAlert`): `title`
    /// as its message, `message` under it, and two buttons, `continue_label` and Cancel. A
    /// `destructive` one is styled as a warning and has Cancel as its default button, so a stray
    /// ↵ doesn't confirm. Whether the user chose to continue; `false` too when the app can't show
    /// the alert (another is open, or nothing of the app is on screen).
    async fn confirm(&mut self, title: String, message: String, continue_label: String, destructive: bool, cx: &mut gpui::Context<Self>) -> bool;

    /// Ask the user to give the plugin `permission`: more of one its manifest asks for, as JSON
    /// spelled as in a manifest (`{"permission": "Files", "write": ["~/Projects"]}`, `~` meaning
    /// the home folder), with the system's alert saying what it allows and the plugin's `reason`.
    /// Allowed, it is kept for the plugin and the plugin starts again with it, so the answer never
    /// reaches it. Otherwise the answer is `false` (declined), or `true` when the plugin has it
    /// already; an error when it can't be given (the manifest doesn't ask for that permission, or
    /// what it names isn't there), or the app can't ask (another alert or picker is up).
    async fn request_permission(&mut self, permission: String, reason: String, cx: &mut gpui::Context<Self>) -> bool;

    // TEMPORARY(pick_folders): until GPUI's own picker (`cx.prompt_for_paths`) works in a plugin.
    /// Show the system's folder picker: one folder, or several with `multiple`, its button saying
    /// `prompt`. The folders picked are kept for the plugin, to read or (`write`) to change too; if
    /// any is new, the plugin starts again with them, so the answer never reaches it. Otherwise the
    /// answer is the folders picked (all of which the plugin had), or none when the user
    /// cancelled; an error when the app can't show the picker. Only for a plugin with `Files`.
    async fn pick_folders(&mut self, multiple: bool, write: bool, prompt: Option<String>, cx: &mut gpui::Context<Self>) -> Vec<String>;

    // TEMPORARY(save_file): until GPUI's own save panel (`cx.prompt_for_new_path`) works in a plugin.
    /// Show the system's save panel, `name` suggested (in Downloads), and write `contents` to the
    /// file the user chose: its path, or none when they cancelled; an error when the app can't show
    /// the panel or write the file. The app writes it, so the plugin needs no folder for it, nor
    /// any permission.
    async fn save_file(&mut self, name: String, contents: Bytes, cx: &mut gpui::Context<Self>) -> Option<String>;
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

/// A footer action of a tool.
#[data]
#[derive(PartialEq)]
pub struct Action {
    pub id: String,
    pub label: String,
    pub shortcut: Shortcut,
    /// How its button looks: `Normal` unless said (and then not sent).
    #[serde(default, skip_serializing_if = "ActionStyle::is_normal")]
    pub style: ActionStyle,
    /// No button in the footer: only its key, which the launcher answers as every action's. Shown
    /// unless said (and then not sent).
    #[serde(default, skip_serializing_if = "is_false")]
    pub hidden: bool,
}

fn is_false(value: &bool) -> bool {
    !*value
}

/// How an action's button looks: a named style, drawn in the app's colours for the current
/// theme, so a plugin never picks a colour.
#[data]
#[derive(PartialEq, Copy, Default)]
pub enum ActionStyle {
    /// The footer's usual button.
    #[default]
    Normal,
    /// The main thing to do here: a bolder label and a solid key.
    Primary,
    /// Something needs doing before the rest can be trusted (results gone stale): it stands out in
    /// pink, with a bolder label.
    Attention,
}

impl ActionStyle {
    fn is_normal(&self) -> bool {
        *self == ActionStyle::Normal
    }
}

/// An action's key. Every action states one, so going without a key is a choice
/// rather than an oversight.
///
/// The launcher gives actions only three kinds of key, so a tool's keys are the ones every tool
/// has: ↵, ⌘↵ and ⌥1 to ⌥9 (see [`Shortcut::is_allowed`]). A plugin's own keystroke is on the wire
/// as text for compatibility, but the app answers any other than these by only clicking that action.
#[data]
#[derive(PartialEq)]
pub enum Shortcut {
    /// A GPUI keystroke: `enter`, `cmd-enter` or `alt-1` to `alt-9`.
    Keystroke(String),
    /// No key: the action is only clicked.
    ClickOnly,
}

impl Shortcut {
    /// ↵, as a keystroke.
    pub const ENTER: &'static str = "enter";
    /// ⌘↵, as a keystroke.
    pub const CMD_ENTER: &'static str = "cmd-enter";

    /// ⌥ and a digit, as a keystroke: `alt-1` to `alt-9`; none for another digit.
    pub fn option(digit: u8) -> Option<String> {
        (1..=9).contains(&digit).then(|| format!("alt-{digit}"))
    }

    /// Whether `keystroke` (GPUI's syntax, any case) is one an action may have: ↵, ⌘↵ or ⌥1 to ⌥9.
    pub fn is_allowed(keystroke: &str) -> bool {
        let keystroke = keystroke.to_ascii_lowercase();
        keystroke == Self::ENTER
            || keystroke == Self::CMD_ENTER
            || keystroke.strip_prefix("alt-").and_then(|digit| digit.parse::<u8>().ok()).is_some_and(|digit| Self::option(digit).is_some_and(|option| option == keystroke))
    }
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
                style: ActionStyle::Normal,
                hidden: false,
            },
            r#"{"id":"copy","label":"Copy","shortcut":{"Keystroke":"cmd-enter"}}"#,
        );
        assert_wire(
            &Action {
                id: "copy".into(),
                label: "Copy".into(),
                shortcut: Shortcut::ClickOnly,
                style: ActionStyle::Attention,
                hidden: false,
            },
            r#"{"id":"copy","label":"Copy","shortcut":"ClickOnly","style":"Attention"}"#,
        );
        assert_wire(
            &Action {
                id: "prd".into(),
                label: "Prd".into(),
                shortcut: Shortcut::Keystroke("alt-1".into()),
                style: ActionStyle::Normal,
                hidden: true,
            },
            r#"{"id":"prd","label":"Prd","shortcut":{"Keystroke":"alt-1"},"hidden":true}"#,
        );
    }

    #[test]
    fn only_enter_cmd_enter_and_option_1_to_9_are_keys() {
        for allowed in ["enter", "cmd-enter", "alt-1", "alt-9", "Enter", "ALT-5"] {
            assert!(Shortcut::is_allowed(allowed), "{allowed}");
        }
        for refused in ["", "cmd-k", "cmd-shift-enter", "shift-enter", "alt-0", "alt-10", "alt-01", "alt-a", "alt-enter", "cmd-1", "1", "cmd-shift-c"] {
            assert!(!Shortcut::is_allowed(refused), "{refused}");
        }
        assert_eq!(Shortcut::option(3).as_deref(), Some("alt-3"));
        assert_eq!(Shortcut::option(0), None);
        assert_eq!(Shortcut::option(10), None);
    }

    #[test]
    fn an_action_from_a_plugin_without_styles_is_normal() {
        let json = r#"{"id":"copy","label":"Copy","shortcut":"ClickOnly"}"#;
        let payload = embedded_gpui::Payload::from_parts(json.into(), Vec::new());
        assert_eq!(decode::<Action>(&payload).unwrap().style, ActionStyle::Normal);
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
            ["detect", "open_tool", "settings_sections", "open_settings_section", "open_window_view"]
        );
        assert_eq!(
            methods(ToolApi::schema()),
            ["on_input_changed", "list_actions", "perform_action", "focus_lost", "visibility_changed"]
        );
        assert_eq!(
            methods(HostApi::schema()),
            ["toast", "hide", "remember_input", "current_theme", "clipboard", "http", "open_url", "dns", "commands", "secret", "set_secret", "set_launcher_input", "settings", "set_settings", "utc_offset_seconds", "show_settings", "open_window", "show_window", "close_window", "confirm", "request_permission", "pick_folders", "save_file"]
        );
    }
}
