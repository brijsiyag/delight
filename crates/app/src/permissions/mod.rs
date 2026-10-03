//! What plugins may do: each plugin's manifest's permissions, with what the user gave it while it
//! ran (a folder it asked for, or that was picked for it), which is kept in `permissions.json`:
//! `{plugin id: [permission, …]}`, each spelled as in a manifest. Every question about what a
//! plugin may do is answered here, and what it was given is changed only here; [`ask`] asks the
//! user for more while a plugin runs, and [`view`] is how permissions look.

pub mod ask;
pub mod view;

use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::{Result, anyhow};
use delight_protocol::{Manifest, Permission, Permissions, PluginProperties};
use gpui::{App, Global};

use crate::files::{read_json, write_json};
use crate::plugins;

/// What the user gave each plugin beyond its manifest, as the file keeps it.
pub struct GivenPermissions {
    path: PathBuf,
    given: BTreeMap<String, Permissions>,
}

impl Global for GivenPermissions {}

/// Load what plugins were given from Delight's folder.
pub fn init(cx: &mut App) {
    cx.set_global(GivenPermissions::open(crate::app_dir().join("permissions.json")));
}

/// What `plugin` may do: what its manifest asks for, each with what the user gave it added.
pub fn of(plugin: &PluginProperties, cx: &App) -> Permissions {
    with_given(Permissions::from(plugin), &given(&plugin.id, cx))
}

/// Whether `plugin` may do everything `asked` allows already, from its manifest or given.
pub fn has(plugin: &PluginProperties, asked: &Permission, cx: &App) -> bool {
    of(plugin, cx).covers(asked)
}

/// Whether `plugin` can be given `asked`: only more of a permission its manifest asks for ("this
/// plugin doesn't have the Files permission"), and what that permission accepts (a program's path
/// in full, say).
pub fn can_give(plugin: &PluginProperties, asked: &Permission) -> Result<()> {
    if Permissions::from(plugin).like(asked).is_none() {
        return Err(anyhow!("this plugin doesn't have the {} permission", asked.spec().name()));
    }
    asked.spec().validate()
}

/// What the user gave plugin `plugin_id` beyond its manifest.
pub fn given(plugin_id: &str, cx: &App) -> Permissions {
    cx.try_global::<GivenPermissions>().and_then(|given| given.given.get(plugin_id).cloned()).unwrap_or_default()
}

/// Give `plugin` `permissions`, with what it was given before, and restart it with them: a plugin
/// has what it may do only from its start. An error, and nothing given, if one is of a permission
/// its manifest doesn't ask for ([`can_give`]).
pub fn give(plugin: &PluginProperties, permissions: Vec<Permission>, cx: &mut App) -> Result<()> {
    for permission in &permissions {
        can_give(plugin, permission)?;
    }
    let plugin_id = &plugin.id;
    if let Err(error) = cx.global_mut::<GivenPermissions>().give(plugin_id, permissions) {
        log::error!("keeping what {plugin_id} was given: {error:#}");
    }
    plugins::restart(plugin_id, cx);
    Ok(())
}

/// Take back `item` from what plugin `plugin_id` was given of `kind`'s kind, and restart it
/// without it.
pub fn take_back(plugin_id: &str, kind: &Permission, item: &str, cx: &mut App) {
    if let Err(error) = cx.global_mut::<GivenPermissions>().take_back(plugin_id, kind, item) {
        log::error!("taking {item} back from {plugin_id}: {error:#}");
    }
    plugins::restart(plugin_id, cx);
}

/// Forget what a deleted plugin was given.
pub fn forget_plugin(plugin_id: &str, cx: &mut App) {
    if let Err(error) = cx.global_mut::<GivenPermissions>().forget(plugin_id) {
        log::error!("forgetting what {plugin_id} was given: {error:#}");
    }
}

/// Whether an update to `new` asks for anything `installed` wasn't granted: a permission it didn't
/// ask for, or more of one (another program to run, another folder, writing where it read). Asking
/// for less, or for the same with another reason, is nothing more. Only the manifests count: what
/// the user gave the plugin is theirs to take back, and a manifest that lists it would keep it.
pub fn asks_for_more(installed: &Manifest, new: &Manifest) -> bool {
    let granted = Permissions::from(&installed.plugin);
    new.plugin.permissions.iter().any(|request| !granted.covers(&request.permission))
}

/// `asked` with what was `given` added to each: nothing of a permission it doesn't ask for (one an
/// update took away), though that is kept.
fn with_given(mut asked: Permissions, given: &Permissions) -> Permissions {
    for permission in given.iter() {
        if asked.like(permission).is_some() {
            asked.add(permission.clone());
        }
    }
    asked
}

impl GivenPermissions {
    fn open(path: PathBuf) -> Self {
        let given = read_json(&path).unwrap_or_default();
        GivenPermissions { path, given }
    }

    fn give(&mut self, plugin_id: &str, permissions: impl IntoIterator<Item = Permission>) -> anyhow::Result<()> {
        let given = self.given.entry(plugin_id.to_string()).or_default();
        for permission in permissions {
            given.add(permission);
        }
        write_json(&self.path, &self.given)
    }

    fn take_back(&mut self, plugin_id: &str, kind: &Permission, item: &str) -> anyhow::Result<()> {
        let Some(given) = self.given.get_mut(plugin_id) else { return Ok(()) };
        given.remove(kind, item);
        if given.is_empty() {
            self.given.remove(plugin_id);
        }
        write_json(&self.path, &self.given)
    }

    fn forget(&mut self, plugin_id: &str) -> anyhow::Result<()> {
        if self.given.remove(plugin_id).is_some() { write_json(&self.path, &self.given) } else { Ok(()) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use delight_protocol::{FilesPermission, PermissionRequest};

    const NONE: [&str; 0] = [];

    fn file(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("delight-permissions-{name}-{}.json", std::process::id()));
        std::fs::remove_file(&path).ok();
        path
    }

    fn manifest(permissions: Vec<Permission>) -> Manifest {
        Manifest {
            plugin: PluginProperties {
                id: "acme.one".into(),
                name: "One".into(),
                version: "1.0.0".into(),
                description: String::new(),
                author: String::new(),
                icon: "<svg/>".into(),
                tags: Vec::new(),
                permissions: permissions.into_iter().map(|permission| PermissionRequest { permission, reason: "Why".into() }).collect(),
                tips: Vec::new(),
                update: None,
            },
            operations: Vec::new(),
        }
    }

    #[test]
    fn what_was_given_is_kept_in_the_file_as_a_manifest_spells_it() {
        let path = file("kept");
        let mut given = GivenPermissions::open(path.clone());
        given.give("acme.one", [Permission::files(["~/Projects"], NONE)]).unwrap();
        given.give("acme.one", [Permission::files(NONE, ["~/Projects"]), Permission::files(["/Volumes/Work"], NONE)]).unwrap();
        given.give("acme.two", [Permission::files(["/tmp"], NONE)]).unwrap();
        let json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(json["acme.one"], serde_json::json!([{"permission": "Files", "read": ["/Volumes/Work"], "write": ["~/Projects"]}]));

        let mut reopened = GivenPermissions::open(path.clone());
        assert_eq!(reopened.given["acme.one"], [Permission::files(["/Volumes/Work"], ["~/Projects"])].into_iter().collect());
        reopened.forget("acme.one").unwrap();
        let reopened = GivenPermissions::open(path.clone());
        assert!(!reopened.given.contains_key("acme.one"), "forgotten in the file");
        assert!(reopened.given.contains_key("acme.two"), "the others stay");
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn what_was_given_is_taken_back_an_item_at_a_time() {
        let path = file("taken");
        let mut given = GivenPermissions::open(path.clone());
        given.give("acme.one", [Permission::files(["/Volumes/Work"], ["~/Projects"])]).unwrap();
        let files = Permission::files(NONE, NONE);
        given.take_back("acme.one", &files, "~/Projects").unwrap();
        assert_eq!(GivenPermissions::open(path.clone()).given["acme.one"], [Permission::files(["/Volumes/Work"], NONE)].into_iter().collect());
        given.take_back("acme.one", &files, "/Volumes/Work").unwrap();
        assert!(GivenPermissions::open(path.clone()).given.is_empty(), "nothing left: the plugin goes");
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn no_file_is_nothing_given() {
        assert!(GivenPermissions::open(file("none")).given.is_empty());
    }

    #[test]
    fn a_plugin_has_its_manifests_permissions_with_what_it_was_given() {
        let asked = Permissions::from(&manifest(vec![Permission::network(), Permission::files(["~/Desktop"], NONE)]).plugin);
        let given: Permissions = [Permission::files(NONE, ["~/Desktop"]), Permission::files(["/Volumes/Work"], NONE)].into_iter().collect();
        let has = with_given(asked, &given);
        assert!(has.like(&Permission::network()).is_some());
        assert_eq!(has.get::<FilesPermission>(), Some(&FilesPermission { read: vec!["/Volumes/Work".into()], write: vec!["~/Desktop".into()] }));

        // An update took Files away: what it was given of it is no more.
        let without = with_given(Permissions::from(&manifest(vec![Permission::network()]).plugin), &given);
        assert!(without.get::<FilesPermission>().is_none());
    }

    #[test]
    fn a_plugin_is_given_more_only_of_what_its_manifest_asks_for() {
        let files = manifest(vec![Permission::files(NONE, NONE)]).plugin;
        assert!(can_give(&files, &Permission::files(["/Volumes/Work"], NONE)).is_ok());
        let network = manifest(vec![Permission::network()]).plugin;
        let error = can_give(&network, &Permission::files(["/Volumes/Work"], NONE)).unwrap_err();
        assert_eq!(error.to_string(), "this plugin doesn't have the Files permission");
    }

    #[test]
    fn an_update_asks_for_more_only_for_what_wasnt_granted() {
        let installed = manifest(vec![Permission::network(), Permission::commands(["/bin/ps", "/bin/kill"])]);
        assert!(!asks_for_more(&installed, &manifest(vec![Permission::network()])), "a permission fewer");
        assert!(!asks_for_more(&installed, &manifest(vec![Permission::commands(["/bin/ps"])])), "a program fewer");
        let mut reworded = installed.clone();
        reworded.plugin.permissions[0].reason = "Another reason".into();
        assert!(!asks_for_more(&installed, &reworded), "a new reason");
        assert!(asks_for_more(&manifest(vec![Permission::network()]), &installed), "a new permission");
        assert!(asks_for_more(&installed, &manifest(vec![Permission::commands(["/bin/ps", "/bin/ls"])])), "another program");

        let reads = manifest(vec![Permission::files(["~/Desktop", "~/Documents"], NONE)]);
        assert!(!asks_for_more(&reads, &manifest(vec![Permission::files(["~/Desktop"], NONE)])), "a folder fewer");
        assert!(!asks_for_more(&reads, &manifest(vec![Permission::files(["~/Desktop/Shots"], NONE)])), "a folder inside");
        assert!(asks_for_more(&reads, &manifest(vec![Permission::files(["~/Desktop", "~/Music"], NONE)])), "another folder");
        assert!(asks_for_more(&reads, &manifest(vec![Permission::files(NONE, ["~/Desktop"])])), "writing too");
    }
}
