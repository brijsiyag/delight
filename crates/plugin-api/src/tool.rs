//! [`AnyTool`]: a tool of any type, so one plugin can offer tools of different types
//! from a single `open_tool`, and the glue can drive whichever tool it holds.

use std::rc::Rc;

use crate::gpui::{AnyView, App, Entity, Subscription};
use crate::{Action, Input, Tool};

/// A tool of any type, as [`Plugin::open_tool`](crate::Plugin::open_tool) returns it:
/// `entity.into()`.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub struct AnyTool {
    pub(crate) view: AnyView,
    pub(crate) tool: Rc<dyn DynTool>,
}

impl<T: Tool> From<Entity<T>> for AnyTool {
    fn from(entity: Entity<T>) -> Self {
        AnyTool {
            view: entity.clone().into(),
            tool: Rc::new(entity),
        }
    }
}

/// [`Tool`] without its type, for the glue to drive whichever tool it holds.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub(crate) trait DynTool {
    fn on_input_changed(&self, input: &Input, cx: &mut App);
    fn list_actions(&self, cx: &App) -> Vec<Action>;
    fn perform_action(&self, action: &str, cx: &mut App);
    fn observe(&self, on_notify: Box<dyn FnMut(&mut App)>, cx: &mut App) -> Subscription;
}

impl<T: Tool> DynTool for Entity<T> {
    fn on_input_changed(&self, input: &Input, cx: &mut App) {
        self.update(cx, |tool, cx| tool.on_input_changed(input, cx));
    }

    fn list_actions(&self, cx: &App) -> Vec<Action> {
        self.read(cx).list_actions(cx)
    }

    fn perform_action(&self, action: &str, cx: &mut App) {
        self.update(cx, |tool, cx| tool.perform_action(action, cx));
    }

    fn observe(&self, mut on_notify: Box<dyn FnMut(&mut App)>, cx: &mut App) -> Subscription {
        cx.observe(self, move |_, cx| on_notify(cx))
    }
}
