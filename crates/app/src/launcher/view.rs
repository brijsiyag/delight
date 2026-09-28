//! Drawing the launcher: the input bar, then (with input) the tool list and the
//! selected tool.

use delight_ui::theme::INPUT_LINE_HEIGHT;
use delight_ui::{ActiveTheme, Divider, Icon, IconButton, IconName, Theme, h_flex, v_flex};
use gpui::{
    AnyElement, Context, FocusHandle, Focusable, FontWeight, IntoElement, MouseButton, ParentElement, Render,
    Styled, Window, div, prelude::*, px,
};

use super::{
    BAR_HEIGHT, BAR_ICON_GAP, BAR_ICON_SIZE, BAR_PADDING_X, CONTEXT, ClearInput, Dismiss, Launcher, hide,
};
use crate::platform;

const LIST_WIDTH: f32 = 200.;

impl Focusable for Launcher {
    fn focus_handle(&self, _: &gpui::App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for Launcher {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_window_size(cx);
        let t = cx.theme().clone();
        let root = v_flex()
            .key_context(CONTEXT)
            .track_focus(&self.focus_handle)
            .size_full()
            .overflow_hidden()
            .rounded(px(self.corner_radius()))
            // The tint decides the contrast; the native glass or blur underneath
            // adds the blur, and glass its shadow.
            .bg(t.window_tint())
            .border_1()
            .border_color(if platform::uses_liquid_glass() {
                // Spotlight's light rim.
                gpui::hsla(0., 0., 1., if t.dark { 0.2 } else { 0.6 })
            } else {
                t.border
            })
            .font_family(t.font.clone())
            .text_color(t.text)
            .text_size(t.text_size)
            .on_action(cx.listener(|_, _: &Dismiss, _, cx| cx.defer(hide)))
            .on_action(cx.listener(|this, _: &ClearInput, window, cx| this.clear_input(window, cx)))
            .child(self.render_bar(&t, cx));
        if !self.is_expanded(cx) {
            return root;
        }
        root.child(Divider::horizontal())
            .child(
                h_flex()
                    .flex_1()
                    .min_h(px(0.))
                    .child(self.render_list(&t))
                    .child(Divider::vertical())
                    .child(self.render_detail(&t)),
            )
    }
}

impl Launcher {
    fn render_bar(&self, t: &Theme, cx: &mut Context<Self>) -> impl IntoElement {
        // The first input line sits centred in the bar; more lines grow it down. The
        // icon and the clear button stay centred on that first line.
        let line = px(INPUT_LINE_HEIGHT);
        let on_first_line = |element: AnyElement| h_flex().h(line).flex_shrink_0().child(element);
        let icon = Icon::new(IconName::Zap).size(px(BAR_ICON_SIZE)).color(t.text_muted);
        h_flex()
            .flex_shrink_0()
            .items_start()
            .min_h(px(BAR_HEIGHT))
            .py((px(BAR_HEIGHT) - line) / 2.)
            .px(px(BAR_PADDING_X))
            .gap(px(BAR_ICON_GAP))
            // The bolt is a handle for moving the window; the rest of it selects text.
            .child(
                on_first_line(icon.into_any_element())
                    .on_mouse_down(MouseButton::Left, |_, window, _| window.start_window_move()),
            )
            .child(div().flex_1().min_w(px(0.)).child(self.input.clone()))
            .when(self.is_expanded(cx), |bar| {
                let clear = IconButton::new("clear", IconName::CircleX)
                    .on_click(cx.listener(|this, _, window, cx| this.clear_input(window, cx)));
                bar.child(on_first_line(clear.into_any_element()))
            })
    }

    /// The tools that fit the input: none yet.
    fn render_list(&self, t: &Theme) -> impl IntoElement {
        v_flex().w(px(LIST_WIDTH)).flex_shrink_0().h_full().py(px(6.)).child(
            div()
                .px(px(14.))
                .py(px(6.))
                .text_size(t.text_size_small())
                .text_color(t.text_faint)
                .child("No recommendations for this input"),
        )
    }

    /// The selected tool; with none, why.
    fn render_detail(&self, t: &Theme) -> impl IntoElement {
        v_flex()
            .flex_1()
            .min_w(px(0.))
            .h_full()
            .px(px(18.))
            .py(px(14.))
            .items_center()
            .justify_center()
            .gap(px(6.))
            .child(Icon::new(IconName::Sparkles).size(px(28.)).color(t.text_faint))
            .child(
                div()
                    .text_size(t.text_size_large())
                    .font_weight(FontWeight::MEDIUM)
                    .child("No tool fits this input"),
            )
            .child(
                div()
                    .text_size(t.text_size_small())
                    .text_color(t.text_muted)
                    .child("Plugins in the plugins folder add tools."),
            )
    }
}

