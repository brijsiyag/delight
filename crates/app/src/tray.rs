//! The menu bar icon and its menu. Restarting joins the menu in its own step.

use anyhow::{Context as _, Result};
use futures::StreamExt as _;
use futures::channel::mpsc;
use gpui::{App, Global, Keystroke};
use resvg::{tiny_skia, usvg};
use tray_icon::menu::accelerator::Accelerator;
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

use crate::updater::Status;
use crate::{hotkey, launcher, settings_window, updater};

/// Black on transparent, so macOS can tint it for light and dark menu bars.
const LOGO: &[u8] = delight_ui::LOGO_SVG;
/// The icon's size in pixels: 18pt at 2x.
const ICON_PIXELS: u32 = 36;

/// The icon stays in the menu bar for as long as this lives.
struct Tray {
    _icon: TrayIcon,
    /// "Open Delight", which shows the launcher shortcut.
    open: MenuItem,
    /// "Check for Updates…": dimmed in a build that can't update itself.
    updates: MenuItem,
}

impl Global for Tray {}

/// Put Delight's icon in the menu bar, with "Open Delight" (and the launcher
/// shortcut), "Check for Updates…" (dimmed if this build can't update itself), "Settings…" and
/// "Quit Delight". The menu's clicks arrive on the system's
/// thread and reach GPUI through a channel.
pub fn install(cx: &mut App) -> Result<()> {
    let shortcut = hotkey::current(cx).and_then(|keystroke| accelerator(&keystroke));
    let open = MenuItem::with_id("open", "Open Delight", true, shortcut);
    let settings = MenuItem::with_id("settings", "Settings", true, None);
    let quit = MenuItem::with_id("quit", "Quit Delight", true, None);
    let separator = PredefinedMenuItem::separator();
    let updates = MenuItem::with_id("updates", UPDATES, updater::available(cx), None);
    let menu = Menu::new();
    menu.append(&open)?;
    menu.append(&separator)?;
    menu.append(&updates)?;
    menu.append(&settings)?;
    menu.append(&PredefinedMenuItem::separator())?;
    menu.append(&quit)?;
    let icon = TrayIconBuilder::new()
        .with_icon(icon()?)
        .with_icon_as_template(true)
        .with_tooltip("Delight")
        .with_menu(Box::new(menu))
        .build()?;
    cx.set_global(Tray { _icon: icon, open, updates });

    let (clicks, mut clicked) = mpsc::unbounded();
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        clicks.unbounded_send(event.id.0).ok();
    }));
    cx.spawn(async move |cx| {
        while let Some(item) = clicked.next().await {
            cx.update(|cx| match item.as_str() {
                "open" => launcher::show(cx),
                "updates" => updater::check(cx),
                "settings" => settings_window::open(cx),
                "quit" => cx.quit(),
                _ => {}
            });
        }
    })
    .detach();
    Ok(())
}

/// Show the new launcher shortcut on "Open Delight".
pub fn set_shortcut(keystroke: &Keystroke, cx: &App) {
    if let Some(tray) = cx.try_global::<Tray>()
        && let Err(error) = tray.open.set_accelerator(accelerator(keystroke))
    {
        log::warn!("showing the shortcut in the menu: {error}");
    }
}

/// The menu item's words for what the updater found: it offers the new version once there is one.
pub fn set_update_status(status: &Status, cx: &App) {
    let Some(item) = cx.try_global::<Tray>().map(|tray| &tray.updates) else { return };
    item.set_text(match status {
        Status::Found(version) => format!("Update Available — {version}…"),
        _ => UPDATES.to_string(),
    });
}

const UPDATES: &str = "Check for Updates…";

/// The keystroke as the menu writes it.
fn accelerator(keystroke: &Keystroke) -> Option<Accelerator> {
    hotkey::plus_separated(keystroke).parse().ok()
}

/// The logo, drawn at [`ICON_PIXELS`].
fn icon() -> Result<Icon> {
    let svg = usvg::Tree::from_data(LOGO, &usvg::Options::default())?;
    let mut pixmap = tiny_skia::Pixmap::new(ICON_PIXELS, ICON_PIXELS).context("an empty icon")?;
    let scale = ICON_PIXELS as f32 / svg.size().width().max(svg.size().height());
    resvg::render(&svg, tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());
    Ok(Icon::from_rgba(pixmap.take(), ICON_PIXELS, ICON_PIXELS)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_logo_renders_to_an_icon() {
        let svg = usvg::Tree::from_data(LOGO, &usvg::Options::default()).unwrap();
        let mut pixmap = tiny_skia::Pixmap::new(ICON_PIXELS, ICON_PIXELS).unwrap();
        let scale = ICON_PIXELS as f32 / svg.size().width();
        resvg::render(&svg, tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());
        let drawn = pixmap.pixels().iter().filter(|pixel| pixel.alpha() > 0).count();
        assert!(drawn > 100, "the logo draws something ({drawn} pixels)");
        icon().unwrap();
    }
}
