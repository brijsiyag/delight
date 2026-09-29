//! The plugin Delight's headless tests drive: one tool that shows the input, with
//! actions that depend on it, and some the tests perform to try what the app offers.

mod commands;
mod host_facts;
// TEMPORARY(network)
mod network;

use delight_plugin_api::gpui::{App, AppContext as _, ClipboardItem, Context, IntoElement, Render, Window, div};
use delight_plugin_api::gpui::{Hsla, ParentElement as _, Styled as _, prelude::FluentBuilder as _};
use delight_plugin_api::{
    Action, Actions, AnyTool, Confirm, Detection, Input, Operations, Plugin, SettingsSection, Shortcut, Tool, WindowOptions, host,
    plugin, settings_changed,
    theme,
};

#[plugin(
    id = "dev.delight.fixture",
    name = "Fixture",
    description = "Echoes the input, for Delight's tests",
    author = "Delight",
    icon = "assets/icon.svg",
    permissions = [
        Network("Nothing: it's here to test how permissions are read"),
        Commands(
            "Nothing: it's here to test running programs",
            programs = ["/bin/echo", "/bin/pwd", "/usr/bin/env", "/bin/cat"],
        ),
    ],
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

    fn settings_sections(&mut self, cx: &mut App) -> Vec<SettingsSection> {
        vec![
            SettingsSection::new("main", "Fixture", 41., cx.new(|_| FixtureSettings)).footer("Only for the tests"),
            SettingsSection::new("second", "Second", 82., cx.new(|_| FixtureSettings)),
        ]
    }
}

/// The fixture's settings sections' content: only there to be shown.
struct FixtureSettings;

impl Render for FixtureSettings {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().p_2().child("The fixture has no settings")
    }
}

#[derive(Default)]
struct Echo {
    text: String,
    /// TEMPORARY(network): the listener `Listen` opened, kept open.
    listener: Option<delight_plugin_api::network::HttpListener>,
    /// Requests `Hang` started and `Release` gives up on.
    hanging: Vec<delight_plugin_api::gpui::Task<()>>,
}

#[derive(Actions)]
enum EchoAction {
    Copy,
    Clear,
    /// Not in the footer: the tests perform it to see what the plugin can read.
    ReadClipboard,
    // TEMPORARY(network): not in the footer either; see `network`.
    Fetch,
    /// Start a request to `{input}/hang`, which is never answered, and keep it going; toasts what became of it.
    Hang,
    /// Give up on every request `Hang` started.
    Release,
    /// One request to `{input}/ok`; toasts "ok {status}" or the error.
    Ok,
    Listen,
    Grpc,
    /// Runs the input's first line as a program with the other lines as its arguments.
    Run,
    /// Secrets, the UTC offset, and the settings page.
    Secrets,
    Facts,
    ShowSettings,
    /// Log a warning and an error, to be found in the app's log.
    Log,
    /// Ask the user to confirm, destructively; toasts "confirmed" or "cancelled".
    Confirm,
    /// Ask the app for a window; toasts "window ok" once it has drawn there, or why not.
    OpenWindow,
    /// Says its settings sections changed.
    SectionsChanged,
    /// Settings: save, read back, clear; and read only.
    Settings,
    ReadSettings,
    /// Sets the launcher's input to the tool's text and an exclamation mark.
    SetInput,
    // TEMPORARY(open_url): opens the input as a URL, toasting "opened" or why not.
    OpenUrl,
}

impl Tool for Echo {
    type Action = EchoAction;

    fn on_input_changed(&mut self, input: &Input, cx: &mut Context<Self>) {
        self.text = input.text.clone();
        cx.notify();
    }

    fn list_actions(&self, _cx: &App) -> Vec<Action<EchoAction>> {
        let mut actions = vec![Action::new(EchoAction::Copy, "Copy", Shortcut::Keystroke("cmd-enter".into()))];
        if !self.text.is_empty() {
            actions.push(Action::new(EchoAction::Clear, "Clear", Shortcut::ClickOnly));
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
            // TEMPORARY(network)
            EchoAction::Fetch => network::fetch(&self.text.clone(), cx),
            EchoAction::Hang => {
                let task = network::hang(&self.text.clone(), cx);
                self.hanging.push(task);
            }
            EchoAction::Release => self.hanging.clear(),
            EchoAction::Ok => network::ok(&self.text.clone(), cx),
            EchoAction::Listen => network::listen(cx),
            EchoAction::Grpc => network::grpc(&self.text.clone(), cx),
            EchoAction::Run => commands::run(&self.text.clone(), cx),
            EchoAction::Secrets => host_facts::secrets(&self.text.clone(), cx),
            EchoAction::Facts => host_facts::facts(cx),
            EchoAction::Log => {
                log::warn!("the fixture warns");
                log::error!("the fixture fails");
            }
            EchoAction::Confirm => {
                let asked = host(cx).confirm(Confirm::new("Remove it?", "It cannot be undone.").continue_label("Remove").destructive(), cx);
                cx.spawn(async move |_, cx| {
                    let message = match asked.await {
                        Ok(true) => "confirmed".to_string(),
                        Ok(false) => "cancelled".to_string(),
                        Err(error) => format!("{error:#}"),
                    };
                    cx.update(|cx| host(cx).toast(message, cx));
                })
                .detach();
            }
            EchoAction::OpenWindow => {
                let opened = host(cx).open_window(WindowOptions::new("fixture", "Fixture window").size(500., 400.), cx.new(|_| FixtureSettings), cx);
                cx.spawn(async move |_, cx| {
                    let message = match opened.await {
                        Ok(()) => "window ok".to_string(),
                        Err(error) => format!("{error:#}"),
                    };
                    cx.update(|cx| host(cx).toast(message, cx));
                })
                .detach();
            }
            EchoAction::ShowSettings => host(cx).open_settings(cx),
            EchoAction::SectionsChanged => settings_changed(cx),
            EchoAction::Settings => host_facts::settings(&self.text.clone(), cx),
            EchoAction::ReadSettings => host_facts::read_settings(cx),
            EchoAction::SetInput => host(cx).set_input(format!("{}!", self.text), cx),
            // TEMPORARY(open_url)
            EchoAction::OpenUrl => {
                let opened = host(cx).open_url(self.text.clone(), cx);
                cx.spawn(async move |_, cx| {
                    let message = match opened.await {
                        Ok(()) => "opened".to_string(),
                        Err(error) => format!("{error:#}"),
                    };
                    cx.update(|cx| host(cx).toast(message, cx));
                })
                .detach();
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
