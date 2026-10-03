//! The settings window, like macOS System Settings: a sidebar of pages (General, then
//! each plugin, and the plugin files that don't load), and the page chosen, under its
//! fixed header. One window, brought forward when it's open already.
//!
//! * `general`: Delight's own settings.
//! * `plugins`: a plugin's page (its tools and permissions), and a broken file's.
//! * `install`: the Install Plugin… button and its menu: from files, or from a link (the install
//!   window does the rest).
//! * `shortcut_recorder`: the field that records the launcher shortcut.

mod general;
mod install;
mod plugins;
mod shortcut_recorder;

use std::path::PathBuf;

use delight_runtime::Plugin;
use delight_ui::{
    ActiveTheme, Caption, EditorEvent, Icon, IconName, LogoBadge, StyledExt as _, TextEditor, Theme, h_flex, v_flex,
};
use gpui::{
    AnyElement, App, AppContext as _, Bounds, Context, ElementId, Entity, FocusHandle, Focusable, FontWeight, Global,
    Hsla, IntoElement, ParentElement, Render, SharedString, Styled, Subscription, TitlebarOptions, Window,
    WindowBackgroundAppearance, WindowBounds, WindowHandle, WindowKind, WindowOptions, actions, div, hsla, point,
    prelude::*, px, size,
};

use crate::{hotkey, launcher};
use crate::permissions::view::OpenPermissions;
use crate::plugins::{self as loaded, Broken, Source};
pub(crate) use plugins::file_name;
use plugins::SettingsPage;
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
    /// Which permission rows are open on each plugin's page, by plugin id.
    open_permissions: std::collections::HashMap<String, OpenPermissions>,
    /// The shown plugin's own settings page, while its page shows.
    settings_page: Option<SettingsPage>,
    /// The Install Plugin… button's menu is open.
    install_menu: bool,
    /// A plugin or a file that doesn't load was deleted from its page: its file, and the page beside
    /// it. The page stays selected until the sidebar no longer lists it, then that one takes its place.
    deleted: Option<(PathBuf, Page)>,
    _subscriptions: Vec<Subscription>,
}

struct OpenSettingsWindow(WindowHandle<SettingsWindow>);

impl Global for OpenSettingsWindow {}

/// Open the settings window, or bring it forward if it's open.
pub fn open(cx: &mut App) {
    open_on(None, cx);
}

/// Open the settings window on a plugin's own page.
pub fn open_plugin(plugin_id: String, cx: &mut App) {
    open_on(Some(Page::Plugin(plugin_id)), cx);
}

/// Open the window, or bring it forward, on `page` if one is given. The launcher hides first, so the
/// window comes up in its place rather than under it.
fn open_on(page: Option<Page>, cx: &mut App) {
    launcher::hide_then(
        move |cx| {
            let Some(handle) = show(cx) else { return };
            if let Some(page) = page {
                handle
                    .update(cx, |this, _, cx| {
                        this.page = page;
                        this.deleted = None;
                        cx.notify();
                    })
                    .ok();
            }
        },
        cx,
    );
}

/// Open the window, or bring it forward if it's open.
fn show(cx: &mut App) -> Option<WindowHandle<SettingsWindow>> {
    cx.activate(true);
    if let Some(handle) = cx.try_global::<OpenSettingsWindow>().map(|open| open.0)
        && handle.update(cx, |_, window, _| window.activate_window()).is_ok()
    {
        return Some(handle);
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
            Some(handle)
        }
        Err(error) => {
            log::error!("opening the settings: {error:#}");
            None
        }
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
            cx.observe_window_appearance(window, |_, _, cx| crate::theme::appearance_changed(cx)),
            // TEMPORARY(clipboard): something may have been copied elsewhere; plugins see it before a paste.
            cx.observe_window_activation(window, |_, window, cx| {
                if window.is_window_active() {
                    loaded::refresh_clipboards(cx);
                }
            }),
        ];
        Self {
            focus_handle: cx.focus_handle(),
            page: Page::General,
            search,
            shortcut,
            open_permissions: Default::default(),
            settings_page: None,
            install_menu: false,
            deleted: None,
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

    /// The plugins and the files that don't load, built in or installed, as the sidebar lists them
    /// under General: in its order, those the search leaves.
    fn listed(&self, built_in: bool, cx: &App) -> Vec<Page> {
        let query = self.search.read(cx).text().trim().to_lowercase();
        let matches = |text: &str| query.is_empty() || text.to_lowercase().contains(&query);
        let plugin_matches = |plugin: &Plugin| {
            let manifest = plugin.manifest();
            let properties = &manifest.plugin;
            [&properties.name, &properties.id, &properties.description].into_iter().any(|text| matches(text))
                || manifest.operations.iter().any(|operation| matches(&operation.title) || matches(&operation.description))
        };
        let (all, sources, broken) = (loaded::all(cx), loaded::sources(cx), loaded::broken(cx));
        let running = all
            .iter()
            .zip(sources.iter())
            .filter(|(plugin, source)| source.built_in == built_in && plugin_matches(plugin))
            .map(|(plugin, _)| Page::Plugin(plugin.manifest().plugin.id.clone()));
        let files = broken
            .iter()
            .filter(|file| file.source.built_in == built_in && matches(&plugins::file_name(&file.source.file)))
            .map(|file| Page::Broken(file.source.file.clone()));
        running.chain(files).collect()
    }

    /// The page beside `page` in the sidebar: the one below it, or else the one above.
    fn beside(&self, page: &Page, cx: &App) -> Page {
        let pages: Vec<Page> = std::iter::once(Page::General).chain(self.listed(true, cx)).chain(self.listed(false, cx)).collect();
        let Some(index) = pages.iter().position(|listed| listed == page) else { return Page::General };
        pages.get(index + 1).or_else(|| pages.get(index.checked_sub(1)?)).cloned().unwrap_or(Page::General)
    }

    /// The shown page's plugin or file was deleted, from `file`: keep the page until the sidebar
    /// no longer lists it (a plugin stops a moment later), so the selection doesn't move first.
    fn deleted(&mut self, file: PathBuf, cx: &App) {
        self.deleted = Some((file.clone(), self.beside(&self.page, cx)));
    }

    /// A deleted plugin or file is no longer listed: the page beside it takes its place, if its page
    /// is still the one shown.
    fn follow_deletion(&mut self, cx: &App) {
        let Some((file, _)) = &self.deleted else { return };
        let listed = loaded::sources(cx).iter().any(|source| source.file == *file)
            || loaded::broken(cx).iter().any(|broken| broken.source.file == *file);
        if !listed && let Some((_, next)) = self.deleted.take() {
            self.page = next;
        }
    }

    fn render_sidebar(&self, t: &Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let (all, broken) = (loaded::all(cx), loaded::broken(cx));
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
                    this.deleted = None;
                    cx.notify();
                }))
                .into_any_element()
        };
        // The plugins, and the files that don't load, built in or installed.
        let group = |built_in: bool, cx: &mut Context<Self>| -> Vec<AnyElement> {
            let mut entries = Vec::new();
            for page in self.listed(built_in, cx) {
                match &page {
                    Page::Plugin(id) => {
                        let Some((index, plugin)) = all.iter().enumerate().find(|(_, plugin)| plugin.manifest().plugin.id == *id) else { continue };
                        let properties = &plugin.manifest().plugin;
                        let logo = LogoBadge::new(properties.icon.as_bytes()).size(px(18.)).into_any_element();
                        entries.push(entry(("plugin", index).into(), logo, properties.name.clone(), page.clone(), cx));
                    }
                    Page::Broken(file) => {
                        let Some(index) = broken.iter().position(|broken| broken.source.file == *file) else { continue };
                        let warning = Icon::new(IconName::TriangleAlert).size(px(18.)).color(t.warning).into_any_element();
                        entries.push(entry(("broken", index).into(), warning, plugins::file_name(file), page.clone(), cx));
                    }
                    Page::General => {}
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
            div().px(px(8.)).py(px(4.)).text_size(px(11.)).text_color(t.text_muted).child("Loading plugins…")
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
        let install = self.install_button(t, cx);
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
            .child(install)
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
        self.follow_deletion(cx);
        let shown = self.shown(cx);
        // A plugin's settings page lives while its page shows.
        if !matches!(shown, Shown::Plugin(..)) {
            self.settings_page = None;
        }
        let (header, page) = match shown {
            Shown::General => (
                div().text_size(px(20.)).font_weight(FontWeight::SEMIBOLD).child("General").into_any_element(),
                self.render_general(&t, cx),
            ),
            Shown::Plugin(plugin, source) => {
                (plugins::plugin_header(&plugin, &t, cx), self.render_plugin(&plugin, &source, &t, cx))
            }
            Shown::Broken(broken) => (plugins::broken_header(&broken, &t), self.render_broken(&broken, &t, cx)),
        };
        h_flex()
            .key_context(CONTEXT)
            .track_focus(&self.focus_handle)
            // TEMPORARY(clipboard): a click anywhere; something may have been copied since.
            .capture_any_mouse_down(|_, _, cx| loaded::refresh_clipboards(cx))
            .on_action(cx.listener(|_, _: &CloseSettings, window, _| window.remove_window()))
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
    }
}

/// The page's background, under its cards ([`Theme::card`]): white in light mode, as in
/// System Settings, where the cards are a step darker.
pub(crate) fn content_color(t: &Theme) -> Hsla {
    if t.dark { hsla(0., 0., 0.12, 1.) } else { hsla(0., 0., 1., 1.) }
}

fn sidebar_color(t: &Theme) -> Hsla {
    if t.dark { hsla(0., 0., 0.16, 1.) } else { hsla(0., 0., 0.92, 1.) }
}

/// The lines of an [`item`]'s text, in pixels: a title line, the gap, and a one-line detail add up to
/// [`ITEM_ICON`], so a tool's logo is as tall as its text.
const ITEM_TITLE_LINE: f32 = 18.;
const ITEM_GAP: f32 = 2.;
const ITEM_DETAIL_LINE: f32 = 16.;
/// The size of the logo beside an item's text.
pub(crate) const ITEM_ICON: f32 = ITEM_TITLE_LINE + ITEM_GAP + ITEM_DETAIL_LINE;

/// A row with an icon, a bold title and its detail, and maybe a control: a tool or a
/// permission.
pub(crate) fn item(icon: AnyElement, title: SharedString, detail: Option<SharedString>, control: Option<AnyElement>, t: &Theme) -> AnyElement {
    let detail = detail.map(|detail| {
        // At most three lines, the last ending in an ellipsis: a description can be as long as its author likes.
        div().mt(px(ITEM_GAP)).text_size(px(12.)).line_height(px(ITEM_DETAIL_LINE)).text_color(t.text_muted).clamp_lines(3).child(delight_ui::ellipsize(&detail, 400)).into_any_element()
    });
    item_with(icon, title, detail.into_iter().collect(), control)
}

/// An [`item`] with its own lines under the title. The icon lines up with the title, at the top of
/// the text; a control stays in the middle of the row.
fn item_with(icon: AnyElement, title: SharedString, lines: Vec<AnyElement>, control: Option<AnyElement>) -> AnyElement {
    h_flex()
        .items_center()
        .gap(px(12.))
        .px(px(14.))
        .py(px(10.))
        .child(
            h_flex()
                .flex_1()
                .min_w(px(0.))
                .items_start()
                .gap(px(12.))
                .child(icon)
                .child(
                    v_flex()
                        .flex_1()
                        .min_w(px(0.))
                        .child(div().line_height(px(ITEM_TITLE_LINE)).font_weight(FontWeight::SEMIBOLD).truncate().child(delight_ui::ellipsize(&delight_ui::one_line(&title), 120)))
                        .children(lines),
                ),
        )
        .children(control)
        .into_any_element()
}
