//! Asking the user to give a plugin more of a permission while it runs (`host(cx).request_permission`): each
//! permission says how what it names is kept, whether it is there, and what the alert asks
//! ([`Ask`]); [`ask`] asks the same way for all of them.

use std::path::Path;

use anyhow::{Result, anyhow};
use delight_protocol::{CommandsPermission, FilesPermission, NetworkPermission, Permission, PluginProperties, expand_home, home_spelled};
use gpui::{App, Task};

use crate::dialogs;

/// How the user is asked to give a plugin more of a permission while it runs.
trait Ask {
    /// It as it is kept, spelled as a manifest spells it: what is in `home` as `~/…`.
    fn spelled(&self, home: Option<&Path>) -> Permission;

    /// An error when what it names isn't there, with `~` as `home`: one that isn't would be kept,
    /// and given to no one.
    fn there(&self, home: Option<&Path>) -> Result<()>;

    /// The alert's question, for the plugin `name`d: "Allow “Notes” to read and write the files in
    /// ~/Projects?". None for a permission the user doesn't give while a plugin runs.
    fn question(&self, name: &str) -> Option<String>;
}

/// `permission` as [`Ask`], whichever it is.
fn ask_of(permission: &Permission) -> &dyn Ask {
    match permission {
        Permission::Network(network) => network,
        Permission::Commands(commands) => commands,
        Permission::Files(files) => files,
    }
}

/// All or nothing, when the plugin is installed: nothing to ask for while it runs.
impl Ask for NetworkPermission {
    fn spelled(&self, _home: Option<&Path>) -> Permission {
        Permission::Network(self.clone())
    }

    fn there(&self, _home: Option<&Path>) -> Result<()> {
        Ok(())
    }

    fn question(&self, _name: &str) -> Option<String> {
        None
    }
}

/// Programs, each a file.
impl Ask for CommandsPermission {
    fn spelled(&self, home: Option<&Path>) -> Permission {
        Permission::commands(self.programs.iter().map(|program| home_spelled(&expand_home(program, home), home)))
    }

    fn there(&self, home: Option<&Path>) -> Result<()> {
        for program in &self.programs {
            let path = expand_home(program, home);
            if !path.is_file() {
                return Err(anyhow!("there is no program at {}", path.display()));
            }
        }
        Ok(())
    }

    fn question(&self, name: &str) -> Option<String> {
        Some(format!("Allow “{name}” to run {}?", self.programs.join(", ")))
    }
}

/// Folders, each to read or to write.
impl Ask for FilesPermission {
    fn spelled(&self, home: Option<&Path>) -> Permission {
        let spell = |folders: &[String]| folders.iter().map(|folder| home_spelled(&expand_home(folder, home), home)).collect::<Vec<_>>();
        Permission::files(spell(&self.read), spell(&self.write))
    }

    fn there(&self, home: Option<&Path>) -> Result<()> {
        for (folder, _) in self.folders() {
            let path = expand_home(folder, home);
            // A relative path would be looked for wherever Delight happens to run.
            if !path.is_absolute() {
                return Err(anyhow!("{folder} isn't a folder's path: give it in full, from / or ~/"));
            }
            if !path.is_dir() {
                return Err(anyhow!("there is no folder at {}", path.display()));
            }
        }
        Ok(())
    }

    fn question(&self, name: &str) -> Option<String> {
        let reads = (!self.read.is_empty()).then(|| format!("read the files in {}", self.read.join(", ")));
        let writes = (!self.write.is_empty()).then(|| format!("read and write the files in {}", self.write.join(", ")));
        let allows: Vec<String> = reads.into_iter().chain(writes).collect();
        Some(format!("Allow “{name}” to {}?", allows.join(", and to ")))
    }
}

/// `permission` as it is kept: what it names in `home` spelled `~/…`, as a manifest spells it.
pub fn spelled(permission: &Permission, home: Option<&Path>) -> Permission {
    ask_of(permission).spelled(home)
}

/// Ask the user to give `plugin` `asked`, with the system's alert: what it allows, and the plugin's
/// `reason` under it. Allowed, it is given and the plugin restarts with it, so the answer never
/// reaches the plugin that asked; `false` when declined. Without asking: an error when it can't be
/// given or what it names isn't there, `true` when the plugin has it already.
pub fn ask(plugin: &PluginProperties, asked: Permission, reason: String, cx: &mut App) -> Task<Result<bool>> {
    let home = std::env::home_dir();
    let asked = spelled(&asked, home.as_deref());
    if let Err(error) = super::can_give(plugin, &asked) {
        return Task::ready(Err(error));
    }
    if super::has(plugin, &asked, cx) {
        return Task::ready(Ok(true));
    }
    let ask = ask_of(&asked);
    if let Err(error) = ask.there(home.as_deref()) {
        return Task::ready(Err(error));
    }
    let Some(question) = ask.question(&plugin.name) else {
        return Task::ready(Err(anyhow!("{} is given only when the plugin is installed", asked.spec().name())));
    };
    let plugin = plugin.clone();
    // The alert is shown outside this update, on the window that has the keyboard.
    cx.spawn(async move |cx| {
        let allowed = cx.update(|cx| dialogs::allow(question, reason, cx)).await?;
        if !allowed {
            return Ok(false);
        }
        cx.update(|cx| super::give(&plugin, vec![asked], cx))?;
        // The plugin that asked is gone: it starts again with what it was given.
        futures::future::pending().await
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const NONE: [&str; 0] = [];

    #[test]
    fn what_is_asked_for_is_kept_as_a_manifest_spells_it() {
        let home = Some(Path::new("/Users/ada"));
        let files = Permission::files(["/Users/ada/Desktop", "/Volumes/Work"], ["~/Notes", "/Users/ada"]);
        assert_eq!(spelled(&files, home), Permission::files(["~/Desktop", "/Volumes/Work"], ["~/Notes", "~"]));
        let commands = Permission::commands(["/Users/ada/.cargo/bin/rg", "/opt/homebrew/bin/git"]);
        assert_eq!(spelled(&commands, home), Permission::commands(["~/.cargo/bin/rg", "/opt/homebrew/bin/git"]));
        assert_eq!(spelled(&files, None), files, "no home: as it is");
        assert_eq!(spelled(&Permission::network(), home), Permission::network());
    }

    #[test]
    fn the_alert_says_what_is_asked_for() {
        let question = |permission: Permission| ask_of(&permission).question("Notes");
        assert_eq!(question(Permission::files(NONE, ["~/Projects"])).unwrap(), "Allow “Notes” to read and write the files in ~/Projects?");
        assert_eq!(question(Permission::files(["~/Desktop", "/Volumes/Work"], NONE)).unwrap(), "Allow “Notes” to read the files in ~/Desktop, /Volumes/Work?");
        assert_eq!(
            question(Permission::files(["~/Desktop"], ["~/Notes"])).unwrap(),
            "Allow “Notes” to read the files in ~/Desktop, and to read and write the files in ~/Notes?"
        );
        assert_eq!(question(Permission::commands(["/opt/homebrew/bin/git"])).unwrap(), "Allow “Notes” to run /opt/homebrew/bin/git?");
        assert!(question(Permission::network()).is_none(), "Network is given at install");
    }

    #[test]
    fn what_isnt_there_isnt_asked_for() {
        let folder = std::env::temp_dir().join(format!("delight-ask-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let program = folder.join("tool");
        std::fs::write(&program, "").unwrap();
        let path = |path: &Path| path.to_string_lossy().into_owned();
        let there = |permission: Permission| ask_of(&permission).there(None);

        assert!(there(Permission::files([path(&folder)], NONE)).is_ok());
        assert!(there(Permission::files([path(&folder.join("gone"))], NONE)).unwrap_err().to_string().contains("no folder"));
        assert!(there(Permission::files(["Projects"], NONE)).unwrap_err().to_string().contains("in full"), "relative");
        assert!(there(Permission::files([path(&program)], NONE)).is_err(), "a file isn't a folder");
        assert!(there(Permission::commands([path(&program)])).is_ok());
        assert!(there(Permission::commands([path(&folder)])).unwrap_err().to_string().contains("no program"), "a folder isn't a program");
        std::fs::remove_dir_all(&folder).ok();
    }
}
