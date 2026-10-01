//! [`progress_bar`]: how far something has got, as a thin bar.

use gpui::{Hsla, IntoElement, ParentElement, Styled, div, px, relative};

use crate::Theme;

/// A thin bar `fraction` (0 to 1) full, in `color`, on the theme's control fill. It takes the width
/// it is given.
pub fn progress_bar(fraction: f32, color: Hsla, t: &Theme) -> impl IntoElement {
    div()
        .h(px(3.))
        .w_full()
        .rounded(px(2.))
        .bg(t.fill)
        .child(div().h_full().rounded(px(2.)).bg(color).w(relative(fraction.clamp(0., 1.))))
}
