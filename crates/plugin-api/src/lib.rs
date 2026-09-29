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
// TEMPORARY(open_url): `Host::open_url`, until embedded_gpui forwards GPUI's own; README,
// "Temporary host APIs".
mod open_url;
// TEMPORARY(network): HTTP, callbacks and gRPC through the app, until embedded_gpui links
// `wasi:http`; README, "Temporary host APIs".
pub mod network;
mod tool;

pub use delight_plugin_api_macros::{Actions, Operations, plugin};
pub use delight_protocol::{Color, Input, Shortcut, Theme};
pub use embedded_gpui::gpui;
/// The `http` crate the network's requests and responses are made of.
// TEMPORARY(network)
pub use http;
/// tonic, for gRPC clients over the app's HTTP (the `grpc` feature).
// TEMPORARY(network)
#[cfg(feature = "grpc")]
pub use tonic;
pub use host::{Host, host, theme};
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

    /// The plugin's settings page, shown in its page of Delight's settings window.
    /// `None` by default, for a plugin without settings. It keeps what it saves
    /// itself, such as in its data folder (`/data`).
    fn settings_page(&mut self, cx: &mut App) -> Option<AnyView> {
        let _ = cx;
        None
    }

    /// Files the plugin's views load by path, such as the SVG icons GPUI's `svg()`
    /// draws. None by default.
    fn assets() -> Option<Box<dyn AssetSource>> {
        None
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
}

/// A tool's footer actions (its [`Tool::Action`]). Derive it for a fieldless enum, one
/// variant per action; see [`derive@Actions`].
pub trait Actions: Sized + 'static {
    /// This action's id, as the app hands it back: the variant's name.
    fn id(&self) -> &'static str;

    /// The action with this id, if the tool has one.
    fn from_id(id: &str) -> Option<Self>;
}

/// A footer action a tool offers: which one, its button's label, and its key. Every
/// action states a shortcut, so going without a key is a choice rather than an
/// oversight.
pub struct Action<A> {
    pub id: A,
    pub label: String,
    pub shortcut: Shortcut,
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
