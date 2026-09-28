//! The global hotkey, ⌘⇧Space (a setting later): each press toggles the launcher.

use anyhow::Result;
use futures::StreamExt as _;
use futures::channel::mpsc;
use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use gpui::{App, Global};

use crate::launcher;

/// The hotkey is registered for as long as its manager lives.
struct Hotkey(#[allow(dead_code)] GlobalHotKeyManager);

impl Global for Hotkey {}

/// Register the hotkey, and toggle the launcher on every press. The presses arrive on
/// the system's thread and reach GPUI through a channel.
pub fn listen(cx: &mut App) -> Result<()> {
    let manager = GlobalHotKeyManager::new()?;
    manager.register(HotKey::new(Some(Modifiers::SUPER | Modifiers::SHIFT), Code::Space))?;
    cx.set_global(Hotkey(manager));

    let (presses, mut pressed) = mpsc::unbounded();
    GlobalHotKeyEvent::set_event_handler(Some(move |event: GlobalHotKeyEvent| {
        if event.state == HotKeyState::Pressed {
            presses.unbounded_send(()).ok();
        }
    }));
    cx.spawn(async move |cx| {
        while pressed.next().await.is_some() {
            cx.update(launcher::toggle);
        }
    })
    .detach();
    Ok(())
}
