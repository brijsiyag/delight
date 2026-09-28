//! The plugin Delight's headless tests drive: one tool that shows the input, with
//! actions that depend on it.

use delight_plugin_api::gpui::{App, AppContext as _, Context, IntoElement, Render, Window, div};
use delight_plugin_api::gpui::{ParentElement as _, Styled as _};
use delight_plugin_api::{Action, AnyTool, Detection, Input, Plugin, Shortcut, Tool, host};

struct Fixture;

impl Plugin for Fixture {
    fn new(_cx: &mut App) -> Self {
        Fixture
    }

    fn detect(&mut self, input: &Input, _cx: &mut App) -> Vec<Detection> {
        if input.text.trim().is_empty() {
            return Vec::new();
        }
        vec![Detection {
            operation: "echo".into(),
            confidence: 1.0,
        }]
    }

    fn open_tool(
        &mut self,
        operation: &str,
        _window: &mut Window,
        cx: &mut App,
    ) -> Option<AnyTool> {
        (operation == "echo").then(|| cx.new(|_| Echo::default()).into())
    }
}

#[derive(Default)]
struct Echo {
    text: String,
}

impl Tool for Echo {
    fn on_input_changed(&mut self, input: &Input, cx: &mut Context<Self>) {
        self.text = input.text.clone();
        cx.notify();
    }

    fn list_actions(&self, _cx: &App) -> Vec<Action> {
        let mut actions = vec![Action {
            id: "copy".into(),
            label: "Copy".into(),
            shortcut: Shortcut::Keystroke("cmd-enter".into()),
        }];
        if !self.text.is_empty() {
            actions.push(Action {
                id: "clear".into(),
                label: "Clear".into(),
                shortcut: Shortcut::ClickOnly,
            });
        }
        actions
    }

    fn perform_action(&mut self, action: &str, cx: &mut Context<Self>) {
        match action {
            "copy" => {
                host(cx).copy_text(self.text.clone(), cx);
                host(cx).toast("Copied", cx);
            }
            "clear" => {
                self.text.clear();
                cx.notify();
            }
            _ => {}
        }
    }
}

impl Render for Echo {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().p_2().child(self.text.clone())
    }
}

delight_plugin_api::export_plugin!(Fixture);
