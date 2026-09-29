//! [`Button`] (push, primary and borderless text buttons) and
//! [`IconButton`] (toolbar style).

use std::rc::Rc;

use gpui::{
    App, ClickEvent, ElementId, FontWeight, Hsla, InteractiveElement, IntoElement, MouseButton, ParentElement,
    RenderOnce, SharedString, StatefulInteractiveElement, Styled, Window, div, prelude::FluentBuilder, px,
};

use crate::{ActiveTheme, Disableable, Icon, IconName, Keycap, KeycapStyle, Selectable, Size, Sizable, Theme, Tooltip};

type ClickHandler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ButtonVariant {
    /// Filled with the accent colour: the window's default action.
    Primary,
    /// AppKit "bezel" push button.
    #[default]
    Secondary,
    /// Borderless, accent-coloured label (AppKit "borderless" / link style).
    Text,
    /// Borderless like `Text`, but on a tint of the accent colour: something needs doing first.
    Attention,
}

/// A button's colours for its variant.
struct Colors {
    bg: Option<Hsla>,
    fg: Hsla,
    hover: Hsla,
    keycap: KeycapStyle,
}

impl ButtonVariant {
    fn colors(self, t: &Theme) -> Colors {
        match self {
            ButtonVariant::Primary => {
                Colors { bg: Some(t.accent), fg: t.accent_text, hover: t.accent.opacity(0.85), keycap: KeycapStyle::OnAccent }
            }
            ButtonVariant::Secondary => {
                Colors { bg: Some(t.fill), fg: t.text, hover: t.fill.opacity(1.5), keycap: KeycapStyle::Plain }
            }
            ButtonVariant::Text => Colors {
                bg: None,
                fg: t.accent,
                hover: t.tint(t.accent),
                keycap: KeycapStyle::Accent,
            },
            ButtonVariant::Attention => Colors {
                bg: Some(t.tint(t.accent)),
                fg: t.accent,
                hover: t.accent.opacity(0.28),
                keycap: KeycapStyle::Accent,
            },
        }
    }
}

/// Stops a click on a disabled control from reaching its parents.
fn swallow_clicks<E: InteractiveElement>(element: E) -> E {
    element.on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
}

#[derive(IntoElement)]
pub struct Button {
    id: ElementId,
    label: SharedString,
    icon: Option<IconName>,
    shortcut: Option<SharedString>,
    variant: ButtonVariant,
    size: Size,
    disabled: bool,
    on_click: Option<ClickHandler>,
}

impl Button {
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            icon: None,
            shortcut: None,
            variant: ButtonVariant::default(),
            size: Size::default(),
            disabled: false,
            on_click: None,
        }
    }

    pub fn variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = variant;
        self
    }

    pub fn primary(self) -> Self {
        self.variant(ButtonVariant::Primary)
    }

    pub fn text(self) -> Self {
        self.variant(ButtonVariant::Text)
    }

    pub fn icon(mut self, icon: IconName) -> Self {
        self.icon = Some(icon);
        self
    }

    /// A keycap after the label, e.g. `"↵"` or `"⌘↵"`.
    pub fn shortcut(mut self, shortcut: impl Into<SharedString>) -> Self {
        self.shortcut = Some(shortcut.into());
        self
    }

    pub fn on_click(mut self, handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Rc::new(handler));
        self
    }
}

impl Sizable for Button {
    fn with_size(mut self, size: Size) -> Self {
        self.size = size;
        self
    }
}

impl Disableable for Button {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl RenderOnce for Button {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let c = self.variant.colors(cx.theme());
        let text = self.variant == ButtonVariant::Text;
        let padding = if text { self.size.padding_x() - px(2.) } else { self.size.padding_x() };
        let weight = if text { FontWeight::NORMAL } else { FontWeight::MEDIUM };
        div()
            .id(self.id)
            .flex_shrink_0()
            .h(self.size.control_height())
            .pl(padding)
            .pr(if self.shortcut.is_some() && !text { px(5.) } else { padding })
            .flex()
            .items_center()
            .gap(px(6.))
            .rounded(px(6.))
            .when_some(c.bg, |d, bg| d.bg(bg))
            .when(self.variant == ButtonVariant::Primary, |d| d.shadow_sm())
            .text_color(c.fg)
            .text_size(self.size.text_size())
            .font_weight(weight)
            .when_some(self.icon, |d, icon| d.child(Icon::new(icon).size(self.size.icon_size() - px(2.))))
            .child(self.label)
            .when_some(self.shortcut, |d, s| d.child(Keycap::new(s).style(c.keycap)))
            .map(|d| {
                if self.disabled {
                    swallow_clicks(d.opacity(0.5))
                } else {
                    let hover = c.hover;
                    d.cursor_pointer()
                        .hover(move |s| s.bg(hover))
                        .when_some(self.on_click, |d, f| d.on_click(move |e, window, cx| f(e, window, cx)))
                }
            })
    }
}

/// An icon (and optionally a short label) that's clicked, or toggled with
/// [`Selectable::selected`] (it's accent-tinted while on).
#[derive(IntoElement)]
pub struct IconButton {
    id: ElementId,
    icon: IconName,
    label: Option<SharedString>,
    tooltip: Option<SharedString>,
    size: Size,
    selected: bool,
    disabled: bool,
    on_click: Option<ClickHandler>,
}

impl IconButton {
    pub fn new(id: impl Into<ElementId>, icon: IconName) -> Self {
        Self {
            id: id.into(),
            icon,
            label: None,
            tooltip: None,
            size: Size::default(),
            selected: false,
            disabled: false,
            on_click: None,
        }
    }

    /// A short label after the icon, e.g. an indent width.
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// What the button does, shown while hovering it.
    pub fn tooltip(mut self, tooltip: impl Into<SharedString>) -> Self {
        self.tooltip = Some(tooltip.into());
        self
    }

    pub fn on_click(mut self, handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Rc::new(handler));
        self
    }
}

impl Sizable for IconButton {
    fn with_size(mut self, size: Size) -> Self {
        self.size = size;
        self
    }
}

impl Disableable for IconButton {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl Selectable for IconButton {
    fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }
}

impl RenderOnce for IconButton {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = cx.theme();
        let hover = t.fill_subtle();
        let fg = if self.selected { t.accent } else { t.text_muted };
        let height = self.size.control_height();
        div()
            .id(self.id)
            .flex_shrink_0()
            .h(height)
            .min_w(height)
            .when(self.label.is_some(), |d| d.px(px(6.)))
            .rounded(px(6.))
            .flex()
            .items_center()
            .justify_center()
            .gap(px(4.))
            .text_size(self.size.text_size())
            .text_color(fg)
            .when(self.selected, |d| d.bg(t.tint(t.accent)))
            .child(Icon::new(self.icon).size(self.size.icon_size()).color(fg))
            .when_some(self.label, |d, label| d.child(label))
            .when_some(self.tooltip, |d, tooltip| d.tooltip(Tooltip::text(tooltip)))
            .map(|d| {
                if self.disabled {
                    swallow_clicks(d.opacity(0.5))
                } else {
                    d.cursor_pointer()
                        .hover(move |s| s.bg(hover))
                        .when_some(self.on_click, |d, f| d.on_click(move |e, window, cx| f(e, window, cx)))
                }
            })
    }
}
