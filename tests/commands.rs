//! Running programs for a plugin, headless: the fixture runs the programs its manifest
//! lists, with arguments that are only text, and can't run any other, nor any without
//! the `Commands` permission.

use super::*;

/// Perform the fixture's `Run` with `input` (program, then arguments, a line each), and
/// the toast it makes.
async fn run(test: &str, input: &str, cx: &mut TestAppContext) -> String {
    cx.executor().allow_parking();
    let (plugin, app) = start(test, cx).await;
    let tool = tool_with(&plugin, input, cx).await;
    cx.update(|cx| drop(tool.perform_action("Run".into(), cx)));
    wait_for_toast(&app, cx, |toast| !toast.is_empty())
}

#[gpui::test]
async fn a_listed_program_runs_and_its_arguments_are_only_text(cx: &mut TestAppContext) {
    let toast = run("echo", "/bin/echo\nhi\n|\n;\n$(id)", cx).await;
    assert_eq!(toast, "Some(0)|hi | ; $(id)\n|");
}

#[gpui::test]
async fn it_runs_in_the_plugins_data_folder_with_nothing_in_the_environment(cx: &mut TestAppContext) {
    let toast = run("pwd", "/bin/pwd", cx).await;
    let folder = data_dir("pwd").canonicalize().unwrap();
    assert_eq!(toast, format!("Some(0)|{}\n|", folder.display()));
    assert_eq!(run("env", "/usr/bin/env", cx).await, "Some(0)||");
}

#[gpui::test]
async fn a_program_the_manifest_does_not_list_is_refused(cx: &mut TestAppContext) {
    let toast = run("unlisted", "/bin/ls", cx).await;
    // The app's refusal reaches the plugin as a failed call.
    assert_eq!(toast, "call failed: /bin/ls isn't one of the programs this plugin's manifest lists");
}

#[gpui::test]
async fn without_commands_a_plugin_runs_nothing(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let wasm = fixture();
    let mut manifest = read_manifest(&std::fs::read(&wasm).unwrap()).unwrap();
    manifest.plugin.permissions.retain(|request| !matches!(request.permission, Permission::Commands(_)));
    let permissions = Permissions::from(&manifest.plugin);
    let options = plugin_options(&manifest, &permissions, data_dir("no-commands"), Arc::new(gpui::NoopTextSystem::new()));
    let app = cx.new(|_| FakeApp::default());
    let started = cx.update(|cx| Plugin::start(wasm, manifest, permissions, options, data_dir("no-commands"), root_of(&app), cx));
    settle(cx);
    let plugin = started.await.expect("starts");
    let tool = tool_with(&plugin, "/bin/echo", cx).await;
    cx.update(|cx| drop(tool.perform_action("Run".into(), cx)));
    let toast = wait_for_toast(&app, cx, |toast| !toast.is_empty());
    assert_eq!(toast, "this plugin doesn't have the Commands permission");
}
