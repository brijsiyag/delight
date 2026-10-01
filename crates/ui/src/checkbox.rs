//! [`Checkbox`]: a square box, ticked or not, with an optional label after it. Controlled: pass
//! `checked`, get the new value in `on_change`. Without a handler it only shows, for a row that
//! takes the clicks itself.

use std::rc::Rc;

use gpui::{
    App, ElementId, InteractiveElement, IntoElement, ParentElement, RenderOnce, SharedString, StatefulInteractiveElement,
    Styled, Window, div, prelude::FluentBuilder, px,
};

use crate::{ActiveTheme, Icon, IconName, Sizable, Size, h_flex};

type ChangeHandler = Rc<dyn Fn(&bool, &mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct Checkbox {
    id: ElementId,
    checked: bool,
    label: Option<SharedString>,
    size: Size,
    on_change: Option<ChangeHandler>,
}

impl Checkbox {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self { id: id.into(), checked: false, label: None, size: Size::default(), on_change: None }
    }

    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }

    /// Text after the box: a click on it ticks the box too.
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Called with the new value when the user clicks it.
    pub fn on_change(mut self, handler: impl Fn(&bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_change = Some(Rc::new(handler));
        self
    }
}

impl Sizable for Checkbox {
    fn with_size(mut self, size: Size) -> Self {
        self.size = size;
        self
    }
}

impl RenderOnce for Checkbox {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = cx.theme();
        let checked = self.checked;
        // A checklist's box is 16 pt; a small one, 14 pt, sits under a row's title.
        let (side, tick, label_size) = match self.size {
            Size::Small => (14., 10., 12.),
            Size::Medium | Size::Large => (16., 12., 13.),
        };
        let tick_box = div()
            .size(px(side))
            .flex_shrink_0()
            .rounded(px(4.))
            .flex()
            .items_center()
            .justify_center()
            .when(checked, |tick_box| tick_box.bg(t.accent).child(Icon::new(IconName::Check).size(px(tick)).color(t.accent_text)))
            .when(!checked, |tick_box| tick_box.bg(t.surface).border_1().border_color(t.border));
        h_flex()
            .id(self.id)
            .flex_shrink_0()
            .gap(px(6.))
            .child(tick_box)
            .when_some(self.label, |checkbox, label| checkbox.child(div().text_size(px(label_size)).child(label)))
            .when_some(self.on_change, |checkbox, on_change| {
                checkbox.cursor_pointer().on_click(move |_, window, cx| on_change(&!checked, window, cx))
            })
    }
}
