//! The YAML tool: the converted text and its action.

use delight_plugin_api::{Action, Actions, Input, Shortcut, Tool, host};
use delight_ui::conversion::Output;
use delight_ui::v_flex;
use gpui::{
    App, ClipboardItem, Context, InteractiveElement, IntoElement, ParentElement, Render, StatefulInteractiveElement,
    Styled, Task, Window, px,
};

use super::convert::{json_to_yaml, yaml_to_json};

pub struct YamlView {
    /// YAML → JSON, or JSON → YAML.
    to_json: bool,
    output: Output,
    /// Replacing it cancels the conversion in progress.
    _task: Option<Task<()>>,
}

/// The footer's action.
#[derive(Actions)]
pub enum YamlAction {
    /// The converted text.
    Copy,
}

impl YamlView {
    pub fn new(to_json: bool) -> Self {
        Self { to_json, output: Output::default(), _task: None }
    }

    fn copy_label(&self) -> &'static str {
        if self.to_json { "Copy JSON" } else { "Copy YAML" }
    }
}

impl Tool for YamlView {
    type Action = YamlAction;

    /// Converts the input in the background, then shows the result.
    fn on_input_changed(&mut self, input: &Input, cx: &mut Context<Self>) {
        let (to_json, input) = (self.to_json, input.text.clone());
        self._task = Some(cx.spawn(async move |this, cx| {
            let convert = async move {
                let text = input.trim();
                let conversion = if to_json { yaml_to_json(text) } else { json_to_yaml(text) };
                Output::new(conversion, text)
            };
            let output = cx.background_executor().spawn(convert).await;
            this.update(cx, |this, cx| {
                this.output = output;
                cx.notify();
            })
            .ok();
        }));
    }

    fn list_actions(&self, _: &App) -> Vec<Action<YamlAction>> {
        if self.output.text().is_none() {
            return Vec::new();
        }
        vec![Action::new(YamlAction::Copy, self.copy_label(), Shortcut::Enter)]
    }

    fn perform_action(&mut self, action: YamlAction, cx: &mut Context<Self>) {
        match action {
            YamlAction::Copy => {
                let Some(text) = self.output.text() else { return };
                cx.write_to_clipboard(ClipboardItem::new_string(text.to_string()));
                host(cx).toast(format!("{} — copied to clipboard", self.copy_label()), cx);
            }
        }
    }
}

impl Render for YamlView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex().id("yaml").size_full().overflow_y_scroll().gap(px(14.)).child(self.output.render(None, cx))
    }
}
