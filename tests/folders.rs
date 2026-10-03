//! Folders a plugin has (`Files`): each in its sandbox where it is on the Mac, for `std::fs`,
//! read-only unless it may write there, and nothing outside them.

use super::*;

#[gpui::test]
async fn a_plugin_reads_and_writes_its_folders_with_std_fs_and_nothing_else(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let root = data_dir("folders-outside");
    let (reads, writes) = (root.join("reads"), root.join("writes"));
    for folder in [&reads, &writes] {
        std::fs::create_dir_all(folder).unwrap();
    }
    std::fs::write(reads.join("a.txt"), "a").unwrap();
    std::fs::write(reads.join("b.txt"), "b").unwrap();
    std::fs::write(root.join("secret.txt"), "not the plugin's").unwrap();
    let path = |path: PathBuf| path.to_string_lossy().into_owned();
    let given = Permission::files([path(reads.clone())], [path(writes.clone())]);
    let (plugin, app) = start_given("folders", vec![given], cx).await;

    // Its folders, at their own paths.
    assert_eq!(toasted(&plugin, &app, "ListFolder", &path(reads.clone()), cx).await, "a.txt, b.txt");
    assert_eq!(toasted(&plugin, &app, "WriteFile", &path(writes.join("new.txt")), cx).await, "written");
    assert_eq!(std::fs::read_to_string(writes.join("new.txt")).unwrap(), "hi");

    // A read-only one isn't written to.
    assert_ne!(toasted(&plugin, &app, "WriteFile", &path(reads.join("c.txt")), cx).await, "written");
    assert!(!reads.join("c.txt").exists());

    // Outside its folders there is nothing, even through `..` from one of them.
    assert_ne!(toasted(&plugin, &app, "ListFolder", &path(root.clone()), cx).await, "reads, secret.txt, writes");
    assert_ne!(toasted(&plugin, &app, "WriteFile", &path(writes.join("../escaped.txt")), cx).await, "written");
    assert!(!root.join("escaped.txt").exists());

    // `~` works the usual way.
    let home = std::env::home_dir().expect("a home folder");
    assert_eq!(toasted(&plugin, &app, "Home", "", cx).await, path(home));
}

#[gpui::test]
async fn a_plugin_asks_for_a_folder(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let (plugin, app) = start("asks-for-folders", cx).await;

    // Declined: the answer reaches the plugin. (Allowed, the app starts it again instead.)
    assert_eq!(toasted(&plugin, &app, "RequestFolder", "~/Projects", cx).await, "declined");
    // Had already: `true`, which the fake says for every folder once told to.
    app.update(cx, |app, _| app.folder_answer = true);
    assert_eq!(toasted(&plugin, &app, "RequestFolder", "/Volumes/Work", cx).await, "allowed");
    let asked = app.read_with(cx, |app, _| app.folders_asked.clone());
    assert_eq!(asked[0], ("~/Projects".to_string(), true, "Nothing: it's here to test asking".to_string()));
}
