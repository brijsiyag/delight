//! The plugin Delight's headless tests drive: one tool that shows the input, with
//! actions that depend on it, and some the tests perform to try what the app offers.

mod commands;
mod host_facts;
// TEMPORARY(network)
mod network;

use delight_plugin_api::gpui::{App, AppContext as _, ClipboardItem, Context, IntoElement, Render, Window, div};
use delight_plugin_api::gpui::{Hsla, ParentElement as _, Styled as _, prelude::FluentBuilder as _};
use delight_plugin_api::{
    Action, Actions, AnyTool, Confirm, Detection, Input, Operations, Permission, Plugin, SettingsSection, Shortcut, Tool,
    WindowOptions, host,
    plugin, settings_changed,
    theme,
};
// TEMPORARY(pick_folders)
use delight_plugin_api::PickFolders;

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
        Files("Nothing: it's here to test folders"),
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
    /// Ask the app for a window that stays up when the launcher hides; toasts "window ok" once it
    /// has drawn there, or why not.
    OpenWindow,
    /// Brings that window back on screen and closes it, then closes one it doesn't have; toasts
    /// what became of each.
    ShowAndCloseWindow,
    /// Says its settings sections changed.
    SectionsChanged,
    /// Spins forever: the turn budget stops the plugin in a call to its tool, not to its root.
    Spin,
    /// Settings: save, read back, clear; and read only.
    Settings,
    ReadSettings,
    /// Sets the launcher's input to the tool's text and an exclamation mark.
    SetInput,
    // TEMPORARY(open_url): opens the input as a URL, toasting "opened" or why not.
    OpenUrl,
    /// Lists the folder the input names with `std::fs`: toasts its entries' names, sorted and
    /// joined by ", ", or why not.
    ListFolder,
    /// Writes "hi" to the file the input names with `std::fs`: toasts "written", or why not.
    WriteFile,
    /// Toasts `HOME`, or "no HOME".
    Home,
    /// Asks for the folder the input names, to write: toasts "allowed", "declined", or
    /// why it can't.
    RequestFolder,
    /// Asks to run the program the input names: toasts "allowed", "declined", or why it can't.
    RequestProgram,
    // TEMPORARY(pick_folders): opens the folder picker for several folders, read-only: toasts what
    // was picked, joined by ", ", "none", or why not.
    PickFolders,
    // TEMPORARY(save_file): saves the input as `notes.txt`: toasts where it went, "cancelled", or
    // why not.
    SaveFile,
}

impl Tool for Echo {
    type Action = EchoAction;

    fn on_input_changed(&mut self, input: &Input, cx: &mut Context<Self>) {
        self.text = input.text.clone();
        cx.notify();
    }

    fn list_actions(&self, _cx: &App) -> Vec<Action<EchoAction>> {
        let mut actions = vec![Action::new(EchoAction::Copy, "Copy", Shortcut::CmdEnter)];
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
            #[allow(clippy::empty_loop)]
            EchoAction::Spin => loop {},
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
            EchoAction::ListFolder => {
                let listed = std::fs::read_dir(self.text.trim()).and_then(|entries| {
                    let mut names = entries.map(|entry| Ok(entry?.file_name().to_string_lossy().into_owned())).collect::<std::io::Result<Vec<_>>>()?;
                    names.sort();
                    Ok(names.join(", "))
                });
                host(cx).toast(listed.unwrap_or_else(|error| format!("{error}")), cx);
            }
            EchoAction::WriteFile => {
                let written = std::fs::write(self.text.trim(), "hi").map(|()| "written".to_string());
                host(cx).toast(written.unwrap_or_else(|error| format!("{error}")), cx);
            }
            EchoAction::Home => host(cx).toast(std::env::var("HOME").unwrap_or_else(|_| "no HOME".into()), cx),
            EchoAction::RequestFolder => {
                let none: [&str; 0] = [];
                let asked = host(cx).request_permission(Permission::files(none, [self.text.trim()]), "Nothing: it's here to test asking", cx);
                toast_when_done(asked, |allowed| if allowed { "allowed".into() } else { "declined".into() }, cx);
            }
            EchoAction::RequestProgram => {
                let asked = host(cx).request_permission(Permission::commands([self.text.trim()]), "Nothing: it's here to test asking", cx);
                toast_when_done(asked, |allowed| if allowed { "allowed".into() } else { "declined".into() }, cx);
            }
            // TEMPORARY(pick_folders)
            EchoAction::PickFolders => {
                let picked = host(cx).pick_folders(PickFolders::new().multiple().prompt("Choose"), cx);
                let said = |picked: Vec<std::path::PathBuf>| {
                    if picked.is_empty() { "none".into() } else { picked.iter().map(|path| path.display().to_string()).collect::<Vec<_>>().join(", ") }
                };
                toast_when_done(picked, said, cx);
            }
            // TEMPORARY(save_file)
            EchoAction::SaveFile => {
                let saved = host(cx).save_file("notes.txt", self.text.clone(), cx);
                toast_when_done(saved, |saved| saved.map_or_else(|| "cancelled".into(), |path| path.display().to_string()), cx);
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
                let options = WindowOptions::new("fixture", "Fixture window").size(500., 400.).hide_with_launcher(false);
                let opened = host(cx).open_window(options, cx.new(|_| FixtureSettings), cx);
                cx.spawn(async move |_, cx| {
                    let message = match opened.await {
                        Ok(()) => "window ok".to_string(),
                        Err(error) => format!("{error:#}"),
                    };
                    cx.update(|cx| host(cx).toast(message, cx));
                })
                .detach();
            }
            EchoAction::ShowAndCloseWindow => {
                let asked = [host(cx).show_window("fixture", cx), host(cx).close_window("fixture", cx), host(cx).close_window("none", cx)];
                cx.spawn(async move |_, cx| {
                    let mut said = Vec::new();
                    for answer in asked {
                        said.push(match answer.await {
                            Ok(()) => "ok".to_string(),
                            Err(error) => format!("{error:#}"),
                        });
                    }
                    cx.update(|cx| host(cx).toast(said.join(", "), cx));
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

    fn on_shown(&mut self, cx: &mut Context<Self>) {
        host(cx).toast("shown", cx);
    }

    fn on_hidden(&mut self, cx: &mut Context<Self>) {
        host(cx).toast("hidden", cx);
    }
}

impl Render for Echo {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // The app's text colour, once the app has said what it is.
        let color = theme(cx).map(|theme| Hsla::from(theme.text));
        div().p_2().when_some(color, |div, color| div.text_color(color)).child(self.text.clone())
    }
}

/// Toast what `task` came to, said by `say`, or its error.
fn toast_when_done<T: 'static, E: std::fmt::Display + 'static>(
    task: delight_plugin_api::gpui::Task<Result<T, E>>,
    say: impl FnOnce(T) -> String + 'static,
    cx: &mut Context<Echo>,
) {
    cx.spawn(async move |_, cx| {
        let message = match task.await {
            Ok(value) => say(value),
            Err(error) => format!("{error:#}"),
        };
        cx.update(|cx| host(cx).toast(message, cx));
    })
    .detach();
}

