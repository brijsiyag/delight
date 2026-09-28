//! The look: macOS system colours (HIG), the system font and SF Mono, in light and
//! dark.
//!
//! [`Theme`] is deliberately small, so it can be handed to plugins as it is: a few
//! colours and sizes, with everything else derived from them by its methods. It is a
//! GPUI global, read with [`ActiveTheme::theme`] (`cx.theme()`), and resolved again
//! only when the [`ThemeMode`] or the macOS appearance changes.

use gpui::{App, Global, Hsla, Pixels, SharedString, WindowAppearance, hsla, px, rgb};

/// Light, dark, or following macOS.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThemeMode {
    #[default]
    System,
    Dark,
    Light,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    pub dark: bool,
    /// Body text.
    pub text: Hsla,
    /// Secondary text: captions, descriptions.
    pub text_muted: Hsla,
    /// The quietest text: placeholders, hints.
    pub text_faint: Hsla,
    /// Raised areas: tooltips, popovers.
    pub surface: Hsla,
    /// Controls: buttons, switches, keycaps. Half of it is a hover or a group.
    pub fill: Hsla,
    /// Borders; separators are a lighter version.
    pub border: Hsla,
    /// Selection, focus and primary actions.
    pub accent: Hsla,
    /// Text on the accent.
    pub accent_text: Hsla,
    pub success: Hsla,
    pub warning: Hsla,
    pub error: Hsla,
    pub font: SharedString,
    pub mono_font: SharedString,
    /// The body text size; the small and large sizes are 2px either side.
    pub text_size: Pixels,
    /// The corner radius; small elements use 2px less.
    pub radius: Pixels,
}

impl Theme {
    pub fn light(mono_font: SharedString) -> Self {
        Theme {
            dark: false,
            text: gray(0., 0.85),
            text_muted: gray(0., 0.5),
            text_faint: gray(0., 0.26),
            surface: gray(0.99, 1.),
            fill: gray(0., 0.08),
            border: gray(0., 0.15),
            accent: color(0x007AFF),
            accent_text: gray(1., 1.),
            success: color(0x28A745),
            warning: color(0xFF9500),
            error: color(0xFF3B30),
            font: ".SystemUIFont".into(),
            mono_font,
            text_size: px(13.),
            radius: px(8.),
        }
    }

    pub fn dark(mono_font: SharedString) -> Self {
        Theme {
            dark: true,
            text: gray(1., 0.88),
            text_muted: gray(1., 0.55),
            text_faint: gray(1., 0.28),
            surface: gray(0.18, 1.),
            fill: gray(1., 0.1),
            border: gray(1., 0.15),
            accent: color(0x0A84FF),
            accent_text: gray(1., 1.),
            success: color(0x30D158),
            warning: color(0xFF9F0A),
            error: color(0xFF453A),
            font: ".SystemUIFont".into(),
            mono_font,
            text_size: px(13.),
            radius: px(8.),
        }
    }

    /// Half the control fill: hovers, and the background of a group of rows.
    pub fn fill_subtle(&self) -> Hsla {
        self.fill.opacity(0.5)
    }

    /// A hairline between rows.
    pub fn separator(&self) -> Hsla {
        self.border.opacity(0.66)
    }

    /// Selected text's background.
    pub fn selection(&self) -> Hsla {
        self.accent.opacity(if self.dark { 0.35 } else { 0.25 })
    }

    /// The ring around a focused control.
    pub fn focus_ring(&self) -> Hsla {
        self.accent.opacity(0.5)
    }

    /// A tinted background for `color` (a status colour, or the accent).
    pub fn tint(&self, color: Hsla) -> Hsla {
        color.opacity(if self.dark { 0.18 } else { 0.12 })
    }

    pub fn text_size_small(&self) -> Pixels {
        self.text_size - px(2.)
    }

    pub fn text_size_large(&self) -> Pixels {
        self.text_size + px(2.)
    }

    /// Code is a pixel smaller than text, as monospace looks larger.
    pub fn mono_size(&self) -> Pixels {
        self.text_size - px(1.)
    }

    pub fn radius_small(&self) -> Pixels {
        self.radius - px(2.)
    }

    /// The launcher's background, painted over its native blur. Mostly opaque, so the
    /// contrast doesn't depend on what's behind the window.
    pub fn window_tint(&self) -> Hsla {
        if self.dark { gray(0.13, 0.88) } else { gray(0.97, 0.86) }
    }
}

fn color(hex: u32) -> Hsla {
    rgb(hex).into()
}

fn gray(lightness: f32, alpha: f32) -> Hsla {
    hsla(0., 0., lightness, alpha)
}

impl Global for Theme {}

/// `cx.theme()`: the current theme, in `Render` and `RenderOnce` alike.
pub trait ActiveTheme {
    fn theme(&self) -> &Theme;
}

impl ActiveTheme for App {
    fn theme(&self) -> &Theme {
        self.global::<Theme>()
    }
}

/// The launcher's input text: Lilex at 13px, on a 21px line.
pub const INPUT_FONT_SIZE: f32 = 13.;
pub const INPUT_LINE_HEIGHT: f32 = 21.;

/// Lilex 2.700 (SIL Open Font License 1.1, `assets/fonts/lilex/OFL.txt`).
const LILEX: &[u8] = include_bytes!("../assets/fonts/lilex/Lilex-Regular.ttf");

/// What the theme is resolved from.
struct Preference {
    mode: ThemeMode,
    mono_font: SharedString,
    input_font: SharedString,
}

impl Global for Preference {}

/// Load the bundled font, pick the installed fonts, and resolve the theme.
pub(crate) fn init(cx: &mut App, mode: ThemeMode) {
    if let Err(error) = cx.text_system().add_fonts(vec![std::borrow::Cow::Borrowed(LILEX)]) {
        log::warn!("loading the Lilex font failed: {error:#}");
    }
    let names = cx.text_system().all_font_names();
    let first = |fonts: &[&'static str]| -> SharedString {
        fonts
            .iter()
            .copied()
            .find(|font| names.iter().any(|name| name == font))
            .unwrap_or("Menlo")
            .into()
    };
    let mono_font = first(&["SF Mono", "Menlo", "Monaco"]);
    let input_font = first(&["Lilex", "SF Mono", "Menlo"]);
    cx.set_global(Preference {
        mode,
        mono_font,
        input_font,
    });
    resolve(cx);
}

/// The launcher input's font: Lilex, or a monospace fallback if it didn't load.
pub fn input_font(cx: &App) -> SharedString {
    cx.global::<Preference>().input_font.clone()
}

pub fn mode(cx: &App) -> ThemeMode {
    cx.global::<Preference>().mode
}

/// Switch between light, dark and following macOS, and redraw.
pub fn set_mode(cx: &mut App, mode: ThemeMode) {
    cx.global_mut::<Preference>().mode = mode;
    resolve(cx);
}

/// Resolve again after the macOS appearance changed; the app calls this from a
/// window's `observe_window_appearance`.
pub fn appearance_changed(cx: &mut App) {
    resolve(cx);
}

fn resolve(cx: &mut App) {
    let preference = cx.global::<Preference>();
    let dark = match preference.mode {
        ThemeMode::Dark => true,
        ThemeMode::Light => false,
        ThemeMode::System => matches!(
            cx.window_appearance(),
            WindowAppearance::Dark | WindowAppearance::VibrantDark
        ),
    };
    let mono_font = preference.mono_font.clone();
    let theme = if dark { Theme::dark(mono_font) } else { Theme::light(mono_font) };
    if cx.try_global::<Theme>() != Some(&theme) {
        cx.set_global(theme);
        cx.refresh_windows();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every colour has a real value in both appearances: an unset one would be
    /// transparent.
    #[test]
    fn both_themes_set_every_colour() {
        for theme in [Theme::light("Menlo".into()), Theme::dark("Menlo".into())] {
            let colours = [
                theme.text,
                theme.text_muted,
                theme.text_faint,
                theme.surface,
                theme.fill,
                theme.border,
                theme.accent,
                theme.accent_text,
                theme.success,
                theme.warning,
                theme.error,
            ];
            for (index, colour) in colours.iter().enumerate() {
                assert!(colour.a > 0., "colour {index} is unset (dark: {})", theme.dark);
            }
            assert!(theme.text_size_small() > px(0.) && theme.radius_small() > px(0.));
            assert!(!theme.font.is_empty() && !theme.mono_font.is_empty());
        }
    }
}
