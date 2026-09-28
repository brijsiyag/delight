//! Delight: a macOS launcher whose tools are plugins.
//!
//! This is the app's shell: one instance at a time, no Dock icon, a menu bar icon,
//! and the launcher, shown and hidden with a global hotkey. Plugins and their tools
//! join it next.

mod hotkey;
mod keymap;
mod launcher;
mod platform;
mod single_instance;
mod tray;

use delight_ui::ThemeMode;
use gpui::{App, Application, actions};

use crate::single_instance::Acquired;

actions!(delight, [Quit]);

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let lock = dirs::data_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("Delight")
        .join("delight.lock");
    let _instance = match single_instance::acquire(&lock) {
        Ok(Acquired::Locked(lock)) => lock,
        Ok(Acquired::Running { pid }) => {
            match pid {
                Some(pid) => eprintln!("Delight is already running (process {pid})"),
                None => eprintln!("Delight is already running"),
            }
            return;
        }
        Err(error) => {
            eprintln!("Delight can't start: {error:#}");
            std::process::exit(1);
        }
    };

    let app = Application::with_platform(gpui_platform::current_platform(false)).with_assets(delight_ui::Assets);
    app.run(|cx: &mut App| {
        platform::set_accessory_app();
        delight_ui::init(cx, ThemeMode::System);
        keymap::init(cx);
        cx.on_action(|_: &Quit, cx| cx.quit());
        if let Err(error) = launcher::open(cx) {
            log::error!("opening the launcher: {error:#}");
            cx.quit();
            return;
        }
        if let Err(error) = hotkey::listen(cx) {
            log::error!("registering the hotkey: {error:#}");
        }
        if let Err(error) = tray::install(cx) {
            log::error!("adding the menu bar icon: {error:#}");
        }
    });
}
