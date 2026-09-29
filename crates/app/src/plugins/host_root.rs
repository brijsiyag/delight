//! What one plugin may ask of the app: its host object.

use delight_protocol::{HostApi, Theme};
use delight_runtime::Granted;
use delight_ui::ActiveTheme as _;
use embedded_gpui::shared;
use gpui::{ClipboardItem, Context, Subscription};

use crate::{history, launcher, settings};

/// What one plugin may ask of the app. Each plugin has its own, so every call is
/// that plugin's.
pub(super) struct HostRoot {
    plugin_id: String,
    /// What its permissions let it do, handed out when it asks (nothing yet: see
    /// `Granted`).
    _granted: Granted,
    _theme_changes: Subscription,
}

impl HostRoot {
    pub(super) fn new(plugin_id: String, granted: Granted, cx: &mut Context<Self>) -> Self {
        // The plugin observes this object: tell it when the theme changes.
        let theme_changes = cx.observe_global::<delight_ui::Theme>(|_, cx| cx.notify());
        Self { plugin_id, _granted: granted, _theme_changes: theme_changes }
    }
}

#[shared]
impl HostApi for HostRoot {
    // Deferred: the launcher may be in the middle of an update.
    fn toast(&mut self, message: String, cx: &mut Context<Self>) {
        cx.defer(move |cx| launcher::toast(message.into(), cx));
    }

    fn copy_text(&mut self, text: String, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(text));
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
}
