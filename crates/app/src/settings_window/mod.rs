//! The settings window, like macOS System Settings: a sidebar of pages (General, then
//! each plugin, and the plugin files that don't load), and the page chosen, under its
//! fixed header. One window, brought forward when it's open already.
//!
//! * `general`: Delight's own settings.
//! * `plugins`: a plugin's page (its tools and permissions), and a broken file's.
//! * `install`: picking a plugin and the sheet that shows what it adds.
//! * `shortcut_recorder`: the field that records the launcher shortcut.

mod general;
mod install;
mod plugins;
mod shortcut_recorder;

use std::path::PathBuf;

use delight_runtime::Plugin;
use delight_ui::{
    ActiveTheme, Button, Caption, EditorEvent, Group, Icon, IconName, LogoBadge, TextEditor, Theme, h_flex, v_flex,
};
use gpui::{
    AnyElement, App, AppContext as _, Bounds, Context, ElementId, Entity, FocusHandle, Focusable, FontWeight, Global,
    Hsla, IntoElement, ParentElement, Render, SharedString, Styled, Subscription, TitlebarOptions, Window,
    WindowBackgroundAppearance, WindowBounds, WindowHandle, WindowKind, WindowOptions, actions, div, hsla, point,
    prelude::*, px, size,
};

use crate::hotkey;
use crate::plugins::{self as loaded, Broken, Source};
use install::Pending;
use shortcut_recorder::{Recorded, ShortcutRecorder};

/// The window's size: it doesn't resize.
const WIDTH: f32 = 800.;
const HEIGHT: f32 = 580.;
const SIDEBAR_WIDTH: f32 = 220.;

/// The key context of the settings window.
pub const CONTEXT: &str = "Settings";

actions!(settings_window, [CloseSettings]);

/// A page in the sidebar.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Page {
    General,
    /// A plugin's page, by its id.
    Plugin(String),
    /// A plugin file that doesn't load.
    Broken(PathBuf),
}

/// What a page shows: [`Page`] looked up in the plugins as they are now. A plugin that's
/// gone shows General.
enum Shown {
    General,
    Plugin(Plugin, Source),
    Broken(Broken),
}

pub struct SettingsWindow {
    focus_handle: FocusHandle,
    page: Page,
    /// The sidebar's search: it filters the plugins.
    search: Entity<TextEditor>,
    shortcut: Entity<ShortcutRecorder>,
    /// The plugin picked to install, while its sheet shows.
    installing: Option<Pending>,
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
        let search = cx.new(|cx| TextEditor::new(window, cx).placeholder("Search"));
        let subscriptions = vec![
            cx.subscribe(&search, |_, _, event: &EditorEvent, cx| {
                if *event == EditorEvent::Changed {
                    cx.notify();
                }
            }),
            cx.subscribe_in(&shortcut, window, |_, recorder, Recorded(keystroke), window, cx| {
                match hotkey::change(keystroke, cx) {
                    Ok(()) => recorder.update(cx, |recorder, cx| recorder.set_current(keystroke.clone(), window, cx)),
                    Err(error) => recorder.update(cx, |recorder, cx| recorder.set_error(format!("{error:#}"), cx)),
                }
            }),
            cx.observe_window_appearance(window, |_, _, cx| delight_ui::theme::appearance_changed(cx)),
        ];
        Self {
            focus_handle: cx.focus_handle(),
            page: Page::General,
            search,
            shortcut,
            installing: None,
            _subscriptions: subscriptions,
        }
    }

    fn shown(&self, cx: &App) -> Shown {
        match &self.page {
            Page::General => Shown::General,
            Page::Plugin(id) => loaded::all(cx)
                .iter()
                .zip(loaded::sources(cx).iter())
                .find(|(plugin, _)| plugin.manifest().plugin.id == *id)
                .map_or(Shown::General, |(plugin, source)| Shown::Plugin(plugin.clone(), source.clone())),
            Page::Broken(file) => loaded::broken(cx)
                .iter()
                .find(|broken| broken.source.file == *file)
                .map_or(Shown::General, |broken| Shown::Broken(broken.clone())),
        }
    }

    fn render_sidebar(&self, t: &Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let query = self.search.read(cx).text().trim().to_lowercase();
        let matches = |text: &str| query.is_empty() || text.to_lowercase().contains(&query);
        let plugin_matches = |plugin: &Plugin| {
            let manifest = plugin.manifest();
            let properties = &manifest.plugin;
            [&properties.name, &properties.id, &properties.description].into_iter().any(|text| matches(text))
                || manifest.operations.iter().any(|operation| matches(&operation.title) || matches(&operation.description))
        };
        let (all, sources, broken) = (loaded::all(cx), loaded::sources(cx), loaded::broken(cx));
        let shown = self.page.clone();
        let entry = |id: ElementId, icon: AnyElement, label: String, page: Page, cx: &mut Context<Self>| {
            let selected = shown == page;
            let hover = t.fill_subtle();
            h_flex()
                .id(id)
                .flex_shrink_0()
                .h(px(30.))
                .px(px(8.))
                .gap(px(9.))
                .rounded(t.radius_small())
                .cursor_pointer()
                .when(selected, |row| row.bg(t.accent).text_color(t.accent_text))
                .when(!selected, |row| row.hover(move |style| style.bg(hover)))
                .child(icon)
                .child(div().flex_1().min_w(px(0.)).truncate().child(label))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.page = page.clone();
                    cx.notify();
                }))
                .into_any_element()
        };
        // The plugins, and the files that don't load, built in or installed.
        let group = |built_in: bool, cx: &mut Context<Self>| -> Vec<AnyElement> {
            let mut entries = Vec::new();
            for (index, (plugin, source)) in all.iter().zip(sources.iter()).enumerate() {
                if source.built_in == built_in && plugin_matches(plugin) {
                    let properties = &plugin.manifest().plugin;
                    let logo = LogoBadge::new(properties.icon.as_bytes()).size(px(18.)).into_any_element();
                    let page = Page::Plugin(properties.id.clone());
                    entries.push(entry(("plugin", index).into(), logo, properties.name.clone(), page, cx));
                }
            }
            for (index, file) in broken.iter().enumerate() {
                let name = plugins::file_name(&file.source.file);
                if file.source.built_in == built_in && matches(&name) {
                    let warning = Icon::new(IconName::TriangleAlert).size(px(18.)).color(t.warning).into_any_element();
                    let page = Page::Broken(file.source.file.clone());
                    entries.push(entry(("broken", index).into(), warning, name, page, cx));
                }
            }
            entries
        };
        let built_in = group(true, cx);
        let installed = group(false, cx);
        let caption = |text: &'static str| div().flex_shrink_0().px(px(8.)).pt(px(12.)).pb(px(4.)).child(Caption::new(text));
        let general = entry(
            "page-general".into(),
            Icon::new(IconName::Settings)
                .size(px(18.))
                .color(if self.page == Page::General { t.accent_text } else { t.text_muted })
                .into_any_element(),
            "General".into(),
            Page::General,
            cx,
        );
        let search = h_flex()
            .flex_shrink_0()
            .h(px(28.))
            .mb(px(10.))
            .px(px(8.))
            .gap(px(6.))
            .rounded(px(7.))
            .bg(t.surface)
            .border_1()
            .border_color(t.border)
            .child(Icon::new(IconName::Search).size(px(14.)).color(t.text_muted))
            .child(div().flex_1().min_w(px(0.)).child(self.search.clone()));
        let starting = loaded::loading(cx).then(|| {
            div().px(px(8.)).py(px(4.)).text_size(px(11.)).text_color(t.text_muted).child("Starting plugins…")
        });
        let list = v_flex()
            .id("sidebar-entries")
            .flex_1()
            .min_h(px(0.))
            .overflow_y_scroll()
            .gap(px(2.))
            .child(general)
            .when(!built_in.is_empty(), |list| list.child(caption("Built-in")).children(built_in))
            .child(caption("Installed"))
            .children(installed)
            .children(starting);
        let install = Button::new("install-plugin", "Install Plugin…")
            .icon(IconName::Plus)
            .on_click(cx.listener(|this, _, window, cx| this.pick_plugin(window, cx)));
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
            .child(search)
            .child(list)
            .child(div().flex().justify_center().pt(px(8.)).child(install))
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
        let (header, page) = match self.shown(cx) {
            Shown::General => (
                div().text_size(px(20.)).font_weight(FontWeight::SEMIBOLD).child("General").into_any_element(),
                self.render_general(&t, cx),
            ),
            Shown::Plugin(plugin, source) => {
                (plugins::plugin_header(&plugin, &source, &t, cx), self.render_plugin(&plugin, &source, &t, cx))
            }
            Shown::Broken(broken) => (plugins::broken_header(&broken, &t), self.render_broken(&broken, &t, cx)),
        };
        let install = self.render_install(&t, cx);
        h_flex()
            .key_context(CONTEXT)
            .track_focus(&self.focus_handle)
            // Esc closes the install sheet first.
            .on_action(cx.listener(|this, _: &CloseSettings, window, cx| {
                if this.installing.is_some() {
                    this.cancel_install(cx);
                } else {
                    window.remove_window();
                }
            }))
            .relative()
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
                    .child(div().flex_shrink_0().px(px(32.)).pt(px(28.)).pb(px(16.)).child(header))
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
            .children(install)
    }
}

/// The page's background, under its groups: a step darker than the groups (which are
/// the theme's surface), as in System Settings.
pub(super) fn content_color(t: &Theme) -> Hsla {
    if t.dark { hsla(0., 0., 0.12, 1.) } else { hsla(0., 0., 0.96, 1.) }
}

fn sidebar_color(t: &Theme) -> Hsla {
    if t.dark { hsla(0., 0., 0.16, 1.) } else { hsla(0., 0., 0.92, 1.) }
}

/// A captioned group of rows.
fn section(caption: &'static str, rows: Vec<AnyElement>) -> impl IntoElement {
    v_flex().gap(px(6.)).child(div().px(px(4.)).child(Caption::new(caption))).child(Group::new().children(rows))
}

/// A row with an icon, a bold title and its detail, and maybe a control: a tool or a
/// permission.
fn item(icon: AnyElement, title: SharedString, detail: Option<SharedString>, control: Option<AnyElement>, t: &Theme) -> AnyElement {
    h_flex()
        .gap(px(12.))
        .px(px(14.))
        .py(px(10.))
        .child(icon)
        .child(
            v_flex()
                .flex_1()
                .min_w(px(0.))
                .child(div().font_weight(FontWeight::SEMIBOLD).child(title))
                .when_some(detail, |column, detail| {
                    column.child(div().mt(px(2.)).text_size(px(12.)).text_color(t.text_muted).child(detail))
                }),
        )
        .children(control)
        .into_any_element()
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
