//! [`host`]: the app, as a plugin reaches it, and [`theme`]: a copy of its theme,
//! updated whenever the app's theme changes.

use delight_protocol::{HostApi, HostApiCaller as _, Theme};
use embedded_gpui::Remote;

use crate::Operations;
use crate::gpui::{App, Global, Subscription};

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

    /// Remember `text` as an input worth coming back to, for `operation`: the
    /// launcher offers it as a completion while typing and in its history search
    /// (⌃R), and brings this tool up when it's used. Nothing else is remembered.
    pub fn remember_input(&self, operation: impl Operations, text: impl Into<String>, cx: &mut App) {
        if let Some(remote) = &self.remote {
            drop(remote.remember_input(operation.id().to_string(), text.into(), cx));
        }
    }
}

/// A copy of the app's theme, updated whenever the app's theme changes: `None` until
/// the app has answered, and natively (in a plugin's unit tests). It's a GPUI global,
/// so `cx.observe_global::<Theme>()` follows it; the plugin's views are drawn again
/// when it changes.
pub fn theme(cx: &App) -> Option<&Theme> {
    cx.try_global::<Theme>()
}

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
