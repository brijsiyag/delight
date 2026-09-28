//! Delight's own settings, in `settings.json`: what the settings window's General
//! page changes. Missing fields take their defaults and unknown ones are ignored, so
//! the previous attempt's file reads as it is. Plugins' own settings live elsewhere.

use delight_ui::ThemeMode;
use gpui::{App, Global};
use serde::{Deserialize, Serialize};

use crate::files::{read_json, write_json};
use crate::{history, login, macos};

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
    delight_ui::theme::set_mode(cx, mode);
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
}
