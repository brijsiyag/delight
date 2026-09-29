//! The fixture plugin (`fixture/`), run by delight-runtime without a window: read its
//! manifest from its `.wasm`, start it with a fake app root, detect, open its tool on a
//! surface that isn't in any window, give the tool input, read its actions, perform
//! one, and see a plugin that overruns its turn stopped. GPUI's test executor drives
//! everything deterministically.

use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, Once};
use std::time::Duration;

use delight_protocol::{
    Action, Color, HostApi, Input, Permission, PermissionRequest, Shortcut, Theme, ToolApi, ToolApiCaller as _,
};
use delight_runtime::{Candidate, Granted, Plugin, detect_all, plugin_options, read_manifest};
use embedded_gpui::{Remote, Surface, shared};
use gpui::{App, AppContext as _, Context, Entity, TestAppContext};

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
    toasts: Vec<String>,
    copied: Vec<String>,
    hides: usize,
    /// (operation, text) pairs.
    remembered: Vec<(String, String)>,
    /// How many times the plugin asked for the theme.
    theme_requests: usize,
}

#[shared]
impl HostApi for FakeApp {
    fn toast(&mut self, message: String, _cx: &mut Context<Self>) {
        self.toasts.push(message);
    }

    fn copy_text(&mut self, text: String, _cx: &mut Context<Self>) {
        self.copied.push(text);
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
}

/// `app` as the root `Plugin::start` makes: it has nothing to grant.
fn root_of(app: &Entity<FakeApp>) -> impl FnOnce(Granted, &mut App) -> Entity<FakeApp> + 'static {
    let app = app.clone();
    move |_granted, _| app
}

/// Run everything queued, including the plugin's turns and the timers they set.
fn settle(cx: &mut TestAppContext) {
    for _ in 0..5 {
        cx.executor().run_until_parked();
        cx.executor().advance_clock(Duration::from_millis(100));
    }
    cx.executor().run_until_parked();
}

/// A fresh data folder for one test's plugin.
fn data_dir(test: &str) -> PathBuf {
    std::env::temp_dir()
        .join("delight-tests")
        .join(format!("{test}-{}", std::process::id()))
}

/// Start the fixture with a fake app root, as the app will start any plugin.
async fn start(test: &str, cx: &mut TestAppContext) -> (Plugin, Entity<FakeApp>) {
    let wasm = fixture();
    let manifest = read_manifest(&std::fs::read(&wasm).unwrap()).unwrap();
    let options = plugin_options(&manifest, data_dir(test), Arc::new(gpui::NoopTextSystem::new()));
    let app = cx.new(|_| FakeApp::default());
    let started = cx.update(|cx| Plugin::start(wasm, manifest, options, root_of(&app), cx));
    settle(cx);
    (started.await.expect("the fixture starts"), app)
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
    assert_eq!(manifest.plugin.version, "0.1.0");
    assert_eq!(manifest.operations.len(), 1);
    assert_eq!(manifest.operations[0].id, "echo");
    assert!(manifest.plugin.icon.starts_with("<svg"));
    let reason = "Nothing: it's here to test how permissions are read".to_string();
    assert_eq!(manifest.plugin.permissions, [PermissionRequest { permission: Permission::Network, reason }]);
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
    assert_eq!(ids(&actions(&tool, cx).await), ["copy"]);

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
    assert_eq!(ids(&offered), ["copy", "clear"]);
    assert_eq!(offered[0].shortcut, Shortcut::Keystroke("cmd-enter".into()));
    assert_eq!(offered[1].shortcut, Shortcut::ClickOnly);

    // Performing Copy reaches the app's root object.
    let performed = cx.update(|cx| tool.perform_action("copy".into(), cx));
    settle(cx);
    performed.await.expect("perform_action");
    app.read_with(cx, |app, _| {
        assert_eq!(app.copied, ["hi"]);
        assert_eq!(app.toasts, ["Copied"]);
        assert_eq!(app.remembered, [("echo".to_string(), "hi".to_string())]);
        assert_eq!(app.hides, 0);
    });
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
    let started = cx.update(|cx| Plugin::start(wasm, manifest, options, root_of(&app), cx));
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
async fn its_settings_page_opens_on_a_surface(cx: &mut TestAppContext) {
    let (plugin, _app) = start("settings", cx).await;
    let surface = cx.new(Surface::new);
    let opened = cx.update(|cx| plugin.open_settings(&surface, cx));
    settle(cx);
    assert!(opened.await, "the fixture has a settings page");
    assert!(
        surface.read_with(cx, |surface, _| surface.view().is_some()),
        "it's drawn on the surface"
    );
}
