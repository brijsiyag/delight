//! [`host`]: the app, as a plugin reaches it.

use delight_protocol::{HostApi, HostApiCaller as _};
use embedded_gpui::Remote;

use crate::gpui::{App, Global};

/// The app, as a plugin reaches it: `host(cx).toast("Copied", cx)`. Calls don't
/// wait for the app. Natively (in a plugin's unit tests) they do nothing.
#[derive(Clone)]
pub struct Host {
    remote: Option<Remote<HostApi>>,
}

/// The app this plugin runs in.
pub fn host(cx: &App) -> Host {
    Host {
        remote: cx.try_global::<HostRoot>().map(|root| root.0.clone()),
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
}

/// The app's root object, set by the glue when the plugin starts.
pub(crate) struct HostRoot(pub(crate) Remote<HostApi>);

impl Global for HostRoot {}
