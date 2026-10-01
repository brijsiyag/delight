//! What one plugin may ask of the app: its host object.

use delight_protocol::{CommandsApi, DnsApi, HostApi, HttpApi, Theme};
use delight_runtime::Granted;
use delight_ui::ActiveTheme as _;
use embedded_gpui::{ClipboardApi, Ref, shared};
use anyhow::Result;
use gpui::{Context, Subscription, Task};

use crate::{dialogs, history, launcher, plugin_settings, plugin_windows, secrets, settings, settings_window};

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

    fn dns(&mut self, cx: &mut Context<Self>) -> Option<Ref<DnsApi>> {
        self.granted.dns(cx)
    }

    fn commands(&mut self, cx: &mut Context<Self>) -> Option<Ref<CommandsApi>> {
        self.granted.commands(cx)
    }

    fn secret(&mut self, key: String, cx: &mut Context<Self>) -> Task<Result<Option<String>>> {
        Task::ready(secrets::get_mut(cx).get(&self.plugin_id, &key))
    }

    fn set_secret(&mut self, key: String, value: String, cx: &mut Context<Self>) -> Task<Result<()>> {
        Task::ready(secrets::get_mut(cx).set(&self.plugin_id, &key, &value))
    }

    // Deferred: the launcher may be in the middle of an update.
    fn set_launcher_input(&mut self, text: String, cx: &mut Context<Self>) {
        let plugin_id = self.plugin_id.clone();
        cx.defer(move |cx| launcher::set_input(&plugin_id, text, cx));
    }

    fn settings(&mut self, cx: &mut Context<Self>) -> Task<Result<String>> {
        Task::ready(Ok(plugin_settings::get_mut(cx).get(&self.plugin_id)))
    }

    fn set_settings(&mut self, json: String, cx: &mut Context<Self>) -> Task<Result<()>> {
        Task::ready(plugin_settings::get_mut(cx).set_json(&self.plugin_id, &json))
    }

    fn utc_offset_seconds(&mut self, _cx: &mut Context<Self>) -> i32 {
        chrono::Local::now().offset().local_minus_utc()
    }

    fn show_settings(&mut self, cx: &mut Context<Self>) {
        let plugin_id = self.plugin_id.clone();
        cx.defer(move |cx| settings_window::open_plugin(plugin_id, cx));
    }

    fn open_window(
        &mut self,
        key: String,
        title: String,
        width: f32,
        height: f32,
        hide_with_launcher: Option<bool>,
        cx: &mut Context<Self>,
    ) -> Task<Result<bool>> {
        let plugin_id = self.plugin_id.clone();
        // A plugin built before 0.1 doesn't say: its windows hide with the launcher, as they did.
        let hide_with_launcher = hide_with_launcher.unwrap_or(true);
        // Deferred: the launcher may be in the middle of an update, and the window takes the focus.
        let (sender, receiver) = futures::channel::oneshot::channel();
        cx.defer(move |cx| {
            let opened = plugin_windows::open(&plugin_id, key, title, width, height, hide_with_launcher, cx);
            cx.spawn(async move |_| {
                let _ = sender.send(opened.await);
            })
            .detach();
        });
        cx.spawn(async move |_, _| Ok(receiver.await.unwrap_or(false)))
    }

    fn set_window_shown(&mut self, key: String, shown: bool, cx: &mut Context<Self>) -> bool {
        plugin_windows::set_shown(&self.plugin_id, &key, shown, cx)
    }

    fn confirm(&mut self, title: String, message: String, continue_label: String, destructive: bool, cx: &mut Context<Self>) -> Task<Result<bool>> {
        // Deferred: the alert is shown outside this update, on the window that has the keyboard.
        let (sender, receiver) = futures::channel::oneshot::channel();
        cx.defer(move |cx| {
            let chosen = dialogs::confirm(title, message, continue_label, destructive, cx);
            cx.spawn(async move |_| {
                let _ = sender.send(chosen.await);
            })
            .detach();
        });
        cx.spawn(async move |_, _| Ok(receiver.await.unwrap_or(false)))
    }

    // TEMPORARY(open_url)
    fn open_url(&mut self, url: String, cx: &mut Context<Self>) -> Task<Result<()>> {
        if let Err(error) = delight_runtime::open_url::openable(&url) {
            return Task::ready(Err(error));
        }
        // Deferred: the browser taking focus hides the launcher, mid-update otherwise.
        cx.defer(move |cx| cx.open_url(&url));
        Task::ready(Ok(()))
    }
}
