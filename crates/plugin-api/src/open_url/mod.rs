//! TEMPORARY(open_url): opening a web page in the browser, through the app, until
//! embedded_gpui forwards GPUI's own `cx.open_url` from plugins. docs/development.md,
//! "Temporary host APIs".

use anyhow::{Result, anyhow};
use delight_protocol::HostApiCaller as _;

use crate::Host;
use crate::gpui::{App, Task};

impl Host {
    /// Open `url` with the app macOS has for it: a web page in the browser, `mailto:`
    /// in the mail app, another app's own link (`zoommtg:`, `slack:`). For a sign-in,
    /// start a listener for its redirect first ([`Host::listen_http`]). `file:` is
    /// refused, with why. Needs no permission.
    pub fn open_url(&self, url: impl Into<String>, cx: &mut App) -> Task<Result<()>> {
        let Some(remote) = &self.remote else {
            return Task::ready(Err(anyhow!("opening a URL is Delight's: a plugin can only in Delight")));
        };
        let opened = remote.open_url(url.into(), cx);
        cx.spawn(async move |_| opened.await)
    }
}
