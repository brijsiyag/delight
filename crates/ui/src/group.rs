//! Grouping and separators, System Settings style: [`Group`], [`Caption`],
//! [`Divider`].

use gpui::{AnyElement, App, FontWeight, IntoElement, ParentElement, RenderOnce, SharedString, Styled, Window, div, px};
use smallvec::SmallVec;

use crate::ActiveTheme;

/// A rounded inset group on the theme's surface (lighter than the window behind it,
/// as in System Settings); its children are rows separated by hairlines.
#[derive(IntoElement, Default)]
pub struct Group {
    rows: SmallVec<[AnyElement; 4]>,
}

impl Group {
    pub fn new() -> Self {
        Self::default()
    }
}

impl ParentElement for Group {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.rows.extend(elements);
    }
}

impl RenderOnce for Group {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = cx.theme();
        let separator = t.separator();
        let mut group = div().flex().flex_col().rounded(t.radius).bg(t.surface).border_1().border_color(separator);
        let count = self.rows.len();
        for (i, row) in self.rows.into_iter().enumerate() {
            group = group.child(row);
            if i + 1 < count {
                group = group.child(div().h(px(1.)).mx(px(10.)).bg(separator));
            }
        }
        group
    }
}

/// A small section caption above a group.
#[derive(IntoElement)]
pub struct Caption {
    text: SharedString,
}

impl Caption {
    pub fn new(text: impl Into<SharedString>) -> Self {
        Self { text: text.into() }
    }
}

impl RenderOnce for Caption {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        div()
            .text_size(px(11.))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(cx.theme().text_muted)
            .child(self.text)
    }
}

/// A separator line: full-width hairline, or a short vertical divider
/// between inline items.
#[derive(IntoElement)]
pub struct Divider {
    vertical: bool,
}

impl Divider {
    pub fn horizontal() -> Self {
        Self { vertical: false }
    }

    pub fn vertical() -> Self {
        Self { vertical: true }
    }
}

impl RenderOnce for Divider {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let line = div().flex_shrink_0().bg(cx.theme().separator());
        if self.vertical { line.w(px(1.)).h(px(14.)) } else { line.h(px(1.)).w_full() }
    }
}
