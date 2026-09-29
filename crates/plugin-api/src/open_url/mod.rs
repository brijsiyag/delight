//! TEMPORARY(open_url): opening a web page in the browser, through the app, until
//! embedded_gpui forwards GPUI's own `cx.open_url` from plugins. README, "Temporary
//! host APIs".

use anyhow::{Result, anyhow};
use delight_protocol::HostApiCaller as _;

use crate::Host;
use crate::gpui::{App, Task};

impl Host {
    /// Open `url` in the browser: an `http` or `https` page, such as a sign-in's
    /// (start a listener first for its redirect: [`Host::listen_http`]). Anything
    /// else is refused, with why. Needs no permission.
    pub fn open_url(&self, url: impl Into<String>, cx: &mut App) -> Task<Result<()>> {
        let Some(remote) = &self.remote else {
            return Task::ready(Err(anyhow!("opening a web page is Delight's: a plugin can only in Delight")));
        };
        let opened = remote.open_url(url.into(), cx);
        cx.spawn(async move |_| opened.await)
    }
}
