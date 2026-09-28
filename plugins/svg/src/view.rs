//! The SVG tool: the drawing on a canvas (checkerboard, light or dark), its size, and
//! the copy action.

use delight_plugin_api::{Action, Input, Shortcut, Tool, host};
use delight_ui::{ActiveTheme, IconButton, IconName, Selectable, h_flex, v_flex};
use gpui::{
    App, Context, Hsla, ImageSource, IntoElement, ParentElement, Render, SharedString, Styled, Task, Window, div,
    img, prelude::FluentBuilder, px, rgb, size,
};

use super::checkerboard::Checkerboard;
use super::render::{PREVIEW_HEIGHT, PREVIEW_WIDTH, Preview, render};

/// Space around the drawing on the canvas, and its corners.
const CANVAS_PADDING: f32 = 20.;
const CANVAS_RADIUS: f32 = 12.;

/// What the drawing is shown on.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum Backdrop {
    /// Squares, so transparent areas show.
    #[default]
    Checkerboard,
    Light,
    Dark,
}

const BACKDROPS: [(Backdrop, IconName, &str); 3] = [
    (Backdrop::Checkerboard, IconName::Grid, "Show transparency"),
    (Backdrop::Light, IconName::Sun, "On a light background"),
    (Backdrop::Dark, IconName::Moon, "On a dark background"),
];

#[derive(Default)]
pub struct SvgView {
    preview: Preview,
    backdrop: Backdrop,
    /// Replacing it cancels the rendering in progress.
    rendering: Option<Task<()>>,
}

impl Tool for SvgView {
    /// Render the input in the background, then show it.
    fn on_input_changed(&mut self, input: &Input, cx: &mut Context<Self>) {
        let svg = input.text.trim().to_string();
        self.rendering = Some(cx.spawn(async move |this, cx| {
            let preview = cx.background_executor().spawn(async move { render(&svg) }).await;
            this.update(cx, |this, cx| {
                this.preview = preview;
                cx.notify();
            })
            .ok();
        }));
    }

    fn list_actions(&self, _: &App) -> Vec<Action> {
        let Preview::Ready(_) = &self.preview else {
            return Vec::new();
        };
        vec![Action {
            id: "copy_data_uri".into(),
            label: "Copy data URI".into(),
            shortcut: Shortcut::Keystroke("enter".into()),
        }]
    }

    fn perform_action(&mut self, action: &str, cx: &mut Context<Self>) {
        let Preview::Ready(rendered) = &self.preview else {
            return;
        };
        if action == "copy_data_uri" {
            let data_uri = rendered.data_uri.clone();
            let host = host(cx);
            host.copy_text(data_uri, cx);
            host.toast("Copy data URI — copied to clipboard", cx);
        }
    }
}

impl Render for SvgView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let rendered = match &self.preview {
            Preview::Empty => return div(),
            Preview::Failed(message) => return div().child(notice(message.clone(), cx)),
            Preview::Ready(rendered) => rendered,
        };
        let t = cx.theme();
        let drawing = img(ImageSource::Render(rendered.image.clone())).w(px(rendered.shown.0)).h(px(rendered.shown.1));
        // The light and dark backdrops are fixed colours, not the theme's: they show
        // how the drawing looks on either.
        let backdrop: Option<Hsla> = match self.backdrop {
            Backdrop::Checkerboard => None,
            Backdrop::Light => Some(rgb(0xFFFFFF).into()),
            Backdrop::Dark => Some(rgb(0x1C1C1E).into()),
        };
        // About the pane's width; the canvas fills it.
        let canvas_size = size(px(PREVIEW_WIDTH + 2. * CANVAS_PADDING), px(PREVIEW_HEIGHT + 2. * CANVAS_PADDING));
        let canvas = div()
            .relative()
            .w_full()
            .h(canvas_size.height)
            .rounded(px(CANVAS_RADIUS))
            .overflow_hidden()
            .when_some(backdrop, |canvas, color| canvas.bg(color))
            .when(backdrop.is_none(), |canvas| canvas.child(Checkerboard::new(canvas_size).rounded(px(CANVAS_RADIUS))))
            .child(h_flex().absolute().top_0().left_0().size_full().justify_center().child(drawing));

        let (width, height) = rendered.size;
        let size = div()
            .text_size(t.text_size_small())
            .text_color(t.text_faint)
            .child(format!("{} × {} px", round(width), round(height)));
        let backdrops = h_flex().gap(px(2.)).children(BACKDROPS.map(|(backdrop, icon, tooltip)| {
            IconButton::new(SharedString::from(format!("svg-backdrop-{backdrop:?}")), icon)
                .selected(self.backdrop == backdrop)
                .tooltip(tooltip)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.backdrop = backdrop;
                    cx.notify();
                }))
        }));
        div().child(v_flex().gap(px(8.)).child(canvas).child(h_flex().justify_between().child(size).child(backdrops)))
    }
}

/// `24.0` → `24`, `10.5` → `10.5`.
fn round(value: f32) -> String {
    let rounded = (value * 10.).round() / 10.;
    if rounded.fract() == 0. { format!("{rounded:.0}") } else { format!("{rounded:.1}") }
}

/// An error line on its tint.
fn notice(text: SharedString, cx: &App) -> impl IntoElement {
    let t = cx.theme();
    h_flex()
        .px(px(12.))
        .py(px(8.))
        .rounded(t.radius)
        .bg(t.tint(t.error))
        .text_color(t.error)
        .text_size(t.text_size_small())
        .child(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_read_without_needless_decimals() {
        assert_eq!(round(24.), "24");
        assert_eq!(round(10.5), "10.5");
        assert_eq!(round(10.04), "10");
    }
}
