//! Delight's own settings, in `settings.json`: what the settings window's General
//! page changes. Missing fields take their defaults and unknown ones are ignored, so
//! the previous attempt's file reads as it is. Plugins' own settings live elsewhere.

use std::collections::{BTreeMap, BTreeSet};

use gpui::{App, Global};
use serde::{Deserialize, Serialize};

use crate::files::{read_json, write_json};
use crate::theme::{self, ThemeMode};
use crate::{history, launcher, login, macos};

/// Light, dark, or following macOS.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Appearance {
    #[default]
    System,
    Light,
    Dark,
}

/// ⌘⇧Space.
pub const DEFAULT_LAUNCHER_SHORTCUT: &str = "cmd-shift-space";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub appearance: Appearance,
    /// The shortcut that shows and hides the launcher from any app, written like
    /// every key in Delight: `cmd-shift-space`.
    pub launcher_shortcut: String,
    /// Hide the launcher when another app is used.
    pub hide_on_blur: bool,
    /// Keep the input history: the input comes back at launch, and what tools
    /// remembered completes what's typed. Turning it off erases it.
    pub input_history: bool,
    /// Put the clipboard's text into the input when the launcher opens.
    pub paste_clipboard_on_open: bool,
    /// Start Delight when the user logs in. On unless turned off.
    pub open_at_login: bool,
    /// Plugins turned off, by id.
    pub disabled_plugins: BTreeSet<String>,
    /// Tools turned off: by plugin id, the tools' (operations') ids. Kept apart from
    /// the plugins, so a plugin turned back on has its tools as they were.
    pub disabled_tools: BTreeMap<String, BTreeSet<String>>,
    /// Plugins that update only when asked, by id. The others install an update that asks for
    /// nothing new on their own (see `plugins::updates`).
    pub manual_updates: BTreeSet<String>,
}

impl Settings {
    pub fn plugin_on(&self, plugin: &str) -> bool {
        !self.disabled_plugins.contains(plugin)
    }

    /// Whether the tool itself is on (its plugin may still be off).
    pub fn tool_on(&self, plugin: &str, tool: &str) -> bool {
        !self.disabled_tools.get(plugin).is_some_and(|tools| tools.contains(tool))
    }

    /// Whether the tool runs: it and its plugin are both on.
    pub fn tool_runs(&self, plugin: &str, tool: &str) -> bool {
        self.plugin_on(plugin) && self.tool_on(plugin, tool)
    }

    pub fn set_plugin_on(&mut self, plugin: &str, on: bool) {
        if on {
            self.disabled_plugins.remove(plugin);
        } else {
            self.disabled_plugins.insert(plugin.to_string());
        }
    }

    pub fn set_tool_on(&mut self, plugin: &str, tool: &str, on: bool) {
        let tools = self.disabled_tools.entry(plugin.to_string()).or_default();
        if on {
            tools.remove(tool);
        } else {
            tools.insert(tool.to_string());
        }
        if tools.is_empty() {
            self.disabled_tools.remove(plugin);
        }
    }

    /// Whether the plugin installs an update that asks for nothing new on its own: unless turned off.
    pub fn updates_automatically(&self, plugin: &str) -> bool {
        !self.manual_updates.contains(plugin)
    }

    pub fn set_updates_automatically(&mut self, plugin: &str, on: bool) {
        if on {
            self.manual_updates.remove(plugin);
        } else {
            self.manual_updates.insert(plugin.to_string());
        }
    }

    /// Forget a deleted plugin's switches.
    pub fn forget_plugin(&mut self, plugin: &str) {
        self.disabled_plugins.remove(plugin);
        self.disabled_tools.remove(plugin);
        self.manual_updates.remove(plugin);
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            appearance: Appearance::System,
            launcher_shortcut: DEFAULT_LAUNCHER_SHORTCUT.to_string(),
            hide_on_blur: true,
            input_history: true,
            paste_clipboard_on_open: false,
            open_at_login: true,
            disabled_plugins: BTreeSet::new(),
            disabled_tools: BTreeMap::new(),
            manual_updates: BTreeSet::new(),
        }
    }
}

impl Global for Settings {}

fn path() -> std::path::PathBuf {
    crate::app_dir().join("settings.json")
}

/// The saved settings, or the defaults.
pub fn load() -> Settings {
    read_json(&path()).unwrap_or_default()
}

pub fn get(cx: &App) -> &Settings {
    cx.global::<Settings>()
}

/// Change the settings, save them, and apply what changed.
pub fn update(cx: &mut App, change: impl FnOnce(&mut Settings)) {
    let settings = cx.global_mut::<Settings>();
    let before = settings.clone();
    change(settings);
    let after = settings.clone();
    if after == before {
        return;
    }
    if let Err(error) = write_json(&path(), &after) {
        log::error!("saving the settings: {error:#}");
    }
    if after.appearance != before.appearance {
        apply_appearance(after.appearance, cx);
    }
    if after.open_at_login != before.open_at_login {
        login::apply(after.open_at_login);
    }
    if before.input_history && !after.input_history {
        history::erase(cx);
    }
    if after.disabled_plugins != before.disabled_plugins || after.disabled_tools != before.disabled_tools {
        launcher::refresh(cx);
    }
    cx.refresh_windows();
}

/// Draw Delight in the chosen appearance: its theme, and macOS's own parts of its
/// windows (the blur, the glass, the title bar).
pub fn apply_appearance(appearance: Appearance, cx: &mut App) {
    let (mode, dark) = match appearance {
        Appearance::System => (ThemeMode::System, None),
        Appearance::Light => (ThemeMode::Light, Some(false)),
        Appearance::Dark => (ThemeMode::Dark, Some(true)),
    };
    theme::set_mode(cx, mode);
    // Outside this update: macOS redraws the windows, calling back into GPUI.
    cx.spawn(async move |_| macos::set_app_appearance(dark)).detach();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_previous_attempts_file() {
        let old = r#"{"appearance": "dark", "launcher_shortcut": "alt-space", "hide_on_blur": false,
            "plugin_dir": null, "disabled_plugins": [], "crashed_plugins": {}}"#;
        let settings: Settings = serde_json::from_str(old).unwrap();
        assert_eq!(
            settings,
            Settings {
                appearance: Appearance::Dark,
                launcher_shortcut: "alt-space".into(),
                hide_on_blur: false,
                ..Settings::default()
            },
            "what's missing is the default, and what's unknown is ignored"
        );
        let saved: Settings = serde_json::from_str(&serde_json::to_string(&settings).unwrap()).unwrap();
        assert_eq!(saved, settings);
    }

    #[test]
    fn a_tool_runs_while_it_and_its_plugin_are_on() {
        let mut settings = Settings::default();
        settings.set_tool_on("acme.logs", "search", false);
        assert!(!settings.tool_runs("acme.logs", "search"));
        assert!(settings.tool_runs("acme.logs", "tail"));

        // Turning the plugin off and on again keeps its tools' switches.
        settings.set_tool_on("acme.logs", "search", true);
        settings.set_plugin_on("acme.logs", false);
        assert!(!settings.tool_runs("acme.logs", "search") && settings.tool_on("acme.logs", "search"));
        settings.set_plugin_on("acme.logs", true);
        assert!(settings.tool_runs("acme.logs", "search"));
        assert!(settings.disabled_tools.is_empty(), "nothing left to remember");
    }

    #[test]
    fn each_plugin_updates_automatically_unless_turned_off() {
        let mut settings = Settings::default();
        assert!(settings.updates_automatically("acme.logs"), "on by default");
        settings.set_updates_automatically("acme.logs", false);
        assert!(!settings.updates_automatically("acme.logs"));
        assert!(settings.updates_automatically("acme.calendar"), "the others keep theirs");
        settings.forget_plugin("acme.logs");
        assert!(settings.updates_automatically("acme.logs"), "a deleted plugin's switch goes with it");
        assert!(settings.manual_updates.is_empty());
    }
}
