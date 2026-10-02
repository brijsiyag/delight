//! The look the app and every plugin draw with: a light theme and a dark one, macOS's system
//! colours (HIG) and Xcode's for code. They are defined here, once: the app picks one (its
//! Appearance setting, or macOS's appearance) and hands it to every plugin, and Delight's UI kit
//! only reads it.

use embedded_gpui::data;
use embedded_gpui::gpui::{Hsla, hsla, rgb};

/// The app's look, light or dark: every colour and size the app and plugins draw with.
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
    /// Raised areas: fields, tooltips, popovers.
    pub surface: Color,
    /// Controls: buttons, switches, keycaps.
    pub fill: Color,
    /// Half the control fill: a hover, the background of a group of rows.
    pub hover: Color,
    pub border: Color,
    /// A hairline between rows: a lighter border.
    pub separator: Color,
    /// Selection, focus and primary actions.
    pub accent: Color,
    /// Text on the accent.
    pub accent_text: Color,
    /// Selected text's background.
    pub selection: Color,
    /// The ring around a focused control.
    pub focus_ring: Color,
    pub success: Color,
    pub warning: Color,
    pub error: Color,
    /// A button that needs attention (results gone stale): a pink, well apart from the accent.
    pub attention: Color,
    /// How strong a colour's tint is (a quiet background of a status colour or the accent): the
    /// colour at this opacity ([`Theme::tint`]).
    pub tint_opacity: f32,
    /// A card of rows on a page, as System Settings' groups: a slight tint of the text colour.
    pub card: Color,
    /// What a tool's view is drawn on: the launcher's pane, a step lighter than its window.
    pub background: Color,
    /// The launcher around the tool's view: its input, its list, its footer.
    pub window: Color,
    /// Highlighted code.
    pub syntax: Syntax,
    /// Font families: for interface text, and for code.
    pub font: String,
    pub mono_font: String,
    /// The body text size, in pixels; the small and large ones are 2 px either side.
    pub text_size: f32,
    pub text_size_small: f32,
    pub text_size_large: f32,
    /// Code's size: a pixel smaller than text, as monospace looks larger.
    pub mono_size: f32,
    /// The corner radius, in pixels, and small elements' (2 px less).
    pub radius: f32,
    pub radius_small: f32,
}

/// Colours for highlighted code ([`Theme::syntax`]): Xcode's.
#[data]
#[derive(Copy, PartialEq)]
pub struct Syntax {
    /// Keys: a JSON object's, a YAML mapping's.
    pub property: Color,
    pub string: Color,
    pub number: Color,
    /// `true`, `null`, escapes.
    pub constant: Color,
    pub comment: Color,
    pub type_: Color,
    pub keyword: Color,
    pub punctuation: Color,
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

impl Theme {
    /// The light theme, with `mono_font` for code (the app picks an installed one).
    pub fn light(mono_font: impl Into<String>) -> Self {
        let (fill, border, accent) = (gray(0., 0.08), gray(0., 0.15), hex(0x007AFF));
        Theme {
            dark: false,
            text: gray(0., 0.85),
            text_muted: gray(0., 0.5),
            text_faint: gray(0., 0.26),
            surface: gray(0.99, 1.),
            fill,
            hover: faded(fill, 0.5),
            border,
            separator: faded(border, 0.66),
            accent,
            accent_text: gray(1., 1.),
            selection: faded(accent, 0.25),
            focus_ring: faded(accent, 0.5),
            success: hex(0x28A745),
            warning: hex(0xFF9500),
            error: hex(0xFF3B30),
            attention: hex(0xE0397F),
            tint_opacity: 0.12,
            card: gray(0., 0.04),
            background: gray(1., 1.),
            window: gray(0.955, 1.),
            syntax: Syntax {
                property: hex(0x0B4F79),
                string: hex(0xC41A16),
                number: hex(0x1C00CF),
                constant: hex(0x9B2393),
                comment: hex(0x5D6C79),
                type_: hex(0x3900A0),
                keyword: hex(0x9B2393),
                punctuation: gray(0., 0.5),
            },
            font: ".SystemUIFont".into(),
            mono_font: mono_font.into(),
            text_size: 13.,
            text_size_small: 11.,
            text_size_large: 15.,
            mono_size: 12.,
            radius: 8.,
            radius_small: 6.,
        }
    }

    /// The dark theme, with `mono_font` for code.
    pub fn dark(mono_font: impl Into<String>) -> Self {
        let (fill, border, accent) = (gray(1., 0.1), gray(1., 0.15), hex(0x0A84FF));
        Theme {
            dark: true,
            text: gray(1., 0.88),
            text_muted: gray(1., 0.55),
            text_faint: gray(1., 0.28),
            surface: gray(0.18, 1.),
            fill,
            hover: faded(fill, 0.5),
            border,
            separator: faded(border, 0.66),
            accent,
            accent_text: gray(1., 1.),
            selection: faded(accent, 0.35),
            focus_ring: faded(accent, 0.5),
            success: hex(0x30D158),
            warning: hex(0xFF9F0A),
            error: hex(0xFF453A),
            attention: hex(0xFF6AA2),
            tint_opacity: 0.18,
            card: gray(1., 0.07),
            background: gray(0.155, 1.),
            window: gray(0.115, 1.),
            syntax: Syntax {
                property: hex(0x67B7A4),
                string: hex(0xFC6A5D),
                number: hex(0xD0BF69),
                constant: hex(0xFC5FA3),
                comment: hex(0x6C7986),
                type_: hex(0x5DD8FF),
                keyword: hex(0xFC5FA3),
                punctuation: gray(1., 0.55),
            },
            font: ".SystemUIFont".into(),
            mono_font: mono_font.into(),
            text_size: 13.,
            text_size_small: 11.,
            text_size_large: 15.,
            mono_size: 12.,
            radius: 8.,
            radius_small: 6.,
        }
    }

    /// A quiet background of `color` (a status colour, the accent): its tint.
    pub fn tint(&self, color: Color) -> Color {
        faded(color, self.tint_opacity)
    }
}

fn gray(lightness: f32, alpha: f32) -> Color {
    Color { h: 0., s: 0., l: lightness, a: alpha }
}

fn hex(hex: u32) -> Color {
    Hsla::from(rgb(hex)).into()
}

/// `color`, `factor` as opaque.
fn faded(color: Color, factor: f32) -> Color {
    Color { a: color.a * factor, ..color }
}

/// In the app and in a plugin, the theme is a GPUI global, so what draws with it can follow it
/// (`cx.observe_global::<Theme>()`).
impl embedded_gpui::gpui::Global for Theme {}

impl From<Color> for Hsla {
    fn from(color: Color) -> Self {
        hsla(color.h, color.s, color.l, color.a)
    }
}

impl From<Hsla> for Color {
    fn from(color: Hsla) -> Self {
        Color { h: color.h, s: color.s, l: color.l, a: color.a }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use embedded_gpui::{decode, encode};

    fn colours(theme: &Theme) -> Vec<Color> {
        let s = theme.syntax;
        vec![
            theme.text, theme.text_muted, theme.text_faint, theme.surface, theme.fill, theme.hover, theme.border,
            theme.separator, theme.accent, theme.accent_text, theme.selection, theme.focus_ring, theme.success,
            theme.warning, theme.error, theme.attention, theme.card, theme.background, theme.window, s.property,
            s.string, s.number, s.constant, s.comment, s.type_, s.keyword, s.punctuation,
        ]
    }

    /// Every colour has a real value in both themes: an unset one would be transparent.
    #[test]
    fn both_themes_set_every_colour() {
        for theme in [Theme::light("Menlo"), Theme::dark("Menlo")] {
            for (index, colour) in colours(&theme).iter().enumerate() {
                assert!(colour.a > 0., "colour {index} is unset (dark: {})", theme.dark);
            }
            assert!(theme.tint_opacity > 0. && theme.text_size_small > 0. && theme.radius_small > 0.);
        }
    }

    #[test]
    fn the_launcher_is_solid_with_the_tool_view_a_step_lighter() {
        for theme in [Theme::light("Menlo"), Theme::dark("Menlo")] {
            assert_eq!((theme.window.a, theme.background.a), (1., 1.), "dark: {}", theme.dark);
            assert!(theme.background.l > theme.window.l, "the tool view is lighter (dark: {})", theme.dark);
        }
    }

    #[test]
    fn crosses_to_plugins_unchanged() {
        for theme in [Theme::light("SF Mono"), Theme::dark("SF Mono")] {
            assert_eq!(decode::<Theme>(&encode(&theme).unwrap()).unwrap(), theme);
        }
    }
}
