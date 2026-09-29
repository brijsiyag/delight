//! [`Keycap`]: a `⌘1`-style shortcut hint, and how macOS writes keys.

use gpui::{Action, App, FontWeight, IntoElement, Keystroke, Modifiers, ParentElement, RenderOnce, SharedString, Styled, Window, div, px};

use crate::ActiveTheme;

/// Where the keycap sits, which decides its colours.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum KeycapStyle {
    /// On ordinary content.
    #[default]
    Plain,
    /// On the accent colour (a selected row, a primary button).
    OnAccent,
    /// Tinted with the accent colour (a text button).
    Accent,
    /// A solid accent chip: the key of a button that is the main thing to do.
    Solid,
    /// Tinted with the attention colour.
    Attention,
}

#[derive(IntoElement)]
pub struct Keycap {
    label: SharedString,
    style: KeycapStyle,
}

impl Keycap {
    pub fn new(label: impl Into<SharedString>) -> Self {
        Self { label: label.into(), style: KeycapStyle::Plain }
    }

    pub fn style(mut self, style: KeycapStyle) -> Self {
        self.style = style;
        self
    }
}

impl RenderOnce for Keycap {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = cx.theme();
        let (bg, fg) = match self.style {
            KeycapStyle::Plain => (t.fill, t.text_muted),
            KeycapStyle::OnAccent => (t.accent_text.opacity(0.22), t.accent_text.opacity(0.9)),
            KeycapStyle::Accent => (t.tint(t.accent), t.accent),
            KeycapStyle::Solid => (t.accent, t.accent_text),
            KeycapStyle::Attention => (t.tint(t.attention()), t.attention()),
        };
        div()
            .flex_shrink_0()
            .px(px(5.))
            .h(px(18.))
            .min_w(px(18.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(4.))
            .bg(bg)
            .text_color(fg)
            .text_size(px(11.))
            .font_weight(FontWeight::MEDIUM)
            .child(self.label)
    }
}

/// How macOS writes a keystroke, one keycap per key: `cmd-shift-enter` →
/// `["⇧", "⌘", "↵"]`.
pub fn keystroke_keys(keystroke: &Keystroke) -> Vec<SharedString> {
    let mut keys = modifier_keys(&keystroke.modifiers);
    let key = match keystroke.key.as_str() {
        "enter" => "↵".into(),
        "backspace" => "⌫".into(),
        "delete" => "⌦".into(),
        "escape" => "⎋".into(),
        "tab" => "⇥".into(),
        "space" => "Space".into(),
        "up" => "↑".into(),
        "down" => "↓".into(),
        "left" => "←".into(),
        "right" => "→".into(),
        other => other.to_uppercase(),
    };
    keys.push(key.into());
    keys
}

/// The modifiers in macOS's order: `["⌃", "⌥", "⇧", "⌘"]`.
pub fn modifier_keys(m: &Modifiers) -> Vec<SharedString> {
    let modifiers = [(m.control, "⌃"), (m.alt, "⌥"), (m.shift, "⇧"), (m.platform, "⌘")];
    modifiers.iter().filter(|(on, _)| *on).map(|(_, symbol)| (*symbol).into()).collect()
}

/// A keystroke as one label, e.g. `⇧⌘↵`.
pub fn keystroke_label(keystroke: &Keystroke) -> String {
    keystroke_keys(keystroke).concat()
}

/// The keystroke the keymap binds to `action` where the focus is now (the
/// user's binding over the default), e.g. for a button's shortcut hint.
pub fn keystroke_for(action: &dyn Action, window: &Window) -> Option<Keystroke> {
    let binding = window.highest_precedence_binding_for_action(action)?;
    // Hints show single keystrokes only.
    match binding.keystrokes() {
        [keystroke] => Some(keystroke.inner().clone()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_keys_as_macos_does() {
        let label = |s: &str| keystroke_label(&Keystroke::parse(s).unwrap());
        assert_eq!(label("cmd-shift-space"), "⇧⌘Space");
        assert_eq!(label("ctrl-alt-1"), "⌃⌥1");
        assert_eq!(label("cmd-shift-backspace"), "⇧⌘⌫");
        assert_eq!(label("alt-k"), "⌥K");
    }
}
