//! TEMPORARY(open_url): a plugin opening a URL, headless: the fake app checks it
//! as the app does and records it instead of opening a browser.

use super::*;

/// The fixture's tool with `url` as its input, after performing `OpenUrl`.
async fn open(url: &str, test: &str, cx: &mut TestAppContext) -> Entity<FakeApp> {
    let (plugin, app) = start(test, cx).await;
    let surface = cx.new(Surface::new);
    let tool = cx.update(|cx| plugin.open_tool("echo", &surface, cx));
    settle(cx);
    let tool = tool.await.expect("open_tool");
    cx.update(|cx| drop(tool.on_input_changed(input(url), cx)));
    settle(cx);
    let performed = cx.update(|cx| tool.perform_action("OpenUrl".into(), cx));
    settle(cx);
    performed.await.expect("perform_action");
    settle(cx);
    app
}

#[gpui::test]
async fn a_plugin_opens_a_web_page(cx: &mut TestAppContext) {
    let url = "https://accounts.example.com/sign-in?redirect=http://127.0.0.1:5000/callback";
    let app = open(url, "open-url", cx).await;
    app.read_with(cx, |app, _| {
        assert_eq!(app.opened, [url]);
        assert_eq!(app.toasts, ["opened"]);
    });
}

#[gpui::test]
async fn a_plugin_opens_no_files(cx: &mut TestAppContext) {
    let app = open("file:///etc/passwd", "open-file", cx).await;
    app.read_with(cx, |app, _| {
        assert!(app.opened.is_empty());
        assert_eq!(app.toasts.len(), 1);
        assert!(app.toasts[0].contains("a plugin can't open files"), "{:?}", app.toasts);
    });
}
