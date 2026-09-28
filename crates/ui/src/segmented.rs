//! [`SegmentedControl`]: an NSSegmentedControl look-alike (tabs). Controlled:
//! pass the selected index, get the picked one in `on_change`.
//!
//! Given a focus handle ([`SegmentedControl::focus`]) it's a Tab stop, and
//! the keymap's `SegmentedControl` context moves between segments (← / →
//! by default). Its key context adds `first` / `last` on the edge segments,
//! so the keymap can send ← on the first one elsewhere.

use std::rc::Rc;

use gpui::{
    App, ElementId, FocusHandle, InteractiveElement, IntoElement, KeyContext, MouseButton, ParentElement, RenderOnce,
    SharedString, StatefulInteractiveElement, Styled, Window, actions, div, prelude::FluentBuilder, px, white,
};

use crate::{ActiveTheme, Disableable};

actions!(segmented_control, [SelectPrevious, SelectNext]);

/// The key context of a focused segmented control.
pub const CONTEXT: &str = "SegmentedControl";

type ChangeHandler = Rc<dyn Fn(&usize, &mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct SegmentedControl {
    id: ElementId,
    options: Vec<SharedString>,
    selected: usize,
    disabled: bool,
    focus: Option<FocusHandle>,
    on_change: Option<ChangeHandler>,
}

impl SegmentedControl {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self { id: id.into(), options: Vec::new(), selected: 0, disabled: false, focus: None, on_change: None }
    }

    /// Makes it focusable (with the keys of the `SegmentedControl` context);
    /// pass a handle made with `.tab_stop(true)` to reach it with Tab.
    pub fn focus(mut self, handle: &FocusHandle) -> Self {
        self.focus = Some(handle.clone());
        self
    }

    pub fn options<S: Into<SharedString>>(mut self, options: impl IntoIterator<Item = S>) -> Self {
        self.options = options.into_iter().map(Into::into).collect();
        self
    }

    pub fn selected(mut self, index: usize) -> Self {
        self.selected = index;
        self
    }

    /// Called with the segment's index when the user picks one.
    pub fn on_change(mut self, handler: impl Fn(&usize, &mut Window, &mut App) + 'static) -> Self {
        self.on_change = Some(Rc::new(handler));
        self
    }
}

impl Disableable for SegmentedControl {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl SegmentedControl {
    /// `SegmentedControl`, plus `first` / `last` on the edge segments.
    fn key_context(&self) -> KeyContext {
        let mut context = KeyContext::default();
        context.add(CONTEXT);
        if self.selected == 0 {
            context.add("first");
        }
        if self.selected + 1 >= self.options.len() {
            context.add("last");
        }
        context
    }
}

impl RenderOnce for SegmentedControl {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = cx.theme();
        let dark = t.dark;
        // The selected segment is a raised white chip (translucent in dark mode).
        let chip = if dark { white().opacity(0.2) } else { white() };
        let focused = self.focus.as_ref().is_some_and(|handle| handle.is_focused(window));
        let (selected, count) = (self.selected, self.options.len());
        let move_to = |target: Option<usize>, handler: Option<ChangeHandler>| {
            move |window: &mut Window, cx: &mut App| {
                if let (Some(index), Some(on_change)) = (target.filter(|i| *i < count), &handler) {
                    on_change(&index, window, cx);
                }
            }
        };
        let previous = move_to(selected.checked_sub(1), self.on_change.clone());
        let next = move_to(Some(selected + 1), self.on_change.clone());
        let row = div()
            .id(self.id.clone())
            .key_context(self.key_context())
            .when_some(self.focus.clone(), |d, handle| {
                // Clicking a segment focuses the control too, so ← / → go on from there.
                let clicked = handle.clone();
                d.track_focus(&handle).on_mouse_down(MouseButton::Left, move |_, window, cx| window.focus(&clicked, cx))
            })
            .when(!self.disabled, |d| {
                d.on_action(move |_: &SelectPrevious, window, cx| previous(window, cx))
                    .on_action(move |_: &SelectNext, window, cx| next(window, cx))
            })
            .flex()
            .flex_shrink_0()
            .p(px(2.))
            .gap(px(2.))
            .rounded(px(7.))
            .bg(t.fill)
            // The focus ring, as macOS draws it around a focused control.
            .border_1()
            .border_color(if focused { t.focus_ring() } else { gpui::transparent_black() })
            .when(self.disabled, |d| d.opacity(0.5).on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation()));
        let (label, secondary) = (t.text, t.text_muted);
        row.children(self.options.into_iter().enumerate().map(|(i, option)| {
            let selected = i == self.selected;
            div()
                .id(ElementId::NamedInteger(SharedString::from(format!("{}-segment", self.id)), i as u64))
                .px(px(10.))
                .h(px(20.))
                .flex()
                .items_center()
                .rounded(px(5.))
                .text_size(px(12.))
                .text_color(if selected { label } else { secondary })
                .when(selected, |d| d.bg(chip).shadow_sm())
                .when(!selected && !self.disabled, |d| d.cursor_pointer().hover(move |s| s.text_color(label)))
                .when_some(self.on_change.clone().filter(|_| !self.disabled && !selected), |d, f| {
                    d.on_click(move |_, window, cx| f(&i, window, cx))
                })
                .child(option)
        }))
    }
}
