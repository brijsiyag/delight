//! Alerts plugins ask for: the system's own (macOS's `NSAlert`), which GPUI shows as a sheet on
//! the window that has the keyboard; and the system's panels ([`panel`]: the folder picker, the
//! save panel). Only one at a time (GPUI allows no more alerts), and while one is up the launcher
//! and the plugins' windows stay: it has the keyboard, and losing it to the alert or the panel is
//! not losing it to something else.

use std::time::Duration;

use anyhow::{Result, anyhow};
use futures::channel::oneshot;
use gpui::{App, Global, PromptButton, PromptLevel, Task};

use crate::macos::FilePanel;

/// Why an alert or a panel isn't shown: there is one already (GPUI shows one alert at a time), or
/// nothing of Delight is on screen to show it on.
const BUSY: &str = "Delight is showing another alert or picker: close it first";
const NO_WINDOW: &str = "Delight has no window on screen to ask on";

/// An alert or a panel is on screen.
#[derive(Default)]
struct Showing(bool);

impl Global for Showing {}

/// Whether an alert or a panel a plugin asked for is on screen.
pub fn is_showing(cx: &App) -> bool {
    cx.try_global::<Showing>().is_some_and(|showing| showing.0)
}

/// Ask the user to confirm: `title` and `message` in the alert, with `continue_label` and Cancel as
/// its buttons. A `destructive` question is a critical alert whose default button (↵) is Cancel;
/// otherwise ↵ continues. Whether they chose to continue. `false` without asking when an alert is
/// up already, or no window of the app has the keyboard to show it on.
pub fn confirm(title: String, message: String, continue_label: String, destructive: bool, cx: &mut App) -> Task<bool> {
    // NSAlert makes its first button the default: Cancel first when it can't be undone.
    let (buttons, continue_at) = if destructive {
        ([PromptButton::cancel("Cancel"), PromptButton::ok(continue_label)], 1)
    } else {
        ([PromptButton::ok(continue_label), PromptButton::cancel("Cancel")], 0)
    };
    let level = if destructive { PromptLevel::Critical } else { PromptLevel::Warning };
    let asked = alert(level, title, message, buttons, continue_at, cx);
    cx.spawn(async move |_| asked.await.unwrap_or(false))
}

/// Ask the user to allow something a plugin wants: `title` and `message` in the alert, with Allow
/// (the default) and Don't Allow. Whether they allowed it; an error without asking when an alert or
/// a picker is up already, or no window of the app has the keyboard to show it on.
pub fn allow(title: String, message: String, cx: &mut App) -> Task<Result<bool>> {
    let buttons = [PromptButton::ok("Allow"), PromptButton::cancel("Don’t Allow")];
    alert(PromptLevel::Warning, title, message, buttons, 0, cx)
}

// TEMPORARY(pick_folders), TEMPORARY(save_file): the system's panels a plugin asks for.
/// Show a system panel as one of the alerts: `open` opens it (GPUI's `prompt_for_paths` or
/// `prompt_for_new_path`), in front of the launcher, which floats above the ordinary windows the
/// panel opens among; `shown` gets it once it is on screen. What the user chose; an error without
/// showing it when an alert or a panel is up already, or saying `what` failed.
pub fn panel<T: 'static>(
    what: &'static str,
    open: impl FnOnce(&mut App) -> oneshot::Receiver<Result<T>>,
    shown: impl FnOnce(&FilePanel) + 'static,
    cx: &mut App,
) -> Task<Result<T>> {
    if is_showing(cx) {
        return Task::ready(Err(anyhow!(BUSY)));
    }
    cx.default_global::<Showing>().0 = true;
    let answer = open(cx);
    let above_launcher = crate::launcher::level(cx).map(|level| level + 1);
    // It appears a moment later: looked for until it does, for a second at most.
    cx.spawn(async move |cx| {
        for _ in 0..40 {
            cx.background_executor().timer(Duration::from_millis(25)).await;
            // Outside a GPUI update: AppKit calls back into it.
            if let Some(panel) = crate::macos::file_panel() {
                if let Some(level) = above_launcher {
                    panel.raise_to(level);
                }
                shown(&panel);
                return;
            }
        }
    })
    .detach();
    cx.spawn(async move |cx| {
        let answer = answer.await;
        cx.update(|cx| cx.default_global::<Showing>().0 = false);
        match answer {
            Ok(Ok(chosen)) => Ok(chosen),
            Ok(Err(error)) => Err(error.context(what)),
            Err(_) => Err(anyhow!("{what} closed without an answer")),
        }
    })
}

/// The window an alert goes on: the one with the keyboard (the launcher, or a plugin's window), else
/// the main window, else the launcher if it is on screen. Not GPUI's `active_window` alone: that is
/// AppKit's main window, which the launcher (a panel) never is.
fn window_to_ask_on(cx: &mut App) -> Option<gpui::AnyWindowHandle> {
    crate::launcher::window_with_keyboard(cx)
        .or_else(|| crate::plugin_windows::with_keyboard(cx))
        .or_else(|| cx.active_window())
        .or_else(|| crate::launcher::window_on_screen(cx))
}

/// The alert, on the window that has the keyboard: whether the button chosen is the one at
/// `continue_at`; an error when it can't be shown.
fn alert(level: PromptLevel, title: String, message: String, buttons: [PromptButton; 2], continue_at: usize, cx: &mut App) -> Task<Result<bool>> {
    if is_showing(cx) {
        return Task::ready(Err(anyhow!(BUSY)));
    }
    let Some(window) = window_to_ask_on(cx) else { return Task::ready(Err(anyhow!(NO_WINDOW))) };
    let detail = (!message.is_empty()).then_some(message);
    cx.default_global::<Showing>().0 = true;
    let asked = window.update(cx, |_, window, cx| window.prompt(level, &title, detail.as_deref(), &buttons, cx));
    let Ok(answer) = asked else {
        cx.default_global::<Showing>().0 = false;
        return Task::ready(Err(anyhow!(NO_WINDOW)));
    };
    cx.spawn(async move |cx| {
        let chosen = answer.await.ok();
        cx.update(|cx| cx.default_global::<Showing>().0 = false);
        Ok(chosen == Some(continue_at))
    })
}
