//! The fixture plugin (`fixture/`), run by delight-runtime without a window: read its
//! manifest from its `.wasm`, start it with a fake app root, detect, open its tool on a
//! surface that isn't in any window, give the tool input, read its actions, perform
//! one, and see a plugin that overruns its turn stopped. GPUI's test executor drives
//! everything deterministically.

use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, Once};
use std::time::{Duration, Instant};

use delight_protocol::{
    Action, Color, CommandsApi, DnsApi, HostApi, HttpApi, Input, Permission, PermissionRequest, Shortcut, Theme, ToolApi, ToolApiCaller as _,
};
use delight_runtime::{Candidate, Granted, Plugin, detect_all, plugin_options, read_manifest};
use embedded_gpui::{ClipboardApi, Ref, Remote, Surface, shared};
use gpui::{App, AppContext as _, ClipboardItem, Context, Entity, TestAppContext};

/// Builds the fixture once per test run and returns its `.wasm`.
fn fixture() -> PathBuf {
    static BUILD: Once = Once::new();
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixture");
    BUILD.call_once(|| {
        let output = std::process::Command::new("cargo")
            .args(["build", "--target", "wasm32-wasip2"])
            .current_dir(&dir)
            .output()
            .expect("running cargo to build the fixture");
        assert!(
            output.status.success(),
            "building the fixture failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
    });
    dir.join("target/wasm32-wasip2/debug/delight_fixture.wasm")
}

/// The app's root object for the plugin, recording what the plugin asks of it.
#[derive(Default)]
struct FakeApp {
    /// What the app hands the plugin, as the app has it: given as the plugin starts.
    granted: Option<Granted>,
    toasts: Vec<String>,
    hides: usize,
    /// (operation, text) pairs.
    remembered: Vec<(String, String)>,
    /// How many times the plugin asked for the theme.
    theme_requests: usize,
    /// TEMPORARY(open_url): the pages the plugin opened.
    opened: Vec<String>,
    /// What the plugin set the launcher's input to.
    inputs: Vec<String>,
    /// The plugin's saved settings, as JSON.
    settings: Option<String>,
    /// What the plugin saved as secrets.
    secrets: std::collections::HashMap<String, String>,
    /// How many times the plugin asked for its settings page.
    settings_shown: usize,
    /// The windows the plugin asked for: (key, title, width, height, whether it hides with the
    /// launcher).
    windows: Vec<(String, String, f32, f32, Option<bool>)>,
    /// What the plugin asked of its windows' showing: (key, shown).
    windows_shown: Vec<(String, bool)>,
    /// The plugin itself, to ask it to draw in them; the surfaces they draw on.
    plugin: Option<Plugin>,
    window_surfaces: Vec<Entity<Surface>>,
    /// The alerts the plugin asked for: (title, message, continue label, destructive); what the
    /// user answers to them.
    confirmations: Vec<(String, String, String, bool)>,
    confirm_answer: bool,
}

#[shared]
impl HostApi for FakeApp {
    fn toast(&mut self, message: String, _cx: &mut Context<Self>) {
        self.toasts.push(message);
    }

    fn hide(&mut self, _cx: &mut Context<Self>) {
        self.hides += 1;
    }

    fn remember_input(&mut self, operation: String, text: String, _cx: &mut Context<Self>) {
        self.remembered.push((operation, text));
    }

    fn current_theme(&mut self, _cx: &mut Context<Self>) -> Theme {
        self.theme_requests += 1;
        let gray = |l| Color { h: 0., s: 0., l, a: 1. };
        Theme {
            dark: true,
            text: gray(0.9),
            text_muted: gray(0.6),
            text_faint: gray(0.4),
            surface: gray(0.2),
            fill: gray(0.3),
            border: gray(0.3),
            accent: gray(0.5),
            accent_text: gray(1.),
            success: gray(0.5),
            warning: gray(0.5),
            error: gray(0.5),
            font: "Test".into(),
            mono_font: "Test Mono".into(),
            text_size: 13.,
            radius: 8.,
        }
    }

    fn clipboard(&mut self, cx: &mut Context<Self>) -> Ref<ClipboardApi> {
        self.granted.as_ref().expect("given as the plugin starts").clipboard(cx)
    }

    fn http(&mut self, cx: &mut Context<Self>) -> Option<Ref<HttpApi>> {
        self.granted.as_ref().expect("given as the plugin starts").http(cx)
    }

    fn dns(&mut self, cx: &mut Context<Self>) -> Option<Ref<DnsApi>> {
        self.granted.as_ref().expect("given as the plugin starts").dns(cx)
    }

    fn secret(&mut self, key: String, _cx: &mut Context<Self>) -> gpui::Task<anyhow::Result<Option<String>>> {
        gpui::Task::ready(Ok(self.secrets.get(&key).cloned()))
    }

    fn set_secret(&mut self, key: String, value: String, _cx: &mut Context<Self>) -> gpui::Task<anyhow::Result<()>> {
        if value.is_empty() {
            self.secrets.remove(&key);
        } else {
            self.secrets.insert(key, value);
        }
        gpui::Task::ready(Ok(()))
    }

    fn set_launcher_input(&mut self, text: String, _cx: &mut Context<Self>) {
        self.inputs.push(text);
    }

    fn settings(&mut self, _cx: &mut Context<Self>) -> gpui::Task<anyhow::Result<String>> {
        gpui::Task::ready(Ok(self.settings.clone().unwrap_or_else(|| "null".into())))
    }

    fn set_settings(&mut self, json: String, _cx: &mut Context<Self>) -> gpui::Task<anyhow::Result<()>> {
        self.settings = (json != "null").then_some(json);
        gpui::Task::ready(Ok(()))
    }

    fn utc_offset_seconds(&mut self, _cx: &mut Context<Self>) -> i32 {
        19_800
    }

    fn show_settings(&mut self, _cx: &mut Context<Self>) {
        self.settings_shown += 1;
    }

    // As the app does it, without an alert: the user answers what the test says.
    fn confirm(&mut self, title: String, message: String, continue_label: String, destructive: bool, _cx: &mut Context<Self>) -> gpui::Task<anyhow::Result<bool>> {
        self.confirmations.push((title, message, continue_label, destructive));
        gpui::Task::ready(Ok(self.confirm_answer))
    }

    // As the app does it, without a window: a surface for the plugin to draw on.
    fn open_window(
        &mut self,
        key: String,
        title: String,
        width: f32,
        height: f32,
        hide_with_launcher: Option<bool>,
        cx: &mut Context<Self>,
    ) -> gpui::Task<anyhow::Result<bool>> {
        self.windows.push((key.clone(), title, width, height, hide_with_launcher));
        let surface = cx.new(Surface::new);
        self.window_surfaces.push(surface.clone());
        let Some(plugin) = self.plugin.clone() else { return gpui::Task::ready(Ok(false)) };
        let drawn = plugin.open_window_view(&key, &surface, cx);
        cx.spawn(async move |_, _| Ok(drawn.await))
    }

    // As the app does it: whether that window is open.
    fn set_window_shown(&mut self, key: String, shown: bool, _cx: &mut Context<Self>) -> bool {
        let open = self.windows.iter().any(|window| window.0 == key);
        self.windows_shown.push((key, shown));
        open
    }

    fn commands(&mut self, cx: &mut Context<Self>) -> Option<Ref<CommandsApi>> {
        self.granted.as_ref().expect("given as the plugin starts").commands(cx)
    }

    // TEMPORARY(open_url): as the app, with its check, but without a browser.
    fn open_url(&mut self, url: String, _cx: &mut Context<Self>) -> gpui::Task<anyhow::Result<()>> {
        let checked = delight_runtime::open_url::openable(&url).map(|url| self.opened.push(url.to_string()));
        gpui::Task::ready(checked)
    }
}

/// `app` as the root `Plugin::start` makes, given what the app hands the plugin.
fn root_of(app: &Entity<FakeApp>) -> impl FnOnce(Granted, &mut App) -> Entity<FakeApp> + 'static {
    let app = app.clone();
    move |granted, cx| {
        app.update(cx, |app, _| app.granted = Some(granted));
        app
    }
}

/// The text on the (test) clipboard.
fn clipboard_text(cx: &mut TestAppContext) -> Option<String> {
    cx.read_from_clipboard().and_then(|item| item.text())
}

/// Run everything queued, including the plugin's turns and the timers they set.
fn settle(cx: &mut TestAppContext) {
    for _ in 0..5 {
        cx.executor().run_until_parked();
        cx.executor().advance_clock(Duration::from_millis(100));
    }
    cx.executor().run_until_parked();
}

/// Keep the app and the plugin running (the network's replies come from other
/// threads) until `found` finds a toast; the toast it found.
fn wait_for_toast(app: &Entity<FakeApp>, cx: &mut TestAppContext, found: impl Fn(&str) -> bool) -> String {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        cx.executor().run_until_parked();
        let toasts = app.read_with(cx, |app, _| app.toasts.clone());
        if let Some(toast) = toasts.iter().find(|toast| found(toast)) {
            return toast.clone();
        }
        assert!(Instant::now() < deadline, "no such toast; the toasts: {toasts:?}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// The fixture's tool, with `text` as its input.
async fn tool_with(plugin: &Plugin, text: &str, cx: &mut TestAppContext) -> Remote<ToolApi> {
    let surface = cx.new(Surface::new);
    let tool = cx.update(|cx| plugin.open_tool("echo", &surface, cx));
    settle(cx);
    let tool = tool.await.expect("open_tool");
    cx.update(|cx| drop(tool.on_input_changed(input(text), cx)));
    settle(cx);
    tool
}

/// A fresh data folder for one test's plugin.
fn data_dir(test: &str) -> PathBuf {
    std::env::temp_dir()
        .join("delight-tests")
        .join(format!("{test}-{}", std::process::id()))
}

/// What the app logged during the tests: (target, level, text), for the tests to look through.
static LOGGED: std::sync::Mutex<Vec<(String, log::Level, String)>> = std::sync::Mutex::new(Vec::new());

struct Capture;

impl log::Log for Capture {
    fn enabled(&self, _: &log::Metadata) -> bool {
        true
    }

    fn log(&self, record: &log::Record) {
        if let Ok(mut logged) = LOGGED.lock() {
            logged.push((record.target().to_string(), record.level(), record.args().to_string()));
        }
    }

    fn flush(&self) {}
}

/// The app's logger, for the tests: one that keeps what is logged.
fn capture_logs() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        if log::set_logger(&Capture).is_ok() {
            log::set_max_level(log::LevelFilter::Info);
        }
    });
}

/// Start the fixture with a fake app root, as the app will start any plugin.
async fn start(test: &str, cx: &mut TestAppContext) -> (Plugin, Entity<FakeApp>) {
    capture_logs();
    let wasm = fixture();
    let manifest = read_manifest(&std::fs::read(&wasm).unwrap()).unwrap();
    let options = plugin_options(&manifest, data_dir(test), Arc::new(gpui::NoopTextSystem::new()));
    let app = cx.new(|_| FakeApp::default());
    let started = cx.update(|cx| Plugin::start(wasm, manifest, options, data_dir(test), root_of(&app), cx));
    settle(cx);
    let started = started.await.expect("the fixture starts");
    app.update(cx, |app, _| app.plugin = Some(started.clone()));
    (started, app)
}

fn input(text: &str) -> Input {
    Input { text: text.into() }
}

async fn actions(tool: &Remote<ToolApi>, cx: &mut TestAppContext) -> Vec<Action> {
    let actions = cx.update(|cx| tool.list_actions(cx));
    settle(cx);
    actions.await.expect("list_actions")
}

#[test]
fn the_manifest_is_read_from_the_wasm() {
    let manifest = read_manifest(&std::fs::read(fixture()).unwrap()).unwrap();
    assert_eq!(manifest.plugin.id, "dev.delight.fixture");
    assert_eq!(manifest.plugin.version, "0.0.4");
    assert_eq!(manifest.operations.len(), 1);
    assert_eq!(manifest.operations[0].id, "echo");
    assert!(manifest.plugin.icon.starts_with("<svg"));
    let reason = "Nothing: it's here to test how permissions are read".to_string();
    let network = PermissionRequest { permission: Permission::network(), reason };
    let commands = PermissionRequest {
        permission: Permission::commands(["/bin/echo", "/bin/pwd", "/usr/bin/env", "/bin/cat"]),
        reason: "Nothing: it's here to test running programs".into(),
    };
    assert_eq!(manifest.plugin.permissions, [network, commands]);
}

#[gpui::test]
async fn it_starts_in_its_sandbox_and_detects(cx: &mut TestAppContext) {
    let (plugin, _app) = start("detects", cx).await;
    assert!(data_dir("detects").is_dir(), "its data folder is created");

    let detected = cx.update(|cx| plugin.detect(&input("hello"), cx));
    settle(cx);
    let detected = detected.await.unwrap();
    assert_eq!(detected.len(), 1);
    assert_eq!(detected[0].operation, "echo");
    assert_eq!(detected[0].confidence, 1.0);

    let detected = cx.update(|cx| plugin.detect(&input("  "), cx));
    settle(cx);
    assert!(detected.await.unwrap().is_empty());
}

#[gpui::test]
async fn detect_all_asks_every_plugin_and_ranks(cx: &mut TestAppContext) {
    let (first, _) = start("ranks-1", cx).await;
    let (second, _) = start("ranks-2", cx).await;
    let plugins = [first, second];

    let ranked = cx.update(|cx| detect_all(&plugins, &input("hi"), cx));
    settle(cx);
    let ranked = ranked.await;
    let expected = |plugin| Candidate {
        plugin,
        operation: 0,
        confidence: 1.0,
    };
    assert_eq!(ranked, [expected(0), expected(1)]);

    let blank = cx.update(|cx| detect_all(&plugins, &input(""), cx));
    assert!(blank.await.is_empty());
}

#[gpui::test]
async fn its_tool_takes_input_and_offers_actions(cx: &mut TestAppContext) {
    let (plugin, app) = start("tool", cx).await;

    // A surface in no window: the tool draws on it all the same.
    let surface = cx.new(Surface::new);
    let tool = cx.update(|cx| plugin.open_tool("echo", &surface, cx));
    settle(cx);
    let tool = tool.await.expect("open_tool");
    assert!(
        surface.read_with(cx, |surface, _| surface.view().is_some()),
        "the tool's view is attached to the surface"
    );

    // Empty text: only Copy.
    let ids = |actions: &[Action]| actions.iter().map(|a| a.id.clone()).collect::<Vec<_>>();
    // An action's id is its variant's name.
    assert_eq!(ids(&actions(&tool, cx).await), ["Copy"]);

    // Text: the tool notifies, and now offers Clear too.
    let notified = Rc::new(Cell::new(0));
    let _observing = cx.update(|cx| {
        let notified = notified.clone();
        tool.observe(cx, move |_| notified.set(notified.get() + 1))
    });
    settle(cx);
    let before = notified.get();
    cx.update(|cx| drop(tool.on_input_changed(input("hi"), cx)));
    settle(cx);
    assert!(notified.get() > before, "the app hears that the actions changed");
    let offered = actions(&tool, cx).await;
    assert_eq!(ids(&offered), ["Copy", "Clear"]);
    assert_eq!(offered[0].shortcut, Shortcut::Keystroke("cmd-enter".into()));
    assert_eq!(offered[1].shortcut, Shortcut::ClickOnly);

    // Performing Copy puts the text on the clipboard, through GPUI's own
    // `write_to_clipboard` in the plugin, and reaches the app's root object.
    let performed = cx.update(|cx| tool.perform_action("Copy".into(), cx));
    settle(cx);
    performed.await.expect("perform_action");
    settle(cx);
    assert_eq!(clipboard_text(cx).as_deref(), Some("hi"));

    // An id the tool has no action for is ignored.
    let performed = cx.update(|cx| tool.perform_action("Nope".into(), cx));
    settle(cx);
    performed.await.expect("perform_action");
    app.read_with(cx, |app, _| {
        assert_eq!(app.toasts, ["Copied"]);
        assert_eq!(app.remembered, [("echo".to_string(), "hi".to_string())]);
        assert_eq!(app.hides, 0);
    });
    assert_eq!(plugin.stopped(), None);
}

#[gpui::test]
async fn a_tool_is_told_when_its_view_is_shown_and_hidden(cx: &mut TestAppContext) {
    let (plugin, app) = start("visibility", cx).await;
    let tool = tool_with(&plugin, "hi", cx).await;
    // The fixture toasts `on_shown` and `on_hidden`, in the order the app tells it.
    for shown in [true, false, true] {
        let told = cx.update(|cx| tool.visibility_changed(shown, cx));
        settle(cx);
        told.await.expect("visibility_changed");
    }
    settle(cx);
    app.read_with(cx, |app, _| assert_eq!(app.toasts, ["shown", "hidden", "shown"]));
    assert_eq!(plugin.stopped(), None);
}

#[gpui::test]
async fn an_unknown_operation_is_an_error_not_a_stop(cx: &mut TestAppContext) {
    let (plugin, _app) = start("unknown", cx).await;
    let surface = cx.new(Surface::new);
    let tool = cx.update(|cx| plugin.open_tool("nope", &surface, cx));
    settle(cx);
    let error = tool.await.expect_err("an unknown operation fails");
    assert!(format!("{error:#}").contains("no operation"), "{error:#}");
    assert_eq!(plugin.stopped(), None);

    let detected = cx.update(|cx| plugin.detect(&input("still here"), cx));
    settle(cx);
    assert_eq!(detected.await.unwrap().len(), 1);
}

#[gpui::test]
async fn a_plugin_that_overruns_its_turn_is_stopped(cx: &mut TestAppContext) {
    let wasm = fixture();
    let manifest = read_manifest(&std::fs::read(&wasm).unwrap()).unwrap();
    let options = plugin_options(&manifest, data_dir("stops"), Arc::new(gpui::NoopTextSystem::new()))
        .with_turn_budget(Duration::from_millis(200));
    let app = cx.new(|_| FakeApp::default());
    let started = cx.update(|cx| Plugin::start(wasm, manifest, options, data_dir("stops"), root_of(&app), cx));
    settle(cx);
    let plugin = started.await.unwrap();

    // The fixture spins forever on "hang"; the turn budget stops it.
    let hung = cx.update(|cx| plugin.detect(&input("hang"), cx));
    settle(cx);
    let error = hung.await.expect_err("the call fails");
    assert!(format!("{error:#}").contains("plugin stopped"), "{error:#}");
    let reason = plugin.stopped().expect("the plugin is marked stopped");
    assert!(reason.contains("wasm trap: interrupt"), "{reason}");

    // After that it isn't called again: calls fail at once, and detect_all skips it.
    let again = cx.update(|cx| plugin.detect(&input("hello"), cx));
    let error = again.await.expect_err("a stopped plugin isn't called");
    assert!(format!("{error:#}").contains("Fixture stopped"), "{error:#}");
    let ranked = cx.update(|cx| detect_all(std::slice::from_ref(&plugin), &input("hello"), cx));
    settle(cx);
    assert!(ranked.await.is_empty());
}

#[gpui::test]
async fn a_plugin_that_stops_outside_the_apps_calls_is_marked_stopped_at_once(cx: &mut TestAppContext) {
    let wasm = fixture();
    let manifest = read_manifest(&std::fs::read(&wasm).unwrap()).unwrap();
    let options = plugin_options(&manifest, data_dir("stops-in-a-tool"), Arc::new(gpui::NoopTextSystem::new()))
        .with_turn_budget(Duration::from_millis(200));
    let app = cx.new(|_| FakeApp::default());
    let started = cx.update(|cx| Plugin::start(wasm, manifest, options, data_dir("stops-in-a-tool"), root_of(&app), cx));
    settle(cx);
    let plugin = started.await.unwrap();

    // It stops in its tool, not in a call through `Plugin`: the app sees it from the host.
    let tool = tool_with(&plugin, "hi", cx).await;
    cx.update(|cx| drop(tool.perform_action("Spin".into(), cx)));
    settle(cx);
    let reason = plugin.stopped().expect("the plugin is marked stopped");
    assert!(reason.contains("plugin stopped"), "{reason}");

    // So the next call fails at once, instead of going out and waiting for CALL_TIMEOUT.
    let again = cx.update(|cx| plugin.detect(&input("hello"), cx));
    let error = futures::FutureExt::now_or_never(again)
        .expect("a stopped plugin's call is answered at once")
        .expect_err("a stopped plugin isn't called");
    assert!(format!("{error:#}").contains("Fixture stopped"), "{error:#}");
}

#[gpui::test]
async fn the_plugin_follows_the_apps_theme(cx: &mut TestAppContext) {
    let (_plugin, app) = start("theme", cx).await;
    settle(cx);
    let asked = app.read_with(cx, |app, _| app.theme_requests);
    assert!(asked >= 1, "the plugin asks for the theme when it starts");

    // The app's theme changed: it notifies its object, and the plugin asks again.
    app.update(cx, |_, cx| cx.notify());
    settle(cx);
    assert!(app.read_with(cx, |app, _| app.theme_requests) > asked, "the plugin asks again");
}

#[gpui::test]
async fn its_settings_are_sections_drawn_on_surfaces(cx: &mut TestAppContext) {
    let (plugin, _app) = start("settings", cx).await;
    let asked = cx.update(|cx| plugin.settings_sections(cx));
    settle(cx);
    let sections = asked.await.expect("the sections");
    assert_eq!(
        sections.iter().map(|s| (s.id.as_str(), s.title.as_str(), s.height, s.footer.as_str())).collect::<Vec<_>>(),
        [("main", "Fixture", 41., "Only for the tests"), ("second", "Second", 82., "")]
    );

    let surface = cx.new(Surface::new);
    let opened = cx.update(|cx| plugin.open_settings_section("second", &surface, cx));
    settle(cx);
    assert!(opened.await, "the section is drawn");
    assert!(surface.read_with(cx, |surface, _| surface.view().is_some()), "on the surface");

    let other = cx.new(Surface::new);
    let missing = cx.update(|cx| plugin.open_settings_section("nope", &other, cx));
    settle(cx);
    assert!(!missing.await, "a section it hasn't");
    assert!(other.read_with(cx, |surface, _| surface.view().is_none()));
}

#[gpui::test]
async fn the_app_hears_when_the_sections_change(cx: &mut TestAppContext) {
    let (plugin, _app) = start("sections-changed", cx).await;
    let heard = Rc::new(Cell::new(0));
    let counted = heard.clone();
    let _observing = cx.update(|cx| plugin.observe_settings(cx, move |_| counted.set(counted.get() + 1)));
    settle(cx);
    let before = heard.get();

    let tool = tool_with(&plugin, "", cx).await;
    cx.update(|cx| drop(tool.perform_action("SectionsChanged".into(), cx)));
    settle(cx);
    assert!(heard.get() > before, "the plugin said its sections changed");
}

#[gpui::test]
async fn the_plugin_reads_the_clipboard(cx: &mut TestAppContext) {
    // What's on the clipboard when the plugin starts, which it reads then; after that
    // embedded_gpui's surface looks again before each ⌘ key it forwards (⌘V).
    cx.write_to_clipboard(ClipboardItem::new_string("copied elsewhere".into()));
    let (plugin, app) = start("clipboard", cx).await;
    let surface = cx.new(Surface::new);
    let tool = cx.update(|cx| plugin.open_tool("echo", &surface, cx));
    settle(cx);
    let tool = tool.await.expect("open_tool");

    // The fixture toasts what GPUI's `read_from_clipboard` gives it.
    let performed = cx.update(|cx| tool.perform_action("ReadClipboard".into(), cx));
    settle(cx);
    performed.await.expect("perform_action");
    app.read_with(cx, |app, _| assert_eq!(app.toasts, ["copied elsewhere"]));
}

// TEMPORARY(clipboard): with the workaround it tests, removed with it.
#[gpui::test]
async fn a_plugin_reads_what_was_copied_after_the_app_refreshed_its_clipboard(cx: &mut TestAppContext) {
    cx.write_to_clipboard(ClipboardItem::new_string("first".into()));
    let (plugin, app) = start("clipboard-refresh", cx).await;
    let surface = cx.new(Surface::new);
    let tool = cx.update(|cx| plugin.open_tool("echo", &surface, cx));
    settle(cx);
    let tool = tool.await.expect("open_tool");

    // Copied elsewhere since. Until the app looks, the plugin still reads what it had.
    cx.write_to_clipboard(ClipboardItem::new_string("second".into()));
    cx.update(|cx| drop(tool.perform_action("ReadClipboard".into(), cx)));
    settle(cx);
    // The app looks when one of its windows gets the keyboard or is clicked: the plugin's next
    // paste reads what was copied last.
    cx.update(|cx| plugin.refresh_clipboard(cx));
    settle(cx);
    cx.update(|cx| drop(tool.perform_action("ReadClipboard".into(), cx)));
    settle(cx);
    app.read_with(cx, |app, _| assert_eq!(app.toasts, ["first", "second"]));
}

mod commands;
mod host_facts;

// TEMPORARY(network): the app's HTTP for plugins.
mod network;

// TEMPORARY(open_url): opening a URL.
mod open_url;
