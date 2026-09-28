//! The launcher window: opening it, and showing and hiding it. The rest of the app
//! reaches it through these functions; the window is a global.

use gpui::{
    App, AppContext as _, Bounds, Focusable as _, Global, Pixels, Point, WindowBackgroundAppearance,
    WindowBounds, WindowHandle, WindowKind, WindowOptions, point, px, size,
};

use super::{BAR_HEIGHT, BAR_RADIUS, BAR_WIDTH, Launcher};
use crate::platform;

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
        // Shown once restyled, by `show`.
        focus: false,
        show: false,
        kind: WindowKind::PopUp,
        is_movable: true,
        is_resizable: false,
        is_minimizable: false,
        display_id: display.map(|display| display.id()),
        // Transparent: the blur is our own backdrop (see `platform`), shaped to our
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
    cx.spawn(async move |cx| {
        if let Some(native) = native {
            native.style_floating_panel(BAR_RADIUS.into());
        }
        cx.update(show);
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
        .update(cx, |_, window, _| platform::is_window_visible(window) && window.is_window_active())
        .unwrap_or(false);
    if in_front { hide(cx) } else { show(cx) }
}

/// Bring the launcher up with the keyboard in its input.
pub fn show(cx: &mut App) {
    let Some(handle) = handle(cx) else { return };
    let native = handle.update(cx, |launcher, _, cx| {
        launcher.sync_window_size(cx);
        launcher.native.clone()
    });
    let Ok(Some(native)) = native else { return };
    // Present outside this update (see `platform`), then focus the input once the
    // window is key: earlier focus doesn't stick.
    cx.spawn(async move |cx| {
        native.present();
        handle
            .update(cx, |launcher, window, cx| {
                window.focus(&launcher.input.focus_handle(cx), cx);
                // Like Spotlight: the previous text stays, selected, so typing
                // replaces it.
                launcher.input.update(cx, |input, cx| input.select_all_text(cx));
            })
            .ok();
    })
    .detach();
}

/// Hide the launcher, keeping its state for the next time it shows.
pub fn hide(cx: &mut App) {
    let Some(handle) = handle(cx) else { return };
    let Ok(Some(native)) = handle.read_with(cx, |launcher, _| launcher.native.clone()) else {
        return;
    };
    cx.spawn(async move |_| native.hide()).detach();
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
