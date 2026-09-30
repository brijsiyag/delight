//! What a Delight plugin is written with.
//!
//! A plugin is a `cdylib` crate built for `wasm32-wasip2`. Its type implements
//! [`Plugin`] and carries [`#[plugin(...)]`](plugin), its operations are an enum with
//! [`#[derive(Operations)]`](derive@Operations), and each of its tools is a GPUI view
//! implementing [`Tool`], whose footer actions are an enum with
//! [`#[derive(Actions)]`](derive@Actions). It reaches the app through [`host`]. GPUI is the one
//! re-exported here, so a plugin builds against the same GPUI as the app.
//!
//! ```ignore
//! use delight_plugin_api::gpui::{App, AppContext as _};
//! use delight_plugin_api::{AnyTool, Detection, Input, Operations, Plugin, plugin};
//!
//! #[plugin(id = "dev.delight.json", name = "JSON", icon = "assets/icon.svg")]
//! struct Json;
//!
//! #[derive(Operations)]
//! enum JsonOperation {
//!     #[operation(id = "format", title = "Format JSON")]
//!     Format,
//! }
//!
//! impl Plugin for Json {
//!     type Operation = JsonOperation;
//!     fn new(_cx: &mut App) -> Self { Json }
//!     fn detect(&mut self, input: &Input, _cx: &mut App) -> Vec<Detection<JsonOperation>> {
//!         vec![Detection::new(JsonOperation::Format, 0.9)]
//!     }
//!     fn open_tool(&mut self, operation: JsonOperation, cx: &mut App) -> AnyTool {
//!         match operation {
//!             JsonOperation::Format => cx.new(|cx| Formatter::new(cx)).into(),
//!         }
//!     }
//! }
//! ```
//!
//! The properties and operations are checked at compile time and stored in the
//! `.wasm`, where the app reads them without running the plugin; the version is the
//! crate's own. Natively all of this compiles and nothing crosses to an app, so a
//! plugin's unit tests run on the Mac.

#[cfg(target_arch = "wasm32")]
mod glue;
mod host;
/// `Host::run`: running the programs the manifest lists.
mod commands;
/// `Host::dns_resolvers`: the Mac's DNS setup.
mod dns;
// TEMPORARY(open_url): `Host::open_url`, until embedded_gpui forwards GPUI's own; README,
// "Temporary host APIs".
mod open_url;
// TEMPORARY(network): HTTP, callbacks and gRPC through the app, until embedded_gpui links
// `wasi:http`; README, "Temporary host APIs".
pub mod network;
mod tool;

pub use delight_plugin_api_macros::{Actions, Operations, plugin};
pub use delight_protocol::{ActionStyle, Color, Command, CommandOutput, DnsResolver, Input, Theme};
pub use embedded_gpui::gpui;
/// The `http` crate the network's requests and responses are made of.
// TEMPORARY(network)
pub use http;
/// tonic, for gRPC clients over the app's HTTP (the `grpc` feature).
// TEMPORARY(network)
#[cfg(feature = "grpc")]
pub use tonic;
pub use host::{Confirm, Host, WindowOptions, host, settings_changed, theme};
pub use tool::AnyTool;

use gpui::{AnyView, App, AssetSource, Context, Render};

/// A plugin. Its type also carries [`#[plugin(...)]`](plugin), which makes it the
/// component's entry point.
pub trait Plugin: Sized + 'static {
    /// The plugin's operations: an enum with `#[derive(Operations)]`.
    type Operation: Operations;

    /// Build the plugin, once, when the app starts it.
    fn new(cx: &mut App) -> Self;

    /// Which of this plugin's operations fit `input`, and how well. It runs on every
    /// change of the input, so keep it quick.
    fn detect(&mut self, input: &Input, cx: &mut App) -> Vec<Detection<Self::Operation>>;

    /// Build the tool for `operation`: `cx.new(|cx| MyTool::new(cx)).into()`. The app
    /// opens each tool once and keeps it.
    fn open_tool(&mut self, operation: Self::Operation, cx: &mut App) -> AnyTool;

    /// The sections of the plugin's settings, in order. Each is shown in Delight's
    /// settings window as a titled card, like the app's own (`Permissions`, `Tips`), with
    /// the section's view inside. Empty by default, for a plugin without settings.
    ///
    /// The app asks whenever the plugin says its sections changed
    /// ([`settings_changed`]), and opens a section's view each time its page is shown, so
    /// keep what a section shows in an entity the plugin holds (or a global) rather than
    /// in a view made here. The plugin keeps what it saves itself, such as in its data
    /// folder (`/data`) or as secrets.
    fn settings_sections(&mut self, cx: &mut App) -> Vec<SettingsSection> {
        let _ = cx;
        Vec::new()
    }

    /// Files the plugin's views load by path, such as the SVG icons GPUI's `svg()`
    /// draws. None by default.
    fn assets() -> Option<Box<dyn AssetSource>> {
        None
    }
}

/// One section of a plugin's settings: a titled card, with a view inside.
pub struct SettingsSection {
    pub id: &'static str,
    /// Above the card.
    pub title: String,
    /// What is inside the card: draw rows with `delight_ui::rows` and `delight_ui::row`.
    pub view: AnyView,
    /// How tall the view is, in pixels: the app can't measure a plugin's view. With
    /// `delight_ui`'s rows it is their count times `ROW_HEIGHT` (or `ROW_DETAIL_HEIGHT`).
    pub height: f32,
    /// A note in small text under the card; empty for none.
    pub footer: String,
}

impl SettingsSection {
    pub fn new(id: &'static str, title: impl Into<String>, height: f32, view: impl Into<AnyView>) -> Self {
        SettingsSection { id, title: title.into(), view: view.into(), height, footer: String::new() }
    }

    /// A note in small text under the card.
    pub fn footer(mut self, footer: impl Into<String>) -> Self {
        self.footer = footer.into();
        self
    }
}

/// A plugin's operations (its tools). Derive it for a fieldless enum, one variant per
/// operation, each with `#[operation(id = "…", title = "…")]`; see
/// [`derive@Operations`].
pub trait Operations: Sized + 'static {
    /// The operations as the plugin's custom section stores them.
    #[doc(hidden)]
    const ENCODED: &'static [u8];

    /// This operation's id, as the app knows it.
    fn id(&self) -> &'static str;

    /// The operation with this id, if the plugin has one.
    fn from_id(id: &str) -> Option<Self>;
}

/// One operation that fits an input, and how well: 0 (not at all) to 1.
pub struct Detection<O> {
    pub operation: O,
    pub confidence: f32,
}

impl<O> Detection<O> {
    pub fn new(operation: O, confidence: f32) -> Self {
        Detection {
            operation,
            confidence,
        }
    }
}

/// One tool: a GPUI view the app shows in its tool pane, plus what the launcher
/// asks of it.
pub trait Tool: Render {
    /// Its footer actions: an enum with `#[derive(Actions)]`.
    type Action: Actions;

    /// The launcher's input changed (or the tool just opened): the app telling the
    /// tool what it now is.
    fn on_input_changed(&mut self, input: &Input, cx: &mut Context<Self>);

    /// The footer actions it offers now, in order. Call `cx.notify()` when they
    /// change; the app asks again then.
    fn list_actions(&self, cx: &App) -> Vec<Action<Self::Action>>;

    /// Run `action`, one it offered, doing the work itself: copying with GPUI's
    /// `cx.write_to_clipboard`, toasting and hiding through [`host`].
    fn perform_action(&mut self, action: Self::Action, cx: &mut Context<Self>);

    /// The user clicked somewhere else in the launcher (its input, the footer, the list). A
    /// click there never reaches the tool's view, so this is how a menu it holds open closes.
    fn on_focus_lost(&mut self, _cx: &mut Context<Self>) {}
}

/// A tool's footer actions (its [`Tool::Action`]). Derive it for a fieldless enum, one
/// variant per action; see [`derive@Actions`].
pub trait Actions: Sized + 'static {
    /// This action's id, as the app hands it back: the variant's name.
    fn id(&self) -> &'static str;

    /// The action with this id, if the tool has one.
    fn from_id(id: &str) -> Option<Self>;
}

/// An action's key. Delight gives actions three kinds of key and no others, so every tool's keys
/// are the same ones and worth remembering: ↵, ⌘↵ and ⌥1 to ⌥9. An action states one (going without
/// a key is a choice, [`Shortcut::ClickOnly`]); two actions with the same key: the earlier one has it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shortcut {
    /// ↵: the main thing to do here.
    Enter,
    /// ⌘↵.
    CmdEnter,
    /// ⌥ and a digit, 1 to 9. Another number gives the action no key (it is only clicked).
    Option(u8),
    /// No key: the action is only clicked.
    ClickOnly,
}

impl From<Shortcut> for delight_protocol::Shortcut {
    fn from(shortcut: Shortcut) -> Self {
        use delight_protocol::Shortcut as Wire;
        match shortcut {
            Shortcut::Enter => Wire::Keystroke(Wire::ENTER.into()),
            Shortcut::CmdEnter => Wire::Keystroke(Wire::CMD_ENTER.into()),
            Shortcut::Option(digit) => match Wire::option(digit) {
                Some(keystroke) => Wire::Keystroke(keystroke),
                None => {
                    log::warn!("⌥{digit} isn't a key an action can have (⌥1 to ⌥9 are): it is only clicked");
                    Wire::ClickOnly
                }
            },
            Shortcut::ClickOnly => Wire::ClickOnly,
        }
    }
}

/// A footer action a tool offers: which one, its button's label, and its key.
pub struct Action<A> {
    pub id: A,
    pub label: String,
    pub shortcut: Shortcut,
    pub style: ActionStyle,
}

impl<A> Action<A> {
    /// An action with the footer's usual button.
    pub fn new(id: A, label: impl Into<String>, shortcut: Shortcut) -> Self {
        Self { id, label: label.into(), shortcut, style: ActionStyle::Normal }
    }

    /// The button is filled: the main thing to do here.
    pub fn primary(mut self) -> Self {
        self.style = ActionStyle::Primary;
        self
    }

    /// The button stands out: something needs doing first (stale results).
    pub fn attention(mut self) -> Self {
        self.style = ActionStyle::Attention;
        self
    }
}

/// What the macros' expansions use (only in wasm builds); not for plugins to use
/// directly.
#[cfg(target_arch = "wasm32")]
#[doc(hidden)]
pub mod __private {
    pub use crate::glue::{concat, start};
    pub use embedded_gpui;
}

#[cfg(test)]
mod tests {
    #[test]
    fn released_with_the_plugin_api() {
        assert_eq!(env!("CARGO_PKG_VERSION"), delight_protocol::PLUGIN_API_VERSION);
    }
}
