//! What components share: the size scale, the `Sizable` / `Disableable` /
//! `Selectable` builder traits, and layout shorthands.

use gpui::{Div, Pixels, Styled, div, px};

/// Control sizes. `Medium` is the macOS default (26pt controls).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Size {
    Small,
    #[default]
    Medium,
    Large,
}

impl Size {
    /// Height of a button or single-line control.
    pub fn control_height(self) -> Pixels {
        match self {
            Size::Small => px(20.),
            Size::Medium => px(26.),
            Size::Large => px(32.),
        }
    }

    /// Label text inside a control.
    pub fn text_size(self) -> Pixels {
        match self {
            Size::Small => px(11.),
            Size::Medium => px(12.),
            Size::Large => px(13.),
        }
    }

    /// An icon inside a control.
    pub fn icon_size(self) -> Pixels {
        match self {
            Size::Small => px(12.),
            Size::Medium => px(14.),
            Size::Large => px(16.),
        }
    }

    /// Horizontal padding of a labelled control.
    pub fn padding_x(self) -> Pixels {
        match self {
            Size::Small => px(8.),
            Size::Medium => px(10.),
            Size::Large => px(12.),
        }
    }
}

pub trait Sizable: Sized {
    fn with_size(self, size: Size) -> Self;

    fn small(self) -> Self {
        self.with_size(Size::Small)
    }

    fn large(self) -> Self {
        self.with_size(Size::Large)
    }
}

/// A disabled control shows dimmed, ignores clicks and doesn't let them reach
/// its parents.
pub trait Disableable {
    fn disabled(self, disabled: bool) -> Self;
}

pub trait Selectable {
    fn selected(self, selected: bool) -> Self;
}

/// Text as one line: tabs and line breaks (and other control characters) become spaces, runs of
/// spaces one, the ends trimmed. For a title, a name or a label, which show on a single line: a line
/// break in the text would otherwise start a second one.
pub fn one_line(text: &str) -> String {
    text.split(|c: char| c.is_control() || c.is_whitespace()).filter(|word| !word.is_empty()).collect::<Vec<_>>().join(" ")
}

/// `text` cut to at most `max` characters, ending in "…" if it was longer. Text is cut before it is laid
/// out, not only when it is drawn: shaping and wrapping a very long string costs time on every frame
/// even when only its first lines show.
pub fn ellipsize(text: &str, max: usize) -> String {
    match text.char_indices().nth(max) {
        Some((cut, _)) => format!("{}…", text[..cut].trim_end()),
        None => text.to_string(),
    }
}

pub trait StyledExt: Styled + Sized {
    /// At most `lines` lines of text, the last ending in an ellipsis if there is more. Anything that
    /// spills out of the lines (stacked accents, say) is clipped.
    fn clamp_lines(self, lines: usize) -> Self {
        self.text_ellipsis().line_clamp(lines)
    }

    /// A row, children centred vertically.
    fn h_flex(self) -> Self {
        self.flex().flex_row().items_center()
    }

    /// A column.
    fn v_flex(self) -> Self {
        self.flex().flex_col()
    }
}

impl<E: Styled> StyledExt for E {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_text_is_cut_before_it_is_laid_out() {
        assert_eq!(ellipsize("short", 10), "short");
        assert_eq!(ellipsize("exactly ten", 11), "exactly ten");
        assert_eq!(ellipsize("abcdefghij", 4), "abcd…");
        assert_eq!(ellipsize("ab cdefghij", 3), "ab…", "no space before the ellipsis");
        assert_eq!(ellipsize("日本語のテスト", 3), "日本語…", "characters, not bytes");
        assert_eq!(ellipsize(&"x".repeat(100_000), 50).chars().count(), 51);
    }

    #[test]
    fn text_becomes_one_line() {
        assert_eq!(one_line("Line one\nLine two\r\n\tthree"), "Line one Line two three");
        assert_eq!(one_line("   padded   with   spaces  "), "padded with spaces");
        assert_eq!(one_line(""), "");
        assert_eq!(one_line("日本語 テスト"), "日本語 テスト");
    }
}

pub fn h_flex() -> Div {
    div().h_flex()
}

pub fn v_flex() -> Div {
    div().v_flex()
}
