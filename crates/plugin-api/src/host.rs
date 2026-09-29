//! [`host`]: the app, as a plugin reaches it, and [`theme`]: a copy of its theme,
//! updated whenever the app's theme changes.

use anyhow::{Context as _, Result, anyhow};
use delight_protocol::{HostApi, HostApiCaller as _, Theme};
use embedded_gpui::Remote;

use crate::Operations;
use crate::gpui::{App, Global, Subscription, Task};

/// The app, as a plugin reaches it: `host(cx).toast("Copied", cx)`. Calls don't
/// wait for the app. Natively (in a plugin's unit tests) they do nothing.
#[derive(Clone)]
pub struct Host {
    // Crate-wide for `dns/`; TEMPORARY(network), TEMPORARY(open_url): and for
    // `network/` and `open_url/`.
    pub(crate) remote: Option<Remote<HostApi>>,
}

/// The app this plugin runs in.
pub fn host(cx: &App) -> Host {
    Host {
        remote: cx.try_global::<HostRoot>().map(|root| root.remote.clone()),
    }
}

impl Host {
    /// Show `message` in the launcher's footer for a moment.
    pub fn toast(&self, message: impl Into<String>, cx: &mut App) {
        if let Some(remote) = &self.remote {
            drop(remote.toast(message.into(), cx));
        }
    }

    /// Hide the launcher.
    pub fn hide(&self, cx: &mut App) {
        if let Some(remote) = &self.remote {
            drop(remote.hide(cx));
        }
    }

    /// Make the launcher's input `text`, so another tool can take over: a tool offering
    /// "open the inner value" of what it shows puts it here. An undoable edit (⌘Z brings the
    /// old text back) after which the launcher detects again. The app applies it only while
    /// one of this plugin's own tools is the selected one, and a moment later.
    pub fn set_input(&self, text: impl Into<String>, cx: &mut App) {
        if let Some(remote) = &self.remote {
            drop(remote.set_launcher_input(text.into(), cx));
        }
    }

    /// A secret this plugin saved with [`Host::set_secret`] (an API key, a sign-in's
    /// tokens): `None` if there is none by this name. Kept encrypted by the app.
    pub fn secret(&self, key: impl Into<String>, cx: &mut App) -> Task<Result<Option<String>>> {
        let Some(remote) = &self.remote else {
            return Task::ready(Err(anyhow!("secrets are Delight's: a plugin has them only in Delight")));
        };
        let asked = remote.secret(key.into(), cx);
        cx.spawn(async move |_| asked.await)
    }

    /// Save a secret under `key` (1 to 256 bytes), encrypted by the app. An empty
    /// `value` deletes it.
    pub fn set_secret(&self, key: impl Into<String>, value: impl Into<String>, cx: &mut App) -> Task<Result<()>> {
        let Some(remote) = &self.remote else {
            return Task::ready(Err(anyhow!("secrets are Delight's: a plugin has them only in Delight")));
        };
        let saved = remote.set_secret(key.into(), value.into(), cx);
        cx.spawn(async move |_| saved.await)
    }

    /// This plugin's settings, as the type `T` the plugin defines (`#[derive(Serialize,
    /// Deserialize)]`): what it last saved with [`Host::set_settings`], or `None` if it
    /// saved none. The app keeps them, a small value apart from the data folder. An error if
    /// what is saved doesn't fit `T` (the type changed): `unwrap_or_default()` starts over.
    pub fn settings<T: serde::de::DeserializeOwned + 'static>(&self, cx: &mut App) -> Task<Result<Option<T>>> {
        let Some(remote) = &self.remote else {
            return Task::ready(Err(anyhow!("settings are Delight's: a plugin has them only in Delight")));
        };
        let asked = remote.settings(cx);
        cx.spawn(async move |_| {
            let json = asked.await?;
            let value: serde_json::Value = serde_json::from_str(&json).context("the saved settings aren't JSON")?;
            if value.is_null() {
                return Ok(None);
            }
            serde_json::from_value(value).map(Some).context("the saved settings don't fit the type the plugin reads them as")
        })
    }

    /// Save this plugin's settings, replacing what was saved: `T` as JSON, at most
    /// [`delight_protocol::MAX_SETTINGS_BYTES`]. Keep more in the data folder.
    pub fn set_settings<T: serde::Serialize>(&self, settings: &T, cx: &mut App) -> Task<Result<()>> {
        match serde_json::to_string(settings) {
            Ok(json) => self.save_settings(json, cx),
            Err(error) => Task::ready(Err(anyhow!("the settings can't be saved as JSON: {error}"))),
        }
    }

    /// Remove this plugin's saved settings.
    pub fn clear_settings(&self, cx: &mut App) -> Task<Result<()>> {
        self.save_settings("null".to_string(), cx)
    }

    fn save_settings(&self, json: String, cx: &mut App) -> Task<Result<()>> {
        let Some(remote) = &self.remote else {
            return Task::ready(Err(anyhow!("settings are Delight's: a plugin has them only in Delight")));
        };
        let saved = remote.set_settings(json, cx);
        cx.spawn(async move |_| saved.await)
    }

    /// This Mac's time zone as its offset from UTC, in seconds, as it was when the plugin
    /// started (the plugin's own clock is UTC). 0 until the app has answered, and
    /// natively.
    pub fn utc_offset_seconds(&self, cx: &App) -> i32 {
        cx.try_global::<UtcOffset>().map_or(0, |offset| offset.0)
    }

    /// Open the settings window on this plugin's own page.
    pub fn open_settings(&self, cx: &mut App) {
        if let Some(remote) = &self.remote {
            drop(remote.show_settings(cx));
        }
    }

    /// Remember `text` as an input worth coming back to, for `operation`: the
    /// launcher offers it as a completion while typing and in its history search
    /// (⌃R), and brings this tool up when it's used. Nothing else is remembered.
    pub fn remember_input(&self, operation: impl Operations, text: impl Into<String>, cx: &mut App) {
        if let Some(remote) = &self.remote {
            drop(remote.remember_input(operation.id().to_string(), text.into(), cx));
        }
    }
}

/// The plugin's root object, which the app observes to know when its settings sections
/// changed: set by the glue when the plugin starts.
pub(crate) struct RootId(pub(crate) crate::gpui::EntityId);

impl Global for RootId {}

/// Tell the app that the plugin's settings sections changed (one added, removed or
/// resized, a title changed): it asks for them again. Content that changes inside a
/// section's own view needs no call: the view redraws itself. Natively it does nothing.
pub fn settings_changed(cx: &mut App) {
    if let Some(root) = cx.try_global::<RootId>().map(|root| root.0) {
        cx.notify(root);
    }
}

/// A copy of the app's theme, updated whenever the app's theme changes: `None` until
/// the app has answered, and natively (in a plugin's unit tests). It's a GPUI global,
/// so `cx.observe_global::<Theme>()` follows it; the plugin's views are drawn again
/// when it changes.
pub fn theme(cx: &App) -> Option<&Theme> {
    cx.try_global::<Theme>()
}

/// The Mac's UTC offset in seconds, as the app said when the plugin started.
struct UtcOffset(i32);

impl Global for UtcOffset {}

/// The app's root object, and what follows the app's theme. Set by the glue when the
/// plugin starts.
pub(crate) struct HostRoot {
    remote: Remote<HostApi>,
    _observing: Subscription,
}

impl Global for HostRoot {}

#[cfg(target_arch = "wasm32")]
impl HostRoot {
    /// Keep the app's root object, follow its theme (the app notifies the object when
    /// its theme changes, and once when observing starts), and give GPUI the app's
    /// clipboard, so its clipboard calls reach the Mac's.
    pub(crate) fn connect(remote: Remote<HostApi>, cx: &mut App) {
        let clipboard = remote.clipboard(cx);
        cx.spawn(async move |cx| match clipboard.await {
            Ok(clipboard) => cx.update(|cx| embedded_gpui::use_clipboard(clipboard, cx)),
            Err(error) => log::error!("connecting the clipboard: {error:#}"),
        })
        .detach();
        let offset = remote.utc_offset_seconds(cx);
        cx.spawn(async move |cx| match offset.await {
            Ok(seconds) => cx.update(|cx| cx.set_global(UtcOffset(seconds))),
            Err(error) => log::error!("asking for the UTC offset: {error:#}"),
        })
        .detach();
        let observing = remote.observe(cx, ask_for_theme);
        cx.set_global(HostRoot { remote, _observing: observing });
    }
}

/// Ask the app for its theme, then draw again with it.
#[cfg(target_arch = "wasm32")]
fn ask_for_theme(cx: &mut App) {
    let Some(remote) = cx.try_global::<HostRoot>().map(|root| root.remote.clone()) else {
        return;
    };
    let asked = remote.current_theme(cx);
    cx.spawn(async move |cx| {
        let Ok(theme) = asked.await else { return };
        cx.update(|cx| {
            cx.set_global(theme);
            cx.refresh_windows();
        });
    })
    .detach();
}
