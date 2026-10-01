//! The launcher window: opening it, and showing and hiding it. The rest of the app
//! reaches it through these functions; the window is a global.

use gpui::{
    App, AppContext as _, Bounds, Focusable as _, Global, Pixels, Point, SharedString, WindowBackgroundAppearance,
    WindowBounds, WindowHandle, WindowKind, WindowOptions, point, px, size,
};

use super::{BAR_HEIGHT, BAR_RADIUS, BAR_WIDTH, Launcher};
use crate::{history, macos, plugin_windows, settings};

/// Where the bar sits: this far down the screen, centred across it.
const FROM_TOP: f32 = 0.22;

struct LauncherWindow(WindowHandle<Launcher>);

impl Global for LauncherWindow {}

/// Open the launcher window, shown and active, as Delight is at launch.
pub fn open(cx: &mut App) -> anyhow::Result<()> {
    let display = cx.primary_display();
    let screen = display
        .as_ref()
        .map(|display| display.bounds())
        .unwrap_or_else(|| Bounds::new(point(px(0.), px(0.)), size(px(1440.), px(900.))));
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::new(
            bar_origin(screen),
            size(px(BAR_WIDTH), px(BAR_HEIGHT)),
        ))),
        titlebar: None,
        // Not shown at launch: the hotkey or the menu bar icon shows it (`show`).
        focus: false,
        show: false,
        kind: WindowKind::PopUp,
        is_movable: true,
        is_resizable: false,
        is_minimizable: false,
        display_id: display.map(|display| display.id()),
        // Transparent: the blur is our own backdrop (see `macos`), shaped to our
        // corners; GPUI's blurred background keeps macOS's own radius.
        window_background: WindowBackgroundAppearance::Transparent,
        ..Default::default()
    };
    let handle = cx.open_window(options, |window, cx| cx.new(|cx| Launcher::new(window, cx)))?;
    let native = handle.update(cx, |launcher, window, cx| {
        window.focus(&launcher.input.focus_handle(cx), cx);
        launcher.sync_window_size(cx);
        launcher.native.clone()
    })?;
    cx.set_global(LauncherWindow(handle));
    // Quitting while it shows keeps the input too.
    cx.on_app_quit(|cx| {
        save_input(cx);
        async {}
    })
    .detach();
    cx.spawn(async move |_| {
        if let Some(native) = native {
            native.style_floating_panel(BAR_RADIUS.into());
        }
    })
    .detach();
    Ok(())
}

/// The bar's top-left corner on `screen`: centred across it, [`FROM_TOP`] down.
fn bar_origin(screen: Bounds<Pixels>) -> Point<Pixels> {
    point(
        screen.origin.x + (screen.size.width - px(BAR_WIDTH)) / 2.,
        screen.origin.y + screen.size.height * FROM_TOP,
    )
}

fn handle(cx: &App) -> Option<WindowHandle<Launcher>> {
    cx.try_global::<LauncherWindow>().map(|window| window.0)
}

/// The hotkey: hide the launcher if it's in front, show it otherwise.
pub fn toggle(cx: &mut App) {
    let Some(handle) = handle(cx) else { return };
    let in_front = handle
        .update(cx, |_, window, _| macos::is_window_visible(window) && window.is_window_active())
        .unwrap_or(false);
    if in_front { hide(cx) } else { show(cx) }
}

/// Bring the launcher up with the keyboard in its input.
pub fn show(cx: &mut App) {
    let Some(handle) = handle(cx) else { return };
    let native = handle.update(cx, |launcher, _, cx| {
        launcher.sync_window_size(cx);
        launcher.show_next_tip(cx);
        launcher.window_shown(true, cx);
        launcher.native.clone()
    });
    let Ok(Some(native)) = native else { return };
    // Present outside this update (see `macos`), then focus the input once the
    // window is key: earlier focus doesn't stick.
    cx.spawn(async move |cx| {
        native.present();
        // The windows that hid with it come back, in front of it, as they were.
        cx.update(plugin_windows::restore_hidden);
        handle
            .update(cx, |launcher, window, cx| {
                window.focus(&launcher.input.focus_handle(cx), cx);
                if settings::get(cx).paste_clipboard_on_open {
                    launcher.paste_clipboard(cx);
                }
                // Like Spotlight: the previous text stays, selected, so typing
                // replaces it.
                launcher.input.update(cx, |input, cx| input.select_all_text(cx));
            })
            .ok();
    })
    .detach();
}

/// Hide the launcher, keeping its state for the next time it shows, and its input
/// for the next launch.
pub fn hide(cx: &mut App) {
    if let Some(native) = put_away(cx) {
        cx.spawn(async move |_| native.hide()).detach();
    }
}

/// Hide the launcher as [`hide`] does, for another of Delight's windows (Settings) to come up in its
/// place, then run `next`: the launcher is off screen first, and the keyboard stays with Delight
/// instead of going back to the app that was in front. Hidden already, `next` runs at once.
pub fn hide_then(next: impl FnOnce(&mut App) + 'static, cx: &mut App) {
    let visible = handle(cx)
        .and_then(|handle| handle.update(cx, |_, window, _| macos::is_window_visible(window)).ok())
        .unwrap_or(false);
    match visible.then(|| put_away(cx)).flatten() {
        Some(native) => cx
            .spawn(async move |cx| {
                native.order_out();
                cx.update(next);
            })
            .detach(),
        None => next(cx),
    }
}

/// What hiding changes besides the window: the plugins' windows go with it, the history search
/// closes, the tool's view goes, and the input is kept. The window's AppKit side, to take off screen
/// outside this update.
fn put_away(cx: &mut App) -> Option<macos::NativeWindow> {
    // The windows plugins opened go with it, and come back with it.
    plugin_windows::hide_all(cx);
    let native = handle(cx).and_then(|handle| {
        handle
            .update(cx, |launcher, window, cx| {
                launcher.close_history(window, cx);
                launcher.window_shown(false, cx);
                launcher.native.clone()
            })
            .ok()
            .flatten()
    });
    save_input(cx);
    native
}

/// The launcher was clicked. Without the keyboard (it stayed up while another app was used), it takes
/// it back, as the shortcut gives it: macOS doesn't on its own, since GPUI made the window a panel
/// that doesn't activate Delight, and AppKit keeps that after the restyle (`macos`).
pub(super) fn clicked(window: &gpui::Window, cx: &mut App) {
    let Some(native) = macos::NativeWindow::of(window) else { return };
    // Outside this update: AppKit calls back into GPUI.
    cx.spawn(async move |_| {
        if !native.has_keyboard() {
            native.present();
        }
    })
    .detach();
}

/// Where the launcher window is on screen.
pub fn bounds(cx: &mut App) -> Option<Bounds<Pixels>> {
    handle(cx)?.update(cx, |_, window, _| window.bounds()).ok()
}

/// The launcher window's level (how far in front it floats), for the windows of plugins to
/// match.
pub fn level(cx: &mut App) -> Option<isize> {
    let handle = handle(cx)?;
    handle.update(cx, |_, window, _| macos::NativeWindow::of(window).map(|native| native.level())).ok().flatten()
}

/// The keyboard moved between windows, so far: bumped whenever a window of the launcher's group
/// gets it, to cancel a hide that was waiting to see where it went.
#[derive(Default)]
struct FocusMoves(u64);

impl Global for FocusMoves {}

/// A window of the launcher or of a plugin got the keyboard: no hide waiting for it to go
/// elsewhere applies now.
pub fn focus_gained(cx: &mut App) {
    cx.default_global::<FocusMoves>().0 += 1;
    // TEMPORARY(clipboard): something may have been copied elsewhere; plugins see it before a paste.
    crate::plugins::refresh_clipboards(cx);
}

/// A window of the launcher or of a plugin lost the keyboard. If it went to something else (another
/// app, Settings), the launcher and the plugins' windows all hide, when that is on and the launcher
/// isn't pinned; if it went to one of them, nothing does. Judged a moment later, as the keyboard passes from one window to
/// the other in two steps: only if no window of the group got it meanwhile, and none has it now
/// (asked of AppKit, which knows before GPUI does).
pub fn focus_left(cx: &mut App) {
    // An alert a plugin asked for has the keyboard: that is not leaving.
    if !settings::get(cx).hide_on_blur || pinned(cx) || crate::dialogs::is_showing(cx) {
        return;
    }
    let moves = cx.default_global::<FocusMoves>().0;
    cx.spawn(async move |cx| {
        cx.background_executor().timer(FOCUS_SETTLE).await;
        cx.update(|cx| {
            if cx.default_global::<FocusMoves>().0 != moves || pinned(cx) || crate::dialogs::is_showing(cx) {
                return;
            }
            let launcher = handle(cx);
            let visible = launcher.and_then(|handle| handle.update(cx, |_, window, _| macos::is_window_visible(window)).ok()).unwrap_or(false);
            let launcher_has_keyboard = launcher
                .and_then(|handle| handle.update(cx, |_, window, _| macos::NativeWindow::of(window).is_some_and(|native| native.is_key())).ok())
                .unwrap_or(false);
            if visible && !launcher_has_keyboard && !plugin_windows::has_keyboard(cx) {
                hide(cx);
            }
        });
    })
    .detach();
}

/// Whether the footer's pin keeps the launcher up while another app is used.
fn pinned(cx: &App) -> bool {
    handle(cx).and_then(|handle| handle.read(cx).ok()).is_some_and(|launcher| launcher.pinned)
}

/// How long the keyboard may be in neither the launcher nor a plugin's window before they hide.
const FOCUS_SETTLE: std::time::Duration = std::time::Duration::from_millis(150);

/// Keep the input to bring back at the next launch, if the history is on.
fn save_input(cx: &mut App) {
    if !settings::get(cx).input_history {
        return;
    }
    let Some(text) = handle(cx).and_then(|handle| handle.read(cx).ok()).map(|launcher| launcher.input.read(cx).text().to_string())
    else {
        return;
    };
    if let Err(error) = history::get_mut(cx).set_input_to_restore(&text) {
        log::error!("keeping the input for the next launch: {error:#}");
    }
}

/// A brief message in the footer (a plugin's toast).
pub fn toast(message: SharedString, cx: &mut App) {
    if let Some(handle) = handle(cx) {
        handle.update(cx, |launcher, _, cx| launcher.flash(message, cx)).ok();
    }
}

/// A plugin's `set_input`: make the input `text`, if a tool of that plugin is the selected
/// one.
pub fn set_input(plugin_id: &str, text: String, cx: &mut App) {
    if let Some(handle) = handle(cx) {
        handle.update(cx, |launcher, _, cx| launcher.set_input_from(plugin_id, text, cx)).ok();
    }
}

/// Whether the launcher shows a tool of this plugin now.
pub fn shows_plugin(plugin_id: &str, cx: &App) -> bool {
    handle(cx).and_then(|handle| handle.read(cx).ok()).is_some_and(|launcher| launcher.shows_plugin(plugin_id))
}

/// Ask the plugins again: which are on changed.
pub fn refresh(cx: &mut App) {
    if let Some(handle) = handle(cx) {
        handle.update(cx, |launcher, _, cx| launcher.detect(cx)).ok();
    }
}

/// The plugins have started: ask them about the input. The tools of plugins that went on keep what
/// they show; the others' are closed.
pub fn plugins_loaded(cx: &mut App) {
    if let Some(handle) = handle(cx) {
        handle
            .update(cx, |launcher, _, cx| {
                launcher.drop_stale_tools(cx);
                launcher.detect(cx);
            })
            .ok();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bar_is_centred_across_and_near_the_top() {
        let screen = Bounds::new(point(px(0.), px(25.)), size(px(1440.), px(900.)));
        assert_eq!(bar_origin(screen), point(px(400.), px(25. + 198.)));
    }
}
