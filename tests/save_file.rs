//! TEMPORARY(save_file): a plugin saving a file through the save panel, headless: the fake app
//! answers for the user and keeps what it was given.

use super::*;

#[gpui::test]
async fn a_plugin_saves_a_file(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let (plugin, app) = start("saves-a-file", cx).await;

    // Cancelled, then saved; the app gets the contents.
    assert_eq!(toasted(&plugin, &app, "SaveFile", "hello", cx).await, "cancelled");
    app.update(cx, |app, _| app.save_to = Some("/Users/ada/Downloads/notes.txt".into()));
    assert_eq!(toasted(&plugin, &app, "SaveFile", "hello", cx).await, "/Users/ada/Downloads/notes.txt");
    assert_eq!(app.read_with(cx, |app, _| app.saved[1].clone()), ("notes.txt".to_string(), b"hello".to_vec()));
}
