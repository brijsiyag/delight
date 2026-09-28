//! [`host`]: the app, as a plugin reaches it, and [`theme`]: its theme, kept current.

use delight_protocol::{HostApi, HostApiCaller as _, Theme};
use embedded_gpui::Remote;

use crate::Operations;
use crate::gpui::{App, Global, Subscription};

/// The app, as a plugin reaches it: `host(cx).toast("Copied", cx)`. Calls don't
/// wait for the app. Natively (in a plugin's unit tests) they do nothing.
#[derive(Clone)]
pub struct Host {
    remote: Option<Remote<HostApi>>,
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

    /// Put `text` on the clipboard.
    pub fn copy_text(&self, text: impl Into<String>, cx: &mut App) {
        if let Some(remote) = &self.remote {
            drop(remote.copy_text(text.into(), cx));
        }
    }

    /// Hide the launcher.
    pub fn hide(&self, cx: &mut App) {
        if let Some(remote) = &self.remote {
            drop(remote.hide(cx));
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

/// The app's theme, as the plugin last heard it: `None` until the app has answered,
/// and natively (in a plugin's unit tests). When it changes, the plugin's views are
/// drawn again.
pub fn theme(cx: &App) -> Option<&Theme> {
    cx.try_global::<HostRoot>()?.theme.as_ref()
}

/// The app's root object, and the app's theme, kept current. Set by the glue when
/// the plugin starts.
pub(crate) struct HostRoot {
    remote: Remote<HostApi>,
    theme: Option<Theme>,
    _observing: Subscription,
}

impl Global for HostRoot {}

#[cfg(target_arch = "wasm32")]
impl HostRoot {
    /// Keep the app's root object, and follow its theme: the app notifies the object
    /// when its theme changes, and once when observing starts.
    pub(crate) fn connect(remote: Remote<HostApi>, cx: &mut App) {
        let observing = remote.observe(cx, ask_for_theme);
        cx.set_global(HostRoot { remote, theme: None, _observing: observing });
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
            cx.global_mut::<HostRoot>().theme = Some(theme);
            cx.refresh_windows();
        });
    })
    .detach();
}
