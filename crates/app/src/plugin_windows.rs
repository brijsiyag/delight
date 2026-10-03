//! Windows plugins ask for: a normal window with a plugin's view in it, closed by its user (✕ or
//! ⌘W) or by the plugin. It goes off screen when the launcher hides and comes back with it, unless
//! the plugin asked for one that stays up; the plugin can bring it back before the launcher, but
//! never takes it off screen itself. The plugin draws in the window's surface like it does in the
//! launcher's, and there are only a few open from each.

use std::collections::HashSet;

use delight_ui::ActiveTheme as _;
use embedded_gpui::Surface;
use gpui::{
    AnyWindowHandle, App, AppContext as _, Bounds, Context, Pixels, Entity, Global, InteractiveElement as _, IntoElement, ParentElement as _, Render,
    Styled as _, TitlebarOptions, Window, WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions, actions, div, px, size,
};

use crate::{plugins, settings_window};

/// The key context of a plugin's window, which ⌘W closes. Esc is the plugin's.
pub const CONTEXT: &str = "PluginWindow";

actions!(plugin_window, [Close]);

/// The most windows one plugin has open.
const MOST_PER_PLUGIN: usize = 4;
/// The smallest a window resizes to.
const MIN_SIZE: (f32, f32) = (280., 160.);

/// The plugin windows open now.
#[derive(Default)]
struct Open(Vec<Opened>);

struct Opened {
    plugin_id: String,
    key: String,
    window: AnyWindowHandle,
    /// It goes off screen when the launcher hides, and comes back with it.
    hides_with_launcher: bool,
    /// Off screen because the launcher hid: it comes back with it.
    hidden_with_launcher: bool,
}

impl Global for Open {}

/// What a plugin's window shows: its surface, filling the window.
struct PluginWindow {
    surface: Entity<Surface>,
    /// The keyboard was moved into the surface.
    focused: bool,
}

impl Render for PluginWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        // The keyboard starts in the plugin's surface (its text fields take ⌘A and ⌘C), which is the
        // window's one tab stop and sits inside this key context: ⌘W closes the window from there.
        // With nothing focused it would reach neither. Once drawn, so the stop exists.
        if !std::mem::replace(&mut self.focused, true) {
            cx.defer_in(window, |_, window, cx| window.focus_next(cx));
        }
        div()
            .key_context(CONTEXT)
            .on_action(|_: &Close, window, _| window.remove_window())
            // A click anywhere in it brings it in front of the launcher and the other windows.
            .capture_any_mouse_down(|_, window, cx| raise(window, cx))
            .size_full()
            .bg(settings_window::content_color(&t))
            .font_family(t.font.clone())
            .text_color(t.text)
            .text_size(t.text_size)
            .child(self.surface.clone())
    }
}

/// How far a new window overlaps the launcher, so the launcher can still be clicked, and how
/// far each window that is open already moves the next one down and to the right.
const OVERLAP: f32 = 56.;
const CASCADE: f32 = 28.;

/// Where a window of `size` opens: to the right of the launcher, overlapping it a little so both
/// can be reached, and a little further down and out for each window already open. Where the screen
/// has no room to the right it slides left to fit, overlapping the launcher more, and never goes to
/// the launcher's left. Centred when there is no launcher to be beside.
fn beside_launcher(size: gpui::Size<Pixels>, open: usize, cx: &mut App) -> Bounds<Pixels> {
    let Some(launcher) = crate::launcher::bounds(cx) else { return Bounds::centered(None, size, cx) };
    let screen = cx.primary_display().map(|display| display.bounds()).unwrap_or(launcher);
    let step = px(CASCADE) * open as f32;
    let x = (launcher.right() - px(OVERLAP) + step).min(screen.right() - size.width).max(screen.left());
    let y = (launcher.top() + px(96.) + step).min((screen.bottom() - size.height).max(screen.top())).max(screen.top());
    Bounds::new(gpui::point(x, y), size)
}

/// Open the plugin's window `key`, or bring it to the front if it is open: whether it is
/// open now. Not when the plugin has [`MOST_PER_PLUGIN`] open already, or won't draw in it.
/// `hides_with_launcher`: it goes off screen when the launcher hides, and comes back with it.
pub fn open(plugin_id: &str, key: String, title: String, width: f32, height: f32, hides_with_launcher: bool, cx: &mut App) -> gpui::Task<bool> {
    let existing = cx
        .default_global::<Open>()
        .0
        .iter()
        .find(|open| open.plugin_id == plugin_id && open.key == key)
        .map(|open| open.window);
    if let Some(window) = existing {
        cx.activate(true);
        if window.update(cx, |_, window, _| window.activate_window()).is_ok() {
            // On screen again, even one the launcher had taken off: from here on it hides with the
            // launcher (or not) as any other.
            if let Some(open) = cx.default_global::<Open>().0.iter_mut().find(|open| open.window == window) {
                open.hidden_with_launcher = false;
            }
            return gpui::Task::ready(true);
        }
        forget(plugin_id, &key, cx);
    }
    if cx.default_global::<Open>().0.iter().filter(|open| open.plugin_id == plugin_id).count() >= MOST_PER_PLUGIN {
        return gpui::Task::ready(false);
    }
    let Some(plugin) = plugins::all(cx).iter().find(|plugin| plugin.manifest().plugin.id == plugin_id).cloned() else {
        return gpui::Task::ready(false);
    };

    let surface = cx.new(Surface::new);
    let (width, height) = (px(width.max(MIN_SIZE.0)), px(height.max(MIN_SIZE.1)));
    let opened_before = cx.default_global::<Open>().0.len();
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(beside_launcher(size(width, height), opened_before, cx))),
        titlebar: Some(TitlebarOptions { title: Some(title.into()), appears_transparent: false, traffic_light_position: None }),
        focus: true,
        show: true,
        kind: WindowKind::Normal,
        is_resizable: true,
        is_minimizable: true,
        window_min_size: Some(size(px(MIN_SIZE.0), px(MIN_SIZE.1))),
        window_background: WindowBackgroundAppearance::Opaque,
        ..Default::default()
    };
    let shown = surface.clone();
    // Forget it when it closes, however it does.
    let (closing_plugin, closing_key) = (plugin_id.to_string(), key.clone());
    let opened = cx.open_window(options, move |window, cx| {
        cx.new(|cx| {
            cx.on_release(move |_, cx| forget(&closing_plugin, &closing_key, cx)).detach();
            cx.observe_window_appearance(window, |_, _, cx| crate::theme::appearance_changed(cx)).detach();
            // Losing the keyboard to something outside the launcher and its windows hides them all.
            cx.observe_window_activation(window, |_, window, cx| {
                if window.is_window_active() {
                    crate::launcher::focus_gained(cx);
                } else {
                    cx.defer(crate::launcher::focus_left);
                }
            })
            .detach();
            PluginWindow { surface: shown, focused: false }
        })
    });
    let handle = match opened {
        Ok(handle) => handle,
        Err(error) => {
            log::error!("opening {plugin_id}'s window: {error:#}");
            return gpui::Task::ready(false);
        }
    };
    // In front of the launcher, which floats above ordinary windows: a window opened from it
    // would be behind it.
    let level = crate::launcher::level(cx);
    // AppKit calls back into GPUI as it changes a window, so it is asked outside any update.
    if let (Some(level), Some(native)) = (level, native_of(handle.into(), cx)) {
        cx.spawn(async move |_| native.set_level(level)).detach();
    }
    cx.default_global::<Open>().0.push(Opened {
        plugin_id: plugin_id.to_string(),
        key: key.clone(),
        window: handle.into(),
        hides_with_launcher,
        hidden_with_launcher: false,
    });
    cx.activate(true);

    // The plugin draws in it; if it won't, the window has nothing to show.
    let drawn = plugin.open_window_view(&key, &surface, cx);
    let plugin_id = plugin_id.to_string();
    cx.spawn(async move |cx| {
        let drawn = drawn.await;
        if !drawn {
            cx.update(|cx| {
                forget(&plugin_id, &key, cx);
                handle.update(cx, |_, window, _| window.remove_window()).ok();
            });
        }
        drawn
    })
}

/// Whether a window of this plugin is open (shown, or off screen with the launcher).
pub fn has_open(plugin_id: &str, cx: &App) -> bool {
    cx.try_global::<Open>().is_some_and(|open| open.0.iter().any(|open| open.plugin_id == plugin_id))
}

fn forget(plugin_id: &str, key: &str, cx: &mut App) {
    cx.default_global::<Open>().0.retain(|open| !(open.plugin_id == plugin_id && open.key == key));
}

/// Close the windows of these plugins (by id): they are starting again, or gone, so what they drew
/// is gone. The other plugins' windows stay.
pub fn close_of(plugin_ids: &HashSet<String>, cx: &mut App) {
    let all = std::mem::take(&mut cx.default_global::<Open>().0);
    let (closing, staying): (Vec<Opened>, Vec<Opened>) = all.into_iter().partition(|open| plugin_ids.contains(&open.plugin_id));
    cx.default_global::<Open>().0 = staying;
    for open in closing {
        open.window.update(cx, |_, window, _| window.remove_window()).ok();
    }
}

/// Bring `window` in front of the other windows at its level (the launcher's and the plugins').
pub fn raise(window: &Window, cx: &mut App) {
    // TEMPORARY(clipboard): a click here; something may have been copied since.
    crate::plugins::refresh_clipboards(cx);
    if let Some(native) = crate::macos::NativeWindow::of(window) {
        // Outside this update: AppKit calls back into GPUI.
        cx.spawn(async move |_| native.raise()).detach();
    }
}

/// Whether the keyboard is in one of the plugins' windows, as AppKit says (not as GPUI last heard).
pub fn has_keyboard(cx: &mut App) -> bool {
    with_keyboard(cx).is_some()
}

/// The plugin window the keyboard is in, as AppKit says.
pub fn with_keyboard(cx: &mut App) -> Option<AnyWindowHandle> {
    let windows: Vec<AnyWindowHandle> = cx.default_global::<Open>().0.iter().map(|open| open.window).collect();
    windows.into_iter().find(|window| {
        window
            .update(cx, |_, window, _| crate::macos::NativeWindow::of(window).is_some_and(|native| native.is_key()))
            .unwrap_or(false)
    })
}

/// The AppKit side of a window, taken inside an update; call its methods outside one.
fn native_of(window: AnyWindowHandle, cx: &mut App) -> Option<crate::macos::NativeWindow> {
    window.update(cx, |_, window, _| crate::macos::NativeWindow::of(window)).ok().flatten()
}

/// Take the windows that hide with the launcher off screen, remembering them: the launcher hid, and
/// they go with it. Those that stay up until their user closes them stay.
pub fn hide_all(cx: &mut App) {
    let windows: Vec<AnyWindowHandle> = cx
        .default_global::<Open>()
        .0
        .iter()
        .filter(|open| open.hides_with_launcher && !open.hidden_with_launcher)
        .map(|open| open.window)
        .collect();
    let mut hiding = Vec::new();
    for window in windows {
        let visible = window.update(cx, |_, window, _| crate::macos::is_window_visible(window)).unwrap_or(false);
        if visible && let Some(native) = native_of(window, cx) {
            if let Some(open) = cx.default_global::<Open>().0.iter_mut().find(|open| open.window == window) {
                open.hidden_with_launcher = true;
            }
            hiding.push(native);
        }
    }
    // Outside the update: AppKit calls back into GPUI.
    cx.spawn(async move |_| hiding.iter().for_each(crate::macos::NativeWindow::order_out)).detach();
}

/// Bring back the windows [`hide_all`] took off screen, where they were.
pub fn restore_hidden(cx: &mut App) {
    let mut windows = Vec::new();
    for open in &mut cx.default_global::<Open>().0 {
        if std::mem::replace(&mut open.hidden_with_launcher, false) {
            windows.push(open.window);
        }
    }
    let showing: Vec<_> = windows.into_iter().filter_map(|window| native_of(window, cx)).collect();
    cx.spawn(async move |_| showing.iter().for_each(crate::macos::NativeWindow::order_front)).detach();
}

/// The plugin brings back its window `key` that went off screen with the launcher, where it was:
/// whether it has that window open. Up, it stays up whether the launcher is or not, until the
/// launcher hides again.
pub fn show(plugin_id: &str, key: &str, cx: &mut App) -> bool {
    let Some(open) = cx.default_global::<Open>().0.iter_mut().find(|open| open.plugin_id == plugin_id && open.key == key) else {
        return false;
    };
    let (was_hidden, window) = (std::mem::replace(&mut open.hidden_with_launcher, false), open.window);
    if was_hidden && let Some(native) = native_of(window, cx) {
        // Outside the update: AppKit calls back into GPUI.
        cx.spawn(async move |_| native.order_front()).detach();
    }
    true
}

/// The plugin closes its window `key`, as its user would: whether it had that window open.
pub fn close(plugin_id: &str, key: &str, cx: &mut App) -> bool {
    let Some(window) = cx.default_global::<Open>().0.iter().find(|open| open.plugin_id == plugin_id && open.key == key).map(|open| open.window)
    else {
        return false;
    };
    forget(plugin_id, key, cx);
    // Outside the plugin's call, as a click on ✕ would be.
    cx.defer(move |cx| {
        window.update(cx, |_, window, _| window.remove_window()).ok();
    });
    true
}
