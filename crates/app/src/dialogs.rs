//! Alerts plugins ask for: the system's own (macOS's `NSAlert`), which GPUI shows as a sheet on
//! the window that has the keyboard. Only one at a time (GPUI allows no more), and while it is
//! up the launcher and the plugins' windows stay: the alert has the keyboard, and losing it to
//! the alert is not losing it to something else.

use gpui::{App, Global, PromptButton, PromptLevel, Task};

/// An alert is on screen.
#[derive(Default)]
struct Showing(bool);

impl Global for Showing {}

/// Whether an alert a plugin asked for is on screen.
pub fn is_showing(cx: &App) -> bool {
    cx.try_global::<Showing>().is_some_and(|showing| showing.0)
}

/// Ask the user to confirm: `title` and `message` in the alert, with `continue_label` and Cancel as
/// its buttons. A `destructive` question is a critical alert whose default button (↵) is Cancel;
/// otherwise ↵ continues. Whether they chose to continue. `false` without asking when an alert is
/// up already, or no window of the app has the keyboard to show it on.
pub fn confirm(title: String, message: String, continue_label: String, destructive: bool, cx: &mut App) -> Task<bool> {
    if is_showing(cx) {
        return Task::ready(false);
    }
    let Some(window) = cx.active_window() else { return Task::ready(false) };
    // NSAlert makes its first button the default: Cancel first when it can't be undone.
    let (buttons, continue_at) = if destructive {
        ([PromptButton::cancel("Cancel"), PromptButton::ok(continue_label)], 1)
    } else {
        ([PromptButton::ok(continue_label), PromptButton::cancel("Cancel")], 0)
    };
    let level = if destructive { PromptLevel::Critical } else { PromptLevel::Warning };
    let detail = (!message.is_empty()).then_some(message);
    cx.default_global::<Showing>().0 = true;
    let asked = window.update(cx, |_, window, cx| window.prompt(level, &title, detail.as_deref(), &buttons, cx));
    let Ok(answer) = asked else {
        cx.default_global::<Showing>().0 = false;
        return Task::ready(false);
    };
    cx.spawn(async move |cx| {
        let chosen = answer.await.ok();
        cx.update(|cx| cx.default_global::<Showing>().0 = false);
        chosen == Some(continue_at)
    })
}
