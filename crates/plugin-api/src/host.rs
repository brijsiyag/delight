//! [`host`]: the app, as a plugin reaches it, and [`theme`]: a copy of its theme,
//! updated whenever the app's theme changes.

use anyhow::{Context as _, Result, anyhow};
use delight_protocol::{HostApi, HostApiCaller as _, Permission, Theme};
use embedded_gpui::Remote;

use crate::Operations;
use std::collections::HashMap;

use crate::gpui::{AnyView, App, Global, Subscription, Task};

/// The app, as a plugin reaches it: `host(cx).toast("Copied", cx)`. Calls don't
/// wait for the app. Natively (in a plugin's unit tests) they do nothing.
#[derive(Clone)]
pub struct Host {
    // Crate-wide for `dns/`; TEMPORARY(network), TEMPORARY(open_url), TEMPORARY(pick_folders),
    // TEMPORARY(save_file): and for `network/`, `open_url/`, `pick_folders/` and `save_file/`.
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

    /// Open a window of the plugin's own with `view` in it: a normal window the user resizes and
    /// closes (✕, ⌘W), and the plugin closes with [`Self::close_window`]. It goes off screen when
    /// the launcher hides and comes back with it, unless [`WindowOptions::hide_with_launcher`] says
    /// otherwise; [`Self::show_window`] brings it back before the launcher. A window with the same
    /// [`WindowOptions::key`] that is open comes to the front instead, and `view` is dropped. An
    /// error when the plugin has too many open (close one first), and natively.
    pub fn open_window(&self, options: WindowOptions, view: impl Into<AnyView>, cx: &mut App) -> Task<Result<()>> {
        let Some(remote) = &self.remote else {
            return Task::ready(Err(anyhow!("windows are Delight's: a plugin has them only in Delight")));
        };
        let WindowOptions { key, title, width, height, hide_with_launcher } = options;
        // The app asks for the view once it has made the window: keep it until then.
        cx.default_global::<PendingWindows>().0.insert(key.clone(), view.into());
        let asked = remote.open_window(key.clone(), title, width, height, hide_with_launcher, cx);
        cx.spawn(async move |cx| {
            let opened = asked.await;
            // Taken by now if the window was made; if it wasn't, or already was, it goes.
            cx.update(|cx| cx.default_global::<PendingWindows>().0.remove(&key));
            match opened {
                Ok(true) => Ok(()),
                Ok(false) => Err(anyhow!("Delight has no room for another window from this plugin: close one first")),
                Err(error) => Err(error),
            }
        })
    }

    /// Bring back the plugin's window `key` that went off screen with the launcher, without the
    /// launcher. A plugin doesn't hide its windows: it closes them ([`Self::close_window`]). An
    /// error when no window `key` is open (its user may have closed it), and natively.
    pub fn show_window(&self, key: impl Into<String>, cx: &mut App) -> Task<Result<()>> {
        let Some(remote) = &self.remote else {
            return Task::ready(Err(anyhow!("windows are Delight's: a plugin has them only in Delight")));
        };
        let key = key.into();
        let asked = remote.show_window(key.clone(), cx);
        cx.spawn(async move |_| match asked.await {
            Ok(true) => Ok(()),
            Ok(false) => Err(anyhow!("the plugin has no window {key:?} open")),
            Err(error) => Err(error),
        })
    }

    /// Close the plugin's window `key`, as its user would (✕): the view in it is let go, and
    /// [`Self::open_window`] opens a new window with the view it is given. An error when no window
    /// `key` is open (its user may have closed it), and natively.
    pub fn close_window(&self, key: impl Into<String>, cx: &mut App) -> Task<Result<()>> {
        let Some(remote) = &self.remote else {
            return Task::ready(Err(anyhow!("windows are Delight's: a plugin has them only in Delight")));
        };
        let key = key.into();
        let asked = remote.close_window(key.clone(), cx);
        cx.spawn(async move |_| match asked.await {
            Ok(true) => Ok(()),
            Ok(false) => Err(anyhow!("the plugin has no window {key:?} open")),
            Err(error) => Err(error),
        })
    }

    /// Ask the user to confirm something with the system's own alert (macOS's): `Ok(true)` if they
    /// chose to continue, `Ok(false)` if they cancelled, or the app couldn't show it (another
    /// alert is open, or nothing of Delight is on screen). Use it before what can't be undone.
    /// An error natively.
    pub fn confirm(&self, confirm: Confirm, cx: &mut App) -> Task<Result<bool>> {
        let Some(remote) = &self.remote else {
            return Task::ready(Err(anyhow!("alerts are Delight's: a plugin has them only in Delight")));
        };
        let Confirm { title, message, continue_label, destructive } = confirm;
        let asked = remote.confirm(title, message, continue_label, destructive, cx);
        cx.spawn(async move |_| asked.await)
    }

    /// Ask the user to give the plugin `permission`: more of one its manifest asks for, written as
    /// a manifest writes one (`~` is the home folder):
    ///
    /// ```ignore
    /// let none: [&str; 0] = [];
    /// host(cx).request_permission(Permission::files(none, ["~/Projects"]), "Lists your repositories", cx)
    /// host(cx).request_permission(Permission::commands(["/opt/homebrew/bin/git"]), "Commits for you", cx)
    /// ```
    ///
    /// The system's alert says what it allows (*Allow "Notes" to read and write the files in
    /// ~/Projects?*), the plugin's `reason` under it.
    ///
    /// **When the user allows it, the plugin starts again** with it, and this never returns: save
    /// what it needs (in `/data`, or its settings) before asking. The launcher then asks it about
    /// the input again, so its tool comes back; its windows close. `Ok(false)` when the user
    /// declines, `Ok(true)` at once when the plugin has it already (a folder inside one it has,
    /// with that access, too); an error when it can't be given (the manifest doesn't ask for that
    /// permission, or what it names isn't there), or Delight can't ask (another alert or picker
    /// is open).
    pub fn request_permission(&self, permission: Permission, reason: impl Into<String>, cx: &mut App) -> Task<Result<bool>> {
        let Some(remote) = &self.remote else {
            return Task::ready(Err(anyhow!("permissions are Delight's to give: a plugin has them only in Delight")));
        };
        let permission = match serde_json::to_string(&permission) {
            Ok(permission) => permission,
            Err(error) => return Task::ready(Err(error.into())),
        };
        let asked = remote.request_permission(permission, reason.into(), cx);
        cx.spawn(async move |_| asked.await)
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

/// What to ask the user to confirm ([`Host::confirm`]).
#[derive(Debug, Clone, PartialEq)]
pub struct Confirm {
    /// The question, as the alert's message: "Remove this host?".
    pub title: String,
    /// What follows from it, under the question: "Its API key is removed too."
    pub message: String,
    /// The button that goes ahead: "Remove". "Continue" by default.
    pub continue_label: String,
    /// It can't be undone: the alert warns, and Cancel is the button ↵ presses.
    pub destructive: bool,
}

impl Confirm {
    pub fn new(title: impl Into<String>, message: impl Into<String>) -> Self {
        Self { title: title.into(), message: message.into(), continue_label: "Continue".into(), destructive: false }
    }

    pub fn continue_label(mut self, label: impl Into<String>) -> Self {
        self.continue_label = label.into();
        self
    }

    /// What is asked can't be undone.
    pub fn destructive(mut self) -> Self {
        self.destructive = true;
        self
    }
}

/// What a window of the plugin's own is like ([`Host::open_window`]).
#[derive(Debug, Clone, PartialEq)]
pub struct WindowOptions {
    /// Which window this is: asking again while it is open shows the open one.
    pub key: String,
    pub title: String,
    /// The size it opens at, in points.
    pub width: f32,
    pub height: f32,
    /// It goes off screen when the launcher hides, and comes back with it (the default); or it
    /// stays up until its user closes it.
    pub hide_with_launcher: bool,
}

impl WindowOptions {
    /// A window titled `title`, 640 × 480, that hides with the launcher.
    pub fn new(key: impl Into<String>, title: impl Into<String>) -> Self {
        Self { key: key.into(), title: title.into(), width: 640., height: 480., hide_with_launcher: true }
    }

    pub fn size(mut self, width: f32, height: f32) -> Self {
        self.width = width;
        self.height = height;
        self
    }

    /// Whether the window goes off screen when the launcher hides and comes back with it (`true`,
    /// the default), or stays up until its user closes it (`false`).
    pub fn hide_with_launcher(mut self, hide_with_launcher: bool) -> Self {
        self.hide_with_launcher = hide_with_launcher;
        self
    }
}

/// The views of windows asked for and not yet made, by key: the app asks the plugin for
/// each once its window is there.
#[derive(Default)]
pub(crate) struct PendingWindows(pub(crate) HashMap<String, AnyView>);

impl Global for PendingWindows {}

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
