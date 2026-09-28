//! What a Delight plugin is written with.
//!
//! A plugin is a `cdylib` crate built for `wasm32-wasip2`. Its type implements
//! [`Plugin`] and carries [`#[plugin(...)]`](plugin), its operations are an enum with
//! [`#[derive(Operations)]`](derive@Operations), and each of its tools is a GPUI view
//! implementing [`Tool`]. It reaches the app through [`host`]. GPUI is the one
//! re-exported here, so a plugin builds against the same GPUI as the app.
//!
//! ```ignore
//! use delight_plugin_api::gpui::{App, AppContext as _, Window};
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
//!     fn open_tool(&mut self, operation: JsonOperation, _: &mut Window, cx: &mut App) -> AnyTool {
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
mod tool;

pub use delight_plugin_api_macros::{Operations, plugin};
pub use delight_protocol::{Action, Input, Shortcut};
pub use embedded_gpui::gpui;
pub use host::{Host, host};
pub use tool::AnyTool;

use gpui::{App, Context, Render, Window};

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

    /// Build the tool for `operation` in the window it will draw in:
    /// `cx.new(|cx| MyTool::new(cx)).into()`. The app opens each tool once and keeps
    /// it.
    fn open_tool(&mut self, operation: Self::Operation, window: &mut Window, cx: &mut App)
    -> AnyTool;
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
    /// The launcher's input changed (or the tool just opened): the app telling the
    /// tool what it now is.
    fn on_input_changed(&mut self, input: &Input, cx: &mut Context<Self>);

    /// The footer actions, in order. Call `cx.notify()` when they change; the app
    /// asks again then.
    fn list_actions(&self, cx: &App) -> Vec<Action>;

    /// Run the action with this id, doing the work through [`host`] (copying,
    /// toasting, hiding).
    fn perform_action(&mut self, action: &str, cx: &mut Context<Self>);
}

/// What the macros' expansions use (only in wasm builds); not for plugins to use
/// directly.
#[cfg(target_arch = "wasm32")]
#[doc(hidden)]
pub mod __private {
    pub use crate::glue::{concat, start};
    pub use embedded_gpui;
}
