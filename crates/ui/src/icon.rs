//! [`Icon`]: a built-in icon, tinted. Without an explicit size or colour it
//! takes the surrounding text's, so an icon next to a label matches it.

use gpui::{App, Hsla, IntoElement, Pixels, RenderOnce, Styled, Window, svg};

use crate::IconName;

#[derive(IntoElement)]
pub struct Icon {
    name: IconName,
    size: Option<Pixels>,
    color: Option<Hsla>,
}

impl Icon {
    pub fn new(name: IconName) -> Self {
        Self { name, size: None, color: None }
    }

    pub fn size(mut self, size: Pixels) -> Self {
        self.size = Some(size);
        self
    }

    pub fn color(mut self, color: Hsla) -> Self {
        self.color = Some(color);
        self
    }
}

impl From<IconName> for Icon {
    fn from(name: IconName) -> Self {
        Icon::new(name)
    }
}

impl RenderOnce for Icon {
    fn render(self, window: &mut Window, _: &mut App) -> impl IntoElement {
        let text = window.text_style();
        let size = self.size.unwrap_or_else(|| text.font_size.to_pixels(window.rem_size()));
        svg().path(self.name.path()).size(size).flex_shrink_0().text_color(self.color.unwrap_or(text.color))
    }
}
