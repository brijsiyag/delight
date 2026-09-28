//! [`Switch`]: an NSSwitch look-alike. Controlled: pass `checked`, get the
//! new value in `on_change`.

use std::rc::Rc;

use gpui::{
    App, ElementId, InteractiveElement, IntoElement, MouseButton, ParentElement, RenderOnce, StatefulInteractiveElement,
    Styled, Window, div, prelude::FluentBuilder, px, white,
};

use crate::{ActiveTheme, Disableable};

type ChangeHandler = Rc<dyn Fn(&bool, &mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct Switch {
    id: ElementId,
    checked: bool,
    disabled: bool,
    on_change: Option<ChangeHandler>,
}

impl Switch {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self { id: id.into(), checked: false, disabled: false, on_change: None }
    }

    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }

    /// Called with the new value when the user flips the switch.
    pub fn on_change(mut self, handler: impl Fn(&bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_change = Some(Rc::new(handler));
        self
    }
}

impl Disableable for Switch {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl RenderOnce for Switch {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = cx.theme();
        let checked = self.checked;
        div()
            .id(self.id)
            .flex_shrink_0()
            .w(px(32.))
            .h(px(19.))
            .rounded_full()
            .bg(if checked { t.accent } else { t.fill })
            .p(px(2.))
            .flex()
            .when(checked, |d| d.justify_end())
            .child(div().size(px(15.)).rounded_full().bg(white()).shadow_sm())
            .map(|d| {
                if self.disabled {
                    d.opacity(0.5).on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                } else {
                    d.cursor_pointer().when_some(self.on_change, |d, f| d.on_click(move |_, window, cx| f(&!checked, window, cx)))
                }
            })
    }
}
