//! The YAML ⇄ JSON tool: the converted text, with nothing above it, and its action,
//! which names what it copies (JSON or YAML, as the input went).

use delight_plugin_api::{Action, Actions, Input, Shortcut, Tool, host};
use delight_ui::conversion::Output;
use delight_ui::v_flex;
use gpui::{
    App, ClipboardItem, Context, InteractiveElement, IntoElement, ParentElement, Render, ScrollHandle,
    StatefulInteractiveElement, Styled, Task, Window, px,
};

use super::convert::convert;

#[derive(Default)]
pub struct YamlView {
    /// The last input went from YAML to JSON (else, from JSON to YAML).
    to_json: bool,
    output: Output,
    /// Where the pane is scrolled. Kept here, not in the window's element state, which goes while
    /// the tool is hidden: the tool comes back where it was.
    scroll: ScrollHandle,
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
    fn copy_label(&self) -> &'static str {
        if self.to_json { "Copy JSON" } else { "Copy YAML" }
    }
}

impl Tool for YamlView {
    type Action = YamlAction;

    /// Converts the input in the background, then shows the result.
    fn on_input_changed(&mut self, input: &Input, cx: &mut Context<Self>) {
        let input = input.text.clone();
        self._task = Some(cx.spawn(async move |this, cx| {
            let convert = async move {
                let text = input.trim();
                let (conversion, to_json) = convert(text);
                (Output::new(conversion, text), to_json)
            };
            let (output, to_json) = cx.background_executor().spawn(convert).await;
            this.update(cx, |this, cx| {
                this.output = output;
                this.to_json = to_json;
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
        v_flex()
            .id("yaml")
            .size_full()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            // Room after the last line, above the footer.
            .pb(px(14.))
            .gap(px(14.))
            .child(self.output.render(cx))
    }
}
