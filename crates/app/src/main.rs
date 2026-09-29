//! Delight: a macOS launcher whose tools are plugins.
//!
//! One instance at a time, no Dock icon, a menu bar icon, and the launcher, shown
//! and hidden with a global hotkey, which lists the plugins' tools that fit its
//! input.

mod files;
mod history;
mod plugin_settings;
mod secrets;
mod hotkey;
mod keymap;
mod launcher;
mod login;
mod macos;
mod plugins;
mod settings;
mod settings_window;
mod single_instance;
mod tray;

use delight_ui::ThemeMode;
use std::path::PathBuf;

use gpui::{App, Application, actions};

use crate::single_instance::Acquired;

actions!(delight, [Quit]);

/// Delight's own folder: `~/Library/Application Support/Delight`.
fn app_dir() -> PathBuf {
    dirs::data_dir().unwrap_or_else(std::env::temp_dir).join("Delight")
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let _instance = match single_instance::acquire(&app_dir().join("delight.lock")) {
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

    let saved = settings::load();
    // Follow the app if it moved since the last launch.
    login::apply(saved.open_at_login);

    let platform = gpui_platform::current_platform(false);
    // Plugins' text is shaped by the app's own text system.
    let text_system = platform.text_system();
    let app = Application::with_platform(platform).with_assets(delight_ui::Assets);
    app.run(move |cx: &mut App| {
        macos::set_accessory_app();
        let appearance = saved.appearance;
        cx.set_global(saved);
        delight_ui::init(cx, ThemeMode::System);
        settings::apply_appearance(appearance, cx);
        keymap::init(cx);
        history::init(cx);
        plugin_settings::init(cx);
        secrets::init(cx);
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
        plugins::load(text_system, cx);
    });
}
