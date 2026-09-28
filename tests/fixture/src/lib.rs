//! The plugin Delight's headless tests drive: one tool that shows the input, with
//! actions that depend on it.

use delight_plugin_api::gpui::{App, AppContext as _, Context, IntoElement, Render, Window, div};
use delight_plugin_api::gpui::{ParentElement as _, Styled as _};
use delight_plugin_api::{
    Action, AnyTool, Detection, Input, Operations, Plugin, Shortcut, Tool, host, plugin,
};

#[plugin(
    id = "dev.delight.fixture",
    name = "Fixture",
    description = "Echoes the input, for Delight's tests",
    author = "Delight",
    icon = "assets/icon.svg",
)]
struct Fixture;

#[derive(Operations)]
enum FixtureOperation {
    #[operation(id = "echo", title = "Echo")]
    Echo,
}

impl Plugin for Fixture {
    type Operation = FixtureOperation;

    fn new(_cx: &mut App) -> Self {
        Fixture
    }

    fn detect(&mut self, input: &Input, _cx: &mut App) -> Vec<Detection<FixtureOperation>> {
        // The tests' way to make this plugin overrun its turn, so the app stops it.
        if input.text == "hang" {
            loop {
                std::hint::spin_loop();
            }
        }
        if input.text.trim().is_empty() {
            return Vec::new();
        }
        vec![Detection::new(FixtureOperation::Echo, 1.0)]
    }

    fn open_tool(
        &mut self,
        operation: FixtureOperation,
        _window: &mut Window,
        cx: &mut App,
    ) -> AnyTool {
        match operation {
            FixtureOperation::Echo => cx.new(|_| Echo::default()).into(),
        }
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
