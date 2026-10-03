//! What one plugin may ask of the app: its host object.

use delight_protocol::{Bytes, CommandsApi, DnsApi, FilesPermission, HostApi, HttpApi, Permission, PluginProperties, Theme};
use delight_runtime::Granted;
use embedded_gpui::{ClipboardApi, Ref, shared};
use anyhow::{Result, anyhow};
use gpui::{Context, Subscription, Task};

use crate::{dialogs, history, launcher, permissions, plugin_settings, plugin_windows, secrets, settings, settings_window};

/// What one plugin may ask of the app. Each plugin has its own, so every call is
/// that plugin's.
pub(super) struct HostRoot {
    /// The plugin's properties, from its manifest: its id, the name questions asked for it say,
    /// and the permissions it asks for.
    plugin: PluginProperties,
    /// What the app hands it, when it asks.
    granted: Granted,
    _theme_changes: Subscription,
}

impl HostRoot {
    pub(super) fn new(plugin: PluginProperties, granted: Granted, cx: &mut Context<Self>) -> Self {
        // The plugin observes this object: tell it when the theme changes.
        let theme_changes = cx.observe_global::<Theme>(|_, cx| cx.notify());
        Self { plugin, granted, _theme_changes: theme_changes }
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
        let plugin_id = &self.plugin.id;
        if let Err(error) = history::get_mut(cx).remember(plugin_id, &operation, &text) {
            log::error!("remembering {plugin_id}'s input: {error:#}");
        }
    }

    fn current_theme(&mut self, cx: &mut Context<Self>) -> Theme {
        cx.global::<Theme>().clone()
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
        Task::ready(secrets::get_mut(cx).get(&self.plugin.id, &key))
    }

    fn set_secret(&mut self, key: String, value: String, cx: &mut Context<Self>) -> Task<Result<()>> {
        Task::ready(secrets::get_mut(cx).set(&self.plugin.id, &key, &value))
    }

    // Deferred: the launcher may be in the middle of an update.
    fn set_launcher_input(&mut self, text: String, cx: &mut Context<Self>) {
        let plugin_id = self.plugin.id.clone();
        cx.defer(move |cx| launcher::set_input(&plugin_id, text, cx));
    }

    fn settings(&mut self, cx: &mut Context<Self>) -> Task<Result<String>> {
        Task::ready(Ok(plugin_settings::get_mut(cx).get(&self.plugin.id)))
    }

    fn set_settings(&mut self, json: String, cx: &mut Context<Self>) -> Task<Result<()>> {
        Task::ready(plugin_settings::get_mut(cx).set_json(&self.plugin.id, &json))
    }

    fn utc_offset_seconds(&mut self, _cx: &mut Context<Self>) -> i32 {
        chrono::Local::now().offset().local_minus_utc()
    }

    fn show_settings(&mut self, cx: &mut Context<Self>) {
        let plugin_id = self.plugin.id.clone();
        cx.defer(move |cx| settings_window::open_plugin(plugin_id, cx));
    }

    fn open_window(
        &mut self,
        key: String,
        title: String,
        width: f32,
        height: f32,
        hide_with_launcher: bool,
        cx: &mut Context<Self>,
    ) -> Task<Result<bool>> {
        let plugin_id = self.plugin.id.clone();
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

    fn show_window(&mut self, key: String, cx: &mut Context<Self>) -> bool {
        plugin_windows::show(&self.plugin.id, &key, cx)
    }

    fn close_window(&mut self, key: String, cx: &mut Context<Self>) -> bool {
        plugin_windows::close(&self.plugin.id, &key, cx)
    }

    fn request_permission(&mut self, permission: String, reason: String, cx: &mut Context<Self>) -> Task<Result<bool>> {
        match serde_json::from_str::<Permission>(&permission) {
            Ok(asked) => permissions::ask::ask(&self.plugin, asked, reason, cx),
            Err(error) => Task::ready(Err(anyhow!("{permission} isn't a permission: {error}"))),
        }
    }

    // TEMPORARY(pick_folders)
    fn pick_folders(&mut self, multiple: bool, write: bool, prompt: Option<String>, cx: &mut Context<Self>) -> Task<Result<Vec<String>>> {
        // Before the picker shows: only the kind counts, whatever is picked.
        if let Err(error) = permissions::can_give(&self.plugin, &Permission::Files(FilesPermission::default())) {
            return Task::ready(Err(error));
        }
        let plugin = self.plugin.clone();
        cx.spawn(async move |_, cx| {
            let picked = cx.update(|cx| crate::pick_folders::pick(multiple, prompt, cx)).await?;
            let folders: Vec<String> = picked.iter().map(|path| path.to_string_lossy().into_owned()).collect();
            let (all, none) = (folders.clone(), Vec::<String>::new());
            let picked = if write { Permission::files(none, all) } else { Permission::files(all, none) };
            let picked = permissions::ask::spelled(&picked, std::env::home_dir().as_deref());
            if !cx.update(|cx| permissions::has(&plugin, &picked, cx)) {
                cx.update(|cx| permissions::give(&plugin, vec![picked], cx))?;
                // The plugin that asked is gone: it starts again with them.
                return futures::future::pending().await;
            }
            Ok(folders)
        })
    }

    // TEMPORARY(save_file)
    fn save_file(&mut self, name: String, contents: Bytes, cx: &mut Context<Self>) -> Task<Result<Option<String>>> {
        // The panel is shown outside this update.
        cx.spawn(async move |_, cx| {
            let saved = cx.update(|cx| crate::save_file::save(name, contents.0, cx)).await?;
            Ok(saved.map(|path| path.to_string_lossy().into_owned()))
        })
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
