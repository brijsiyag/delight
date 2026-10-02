//! What the app knows for a plugin, headless: its secrets, the Mac's UTC offset, the
//! and opening its settings page.

use super::*;

#[gpui::test]
async fn a_plugin_saves_reads_and_deletes_a_secret(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let (plugin, app) = start("secrets", cx).await;
    let tool = tool_with(&plugin, "s3cret", cx).await;
    cx.update(|cx| drop(tool.perform_action("Secrets".into(), cx)));
    let toast = wait_for_toast(&app, cx, |toast| !toast.is_empty());
    assert_eq!(toast, r#"Some("s3cret") None"#);
    assert!(app.read_with(cx, |app, _| app.secrets.is_empty()), "deleted");
}

#[gpui::test]
async fn a_plugin_knows_the_macs_time_zone(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let (plugin, app) = start("facts", cx).await;
    let tool = tool_with(&plugin, "", cx).await;
    // The app answers a moment after the plugin starts.
    settle(cx);
    cx.update(|cx| drop(tool.perform_action("Facts".into(), cx)));
    let toast = wait_for_toast(&app, cx, |toast| toast.starts_with("offset "));
    assert_eq!(toast, "offset 19800");
}

#[gpui::test]
async fn a_plugin_opens_its_settings_page(cx: &mut TestAppContext) {
    let (plugin, app) = start("show-settings", cx).await;
    let tool = tool_with(&plugin, "", cx).await;
    cx.update(|cx| drop(tool.perform_action("ShowSettings".into(), cx)));
    settle(cx);
    assert_eq!(app.read_with(cx, |app, _| app.settings_shown), 1);
}

#[gpui::test]
async fn what_a_plugin_logs_is_in_the_apps_log_under_its_name(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let (plugin, _app) = start("plugin-log", cx).await;
    let tool = tool_with(&plugin, "", cx).await;
    cx.update(|cx| drop(tool.perform_action("Log".into(), cx)));
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        cx.executor().run_until_parked();
        let logged = LOGGED.lock().unwrap().clone();
        let found = |level, text: &str| logged.iter().any(|(target, l, message)| target == "plugin::Fixture" && *l == level && message == text);
        if found(log::Level::Warn, "the fixture warns") && found(log::Level::Error, "the fixture fails") {
            break;
        }
        assert!(Instant::now() < deadline, "not in the app's log: {logged:?}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[gpui::test]
async fn a_plugin_asks_the_user_to_confirm_and_hears_the_answer(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let (plugin, app) = start("confirm", cx).await;
    let tool = tool_with(&plugin, "", cx).await;
    for (answer, heard) in [(true, "confirmed"), (false, "cancelled")] {
        app.update(cx, |app, _| {
            app.confirm_answer = answer;
            app.toasts.clear();
        });
        cx.update(|cx| drop(tool.perform_action("Confirm".into(), cx)));
        assert_eq!(wait_for_toast(&app, cx, |toast| !toast.is_empty()), heard);
    }
    let asked = app.read_with(cx, |app, _| app.confirmations.clone());
    assert_eq!(asked.len(), 2);
    assert_eq!(asked[0], ("Remove it?".to_string(), "It cannot be undone.".to_string(), "Remove".to_string(), true));
}

#[gpui::test]
async fn a_plugin_opens_a_window_of_its_own_and_draws_in_it(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let (plugin, app) = start("open-window", cx).await;
    let tool = tool_with(&plugin, "", cx).await;
    cx.update(|cx| drop(tool.perform_action("OpenWindow".into(), cx)));
    let toast = wait_for_toast(&app, cx, |toast| !toast.is_empty());
    assert_eq!(toast, "window ok");
    let windows = app.read_with(cx, |app, _| app.windows.clone());
    assert_eq!(windows, [("fixture".to_string(), "Fixture window".to_string(), 500., 400., false)], "it stays up when the launcher hides");

    // It brings its window back on screen and closes it, and is told when it has no such window.
    cx.update(|cx| drop(tool.perform_action("ShowAndCloseWindow".into(), cx)));
    let toast = wait_for_toast(&app, cx, |toast| toast != "window ok");
    assert_eq!(toast, r#"ok, ok, the plugin has no window "none" open"#);
    let (shown, closed) = app.read_with(cx, |app, _| (app.windows_shown.clone(), app.windows_closed.clone()));
    assert_eq!(shown, ["fixture"]);
    assert_eq!(closed, ["fixture", "none"]);
}

#[gpui::test]
async fn a_plugin_saves_and_reads_its_settings_as_its_own_type(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let (plugin, app) = start("settings-typed", cx).await;
    let tool = tool_with(&plugin, "ada@example.com", cx).await;
    cx.update(|cx| drop(tool.perform_action("Settings".into(), cx)));
    let toast = wait_for_toast(&app, cx, |toast| !toast.is_empty());
    assert_eq!(toast, r#"Some(Saved { account: Some("ada@example.com"), range: 7 }) None"#);
    assert_eq!(app.read_with(cx, |app, _| app.settings.clone()), None, "cleared");
}

#[gpui::test]
async fn settings_that_do_not_fit_the_type_are_an_error_not_a_default(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let (plugin, app) = start("settings-mismatch", cx).await;
    app.update(cx, |app, _| app.settings = Some(r#"{"account":5,"range":"x"}"#.into()));
    let tool = tool_with(&plugin, "", cx).await;
    cx.update(|cx| drop(tool.perform_action("ReadSettings".into(), cx)));
    let toast = wait_for_toast(&app, cx, |toast| !toast.is_empty());
    assert!(toast.contains("don't fit the type"), "{toast}");
}

#[gpui::test]
async fn a_tool_sets_the_launchers_input(cx: &mut TestAppContext) {
    let (plugin, app) = start("set-input", cx).await;
    let tool = tool_with(&plugin, "inner value", cx).await;
    cx.update(|cx| drop(tool.perform_action("SetInput".into(), cx)));
    settle(cx);
    assert_eq!(app.read_with(cx, |app, _| app.inputs.clone()), ["inner value!"]);
}
