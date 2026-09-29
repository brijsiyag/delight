//! Plugins' settings: one small JSON value for each plugin, in `plugin-settings.json`
//! (`{plugin id: value}`), which the plugin reads and replaces as a whole. Apart from the
//! plugin's data folder and its secrets; deleting the plugin removes it.

use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::{Context as _, Result, bail};
use delight_protocol::{MAX_SETTINGS_BYTES, validate_id};
use gpui::{App, Global};
use serde_json::Value;

use crate::files::{read_json, write_json};

pub struct PluginSettings {
    path: PathBuf,
    saved: BTreeMap<String, Value>,
}

impl Global for PluginSettings {}

/// Load the settings from Delight's folder.
pub fn init(cx: &mut App) {
    cx.set_global(PluginSettings::open(crate::app_dir().join("plugin-settings.json")));
}

pub fn get_mut(cx: &mut App) -> &mut PluginSettings {
    cx.global_mut::<PluginSettings>()
}

/// Forget what a deleted plugin saved.
pub fn forget_plugin(plugin_id: &str, cx: &mut App) {
    if let Err(error) = get_mut(cx).set(plugin_id, Value::Null) {
        log::error!("forgetting {plugin_id}'s settings: {error:#}");
    }
}

impl PluginSettings {
    fn open(path: PathBuf) -> Self {
        let saved = read_json(&path).unwrap_or_default();
        PluginSettings { path, saved }
    }

    /// The plugin's settings as JSON text: `null` if it saved none.
    pub fn get(&self, plugin_id: &str) -> String {
        self.saved.get(plugin_id).map_or_else(|| "null".to_string(), Value::to_string)
    }

    /// Replace the plugin's settings with the JSON in `json`; `null` removes them.
    pub fn set_json(&mut self, plugin_id: &str, json: &str) -> Result<()> {
        if json.len() > MAX_SETTINGS_BYTES {
            bail!("settings are at most {MAX_SETTINGS_BYTES} bytes: keep more in the data folder");
        }
        let value: Value = serde_json::from_str(json).context("settings must be JSON")?;
        self.set(plugin_id, value)
    }

    fn set(&mut self, plugin_id: &str, value: Value) -> Result<()> {
        validate_id(plugin_id)?;
        let changed = if value.is_null() {
            self.saved.remove(plugin_id).is_some()
        } else {
            self.saved.insert(plugin_id.to_string(), value.clone()) != Some(value)
        };
        if changed { write_json(&self.path, &self.saved) } else { Ok(()) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(name: &str) -> PluginSettings {
        let path = std::env::temp_dir().join(format!("delight-plugin-settings-{name}-{}.json", std::process::id()));
        std::fs::remove_file(&path).ok();
        PluginSettings::open(path)
    }

    #[test]
    fn a_plugin_saves_reads_and_removes_its_settings() {
        let mut settings = store("saved");
        assert_eq!(settings.get("acme.one"), "null");
        settings.set_json("acme.one", r#"{"account":"ada@example.com","range":7}"#).unwrap();
        assert_eq!(settings.get("acme.one"), r#"{"account":"ada@example.com","range":7}"#);

        let mut reopened = PluginSettings::open(settings.path.clone());
        assert_eq!(reopened.get("acme.one"), r#"{"account":"ada@example.com","range":7}"#, "kept in the file");
        assert_eq!(reopened.get("acme.two"), "null", "each plugin has its own");
        reopened.set_json("acme.one", "null").unwrap();
        assert_eq!(reopened.get("acme.one"), "null");
        assert_eq!(std::fs::read_to_string(&reopened.path).unwrap().trim(), "{}");
        std::fs::remove_file(&reopened.path).ok();
    }

    #[test]
    fn what_is_not_json_or_too_big_or_unnamed_is_refused() {
        let mut settings = store("refused");
        assert!(settings.set_json("acme.one", "{not json").is_err());
        assert!(settings.set_json("acme.one", &format!("\"{}\"", "x".repeat(MAX_SETTINGS_BYTES))).is_err());
        assert!(settings.set_json("../evil", "1").is_err());
        assert_eq!(settings.get("acme.one"), "null");
    }

    #[test]
    fn forgetting_a_plugin_keeps_the_others() {
        let mut settings = store("forget");
        settings.set_json("acme.gone", "1").unwrap();
        settings.set_json("acme.kept", "2").unwrap();
        settings.set("acme.gone", Value::Null).unwrap();
        assert_eq!((settings.get("acme.gone").as_str(), settings.get("acme.kept").as_str()), ("null", "2"));
        std::fs::remove_file(&settings.path).ok();
    }
}
