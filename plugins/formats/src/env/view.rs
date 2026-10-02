//! The .env ⇄ JSON tool. JSON in: the prefix the names start with, in a field at the top
//! right, then the variables, one per line. Variables in: the JSON object, as YAML ⇄ JSON
//! shows its result.

use delight_plugin_api::{Action, Actions, Input, Shortcut, Tool, host};
use delight_ui::conversion::Output;
use delight_ui::{h_flex, v_flex};
use gpui::{
    App, ClipboardItem, Context, InteractiveElement, IntoElement, ParentElement, Render, ScrollHandle, SharedString,
    StatefulInteractiveElement, Styled, Task, Window, div, px,
};

use super::{convert, exported, to_env};
use crate::field::Field;

#[derive(Default)]
pub struct EnvView {
    input: SharedString,
    /// The input is JSON, going to variables (else, variables going to JSON).
    to_env: bool,
    /// Put before every name, as typed in its field.
    prefix: String,
    field: Field,
    output: Output,
    /// Where the pane is scrolled, kept while the tool is hidden.
    scroll: ScrollHandle,
    /// Replacing it cancels the conversion in progress.
    _task: Option<Task<()>>,
}

/// The footer's actions.
#[derive(Actions)]
pub enum EnvAction {
    /// The variables, or the JSON.
    Copy,
    /// Each variable as `export NAME=value`, for a shell.
    CopyExported,
}

impl EnvView {
    fn convert(&mut self, cx: &mut Context<Self>) {
        let (input, prefix) = (self.input.clone(), self.prefix.clone());
        self._task = Some(cx.spawn(async move |this, cx| {
            let output =
                cx.background_executor().spawn(async move { Output::new(convert(input.trim(), &prefix), input.trim()) }).await;
            this.update(cx, |this, cx| {
                this.output = output;
                cx.notify();
            })
            .ok();
        }));
    }

    fn copy_label(&self) -> &'static str {
        if self.to_env { "Copy .env" } else { "Copy JSON" }
    }
}

impl Tool for EnvView {
    type Action = EnvAction;

    fn on_input_changed(&mut self, input: &Input, cx: &mut Context<Self>) {
        self.input = input.text.clone().into();
        self.to_env = to_env(&input.text);
        self.convert(cx);
    }

    fn list_actions(&self, _: &App) -> Vec<Action<EnvAction>> {
        if self.output.text().is_none() {
            return Vec::new();
        }
        let mut actions = vec![Action::new(EnvAction::Copy, self.copy_label(), Shortcut::Enter)];
        if self.to_env {
            actions.push(Action::new(EnvAction::CopyExported, "Copy with export", Shortcut::CmdEnter));
        }
        actions
    }

    fn perform_action(&mut self, action: EnvAction, cx: &mut Context<Self>) {
        let Some(text) = self.output.text().map(ToString::to_string) else { return };
        let (label, text) = match action {
            EnvAction::Copy => (self.copy_label(), text),
            EnvAction::CopyExported => ("Copy with export", exported(&text)),
        };
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        host(cx).toast(format!("{label} — copied to clipboard"), cx);
    }
}

impl Render for EnvView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Above the scrolling pane, not in it (see `field`); only when there are names to prefix.
        let prefix = self.to_env.then(|| {
            let field = self.field.render(
                "Prefix",
                |this: &mut Self, prefix, cx| {
                    this.prefix = prefix;
                    this.convert(cx);
                },
                window,
                cx,
            );
            h_flex().justify_end().child(h_flex().w(px(160.)).child(field))
        });
        v_flex().size_full().gap(px(14.)).children(prefix).child(
            div()
                .id("env")
                .flex_1()
                .min_h(px(0.))
                .overflow_y_scroll()
                .track_scroll(&self.scroll)
                .child(self.output.render(cx)),
        )
    }
}
