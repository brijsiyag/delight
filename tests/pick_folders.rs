//! TEMPORARY(pick_folders): a plugin opening the folder picker, headless: the fake app answers
//! for the user.

use super::*;

#[gpui::test]
async fn a_plugin_picks_folders(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let (plugin, app) = start("picks-folders", cx).await;

    // Cancelled, then two picked.
    assert_eq!(toasted(&plugin, &app, "PickFolders", "", cx).await, "none");
    app.update(cx, |app, _| app.picked = vec!["/Users/ada/a".into(), "/Users/ada/b".into()]);
    assert_eq!(toasted(&plugin, &app, "PickFolders", "", cx).await, "/Users/ada/a, /Users/ada/b");
    assert_eq!(app.read_with(cx, |app, _| app.pickers[0].clone()), (true, false, Some("Choose".to_string())));
}
