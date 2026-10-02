//! The theme, as the kit reads it. It is the app's (`delight_protocol::Theme`, which defines the
//! light and dark ones): a GPUI global the app sets (and, in a plugin, the plugin API). The kit
//! defines no colours or sizes: it follows that global, keeping it with its colours as GPUI's
//! `Hsla`, read with [`ActiveTheme::theme`] (`cx.theme()`).

use gpui::{App, Global, Hsla, Pixels, SharedString, px};

use delight_protocol::Color;

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
    /// Controls: buttons, switches, keycaps.
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
    /// The body text size.
    pub text_size: Pixels,
    /// The corner radius.
    pub radius: Pixels,
    hover: Hsla,
    separator: Hsla,
    selection: Hsla,
    focus_ring: Hsla,
    attention: Hsla,
    tint_opacity: f32,
    card: Hsla,
    background: Hsla,
    window: Hsla,
    syntax: Syntax,
    text_size_small: Pixels,
    text_size_large: Pixels,
    mono_size: Pixels,
    radius_small: Pixels,
}

impl Theme {
    /// The app's light theme, as the kit reads it (for tests).
    pub fn light(mono_font: SharedString) -> Self {
        Theme::from(&delight_protocol::Theme::light(mono_font.to_string()))
    }

    /// The app's dark theme, as the kit reads it (for tests, and in a plugin until the app's has
    /// arrived).
    pub fn dark(mono_font: SharedString) -> Self {
        Theme::from(&delight_protocol::Theme::dark(mono_font.to_string()))
    }

    /// A card of rows on a page (System Settings' groups): a slight tint of the text colour over
    /// the page, with no border.
    pub fn card(&self) -> Hsla {
        self.card
    }

    /// Half the control fill: hovers, and the background of a group of rows.
    pub fn fill_subtle(&self) -> Hsla {
        self.hover
    }

    /// A hairline between rows.
    pub fn separator(&self) -> Hsla {
        self.separator
    }

    /// Selected text's background.
    pub fn selection(&self) -> Hsla {
        self.selection
    }

    /// The ring around a focused control.
    pub fn focus_ring(&self) -> Hsla {
        self.focus_ring
    }

    /// The colour of a button that needs attention (results gone stale): a pink, well apart from
    /// the accent blue.
    pub fn attention(&self) -> Hsla {
        self.attention
    }

    /// A tinted background for `color` (a status colour, or the accent).
    pub fn tint(&self, color: Hsla) -> Hsla {
        color.opacity(self.tint_opacity)
    }

    pub fn text_size_small(&self) -> Pixels {
        self.text_size_small
    }

    pub fn text_size_large(&self) -> Pixels {
        self.text_size_large
    }

    /// Code is a pixel smaller than text, as monospace looks larger.
    pub fn mono_size(&self) -> Pixels {
        self.mono_size
    }

    pub fn radius_small(&self) -> Pixels {
        self.radius_small
    }

    /// The launcher's background around the tool view (the input, the tool list, the footer).
    pub fn window_tint(&self) -> Hsla {
        self.window
    }

    /// The tool view's background, a step lighter than [`Theme::window_tint`] around it.
    pub fn tool_background(&self) -> Hsla {
        self.background
    }

    /// Colours for highlighted code.
    pub fn syntax(&self) -> Syntax {
        self.syntax
    }
}

/// Colours for highlighted code ([`Theme::syntax`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Syntax {
    /// Keys: a JSON object's, a YAML mapping's.
    pub property: Hsla,
    pub string: Hsla,
    pub number: Hsla,
    /// `true`, `null`, escapes.
    pub constant: Hsla,
    pub comment: Hsla,
    pub type_: Hsla,
    pub keyword: Hsla,
    pub punctuation: Hsla,
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

/// Lilex 2.700 (SIL Open Font License 1.1, `assets/fonts/lilex/OFL.txt`). Only the
/// app's launcher input uses it, so plugins don't carry it.
#[cfg(not(target_arch = "wasm32"))]
const LILEX: &[u8] = include_bytes!("../assets/fonts/lilex/Lilex-Regular.ttf");

/// The launcher input's font: Lilex where it loaded.
struct InputFont(SharedString);

impl Global for InputFont {}

/// In the app: load the bundled font, and follow the app's theme.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn init(cx: &mut App) {
    if let Err(error) = cx.text_system().add_fonts(vec![std::borrow::Cow::Borrowed(LILEX)]) {
        log::warn!("loading the Lilex font failed: {error:#}");
    }
    let names = cx.text_system().all_font_names();
    let input = ["Lilex", "SF Mono", "Menlo"].into_iter().find(|font| names.iter().any(|name| name == font)).unwrap_or("Menlo");
    cx.set_global(InputFont(input.into()));
    follow(cx);
}

/// In a plugin: follow the app's theme, which the plugin API keeps; the dark one until the app
/// has answered. The plugin's inputs use its mono font. Once is enough.
pub(crate) fn init_plugin(cx: &mut App) {
    if cx.has_global::<Theme>() {
        return;
    }
    follow(cx);
}

/// Keep the kit's copy of the theme global (`delight_protocol::Theme`) as it changes, and draw
/// again when it does; until there is one, the dark theme.
fn follow(cx: &mut App) {
    let theme = cx.try_global::<delight_protocol::Theme>().map_or_else(|| Theme::dark("Menlo".into()), Theme::from);
    cx.set_global(theme);
    cx.observe_global::<delight_protocol::Theme>(|cx| {
        let theme = Theme::from(cx.global::<delight_protocol::Theme>());
        if cx.global::<Theme>() != &theme {
            cx.set_global(theme);
            cx.refresh_windows();
        }
    })
    .detach();
}

impl From<&delight_protocol::Theme> for Theme {
    fn from(theme: &delight_protocol::Theme) -> Self {
        let color = |color: Color| Hsla::from(color);
        let syntax = &theme.syntax;
        Theme {
            dark: theme.dark,
            text: color(theme.text),
            text_muted: color(theme.text_muted),
            text_faint: color(theme.text_faint),
            surface: color(theme.surface),
            fill: color(theme.fill),
            border: color(theme.border),
            accent: color(theme.accent),
            accent_text: color(theme.accent_text),
            success: color(theme.success),
            warning: color(theme.warning),
            error: color(theme.error),
            font: theme.font.clone().into(),
            mono_font: theme.mono_font.clone().into(),
            text_size: px(theme.text_size),
            radius: px(theme.radius),
            hover: color(theme.hover),
            separator: color(theme.separator),
            selection: color(theme.selection),
            focus_ring: color(theme.focus_ring),
            attention: color(theme.attention),
            tint_opacity: theme.tint_opacity,
            card: color(theme.card),
            background: color(theme.background),
            window: color(theme.window),
            syntax: Syntax {
                property: color(syntax.property),
                string: color(syntax.string),
                number: color(syntax.number),
                constant: color(syntax.constant),
                comment: color(syntax.comment),
                type_: color(syntax.type_),
                keyword: color(syntax.keyword),
                punctuation: color(syntax.punctuation),
            },
            text_size_small: px(theme.text_size_small),
            text_size_large: px(theme.text_size_large),
            mono_size: px(theme.mono_size),
            radius_small: px(theme.radius_small),
        }
    }
}

/// The launcher input's font: Lilex, or a monospace fallback if it didn't load; in a plugin, its
/// mono font.
pub fn input_font(cx: &App) -> SharedString {
    cx.try_global::<InputFont>().map_or_else(|| cx.theme().mono_font.clone(), |font| font.0.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_apps_theme_as_it_is() {
        for theme in [delight_protocol::Theme::light("Menlo"), delight_protocol::Theme::dark("SF Mono")] {
            let read = Theme::from(&theme);
            assert_eq!((read.dark, read.text, read.card()), (theme.dark, theme.text.into(), theme.card.into()));
            assert_eq!(read.tint(read.accent), Hsla::from(theme.tint(theme.accent)));
            assert_eq!((read.mono_font.as_ref(), read.text_size_small()), (theme.mono_font.as_str(), px(theme.text_size_small)));
        }
    }
}
