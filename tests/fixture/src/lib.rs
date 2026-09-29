//! The plugin Delight's headless tests drive: one tool that shows the input, with
//! actions that depend on it.

use delight_plugin_api::gpui::{AnyView, App, AppContext as _, ClipboardItem, Context, IntoElement, Render, Window, div};
use delight_plugin_api::gpui::{Hsla, ParentElement as _, Styled as _, prelude::FluentBuilder as _};
use delight_plugin_api::{
    Action, Actions, AnyTool, Detection, Input, Operations, Plugin, Shortcut, Tool, host, plugin, theme,
};

#[plugin(
    id = "dev.delight.fixture",
    name = "Fixture",
    description = "Echoes the input, for Delight's tests",
    author = "Delight",
    icon = "assets/icon.svg",
    permissions = [Network("Nothing: it's here to test how permissions are read")],
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

    fn open_tool(&mut self, operation: FixtureOperation, cx: &mut App) -> AnyTool {
        match operation {
            FixtureOperation::Echo => cx.new(|_| Echo::default()).into(),
        }
    }

    fn settings_page(&mut self, cx: &mut App) -> Option<AnyView> {
        Some(cx.new(|_| FixtureSettings).into())
    }
}

/// The fixture's settings page: only there to be shown.
struct FixtureSettings;

impl Render for FixtureSettings {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().p_2().child("The fixture has no settings")
    }
}

#[derive(Default)]
struct Echo {
    text: String,
}

#[derive(Actions)]
enum EchoAction {
    Copy,
    Clear,
    /// Not in the footer: the tests perform it to see what the plugin can read.
    ReadClipboard,
}

impl Tool for Echo {
    type Action = EchoAction;

    fn on_input_changed(&mut self, input: &Input, cx: &mut Context<Self>) {
        self.text = input.text.clone();
        cx.notify();
    }

    fn list_actions(&self, _cx: &App) -> Vec<Action<EchoAction>> {
        let mut actions = vec![Action {
            id: EchoAction::Copy,
            label: "Copy".into(),
            shortcut: Shortcut::Keystroke("cmd-enter".into()),
        }];
        if !self.text.is_empty() {
            actions.push(Action {
                id: EchoAction::Clear,
                label: "Clear".into(),
                shortcut: Shortcut::ClickOnly,
            });
        }
        actions
    }

    fn perform_action(&mut self, action: EchoAction, cx: &mut Context<Self>) {
        match action {
            EchoAction::Copy => {
                cx.write_to_clipboard(ClipboardItem::new_string(self.text.clone()));
                host(cx).toast("Copied", cx);
                host(cx).remember_input(FixtureOperation::Echo, self.text.clone(), cx);
            }
            EchoAction::Clear => {
                self.text.clear();
                cx.notify();
            }
            EchoAction::ReadClipboard => {
                let text = cx.read_from_clipboard().and_then(|item| item.text());
                host(cx).toast(text.unwrap_or_else(|| "nothing".into()), cx);
            }
        }
    }
}

impl Render for Echo {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // The app's text colour, once the app has said what it is.
        let color = theme(cx).map(|theme| Hsla::from(theme.text));
        div().p_2().when_some(color, |div, color| div.text_color(color)).child(self.text.clone())
    }
}
