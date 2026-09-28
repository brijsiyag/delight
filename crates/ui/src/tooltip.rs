//! [`Tooltip`]: the small label shown while hovering a control.

use gpui::{AnyView, App, AppContext, Context, IntoElement, ParentElement, Render, SharedString, Styled, Window, div, px};

use crate::ActiveTheme;

pub struct Tooltip {
    text: SharedString,
}

impl Tooltip {
    /// For `.tooltip(…)` on an element: `.tooltip(Tooltip::text("Sort keys"))`.
    pub fn text(text: impl Into<SharedString>) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
        let text = text.into();
        move |_, cx| cx.new(|_| Tooltip { text: text.clone() }).into()
    }
}

impl Render for Tooltip {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme();
        div()
            .px(px(8.))
            .py(px(4.))
            .rounded(px(6.))
            .bg(t.surface)
            .border_1()
            .border_color(t.border)
            .shadow_md()
            .font_family(t.font.clone())
            .text_size(px(11.))
            .text_color(t.text)
            .child(self.text.clone())
    }
}
