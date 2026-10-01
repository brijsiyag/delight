//! Grouping and separators, System Settings style: [`Group`], [`Caption`],
//! [`Divider`], and the settings page's [`section`] and [`row`].

use gpui::{
    AnyElement, App, FontWeight, IntoElement, ParentElement, RenderOnce, SharedString, Styled, Window, div, prelude::FluentBuilder as _,
    px,
};
use smallvec::SmallVec;

use crate::{ActiveTheme, StyledExt as _, Theme, h_flex, v_flex};

/// A rounded card of rows, tinted a step off the page behind it ([`Theme::card`]), as in
/// System Settings; its children are rows separated by hairlines.
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
        div()
            .flex()
            .flex_col()
            .rounded(t.radius)
            // Clips what spills out of the card, to its rectangle only: GPUI's clip has no rounded
            // corners. A row with a hover colour rounds its own corners at the card's top and bottom.
            .overflow_hidden()
            .bg(t.card())
            .children(self.rows.into_iter().enumerate().flat_map(|(i, row)| {
                let separator = (i > 0).then(|| div().h(px(1.)).mx(px(10.)).bg(t.separator()).into_any_element());
                separator.into_iter().chain(std::iter::once(row))
            }))
    }
}

/// Rows separated by hairlines, with no card around them: what goes inside a card the
/// app draws (a section of a plugin's settings), where a [`Group`] would draw a second.
#[derive(IntoElement, Default)]
pub struct Rows {
    rows: SmallVec<[AnyElement; 4]>,
}

impl Rows {
    pub fn new() -> Self {
        Self::default()
    }
}

impl ParentElement for Rows {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.rows.extend(elements);
    }
}

impl RenderOnce for Rows {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = cx.theme();
        let separator = t.separator();
        // A plugin's view starts with GPUI's own text colour and size: the app's here.
        div().flex().flex_col().size_full().text_size(t.text_size).text_color(t.text).children(self.rows.into_iter().enumerate().flat_map(|(i, row)| {
            let line = (i > 0).then(|| div().h(px(1.)).mx(px(10.)).bg(separator).into_any_element());
            line.into_iter().chain(std::iter::once(row))
        }))
    }
}

/// How tall a [`row`] is, and one with a detail line: fixed, so a plugin can say how tall
/// its section of settings is (its rows' heights and the hairlines between them, see
/// [`rows_height`]).
pub const ROW_HEIGHT: f32 = 41.;
pub const ROW_DETAIL_HEIGHT: f32 = 61.;

/// The height of rows of these `heights`, one after another with a hairline between.
pub fn rows_height(heights: &[f32]) -> f32 {
    heights.iter().sum::<f32>() + heights.len().saturating_sub(1) as f32
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
            .truncate()
            .child(crate::ellipsize(&crate::one_line(&self.text), 160))
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

/// A captioned group of rows: a page of settings is a column of these, in the app's
/// settings window and in a plugin's own settings page alike.
pub fn section(caption: impl Into<SharedString>, rows: Vec<AnyElement>) -> impl IntoElement {
    v_flex().gap(px(6.)).child(div().px(px(4.)).child(Caption::new(caption))).child(Group::new().children(rows))
}

/// A `title (detail) … control` row in a group.
pub fn row(title: impl Into<SharedString>, detail: Option<&'static str>, control: impl IntoElement, t: &Theme) -> AnyElement {
    h_flex()
        .h(px(if detail.is_some() { ROW_DETAIL_HEIGHT } else { ROW_HEIGHT }))
        .items_center()
        .gap(px(12.))
        .px(px(14.))
        .child(v_flex().flex_1().min_w(px(0.)).child(div().truncate().child(crate::ellipsize(&crate::one_line(&title.into()), 160))).when_some(detail, |column, detail| {
            column.child(div().mt(px(2.)).text_size(px(11.)).text_color(t.text_muted).clamp_lines(2).child(crate::ellipsize(detail, 300)))
        }))
        .child(control)
        .into_any_element()
}

/// A [`row`] whose detail line is text made when it draws (a status, an error), in `color`.
/// It wraps to a second line; a longer one is cut off.
pub fn row_with(
    title: impl Into<SharedString>,
    detail: impl Into<SharedString>,
    control: impl IntoElement,
    color: gpui::Hsla,
) -> AnyElement {
    h_flex()
        .h(px(ROW_DETAIL_HEIGHT))
        .items_center()
        .gap(px(12.))
        .px(px(14.))
        .child(
            v_flex()
                .flex_1()
                .min_w(px(0.))
                .max_h(px(ROW_DETAIL_HEIGHT - 8.))
                .overflow_hidden()
                .child(title.into())
                .child(div().mt(px(2.)).text_size(px(11.)).text_color(color).child(detail.into())),
        )
        .child(control)
        .into_any_element()
}
