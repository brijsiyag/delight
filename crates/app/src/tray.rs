//! The menu bar icon and its menu. Settings, updates and restarting join the menu in
//! their own steps.

use anyhow::{Context as _, Result};
use futures::StreamExt as _;
use futures::channel::mpsc;
use gpui::{App, Global};
use resvg::{tiny_skia, usvg};
use tray_icon::menu::accelerator::{Accelerator, Code, Modifiers};
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

use crate::launcher;

/// Black on transparent, so macOS can tint it for light and dark menu bars.
const LOGO: &[u8] = delight_ui::LOGO_SVG;
/// The icon's size in pixels: 18pt at 2x.
const ICON_PIXELS: u32 = 36;

/// The icon stays in the menu bar for as long as this lives.
struct Tray(#[allow(dead_code)] TrayIcon);

impl Global for Tray {}

/// Put Delight's icon in the menu bar, with "Open Delight" and "Quit Delight". The
/// menu's clicks arrive on the system's thread and reach GPUI through a channel.
pub fn install(cx: &mut App) -> Result<()> {
    let hotkey = Accelerator::new(Modifiers::META | Modifiers::SHIFT, Code::Space);
    let open = MenuItem::with_id("open", "Open Delight", true, Some(hotkey));
    let quit = MenuItem::with_id("quit", "Quit Delight", true, None);
    let menu = Menu::with_items(&[&open, &PredefinedMenuItem::separator(), &quit])?;
    let tray = TrayIconBuilder::new()
        .with_icon(icon()?)
        .with_icon_as_template(true)
        .with_tooltip("Delight")
        .with_menu(Box::new(menu))
        .build()?;
    cx.set_global(Tray(tray));

    let (clicks, mut clicked) = mpsc::unbounded();
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        clicks.unbounded_send(event.id.0).ok();
    }));
    cx.spawn(async move |cx| {
        while let Some(item) = clicked.next().await {
            cx.update(|cx| match item.as_str() {
                "open" => launcher::show(cx),
                "quit" => cx.quit(),
                _ => {}
            });
        }
    })
    .detach();
    Ok(())
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
