//! Permissions: what a plugin may do outside its sandbox.
//!
//! Each permission is a type of its own that holds whatever data only it needs (the
//! programs `Commands` may run, the folders `Files` has; `Network` needs none) and implements
//! [`PermissionSpec`]: how it is checked and how people are told about it; and
//! [`PermissionData`]: how two of it combine, and whether one allows all another does.
//! [`Permission`] is the enum over them, so the manifest, the macro that writes it, and the app
//! that lists, gives and enforces it treat every permission the same way; [`Permissions`] is a
//! plugin's set of them. A new permission is a new type, its two impls, and a variant (in the
//! enum and in `with_data!`).

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

/// What every permission is: checked when the manifest is, and described to people who
/// install the plugin and look at it in Settings.
pub trait PermissionSpec {
    /// The name the manifest spells it with, such as `Network`.
    fn name(&self) -> &'static str;

    /// Check its data, which the types can't.
    fn validate(&self) -> Result<()>;

    /// What people call it: "Runs commands".
    fn title(&self) -> &'static str;

    /// What it allows, with its data: shown under the title, before the plugin's own
    /// reason for asking.
    fn describe(&self) -> String;

    /// What its data lists, each by its key (a program `Commands` may run, a folder `Files` has):
    /// what [`PermissionData::remove`] takes back. How each looks is the app's. None by default.
    fn items(&self) -> Vec<String> {
        Vec::new()
    }

    /// How many [`items`](Self::items) there are, in words ("3 programs"), shown beside the title
    /// while the list is closed. None when there are none.
    fn count_items(&self) -> Option<String> {
        None
    }

    /// A [Lucide](https://lucide.dev) icon's file name, without `.svg`.
    fn icon(&self) -> &'static str;
}

/// A permission's own type, found in a [`Permission`]: what
/// `PluginProperties::permission::<P>()` looks for. It says how two of it combine: a plugin has
/// its manifest's, with what the user gave it while it ran added.
pub trait PermissionData: PermissionSpec + Sized {
    fn from_permission(permission: &Permission) -> Option<&Self>;

    /// Allow what `more` allows too.
    fn add(&mut self, more: &Self);

    /// Take back `item`, one of its [items](PermissionSpec::items).
    fn remove(&mut self, item: &str);

    /// Whether it allows everything `asked` allows.
    fn covers(&self, asked: &Self) -> bool;
}

/// What a plugin may do outside its sandbox. Each is granted when the plugin is installed (and
/// more of `Files` and `Commands` while it runs), and gates what the app hands it. In the
/// manifest it is an object named by its `permission` field, next to its own data:
/// `{"permission": "Commands", "programs": ["/bin/ps"]}`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "permission")]
pub enum Permission {
    Network(NetworkPermission),
    Commands(CommandsPermission),
    Files(FilesPermission),
}

/// `$body` with `$data` bound to `$permission`'s own data, whichever permission it is: the one
/// place each variant is matched.
macro_rules! with_data {
    ($permission:expr, $data:ident => $body:expr) => {
        match $permission {
            Permission::Network($data) => $body,
            Permission::Commands($data) => $body,
            Permission::Files($data) => $body,
        }
    };
}

/// `permission`'s data if it is of `like`'s type.
fn same_type<'a, P: PermissionData>(_like: &P, permission: &'a Permission) -> Option<&'a P> {
    P::from_permission(permission)
}

impl Permission {
    pub fn network() -> Self {
        Permission::Network(NetworkPermission {})
    }

    pub fn commands(programs: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Permission::Commands(CommandsPermission { programs: programs.into_iter().map(Into::into).collect() })
    }

    /// `Files`, reading the folders in `read` and writing (and reading) those in `write`.
    pub fn files(read: impl IntoIterator<Item = impl Into<String>>, write: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Permission::Files(FilesPermission {
            read: read.into_iter().map(Into::into).collect(),
            write: write.into_iter().map(Into::into).collect(),
        })
    }

    /// The permission as [`PermissionSpec`], whichever it is.
    pub fn spec(&self) -> &dyn PermissionSpec {
        with_data!(self, data => data)
    }

    /// Whether `other` is the same permission (both `Files`, say), whatever their data.
    pub fn same_kind(&self, other: &Permission) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
    }

    /// Allow what `more`, of the same kind, allows too. Nothing for another kind.
    pub fn add(&mut self, more: &Permission) {
        with_data!(self, data => if let Some(more) = same_type(&*data, more) { data.add(more) })
    }

    /// Take back `item`, one of its [items](PermissionSpec::items).
    pub fn remove(&mut self, item: &str) {
        with_data!(self, data => data.remove(item))
    }

    /// Whether it allows everything `asked` allows: never for another kind.
    pub fn covers(&self, asked: &Permission) -> bool {
        with_data!(self, data => same_type(data, asked).is_some_and(|asked| data.covers(asked)))
    }
}

/// Permissions, at most one of each kind: what a plugin's manifest asks for, what the user gave it
/// while it ran, or both together. Adding one of a kind it has adds to that one. It is kept as a
/// list, each permission spelled as in a manifest.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Permissions(Vec<Permission>);

impl Permissions {
    /// Its permission of type `P` (`get::<FilesPermission>()`), if it has one.
    pub fn get<P: PermissionData>(&self) -> Option<&P> {
        self.0.iter().find_map(P::from_permission)
    }

    /// Its permission of `permission`'s kind.
    pub fn like(&self, permission: &Permission) -> Option<&Permission> {
        self.0.iter().find(|had| had.same_kind(permission))
    }

    pub fn iter(&self) -> std::slice::Iter<'_, Permission> {
        self.0.iter()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Add `permission`, to the one of its kind if there is one.
    pub fn add(&mut self, permission: Permission) {
        match self.0.iter_mut().find(|had| had.same_kind(&permission)) {
            Some(had) => had.add(&permission),
            None => self.0.push(permission),
        }
    }

    /// Take back `item` from its permission of `kind`'s kind. One left listing nothing goes.
    pub fn remove(&mut self, kind: &Permission, item: &str) {
        let Some(at) = self.0.iter().position(|had| had.same_kind(kind)) else { return };
        self.0[at].remove(item);
        if self.0[at].spec().items().is_empty() {
            self.0.remove(at);
        }
    }

    /// Whether one of them allows everything `asked` allows.
    pub fn covers(&self, asked: &Permission) -> bool {
        self.like(asked).is_some_and(|had| had.covers(asked))
    }
}

impl FromIterator<Permission> for Permissions {
    fn from_iter<I: IntoIterator<Item = Permission>>(permissions: I) -> Self {
        let mut all = Permissions::default();
        for permission in permissions {
            all.add(permission);
        }
        all
    }
}

/// What a plugin's manifest asks for.
impl From<&crate::PluginProperties> for Permissions {
    fn from(plugin: &crate::PluginProperties) -> Self {
        plugin.permissions.iter().map(|request| request.permission.clone()).collect()
    }
}

/// Reach the network.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NetworkPermission {}

impl PermissionSpec for NetworkPermission {
    fn name(&self) -> &'static str {
        "Network"
    }

    fn validate(&self) -> Result<()> {
        Ok(())
    }

    fn title(&self) -> &'static str {
        "Network"
    }

    fn describe(&self) -> String {
        "Can reach the internet and your local network, and listen on this Mac".into()
    }

    fn icon(&self) -> &'static str {
        "globe"
    }
}

/// All or nothing: there is nothing to add or take back.
impl PermissionData for NetworkPermission {
    fn from_permission(permission: &Permission) -> Option<&Self> {
        match permission {
            Permission::Network(network) => Some(network),
            _ => None,
        }
    }

    fn add(&mut self, _more: &Self) {}

    fn remove(&mut self, _item: &str) {}

    fn covers(&self, _asked: &Self) -> bool {
        true
    }
}

/// Run programs, with any arguments: those it lists, which it has from the start, and those the
/// user gives it while it runs.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandsPermission {
    /// The programs it may run, each a [`validate_program`] path, anywhere: absolute
    /// (`/opt/homebrew/bin/git`) or in the home folder of whoever runs it (`~/.cargo/bin/rg`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub programs: Vec<String>,
}

impl CommandsPermission {
    /// Whether it allows every program `asked` runs, with `~` in both as `home`: the programs are
    /// compared where they are, so `~/.cargo/bin/rg` is `/Users/ada/.cargo/bin/rg`.
    pub fn covers_in(&self, asked: &CommandsPermission, home: Option<&Path>) -> bool {
        asked.programs.iter().all(|program| {
            let program = expand_home(program, home);
            self.programs.iter().any(|had| expand_home(had, home) == program)
        })
    }
}

impl PermissionSpec for CommandsPermission {
    fn name(&self) -> &'static str {
        "Commands"
    }

    /// Each program a [`validate_program`] path, none twice. With none, it runs only those the
    /// user gives it.
    fn validate(&self) -> Result<()> {
        let mut listed = HashSet::new();
        for program in &self.programs {
            validate_program(program)?;
            if !listed.insert(program) {
                bail!("Commands lists {program:?} twice");
            }
        }
        Ok(())
    }

    fn title(&self) -> &'static str {
        "Runs commands"
    }

    fn describe(&self) -> String {
        if self.programs.is_empty() {
            "Can ask you for programs to run, and run those you allow, with any arguments".into()
        } else {
            "Can run these programs, with any arguments, and ask you for others".into()
        }
    }

    fn items(&self) -> Vec<String> {
        self.programs.clone()
    }

    fn count_items(&self) -> Option<String> {
        counted(self.programs.len(), "program")
    }

    fn icon(&self) -> &'static str {
        "terminal"
    }
}

/// Its programs are a set: it covers another that runs none it doesn't, wherever `~` is
/// ([`CommandsPermission::covers_in`]).
impl PermissionData for CommandsPermission {
    fn from_permission(permission: &Permission) -> Option<&Self> {
        match permission {
            Permission::Commands(commands) => Some(commands),
            _ => None,
        }
    }

    fn add(&mut self, more: &Self) {
        for program in &more.programs {
            if !self.programs.contains(program) {
                self.programs.push(program.clone());
            }
        }
    }

    fn remove(&mut self, item: &str) {
        self.programs.retain(|program| program != item);
    }

    /// `~` is the home folder of whoever runs it, as it means.
    fn covers(&self, asked: &Self) -> bool {
        self.covers_in(asked, std::env::home_dir().as_deref())
    }
}

/// Work with the files in folders: those it lists, which it has from the start, and those the
/// user gives it while it runs (asked for, or picked).
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FilesPermission {
    /// The folders whose files it may read: any path, absolute (`/`, `/Volumes/Work`) or in the
    /// home folder of whoever runs it (`~/Downloads`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub read: Vec<String>,
    /// The folders whose files it may write (and add and remove) as well as read.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub write: Vec<String>,
}

impl FilesPermission {
    /// Every folder it lists, with whether it may write there.
    pub fn folders(&self) -> impl Iterator<Item = (&String, bool)> {
        self.read.iter().map(|folder| (folder, false)).chain(self.write.iter().map(|folder| (folder, true)))
    }

    /// Whether it allows everything `asked` allows, with `~` in both as `home`: the folders are
    /// compared where they are, so `~/Desktop` covers `/Users/ada/Desktop/Shots`, and `/` covers
    /// `~/Desktop`.
    pub fn covers_in(&self, asked: &FilesPermission, home: Option<&Path>) -> bool {
        asked.folders().all(|(folder, write)| {
            let folder = expand_home(folder, home);
            self.folders().any(|(had, may_write)| folder.starts_with(expand_home(had, home)) && (may_write || !write))
        })
    }
}

impl PermissionSpec for FilesPermission {
    fn name(&self) -> &'static str {
        "Files"
    }

    /// No folder listed twice, in one list or both. Any path is a folder it may have.
    fn validate(&self) -> Result<()> {
        let mut listed = HashSet::new();
        for (folder, _) in self.folders() {
            if !listed.insert(folder) {
                bail!("Files lists {folder:?} twice");
            }
        }
        Ok(())
    }

    fn title(&self) -> &'static str {
        "Files"
    }

    fn describe(&self) -> String {
        match (self.read.is_empty() && self.write.is_empty(), self.write.is_empty()) {
            (true, _) => "Can ask you for folders, and work with the files in those you give it",
            (false, true) => "Can read the files in these folders, and ask you for others",
            (false, false) => "Can read the files in these folders, write those it may write, and ask you for others",
        }
        .into()
    }

    fn items(&self) -> Vec<String> {
        self.folders().map(|(folder, _)| folder.clone()).collect()
    }

    fn count_items(&self) -> Option<String> {
        counted(self.read.len() + self.write.len(), "folder")
    }

    fn icon(&self) -> &'static str {
        "folder"
    }
}

/// Each folder once, to read or to write; a folder it may write in it may read. It covers a folder
/// inside one it has, with the access that one gives, wherever `~` is ([`FilesPermission::covers_in`]).
impl PermissionData for FilesPermission {
    fn from_permission(permission: &Permission) -> Option<&Self> {
        match permission {
            Permission::Files(files) => Some(files),
            _ => None,
        }
    }

    fn add(&mut self, more: &Self) {
        for (folder, write) in more.folders() {
            if self.write.contains(folder) {
                continue;
            }
            if write {
                self.read.retain(|had| had != folder);
                self.write.push(folder.clone());
            } else if !self.read.contains(folder) {
                self.read.push(folder.clone());
            }
        }
    }

    fn remove(&mut self, item: &str) {
        self.read.retain(|folder| folder != item);
        self.write.retain(|folder| folder != item);
    }

    /// `~` is the home folder of whoever runs it, as it means.
    fn covers(&self, asked: &Self) -> bool {
        self.covers_in(asked, std::env::home_dir().as_deref())
    }
}

/// `~` and `~/…` in `home`; any other path as it is, and all of them without a home.
pub fn expand_home(folder: &str, home: Option<&Path>) -> PathBuf {
    match (folder.strip_prefix('~'), home) {
        (Some(""), Some(home)) => home.to_path_buf(),
        (Some(rest), Some(home)) if rest.starts_with('/') => home.join(rest.trim_start_matches('/')),
        _ => PathBuf::from(folder),
    }
}

/// `path` as people know it, and as a manifest spells it: `~/Projects` in `home`, any other path
/// as it is.
pub fn home_spelled(path: &Path, home: Option<&Path>) -> String {
    match home.and_then(|home| path.strip_prefix(home).ok()) {
        Some(inside) if inside.as_os_str().is_empty() => "~".into(),
        Some(inside) => format!("~/{}", inside.display()),
        None => path.display().to_string(),
    }
}

/// `3 folders`, `1 folder`; none for 0.
fn counted(count: usize, noun: &str) -> Option<String> {
    match count {
        0 => None,
        1 => Some(format!("1 {noun}")),
        _ => Some(format!("{count} {noun}s")),
    }
}

/// Check a program a plugin may run: a path to it in full, anywhere, from `/` or from the home
/// folder (`~/`), so no name looked up in `PATH`; with no `.` or `..` in it, so it says where the
/// program is; and ending in the program's name.
pub fn validate_program(program: &str) -> Result<()> {
    let rest = program.strip_prefix("~/").or_else(|| program.strip_prefix('/'));
    let full = rest.is_some_and(|rest| {
        rest.split('/').all(|part| !matches!(part, "" | "." | "..")) && !rest.chars().any(char::is_control)
    });
    if !full {
        bail!("{program:?} isn't a program's path: give it in full, from / or ~/, with no . or .. in it");
    }
    Ok(())
}

/// A permission a plugin asks for, and why. People see the reason, in the plugin's
/// words, next to what the permission allows, when they install it and in Settings,
/// and decide. In the manifest they are one object: `{"permission": "Commands",
/// "programs": ["/bin/ps"], "reason": "…"}`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PermissionRequest {
    #[serde(flatten)]
    pub permission: Permission,
    /// At most [`MAX_REASON_CHARS`](crate::MAX_REASON_CHARS) characters.
    pub reason: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permissions_are_spelled_as_in_the_manifest() {
        assert_eq!(serde_json::to_string(&Permission::network()).unwrap(), r#"{"permission":"Network"}"#);
        assert_eq!(
            serde_json::to_string(&Permission::commands(["/bin/ps"])).unwrap(),
            r#"{"permission":"Commands","programs":["/bin/ps"]}"#
        );
        assert!(serde_json::from_str::<Permission>(r#"{"permission":"Camera"}"#).is_err());
        assert_eq!(Permission::network().spec().name(), "Network");
        assert_eq!(Permission::commands(["/bin/ps"]).spec().name(), "Commands");
    }

    #[test]
    fn a_permission_comes_with_its_reason() {
        let request = PermissionRequest { permission: Permission::network(), reason: "Syncs".into() };
        let json = r#"{"permission":"Network","reason":"Syncs"}"#;
        assert_eq!(serde_json::to_string(&request).unwrap(), json);
        assert_eq!(serde_json::from_str::<PermissionRequest>(json).unwrap(), request);
        assert!(serde_json::from_str::<PermissionRequest>(r#"{"permission":"Network"}"#).is_err(), "no reason");
        assert!(serde_json::from_str::<PermissionRequest>(r#""Network""#).is_err(), "only a name");
    }

    #[test]
    fn a_permission_holds_only_its_own_data() {
        let json = r#"{"permission":"Commands","programs":["/bin/ps","/usr/sbin/lsof"],"reason":"Lists processes"}"#;
        let request = PermissionRequest {
            permission: Permission::commands(["/bin/ps", "/usr/sbin/lsof"]),
            reason: "Lists processes".into(),
        };
        assert_eq!(serde_json::to_string(&request).unwrap(), json);
        assert_eq!(serde_json::from_str::<PermissionRequest>(json).unwrap(), request);
        // Network has nowhere to put programs; Commands without them runs only those it is given.
        assert!(serde_json::from_str::<PermissionRequest>(r#"{"permission":"Network","programs":["/bin/ps"],"reason":"x"}"#).is_err());
        let given_only = serde_json::from_str::<PermissionRequest>(r#"{"permission":"Commands","reason":"x"}"#).unwrap();
        assert_eq!(given_only.permission, Permission::Commands(CommandsPermission::default()));
        assert_eq!(serde_json::to_string(&given_only.permission).unwrap(), r#"{"permission":"Commands"}"#);
    }

    #[test]
    fn files_are_spelled_as_in_the_manifest() {
        let json = |permission: &Permission| serde_json::to_string(permission).unwrap();
        let none: [&str; 0] = [];
        assert_eq!(json(&Permission::files(["~/Desktop"], none)), r#"{"permission":"Files","read":["~/Desktop"]}"#);
        assert_eq!(
            json(&Permission::files(["~/Desktop"], ["~/Notes"])),
            r#"{"permission":"Files","read":["~/Desktop"],"write":["~/Notes"]}"#
        );
        // Folders only from the user: nothing else to say.
        let given_only = serde_json::from_str::<Permission>(r#"{"permission":"Files"}"#).unwrap();
        assert_eq!(given_only, Permission::Files(FilesPermission::default()));
        assert_eq!(given_only.spec().name(), "Files");
        assert!(given_only.spec().items().is_empty());
        assert!(serde_json::from_str::<Permission>(r#"{"permission":"Files","folders":["~/Desktop"]}"#).is_err());
    }

    #[test]
    fn files_list_each_folder_once_to_read_or_to_write() {
        let files = |read: &[&str], write: &[&str]| Permission::files(read.iter().copied(), write.iter().copied()).spec().validate();
        // Any path at all.
        for good in [&[][..], &["/"], &["~"], &["~/Desktop", "~/Library/Logs", "/Volumes/Work", "/System"]] {
            assert!(files(good, &[]).is_ok(), "{good:?}");
        }
        assert!(files(&["~/Desktop"], &["~/Notes"]).is_ok());
        assert!(files(&[], &["~/Notes"]).is_ok(), "write only: it reads there too");
        assert!(files(&["~/Desktop", "~/Desktop"], &[]).is_err(), "twice");
        assert!(files(&["~/Notes"], &["~/Notes"]).is_err(), "in both lists");
        let many: Vec<String> = (0..500).map(|i| format!("~/f{i}")).collect();
        assert!(Permission::files(many, Vec::<String>::new()).spec().validate().is_ok(), "as many as it needs");
        let both = FilesPermission { read: vec!["~/a".into()], write: vec!["~/b".into()] };
        assert_eq!(both.folders().map(|(folder, write)| (folder.as_str(), write)).collect::<Vec<_>>(), [("~/a", false), ("~/b", true)]);
    }

    #[test]
    fn a_list_is_counted_in_words() {
        assert_eq!(Permission::files(["~/a"], ["~/b"]).spec().count_items().as_deref(), Some("2 folders"));
        assert_eq!(Permission::commands(["/bin/ps"]).spec().count_items().as_deref(), Some("1 program"));
        assert_eq!(Permission::Files(FilesPermission::default()).spec().count_items(), None);
        assert_eq!(Permission::network().spec().count_items(), None);
    }

    #[test]
    fn commands_check_their_programs() {
        // Anywhere, in full; none at all: only those the user gives it.
        for good in [&[][..], &["/bin/ps"], &["/opt/homebrew/bin/git", "/usr/local/bin/brew", "~/.cargo/bin/rg", "/Applications/Tool.app/Contents/MacOS/tool"]] {
            Permission::commands(good.iter().copied()).spec().validate().unwrap();
        }
        for (bad, why) in [
            (&["ps"][..], "looked up in PATH"),
            (&["bin/ps"], "relative"),
            (&["~"], "no name"),
            (&["~/"], "no name"),
            (&["~bob/bin/x"], "another user's home"),
            (&["/bin/../usr/bin/ps"], "up a folder"),
            (&["/bin/./ps"], "a dot"),
            (&["/bin//ps"], "an empty part"),
            (&["/bin/"], "no name"),
            (&["/bin/.."], "no name"),
            (&["/bin/p\ns"], "a control character"),
            (&["/bin/ps", "/bin/ps"], "twice"),
        ] {
            assert!(Permission::commands(bad.iter().copied()).spec().validate().is_err(), "{why}");
        }
        let many = (0..500).map(|n| format!("/opt/tools/p{n}"));
        assert!(Permission::commands(many).spec().validate().is_ok(), "as many as it needs");
    }

    #[test]
    fn people_are_told_what_each_allows() {
        let commands = Permission::commands(["/bin/ps", "/bin/kill"]);
        assert_eq!(commands.spec().title(), "Runs commands");
        assert_eq!(commands.spec().describe(), "Can run these programs, with any arguments, and ask you for others");
        let given_only = Permission::Commands(CommandsPermission::default());
        assert_eq!(given_only.spec().describe(), "Can ask you for programs to run, and run those you allow, with any arguments");
        assert_eq!(commands.spec().items(), ["/bin/ps", "/bin/kill"]);
        assert!(Permission::network().spec().items().is_empty());
        assert_eq!(Permission::files(["~/Desktop"], ["~/Notes"]).spec().items(), ["~/Desktop", "~/Notes"]);
        assert_eq!(commands.spec().icon(), "terminal");
        assert_eq!(Permission::network().spec().icon(), "globe");
    }

    #[test]
    fn a_permission_adds_another_of_its_kind_and_gives_back_its_items() {
        let none: [&str; 0] = [];
        let mut commands = Permission::commands(["/bin/ps"]);
        commands.add(&Permission::commands(["/bin/ps", "/bin/kill"]));
        assert_eq!(commands, Permission::commands(["/bin/ps", "/bin/kill"]), "each program once");
        commands.add(&Permission::network());
        assert_eq!(commands, Permission::commands(["/bin/ps", "/bin/kill"]), "another kind adds nothing");
        commands.remove("/bin/ps");
        assert_eq!(commands, Permission::commands(["/bin/kill"]));

        let mut files = Permission::files(["~/Desktop", "/Volumes/Work"], none);
        files.add(&Permission::files(["~/Projects"], ["~/Desktop"]));
        assert_eq!(files, Permission::files(["/Volumes/Work", "~/Projects"], ["~/Desktop"]), "writing moves it, once");
        files.add(&Permission::files(["~/Desktop"], none));
        assert_eq!(files, Permission::files(["/Volumes/Work", "~/Projects"], ["~/Desktop"]), "reading where it writes adds nothing");
        assert!(files.spec().validate().is_ok());
        files.remove("~/Desktop");
        assert_eq!(files, Permission::files(["/Volumes/Work", "~/Projects"], none), "gone, whatever its access");
    }

    #[test]
    fn a_permission_covers_what_it_allows_already() {
        let none: [&str; 0] = [];
        let commands = Permission::commands(["/bin/ps", "/bin/kill"]);
        assert!(commands.covers(&Permission::commands(["/bin/ps"])));
        assert!(!commands.covers(&Permission::commands(["/bin/ps", "/bin/ls"])));
        assert!(Permission::network().covers(&Permission::network()));
        assert!(!Permission::network().covers(&commands), "another kind");

        let files = Permission::files(["/Users/ada/Desktop"], ["/Users/ada/Projects"]);
        let asked = |read: &[&str], write: &[&str]| files.covers(&Permission::files(read.iter().copied(), write.iter().copied()));
        assert!(asked(&["/Users/ada/Desktop"], &[]));
        assert!(asked(&["/Users/ada/Desktop/Shots"], &[]), "inside");
        assert!(!asked(&[], &["/Users/ada/Desktop"]), "read-only there");
        assert!(asked(&["/Users/ada/Projects"], &["/Users/ada/Projects/delight"]), "writing reads too");
        assert!(!asked(&["/Users/ada/Desktop2"], &[]), "a name that starts the same is another folder");
        assert!(!asked(&["/Users/ada"], &[]), "the folder around one");
        assert!(files.covers(&Permission::files(none, none)), "nothing asked");
    }

    #[test]
    fn home_is_where_the_tilde_is() {
        let home = Some(Path::new("/Users/ada"));
        assert_eq!(expand_home("~", home), Path::new("/Users/ada"));
        assert_eq!(expand_home("~/Desktop", home), Path::new("/Users/ada/Desktop"));
        assert_eq!(expand_home("/Volumes/Work", home), Path::new("/Volumes/Work"));
        assert_eq!(expand_home("~ada/x", home), Path::new("~ada/x"), "another user's home isn't looked up");
        assert_eq!(expand_home("~/Desktop", None), Path::new("~/Desktop"), "no home: as it is");
        assert_eq!(home_spelled(Path::new("/Users/ada/Projects"), home), "~/Projects");
        assert_eq!(home_spelled(Path::new("/Users/ada"), home), "~");
        assert_eq!(home_spelled(Path::new("/Users/adam"), home), "/Users/adam");
        assert_eq!(home_spelled(Path::new("/Users/ada/Projects"), None), "/Users/ada/Projects");
    }

    #[test]
    fn files_cover_a_folder_wherever_home_is() {
        let home = Some(Path::new("/Users/ada"));
        let files = |read: &[&str], write: &[&str]| FilesPermission { read: read.iter().map(|f| f.to_string()).collect(), write: write.iter().map(|f| f.to_string()).collect() };
        let has = files(&["~/Desktop"], &["/Volumes/Work"]);
        assert!(has.covers_in(&files(&["/Users/ada/Desktop/Shots"], &[]), home), "~ is the home folder");
        assert!(has.covers_in(&files(&["~/Desktop"], &[]), home));
        assert!(!has.covers_in(&files(&[], &["~/Desktop"]), home), "read-only there");
        assert!(has.covers_in(&files(&[], &["/Volumes/Work/x"]), home));
        assert!(!has.covers_in(&files(&["/Users/ada"], &[]), home), "the folder around one");
        assert!(files(&["/"], &[]).covers_in(&files(&["~/Desktop"], &[]), home), "a folder around home covers one in it");
        assert!(!has.covers_in(&files(&["/Users/ada/Desktop"], &[]), None), "no home: ~ is only ~");
    }

    #[test]
    fn programs_are_covered_wherever_home_is() {
        let home = Some(Path::new("/Users/ada"));
        let commands = |programs: &[&str]| CommandsPermission { programs: programs.iter().map(|program| program.to_string()).collect() };
        let has = commands(&["~/.cargo/bin/rg", "/opt/homebrew/bin/git"]);
        assert!(has.covers_in(&commands(&["/Users/ada/.cargo/bin/rg"]), home), "~ is the home folder");
        assert!(has.covers_in(&commands(&["~/.cargo/bin/rg", "/opt/homebrew/bin/git"]), home));
        assert!(!has.covers_in(&commands(&["/opt/homebrew/bin/git-lfs"]), home), "another program");
        assert!(!has.covers_in(&commands(&["/opt/homebrew/bin"]), home), "a program's folder isn't a program");
        assert!(!has.covers_in(&commands(&["/Users/ada/.cargo/bin/rg"]), None), "no home: ~ is only ~");
    }

    #[test]
    fn permissions_hold_one_of_each_kind() {
        let none: [&str; 0] = [];
        let mut permissions: Permissions =
            [Permission::network(), Permission::files(["~/Desktop"], none), Permission::files(none, ["~/Notes"])].into_iter().collect();
        assert_eq!(permissions.iter().count(), 2, "the two Files are one");
        assert_eq!(permissions.get::<FilesPermission>(), Some(&FilesPermission { read: vec!["~/Desktop".into()], write: vec!["~/Notes".into()] }));
        assert!(permissions.get::<CommandsPermission>().is_none());
        assert!(permissions.covers(&Permission::files(["~/Notes/Today"], none)));
        assert!(!permissions.covers(&Permission::commands(["/bin/ps"])), "a kind it hasn't");
        assert_eq!(
            serde_json::to_string(&permissions).unwrap(),
            r#"[{"permission":"Network"},{"permission":"Files","read":["~/Desktop"],"write":["~/Notes"]}]"#
        );

        let kind = Permission::files(none, none);
        permissions.remove(&kind, "~/Desktop");
        assert!(permissions.like(&kind).is_some(), "it still lists a folder");
        permissions.remove(&kind, "~/Notes");
        assert!(permissions.like(&kind).is_none(), "listing nothing, it goes");
        assert_eq!(permissions, [Permission::network()].into_iter().collect());
    }
}
