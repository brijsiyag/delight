//! Delight: a macOS launcher whose tools are plugins.
//!
//! One instance at a time, no Dock icon, a menu bar icon, and the launcher, shown
//! and hidden with a global hotkey, which lists the plugins' tools that fit its
//! input.

mod dialogs;
mod files;
mod history;
mod plugin_settings;
mod secrets;
mod hotkey;
mod install_window;
mod keymap;
mod launcher;
mod login;
mod logs;
mod macos;
mod plugin_windows;
mod plugins;
mod settings;
mod settings_window;
mod single_instance;
mod tray;
mod tray_drop;
mod updater;

use delight_ui::ThemeMode;
use std::path::PathBuf;

use gpui::{App, Application, actions};

use crate::single_instance::Acquired;

actions!(delight, [Quit]);

/// Delight's own folder: `~/Library/Application Support/Delight`.
fn app_dir() -> PathBuf {
    dirs::data_dir().unwrap_or_else(std::env::temp_dir).join("Delight")
}

/// Delight's caches: `~/Library/Caches/Delight`. What is in it may be deleted; it is made again.
fn cache_dir() -> PathBuf {
    dirs::cache_dir().unwrap_or_else(std::env::temp_dir).join("Delight")
}

/// Everything logged goes to the console and to this run's file (see [`logs`]).
struct Tee {
    console: env_logger::Logger,
    file: Option<env_logger::Logger>,
}

impl log::Log for Tee {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        self.console.enabled(metadata)
    }

    fn log(&self, record: &log::Record) {
        self.console.log(record);
        if let Some(file) = &self.file {
            file.log(record);
        }
    }

    fn flush(&self) {
        self.console.flush();
        if let Some(file) = &self.file {
            file.flush();
        }
    }
}

fn init_logging() {
    let env = || env_logger::Env::default().default_filter_or("info");
    let console = env_logger::Builder::from_env(env()).build();
    let file = match logs::SessionLog::start() {
        Ok(log) => Some(
            env_logger::Builder::from_env(env())
                .target(env_logger::Target::Pipe(Box::new(log.writer())))
                .write_style(env_logger::WriteStyle::Never)
                .build(),
        ),
        Err(error) => {
            eprintln!("Delight can't write its log file: {error:#}");
            None
        }
    };
    log::set_max_level(console.filter());
    log::set_boxed_logger(Box::new(Tee { console, file })).ok();
}

fn main() {
    init_logging();

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
        updater::init(cx);
        if let Err(error) = tray::install(cx) {
            log::error!("adding the menu bar icon: {error:#}");
        }
        plugins::load(text_system, cx);
        plugins::updates::start(cx);
    });
}
