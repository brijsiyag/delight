//! [`AnyTool`]: a tool of any type, so one plugin can offer tools of different types
//! from a single `open_tool`, and the glue can drive whichever tool it holds.

use std::rc::Rc;

use delight_protocol::Action;

use crate::gpui::{AnyView, App, Entity, Subscription};
use crate::{Actions, Input, Tool};

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
    fn focus_lost(&self, cx: &mut App);
    fn observe(&self, on_notify: Box<dyn FnMut(&mut App)>, cx: &mut App) -> Subscription;
}

impl<T: Tool> DynTool for Entity<T> {
    fn on_input_changed(&self, input: &Input, cx: &mut App) {
        self.update(cx, |tool, cx| tool.on_input_changed(input, cx));
    }

    /// The actions, with their ids for the app.
    fn list_actions(&self, cx: &App) -> Vec<Action> {
        let actions = self.read(cx).list_actions(cx);
        actions
            .into_iter()
            .map(|action| Action { id: action.id.id().to_string(), label: action.label, shortcut: action.shortcut, style: action.style })
            .collect()
    }

    fn focus_lost(&self, cx: &mut App) {
        self.update(cx, |tool, cx| tool.on_focus_lost(cx));
    }

    /// The action with the id the app hands back; one the tool doesn't have is
    /// ignored.
    fn perform_action(&self, action: &str, cx: &mut App) {
        match T::Action::from_id(action) {
            Some(action) => self.update(cx, |tool, cx| tool.perform_action(action, cx)),
            None => log::warn!("the app asked for an action this tool doesn't have: {action:?}"),
        }
    }

    fn observe(&self, mut on_notify: Box<dyn FnMut(&mut App)>, cx: &mut App) -> Subscription {
        cx.observe(self, move |_, cx| on_notify(cx))
    }
}
