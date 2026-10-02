//! Delight's UI kit: the icons and the bundled font, the components, and the text editor, drawn
//! with the app's theme. The kit only reads the theme (`delight_protocol::Theme`, which the app
//! sets as a GPUI global): it defines no colours of its own. The app and its built-in tools use it.
//!
//! Components are builders rendered with `RenderOnce`, reading the theme with
//! `cx.theme()`; controls are controlled (they report the new value, and the caller
//! owns it).
//!
//! Call [`init`] once at startup, and install [`Assets`] as the app's asset source.

mod assets;
mod badge;
mod button;
mod checkbox;
#[cfg(feature = "code")]
pub mod code;
#[cfg(feature = "code")]
pub mod conversion;
pub mod editor;
mod group;
mod icon;
mod keycap;
mod progress;
mod raster;
mod segmented;
mod styled;
mod switch;
pub mod theme;
mod tooltip;

pub use assets::{Assets, IconName, LOGO_SVG};
pub use badge::LogoBadge;
pub use button::{Button, ButtonVariant, IconButton};
pub use checkbox::Checkbox;
pub use editor::{EditorEvent, EditorFont, TextEditor};
pub use group::{Caption, Divider, Group, ROW_DETAIL_HEIGHT, ROW_HEIGHT, Rows, row, row_with, rows_height, section};
pub use icon::Icon;
pub use keycap::{Keycap, KeycapStyle, keystroke_for, keystroke_keys, keystroke_label, modifier_keys};
pub use progress::progress_bar;
pub use raster::render_image;
pub use segmented::SegmentedControl;
pub use styled::{Disableable, Selectable, Sizable, Size, StyledExt, ellipsize, h_flex, one_line, v_flex};
pub use switch::Switch;
pub use theme::{ActiveTheme, Syntax, Theme};
pub use tooltip::Tooltip;

/// In the app: load the bundled font, and follow the app's theme (the app sets it).
#[cfg(not(target_arch = "wasm32"))]
pub fn init(cx: &mut gpui::App) {
    theme::init(cx);
}

/// In a plugin (a built-in tool): the theme is the app's, and follows it, and the
/// kit's keys are bound (the app's keymap doesn't reach a plugin). Call it once,
/// before drawing, and install [`Assets`] as the plugin's assets.
pub fn init_plugin(cx: &mut gpui::App) {
    theme::init_plugin(cx);
    cx.bind_keys(key_bindings());
}

/// The kit's own keys, for the app's keymap and each plugin's: text editing in an input,
/// and ← and → in a segmented control.
pub fn key_bindings() -> Vec<gpui::KeyBinding> {
    let mut bindings: Vec<gpui::KeyBinding> = editor::key_bindings();
    bindings.extend(segmented::key_bindings());
    bindings
}
