//! The settings window, like macOS System Settings: a sidebar of pages, and the page
//! chosen. General is here; each plugin's page joins it next. One window, brought
//! forward when it's open already.
//!
//! * `general`: Delight's own settings.
//! * `shortcut_recorder`: the field that records the launcher shortcut.

mod general;
mod shortcut_recorder;

use delight_ui::{ActiveTheme, Caption, Group, Icon, IconName, Theme, h_flex, v_flex};
use gpui::{
    AnyElement, App, AppContext as _, Bounds, Context, Entity, FocusHandle, Focusable, Global, Hsla, IntoElement,
    ParentElement, Render, SharedString, Styled, Subscription, TitlebarOptions, Window, WindowBackgroundAppearance,
    WindowBounds, WindowHandle, WindowKind, WindowOptions, actions, div, hsla, point, prelude::*, px, size,
};

use crate::hotkey;
use shortcut_recorder::{Recorded, ShortcutRecorder};

/// The window's size: it doesn't resize.
const WIDTH: f32 = 800.;
const HEIGHT: f32 = 580.;
const SIDEBAR_WIDTH: f32 = 220.;

/// The key context of the settings window.
pub const CONTEXT: &str = "Settings";

actions!(settings_window, [CloseSettings]);

/// A page in the sidebar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Page {
    General,
}

impl Page {
    /// Its title, fixed above what scrolls.
    fn title(self) -> &'static str {
        match self {
            Page::General => "General",
        }
    }
}

pub struct SettingsWindow {
    focus_handle: FocusHandle,
    page: Page,
    shortcut: Entity<ShortcutRecorder>,
    _subscriptions: Vec<Subscription>,
}

struct OpenSettingsWindow(WindowHandle<SettingsWindow>);

impl Global for OpenSettingsWindow {}

/// Open the settings window, or bring it forward if it's open.
pub fn open(cx: &mut App) {
    cx.activate(true);
    if let Some(handle) = cx.try_global::<OpenSettingsWindow>().map(|open| open.0)
        && handle.update(cx, |_, window, _| window.activate_window()).is_ok()
    {
        return;
    }
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(None, size(px(WIDTH), px(HEIGHT)), cx))),
        titlebar: Some(TitlebarOptions {
            title: Some("Delight Settings".into()),
            appears_transparent: true,
            traffic_light_position: Some(point(px(14.), px(14.))),
        }),
        focus: true,
        show: true,
        kind: WindowKind::Normal,
        is_resizable: false,
        is_minimizable: false,
        // Solid, like System Settings: GPUI's blur is a washed-out grey in dark mode.
        window_background: WindowBackgroundAppearance::Opaque,
        ..Default::default()
    };
    match cx.open_window(options, |window, cx| cx.new(|cx| SettingsWindow::new(window, cx))) {
        Ok(handle) => {
            handle.update(cx, |this, window, cx| window.focus(&this.focus_handle, cx)).ok();
            cx.set_global(OpenSettingsWindow(handle));
        }
        Err(error) => log::error!("opening the settings: {error:#}"),
    }
}

impl SettingsWindow {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let shortcut = cx.new(|cx| ShortcutRecorder::new(hotkey::current(cx), cx));
        let subscriptions = vec![
            cx.subscribe_in(&shortcut, window, |_, recorder, Recorded(keystroke), window, cx| {
                match hotkey::change(keystroke, cx) {
                    Ok(()) => recorder.update(cx, |recorder, cx| recorder.set_current(keystroke.clone(), window, cx)),
                    Err(error) => recorder.update(cx, |recorder, cx| recorder.set_error(format!("{error:#}"), cx)),
                }
            }),
            cx.observe_window_appearance(window, |_, _, cx| delight_ui::theme::appearance_changed(cx)),
        ];
        Self { focus_handle: cx.focus_handle(), page: Page::General, shortcut, _subscriptions: subscriptions }
    }

    fn render_sidebar(&self, t: &Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let item = |id: &'static str, label: &'static str, icon: IconName, page: Page| {
            let selected = self.page == page;
            let hover = t.fill_subtle();
            h_flex()
                .id(id)
                .h(px(30.))
                .px(px(8.))
                .gap(px(9.))
                .rounded(t.radius_small())
                .cursor_pointer()
                .when(selected, |row| row.bg(t.accent).text_color(t.accent_text))
                .when(!selected, |row| row.hover(move |style| style.bg(hover)))
                .child(Icon::new(icon).size(px(18.)).color(if selected { t.accent_text } else { t.text_muted }))
                .child(label)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.page = page;
                    cx.notify();
                }))
        };
        v_flex()
            .w(px(SIDEBAR_WIDTH))
            .flex_shrink_0()
            .h_full()
            // Below the traffic lights.
            .pt(px(44.))
            .px(px(10.))
            .pb(px(12.))
            .gap(px(2.))
            .bg(sidebar_color(t))
            .border_r_1()
            .border_color(t.separator())
            .child(item("page-general", "General", IconName::Settings, Page::General))
    }
}

impl Focusable for SettingsWindow {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for SettingsWindow {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        let page = match self.page {
            Page::General => self.render_general(&t, cx),
        };
        h_flex()
            .key_context(CONTEXT)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(|_, _: &CloseSettings, window, _| window.remove_window()))
            .size_full()
            .items_start()
            .bg(content_color(&t))
            .font_family(t.font.clone())
            .text_color(t.text)
            .text_size(t.text_size)
            .child(self.render_sidebar(&t, cx))
            .child(
                v_flex()
                    .flex_1()
                    .min_w(px(0.))
                    .h_full()
                    .child(
                        div()
                            .flex_shrink_0()
                            .px(px(32.))
                            .pt(px(28.))
                            .pb(px(16.))
                            .text_size(px(20.))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(self.page.title()),
                    )
                    .child(
                        div()
                            .id("settings-page")
                            .flex_1()
                            .min_h(px(0.))
                            .overflow_y_scroll()
                            .px(px(32.))
                            .pb(px(24.))
                            .child(page),
                    ),
            )
    }
}

/// The page's background, under its groups: a step darker than the groups (which are
/// the theme's surface), as in System Settings.
fn content_color(t: &Theme) -> Hsla {
    if t.dark { hsla(0., 0., 0.12, 1.) } else { hsla(0., 0., 0.96, 1.) }
}

fn sidebar_color(t: &Theme) -> Hsla {
    if t.dark { hsla(0., 0., 0.16, 1.) } else { hsla(0., 0., 0.92, 1.) }
}

/// A captioned group of rows.
fn section(caption: &'static str, rows: Vec<AnyElement>) -> impl IntoElement {
    v_flex().gap(px(6.)).child(div().px(px(4.)).child(Caption::new(caption))).child(Group::new().children(rows))
}

/// A `title (detail) … control` row in a group.
fn row(title: &'static str, detail: Option<&'static str>, control: impl IntoElement, t: &Theme) -> AnyElement {
    h_flex()
        .gap(px(12.))
        .px(px(14.))
        .py(px(10.))
        .child(
            v_flex()
                .flex_1()
                .min_w(px(0.))
                .child(SharedString::from(title))
                .when_some(detail, |column, detail| {
                    column.child(div().mt(px(2.)).text_size(px(11.)).text_color(t.text_muted).child(detail))
                }),
        )
        .child(control)
        .into_any_element()
}
