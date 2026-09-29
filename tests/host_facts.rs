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
