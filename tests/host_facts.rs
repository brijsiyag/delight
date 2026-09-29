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
