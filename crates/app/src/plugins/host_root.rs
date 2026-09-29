//! What one plugin may ask of the app: its host object.

use delight_protocol::{HostApi, HttpApi, Theme};
use delight_runtime::Granted;
use delight_ui::ActiveTheme as _;
use embedded_gpui::{ClipboardApi, Ref, shared};
use anyhow::Result;
use gpui::{Context, Subscription, Task};

use crate::{history, launcher, settings};

/// What one plugin may ask of the app. Each plugin has its own, so every call is
/// that plugin's.
pub(super) struct HostRoot {
    plugin_id: String,
    /// What the app hands it, when it asks.
    granted: Granted,
    _theme_changes: Subscription,
}

impl HostRoot {
    pub(super) fn new(plugin_id: String, granted: Granted, cx: &mut Context<Self>) -> Self {
        // The plugin observes this object: tell it when the theme changes.
        let theme_changes = cx.observe_global::<delight_ui::Theme>(|_, cx| cx.notify());
        Self { plugin_id, granted, _theme_changes: theme_changes }
    }
}

#[shared]
impl HostApi for HostRoot {
    // Deferred: the launcher may be in the middle of an update.
    fn toast(&mut self, message: String, cx: &mut Context<Self>) {
        cx.defer(move |cx| launcher::toast(message.into(), cx));
    }

    fn hide(&mut self, cx: &mut Context<Self>) {
        cx.defer(launcher::hide);
    }

    fn remember_input(&mut self, operation: String, text: String, cx: &mut Context<Self>) {
        if !settings::get(cx).input_history {
            return;
        }
        let plugin_id = &self.plugin_id;
        if let Err(error) = history::get_mut(cx).remember(plugin_id, &operation, &text) {
            log::error!("remembering {plugin_id}'s input: {error:#}");
        }
    }

    fn current_theme(&mut self, cx: &mut Context<Self>) -> Theme {
        Theme::from(cx.theme())
    }

    fn clipboard(&mut self, cx: &mut Context<Self>) -> Ref<ClipboardApi> {
        self.granted.clipboard(cx)
    }

    // TEMPORARY(network)
    fn http(&mut self, cx: &mut Context<Self>) -> Option<Ref<HttpApi>> {
        self.granted.http(cx)
    }

    // TEMPORARY(open_url)
    fn open_url(&mut self, url: String, cx: &mut Context<Self>) -> Task<Result<()>> {
        if let Err(error) = delight_runtime::open_url::web_page(&url) {
            return Task::ready(Err(error));
        }
        // Deferred: the browser taking focus hides the launcher, mid-update otherwise.
        cx.defer(move |cx| cx.open_url(&url));
        Task::ready(Ok(()))
    }
}
