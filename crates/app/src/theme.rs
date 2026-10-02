//! Which theme Delight shows: the light or the dark one (`delight_protocol::Theme`'s), as the
//! Appearance setting says, or as macOS's appearance is. The theme is a GPUI global: the UI kit
//! draws with it, and every plugin's host object hands it to its plugin when it changes.

use delight_protocol::Theme;
use gpui::{App, Global, WindowAppearance};

/// Light, dark, or following macOS.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThemeMode {
    #[default]
    System,
    Dark,
    Light,
}

/// What the theme is chosen from.
struct Preference {
    mode: ThemeMode,
    /// For code: the first of these installed.
    mono_font: String,
}

impl Global for Preference {}

/// Pick the fonts, and choose the theme for `mode`.
pub fn init(cx: &mut App, mode: ThemeMode) {
    let names = cx.text_system().all_font_names();
    let mono_font = ["SF Mono", "Menlo", "Monaco"]
        .into_iter()
        .find(|font| names.iter().any(|name| name == font))
        .unwrap_or("Menlo")
        .to_string();
    cx.set_global(Preference { mode, mono_font });
    choose(cx);
}

/// Switch between light, dark and following macOS, and redraw.
pub fn set_mode(cx: &mut App, mode: ThemeMode) {
    cx.global_mut::<Preference>().mode = mode;
    choose(cx);
}

/// Choose again after macOS's appearance changed: windows call this from
/// `observe_window_appearance`.
pub fn appearance_changed(cx: &mut App) {
    choose(cx);
}

fn choose(cx: &mut App) {
    let preference = cx.global::<Preference>();
    let dark = match preference.mode {
        ThemeMode::Dark => true,
        ThemeMode::Light => false,
        ThemeMode::System => matches!(cx.window_appearance(), WindowAppearance::Dark | WindowAppearance::VibrantDark),
    };
    let mono_font = preference.mono_font.clone();
    let theme = if dark { Theme::dark(mono_font) } else { Theme::light(mono_font) };
    if cx.try_global::<Theme>() != Some(&theme) {
        cx.set_global(theme);
        cx.refresh_windows();
    }
}
