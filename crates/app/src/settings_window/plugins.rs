//! A plugin's page (its tools, each with its own switch, and its permissions), and the
//! page of a plugin file that doesn't load.

use std::collections::HashSet;
use std::path::Path;

use delight_protocol::{PermissionRequest, SettingsSection};
use delight_runtime::Plugin;
use embedded_gpui::Surface;
use delight_ui::{Button, Disableable as _, Icon, IconName, LogoBadge, StyledExt as _, Switch, Theme, h_flex, section, v_flex};
use gpui::{
    AnyElement, App, AppContext as _, ClickEvent, ClipboardItem, Context, Entity, FontWeight, IntoElement, ParentElement,
    PromptLevel, SharedString, Styled, Task, Window, div, prelude::*, px,
};

use super::{ITEM_ICON, Page, SettingsWindow, item};
use crate::plugins::{self, Broken, Source};
use crate::settings;

/// The shown plugin's own settings: the sections it says it has, each with a surface it
/// draws its content on. Made when its page first shows, and kept while it shows.
pub(super) struct SettingsPage {
    /// The plugin instance it was asked of, and its sections' surfaces are opened on: when the
    /// plugin starts again (its file changed, an update), the page is asked of the new one.
    plugin: Plugin,
    /// In the plugin's order. Empty until it has answered, and for a plugin without
    /// settings.
    sections: Vec<SectionSurface>,
    /// Asking for the sections, and for a new one's content.
    _asking: Vec<Task<()>>,
    /// The plugin saying its sections changed.
    _observing: gpui::Subscription,
}

/// One section: what the plugin said about it, and where its content is drawn.
struct SectionSurface {
    section: SettingsSection,
    surface: Entity<Surface>,
    /// Whether the plugin drew it: `None` until it has answered.
    drawn: Option<bool>,
}

impl SettingsWindow {
    /// The plugin's settings sections, asked for when its page first shows (and again
    /// when the plugin has started again, or it says they changed): what the
    /// page draws now, each as (title, height, footer, content).
    fn settings_sections(&mut self, plugin: &Plugin, cx: &mut Context<Self>) -> Vec<(String, f32, String, Entity<Surface>)> {
        let asked = self.settings_page.as_ref().is_some_and(|page| page.plugin.same_instance(plugin));
        if !asked {
            if plugin.stopped().is_some() {
                self.settings_page = None;
                return Vec::new();
            }
            let this = cx.weak_entity();
            let observing = plugin.observe_settings(cx, move |cx| {
                this.update(cx, |this, cx| this.ask_sections(cx)).ok();
            });
            self.settings_page =
                Some(SettingsPage { plugin: plugin.clone(), sections: Vec::new(), _asking: Vec::new(), _observing: observing });
            self.ask_sections_of(plugin.clone(), cx);
        }
        let Some(page) = self.settings_page.as_ref() else { return Vec::new() };
        page.sections
            .iter()
            .filter(|drawn| drawn.drawn == Some(true))
            .map(|drawn| (drawn.section.title.clone(), drawn.section.height, drawn.section.footer.clone(), drawn.surface.clone()))
            .collect()
    }

    /// The plugin said its sections changed: ask again.
    fn ask_sections(&mut self, cx: &mut Context<Self>) {
        let Some(page) = &self.settings_page else { return };
        let plugin = page.plugin.clone();
        self.ask_sections_of(plugin, cx);
    }

    fn ask_sections_of(&mut self, plugin: Plugin, cx: &mut Context<Self>) {
        let asked = plugin.settings_sections(cx);
        let task = cx.spawn(async move |this, cx| {
            let sections = asked.await.unwrap_or_default();
            this.update(cx, |this, cx| this.apply_sections(&plugin, sections, cx)).ok();
        });
        if let Some(page) = &mut self.settings_page {
            page._asking.push(task);
        }
    }

    /// The plugin's sections are these: keep the ones it still has (with what it now says),
    /// drop the others, and have it draw the new ones. An answer from an instance the page no longer
    /// shows is dropped.
    fn apply_sections(&mut self, plugin: &Plugin, sections: Vec<SettingsSection>, cx: &mut Context<Self>) {
        let Some(page) = &mut self.settings_page else { return };
        if !page.plugin.same_instance(plugin) {
            return;
        }
        let mut old = std::mem::take(&mut page.sections);
        let mut drawing = Vec::new();
        for section in sections {
            match old.iter().position(|kept| kept.section.id == section.id) {
                Some(index) => {
                    let mut kept = old.remove(index);
                    kept.section = section;
                    page.sections.push(kept);
                }
                None => {
                    let surface = cx.new(Surface::new);
                    let opened = plugin.open_settings_section(&section.id, &surface, cx);
                    let id = section.id.clone();
                    drawing.push(cx.spawn(async move |this, cx| {
                        let drawn = opened.await;
                        this.update(cx, |this, cx| {
                            if let Some(page) = &mut this.settings_page
                                && let Some(section) = page.sections.iter_mut().find(|s| s.section.id == id)
                            {
                                section.drawn = Some(drawn);
                                cx.notify();
                            }
                        })
                        .ok();
                    }));
                    page.sections.push(SectionSurface { section, surface, drawn: None });
                }
            }
        }
        page._asking.extend(drawing);
        cx.notify();
    }

    /// The plugin's page: what it is, its tools, its permissions, its own settings,
    /// and deleting it.
    pub(super) fn render_plugin(&mut self, plugin: &Plugin, source: &Source, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let manifest = plugin.manifest();
        let id = manifest.plugin.id.clone();
        let current = settings::get(cx).clone();
        let plugin_on = current.plugin_on(&id);

        let stopped = plugin.stopped().map(|reason| {
            notice(format!("Stopped: {reason}. It’s off until Delight restarts."), t)
        });

        let tools = manifest
            .operations
            .iter()
            .map(|operation| {
                let icon = operation.icon.as_deref().unwrap_or(&manifest.plugin.icon);
                let (plugin_id, tool) = (id.clone(), operation.id.clone());
                // While the plugin is off, the tool keeps its own setting but can't change.
                let switch = Switch::new(SharedString::from(format!("tool-{}", operation.id)))
                    .checked(current.tool_on(&id, &operation.id))
                    .disabled(!plugin_on)
                    .on_change(move |on, _, cx| {
                        settings::update(cx, |settings| settings.set_tool_on(&plugin_id, &tool, *on))
                    });
                item(
                    LogoBadge::new(icon.as_bytes()).size(px(ITEM_ICON)).into_any_element(),
                    operation.title.clone().into(),
                    Some(operation.description.clone().into()),
                    Some(switch.into_any_element()),
                    t,
                )
            })
            .collect();

        let footer: AnyElement = if source.built_in {
            div()
                .px(px(4.))
                .text_size(px(11.))
                .text_color(t.text_muted)
                .child("Built-in tools can be turned off, not deleted.")
                .into_any_element()
        } else {
            let (file, reveal) = (source.file.clone(), source.file.clone());
            let name = manifest.plugin.name.clone();
            h_flex()
                .justify_end()
                .gap(px(8.))
                .child(Button::new("show-in-finder", "Show in Finder").on_click(move |_, _, cx| cx.reveal_path(&reveal)))
                .child(Button::new("delete-plugin", "Delete Plugin…").on_click(cx.listener(move |_, _, window, cx| {
                    let answer = window.prompt(
                        PromptLevel::Warning,
                        &format!("Delete “{name}”?"),
                        Some("This removes the plugin, its data and what it remembered. It can’t be undone."),
                        &["Delete", "Cancel"],
                        cx,
                    );
                    let (id, file) = (id.clone(), file.clone());
                    cx.spawn(async move |this, cx| {
                        if answer.await != Ok(0) {
                            return;
                        }
                        this.update(cx, |this, cx| {
                            if let Err(error) = plugins::delete(&id, &file, cx) {
                                log::error!("deleting {id}: {error:#}");
                            }
                            this.page = Page::General;
                            cx.notify();
                        })
                        .ok();
                    })
                    .detach();
                })))
                .into_any_element()
        };

        v_flex()
            .gap(px(18.))
            .when(!manifest.plugin.description.is_empty(), |page| {
                // At most four lines: the rest is cut with an ellipsis.
                page.child(div().text_color(t.text_muted).clamp_lines(4).child(delight_ui::ellipsize(&manifest.plugin.description, 700)))
            })
            .children(stopped)
            .children(updates_section(plugin, source, t, cx))
            .child(section("Tools", tools))
            .child(section("Permissions", self.permissions_of(&manifest.plugin.id, &manifest.plugin.permissions, t, cx)))
            // The plugin's own sections, as cards like the ones above: the app draws each
            // title and card, the plugin the rows inside.
            .children({
                let cards: Vec<AnyElement> = self
                    .settings_sections(plugin, cx)
                    .into_iter()
                    .map(|(title, height, footer, surface)| {
                        let content = div().h(px(height)).overflow_hidden().child(surface).into_any_element();
                        v_flex()
                            .gap(px(6.))
                            .child(section(title, vec![content]))
                            .when(!footer.is_empty(), |column| {
                                column.child(
                                    div().px(px(4.)).text_size(t.text_size_small()).text_color(t.text_muted).clamp_lines(3).child(delight_ui::ellipsize(&footer, 400)),
                                )
                            })
                            .into_any_element()
                    })
                    .collect();
                cards
            })
            // Last of the plugin's own sections: tips are the least needed.
            .when(!manifest.plugin.tips.is_empty(), |page| {
                page.child(section("Tips", tip_rows(&manifest.plugin.tips, t)))
            })
            .child(footer)
            .into_any_element()
    }

    /// The plugin's permission rows, each opening and closing on a click; which are open is kept per
    /// plugin while the window lives.
    fn permissions_of(&self, plugin_id: &str, permissions: &[PermissionRequest], t: &Theme, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let closed = OpenPermissions::default();
        let open = self.open_permissions.get(plugin_id).unwrap_or(&closed);
        permission_rows(
            permissions,
            open,
            |index| {
                let plugin_id = plugin_id.to_string();
                Box::new(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.open_permissions.entry(plugin_id.clone()).or_default().toggle(index);
                    cx.notify();
                }))
            },
            t,
        )
    }

    /// The page of a file that doesn't load: why, and deleting it.
    pub(super) fn render_broken(&self, broken: &Broken, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let name = file_name(&broken.source.file);
        let details = format!(
            "The Delight plugin {name} doesn’t load: {}.\n\n{}\n",
            broken.problem.summary(),
            broken.detail
        );
        let card = v_flex()
            .gap(px(4.))
            .px(px(14.))
            .py(px(12.))
            .rounded(t.radius)
            .bg(t.card())
            .child(div().font_weight(FontWeight::SEMIBOLD).child(broken.problem.summary()))
            .child(div().text_size(t.text_size_small()).text_color(t.text_muted).child(broken.problem.hint()))
            .child(
                div()
                    .mt(px(6.))
                    .px(px(10.))
                    .py(px(8.))
                    .rounded(t.radius_small())
                    .bg(t.fill_subtle())
                    .font_family(t.mono_font.clone())
                    .text_size(t.mono_size())
                    .child(broken.detail.clone()),
            );
        let copy = Button::new("copy-details", "Copy Details")
            .on_click(move |_, _, cx| cx.write_to_clipboard(ClipboardItem::new_string(details.clone())));
        let delete = (!broken.source.built_in).then(|| {
            let file = broken.source.file.clone();
            Button::new("delete-file", "Delete File…").on_click(cx.listener(move |_, _, window, cx| {
                let answer = window.prompt(
                    PromptLevel::Warning,
                    &format!("Delete {}?", file_name(&file)),
                    Some("The file is removed from the plugins folder. It can’t be undone."),
                    &["Delete", "Cancel"],
                    cx,
                );
                let file = file.clone();
                cx.spawn(async move |this, cx| {
                    if answer.await != Ok(0) {
                        return;
                    }
                    this.update(cx, |this, cx| {
                        if let Err(error) = plugins::delete_file(&file, cx) {
                            log::error!("{error:#}");
                        }
                        this.page = Page::General;
                        cx.notify();
                    })
                    .ok();
                })
                .detach();
            }))
        });
        v_flex()
            .gap(px(18.))
            .child(card)
            .child(h_flex().justify_end().gap(px(8.)).child(copy).children(delete))
            .into_any_element()
    }
}

/// A plugin's header: its logo, name and what it is, and its switch.
pub(super) fn plugin_header(plugin: &Plugin, t: &Theme, cx: &App) -> AnyElement {
    let properties = &plugin.manifest().plugin;
    // The same line as the install window's: who made it, its version, and its id.
    let (author, version) = (delight_ui::ellipsize(&delight_ui::one_line(&properties.author), 100), delight_ui::ellipsize(&delight_ui::one_line(&properties.version), 60));
    let by = if author.is_empty() { format!("Version {version}") } else { format!("{author} · Version {version}") };
    let id = properties.id.clone();
    let switch = Switch::new("plugin-on")
        .checked(settings::get(cx).plugin_on(&id))
        .on_change(move |on, _, cx| settings::update(cx, |settings| settings.set_plugin_on(&id, *on)));
    h_flex()
        .gap(px(14.))
        .child(LogoBadge::new(properties.icon.as_bytes()).size(px(48.)))
        .child(
            v_flex()
                .flex_1()
                .min_w(px(0.))
                // One line each, ending in an ellipsis when there is more.
                .child(div().text_size(px(20.)).font_weight(FontWeight::SEMIBOLD).truncate().child(delight_ui::ellipsize(&delight_ui::one_line(&properties.name), 160)))
                .child(
                    h_flex()
                        .mt(px(2.))
                        .gap(px(6.))
                        .text_size(px(12.))
                        // Each ends in its own ellipsis when the line is too short for both.
                        .child(div().min_w(px(0.)).truncate().text_color(t.text_muted).child(by))
                        .child(div().flex_shrink_0().text_color(t.text_faint).child("·"))
                        .child(div().min_w(px(0.)).truncate().text_color(t.text_faint).child(delight_ui::ellipsize(&properties.id, 128))),
                ),
        )
        .child(switch)
        .into_any_element()
}

/// A broken file's header: a warning, its name, and where it is.
pub(super) fn broken_header(broken: &Broken, t: &Theme) -> AnyElement {
    let place = if broken.source.built_in { "Built-in" } else { "In the plugins folder" };
    h_flex()
        .gap(px(14.))
        .child(
            div()
                .size(px(48.))
                .flex_shrink_0()
                .rounded(px(11.))
                .bg(t.tint(t.warning))
                .flex()
                .items_center()
                .justify_center()
                .child(Icon::new(IconName::TriangleAlert).size(px(26.)).color(t.warning)),
        )
        .child(
            v_flex()
                .flex_1()
                .min_w(px(0.))
                .child(div().text_size(px(20.)).font_weight(FontWeight::SEMIBOLD).truncate().child(file_name(&broken.source.file)))
                .child(div().mt(px(2.)).text_size(px(12.)).text_color(t.text_muted).child(format!("{place} · doesn’t load"))),
        )
        .into_any_element()
}

/// Which of a plugin's permission rows are open: none, until one is clicked.
#[derive(Debug, Default)]
pub(crate) struct OpenPermissions(HashSet<usize>);

impl OpenPermissions {
    pub(crate) fn is_open(&self, index: usize) -> bool {
        self.0.contains(&index)
    }

    pub(crate) fn toggle(&mut self, index: usize) {
        if !self.0.remove(&index) {
            self.0.insert(index);
        }
    }
}

/// What a click on a permission's row does: the view that draws the rows makes one for each.
pub(crate) type Toggle = Box<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// The rows of a Permissions section: one per permission. Each shows an icon, the permission's name
/// and the plugin's own reason in one line; click it (or its chevron) for the reason in full, what
/// the permission allows, and, for a permission that lists things (the programs `Commands` runs),
/// each as a chip. Or one row saying there are none.
pub(crate) fn permission_rows(
    permissions: &[PermissionRequest],
    open: &OpenPermissions,
    toggle: impl Fn(usize) -> Toggle,
    t: &Theme,
) -> Vec<AnyElement> {
    if permissions.is_empty() {
        let check = Icon::new(IconName::CircleCheck).size(px(18.)).color(t.success).into_any_element();
        return vec![item(check, "Needs no permissions".into(), None, None, t)];
    }
    let last = permissions.len() - 1;
    permissions
        .iter()
        .enumerate()
        .map(|(index, request)| permission_row(index, index == last, request, open.is_open(index), toggle(index), t))
        .collect()
}

/// One permission's row: `index` in the card, and whether it is the `last` (the card's corners are
/// its own there).
fn permission_row(index: usize, last: bool, request: &PermissionRequest, open: bool, toggle: Toggle, t: &Theme) -> AnyElement {
    let spec = request.permission.spec();
    let icon = IconName::from_name(spec.icon()).unwrap_or(IconName::Puzzle);
    // Just the icon, in the permission's colour: no tile behind it.
    let tile = div().size(px(20.)).flex_shrink_0().flex().items_center().justify_center().child(Icon::new(icon).size(px(18.)).color(t.warning));
    // The reason: one line when the row is closed, all of it when it is open.
    let why = if request.reason.is_empty() {
        // Built before plugins said why.
        div().mt(px(2.)).text_size(px(12.)).text_color(t.text_faint).child("The plugin doesn’t say why")
    } else {
        // Closed: one line; open: all of it, which is at most a sentence.
        div().mt(px(2.)).text_size(px(12.)).clamp_lines(if open { 4 } else { 1 }).child(request.reason.clone())
    };
    let allows = open.then(|| {
        v_flex()
            .mt(px(8.))
            .gap(px(1.))
            .child(div().text_size(px(10.)).font_weight(FontWeight::BOLD).text_color(t.text_faint).child("ALLOWS"))
            .child(div().text_size(px(12.)).text_color(t.text_muted).clamp_lines(3).child(spec.describe()))
    });
    // Orange like the icon, so what the plugin runs stands out; a step darker in light mode,
    // where the orange itself is too pale for text.
    let ink = if t.dark { t.warning } else { gpui::hsla(t.warning.h, t.warning.s, t.warning.l * 0.72, 1.) };
    let items = spec.items();
    let items = (open && !items.is_empty()).then(|| {
        h_flex().flex_wrap().gap(px(6.)).mt(px(7.)).children(items.into_iter().map(|item| {
            // One line, cut with an ellipsis at the card's width: a program's path can be very long.
            div()
                .max_w_full()
                .truncate()
                .px(px(8.))
                .py(px(2.))
                .rounded(px(10.))
                .bg(t.tint(t.warning))
                .border_1()
                .border_color(t.warning.opacity(0.4))
                .font_family(t.mono_font.clone())
                .text_size(px(11.))
                .font_weight(FontWeight::MEDIUM)
                .text_color(ink)
                .child(delight_ui::ellipsize(&item, 100))
        }))
    });
    let chevron = Icon::new(if open { IconName::ChevronUp } else { IconName::ChevronDown }).size(px(14.)).color(t.text_faint);
    // The icon, the name and the chevron share one line, so they line up; the text under it starts
    // at the name.
    v_flex()
        .id(("permission", index))
        .px(px(14.))
        .py(px(10.))
        // The hover colour follows the card's rounded corners at its top and bottom: GPUI clips to
        // rectangles, so the card can't round it.
        .when(index == 0, |row| row.rounded_t(t.radius))
        .when(last, |row| row.rounded_b(t.radius))
        .cursor_pointer()
        .hover(|row| row.bg(t.fill_subtle()))
        .on_click(toggle)
        .child(
            h_flex()
                .items_center()
                .gap(px(12.))
                .child(tile)
                .child(div().flex_1().min_w(px(0.)).font_weight(FontWeight::SEMIBOLD).truncate().child(spec.title()))
                .child(div().flex_shrink_0().child(chevron)),
        )
        .child(v_flex().pl(px(32.)).child(why).children(allows).children(items))
        .into_any_element()
}

/// The rows of a Tips section: each tip as its author wrote it, beside a lightbulb.
fn tip_rows(tips: &[String], t: &Theme) -> Vec<AnyElement> {
    tips.iter()
        .map(|tip| {
            h_flex()
                .items_start()
                .gap(px(12.))
                .px(px(14.))
                .py(px(10.))
                .child(
                    div()
                        .size(px(24.))
                        .flex_shrink_0()
                        .rounded(px(7.))
                        .bg(t.tint(t.accent))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(Icon::new(IconName::Lightbulb).size(px(14.)).color(t.accent)),
                )
                .child(div().flex_1().min_w(px(0.)).pt(px(3.)).child(tip.clone()))
                .into_any_element()
        })
        .collect()
}

/// For an installed plugin that names its location: the newer version published there with the
/// button that installs it, or else what the last look there found with Check Now; and whether the
/// plugin updates on its own.
fn updates_section(plugin: &Plugin, source: &Source, t: &Theme, cx: &App) -> Option<AnyElement> {
    use plugins::updates::{self, Check, State};
    let properties = &plugin.manifest().plugin;
    if source.built_in || properties.update.is_none() {
        return None;
    }
    let mut rows = Vec::new();
    if let Some(offer) = updates::offer(&properties.id, cx) {
        let title = format!("Version {} is available", delight_ui::ellipsize(&offer.release.version, 60));
        let id = properties.id.clone();
        let row = match &offer.state {
            // How far it has come: a bar, and the share of it once its size is known.
            State::Downloading(fraction) => {
                let done = if *fraction > 0. { format!("Downloading… {}%", (fraction * 100.).round()) } else { "Downloading…".to_string() };
                h_flex()
                    .h(px(delight_ui::ROW_DETAIL_HEIGHT))
                    .items_center()
                    .gap(px(12.))
                    .px(px(14.))
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w(px(0.))
                            .gap(px(6.))
                            .child(title)
                            .child(
                                h_flex()
                                    .gap(px(10.))
                                    .items_center()
                                    .child(div().w(px(180.)).child(delight_ui::progress_bar(*fraction, t.accent, t)))
                                    .child(div().text_size(px(11.)).text_color(t.text_muted).child(done)),
                            ),
                    )
                    .child(Button::new("update-plugin", "Update").primary().disabled(true))
                    .into_any_element()
            }
            state => {
                let (detail, color, label) = match state {
                    State::AsksForMore => ("It asks for new permissions: review them to install it".to_string(), t.warning, "Review Update…"),
                    State::Failed(why) => (delight_ui::ellipsize(&delight_ui::one_line(why), 200), t.error, "Try Again"),
                    _ => ("It keeps its data and settings".to_string(), t.text_muted, "Update"),
                };
                let button = Button::new("update-plugin", label).primary().on_click(move |_, _, cx| updates::update(&id, cx));
                delight_ui::row_with(title, detail, button, color)
            }
        };
        rows.push(row);
    } else {
        let (detail, color) = match updates::last_check(&properties.id, cx) {
            None => ("Checked at launch and once a day".to_string(), t.text_muted),
            Some(Check::Checking) => ("Checking…".to_string(), t.text_muted),
            Some(Check::UpToDate) => ("It is up to date".to_string(), t.success),
            Some(Check::Failed(why)) => (delight_ui::ellipsize(&delight_ui::one_line(&why), 200), t.error),
        };
        let id = properties.id.clone();
        let check = Button::new("check-plugin-update", "Check Now").on_click(move |_, _, cx| updates::check(&id, cx));
        rows.push(delight_ui::row_with("Check for updates", detail, check, color));
    }
    let id = properties.id.clone();
    let switch = Switch::new("update-automatically")
        .checked(settings::get(cx).updates_automatically(&id))
        .on_change(move |on, _, cx| settings::update(cx, |settings| settings.set_updates_automatically(&id, *on)));
    rows.push(delight_ui::row("Update automatically", Some("Updates that ask for no new permissions install on their own"), switch, t));
    Some(section("Updates", rows).into_any_element())
}

/// An error on its tint.
fn notice(text: String, t: &Theme) -> AnyElement {
    h_flex()
        .items_start()
        .gap(px(8.))
        .px(px(12.))
        .py(px(9.))
        .rounded(t.radius)
        .bg(t.tint(t.error))
        .child(div().pt(px(1.)).child(Icon::new(IconName::CircleX).size(px(14.)).color(t.error)))
        .child(div().flex_1().min_w(px(0.)).child(text))
        .into_any_element()
}

pub(crate) fn file_name(file: &Path) -> String {
    file.file_name().map_or_else(|| file.display().to_string(), |name| name.to_string_lossy().into_owned())
}
