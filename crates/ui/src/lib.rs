//! Delight's UI kit: the theme, the icons and the bundled font, the components, and
//! the text editor. The app uses it now; the built-in tools will too.
//!
//! Components are builders rendered with `RenderOnce`, reading the theme with
//! `cx.theme()`; controls are controlled (they report the new value, and the caller
//! owns it).
//!
//! Call [`init`] once at startup, and install [`Assets`] as the app's asset source.

mod assets;
mod badge;
mod button;
pub mod editor;
mod group;
mod icon;
mod keycap;
mod raster;
mod segmented;
mod styled;
mod switch;
pub mod theme;
mod tooltip;

pub use assets::{Assets, IconName, LOGO_SVG};
pub use badge::LogoBadge;
pub use button::{Button, ButtonVariant, IconButton};
pub use editor::{EditorEvent, EditorFont, TextEditor};
pub use group::{Caption, Divider, Group};
pub use icon::Icon;
pub use keycap::{Keycap, KeycapStyle, keystroke_for, keystroke_keys, keystroke_label, modifier_keys};
pub use raster::render_image;
pub use segmented::SegmentedControl;
pub use styled::{Disableable, Selectable, Sizable, Size, StyledExt, h_flex, v_flex};
pub use switch::Switch;
pub use theme::{ActiveTheme, Theme, ThemeMode};
pub use tooltip::Tooltip;

/// In the app: load the bundled font and resolve the theme.
#[cfg(not(target_arch = "wasm32"))]
pub fn init(cx: &mut gpui::App, mode: ThemeMode) {
    theme::init(cx, mode);
}

/// In a plugin (a built-in tool): the theme is the app's, and follows it. Call it
/// once, before drawing, and install [`Assets`] as the plugin's assets.
pub fn init_plugin(cx: &mut gpui::App) {
    theme::init_plugin(cx);
}
