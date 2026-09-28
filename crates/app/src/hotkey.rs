//! The shortcut that shows and hides the launcher from any app: a setting, ⌘⇧Space by
//! default. It's a GPUI keystroke (`cmd-shift-space`) like every other key in Delight;
//! macOS registers it through `global-hotkey`, which reads the same key names joined
//! with `+`. It can change while Delight runs.

use anyhow::{Result, anyhow, ensure};
use delight_ui::keystroke_label;
use futures::StreamExt as _;
use futures::channel::mpsc;
use global_hotkey::hotkey::HotKey;
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use gpui::{App, Global, Keystroke};

use crate::launcher;
use crate::settings::{self, DEFAULT_LAUNCHER_SHORTCUT};

/// The registered shortcut. It works for as long as its manager lives.
struct LauncherShortcut {
    manager: GlobalHotKeyManager,
    keystroke: Keystroke,
    hotkey: HotKey,
}

impl Global for LauncherShortcut {}

/// Register the shortcut from the settings, and toggle the launcher on every press.
/// One that can't be registered (invalid, or refused by macOS) is replaced by the
/// default, in the settings too. The presses arrive on the system's thread and reach
/// GPUI through a channel.
pub fn listen(cx: &mut App) -> Result<()> {
    let manager = GlobalHotKeyManager::new()?;
    let register = |shortcut: &str| -> Result<(Keystroke, HotKey)> {
        let (keystroke, hotkey) = parse(shortcut)?;
        manager.register(hotkey)?;
        Ok((keystroke, hotkey))
    };
    let wanted = settings::get(cx).launcher_shortcut.clone();
    let (keystroke, hotkey) = match register(&wanted) {
        Ok(registered) => registered,
        Err(error) => {
            log::error!("launcher shortcut {wanted:?}: {error:#}; using {DEFAULT_LAUNCHER_SHORTCUT}");
            settings::update(cx, |settings| settings.launcher_shortcut = DEFAULT_LAUNCHER_SHORTCUT.into());
            register(DEFAULT_LAUNCHER_SHORTCUT)?
        }
    };
    cx.set_global(LauncherShortcut { manager, keystroke, hotkey });

    let (presses, mut pressed) = mpsc::unbounded();
    // Only Delight's one shortcut is registered: every press is it.
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

/// The shortcut working now, if any.
pub fn current(cx: &App) -> Option<Keystroke> {
    cx.try_global::<LauncherShortcut>().map(|shortcut| shortcut.keystroke.clone())
}

/// Switch to `keystroke`, and save it. The new one is registered before the old one
/// is released, so a refused one leaves the old one working; the error says why.
pub fn change(keystroke: &Keystroke, cx: &mut App) -> Result<()> {
    let hotkey = to_hotkey(keystroke)?;
    ensure!(cx.has_global::<LauncherShortcut>(), "the shortcut isn't working");
    let shortcut = cx.global_mut::<LauncherShortcut>();
    if hotkey != shortcut.hotkey {
        shortcut
            .manager
            .register(hotkey)
            .map_err(|error| anyhow!("macOS refused {}: {error}", keystroke_label(keystroke)))?;
        shortcut.manager.unregister(shortcut.hotkey).ok();
        shortcut.hotkey = hotkey;
    }
    shortcut.keystroke = keystroke.clone();
    settings::update(cx, |settings| settings.launcher_shortcut = keystroke.unparse());
    crate::tray::set_shortcut(keystroke, cx);
    Ok(())
}

/// Read a shortcut as the settings store it (`cmd-shift-space`).
pub fn parse(shortcut: &str) -> Result<(Keystroke, HotKey)> {
    let keystroke = Keystroke::parse(shortcut)?;
    // `shift+super+Space` (an older format) parses as one odd key.
    ensure!(!keystroke.key.contains('+'), "not written like {DEFAULT_LAUNCHER_SHORTCUT}");
    let hotkey = to_hotkey(&keystroke)?;
    Ok((keystroke, hotkey))
}

/// `cmd-shift-space` → `cmd+shift+space`: how `global-hotkey` (and the menu bar menu)
/// write a shortcut.
pub fn plus_separated(keystroke: &Keystroke) -> String {
    let m = &keystroke.modifiers;
    let modifiers = [(m.platform, "cmd"), (m.alt, "alt"), (m.control, "ctrl"), (m.shift, "shift")];
    let mut parts: Vec<&str> = modifiers.iter().filter(|(on, _)| *on).map(|(_, name)| *name).collect();
    parts.push(&keystroke.key);
    parts.join("+")
}

/// The keystroke as a hotkey for all of macOS. It must include ⌘, ⌥ or ⌃: one on a
/// plain key would stop that key typing anywhere.
fn to_hotkey(keystroke: &Keystroke) -> Result<HotKey> {
    let m = &keystroke.modifiers;
    ensure!(m.platform || m.alt || m.control, "a shortcut needs ⌘, ⌥ or ⌃");
    plus_separated(keystroke).parse().map_err(|_| anyhow!("{} can't be a shortcut", keystroke_label(keystroke)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hotkey(shortcut: &str) -> Result<HotKey> {
        to_hotkey(&Keystroke::parse(shortcut)?)
    }

    #[test]
    fn keystrokes_become_hotkeys() {
        assert_eq!(hotkey(DEFAULT_LAUNCHER_SHORTCUT).unwrap().into_string(), "shift+super+Space");
        assert_eq!(hotkey("alt-k").unwrap().into_string(), "alt+KeyK");
        assert!(hotkey("ctrl-alt-1").is_ok());
        assert!(hotkey("cmd-/").is_ok(), "punctuation");
        assert!(hotkey("cmd-f5").is_ok());
    }

    #[test]
    fn refuses_modifier_less_and_unknown_keys() {
        assert!(hotkey("k").is_err(), "a plain key");
        assert!(hotkey("shift-k").is_err(), "shift alone types a capital");
        assert!(hotkey("cmd-nosuchkey").is_err(), "not a key");
        assert!(parse("shift+super+Space").is_err(), "the older format");
        assert!(parse(DEFAULT_LAUNCHER_SHORTCUT).is_ok());
    }
}
