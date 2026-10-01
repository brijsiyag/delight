//! Installing from a link, on one page: the link's field, then, as soon as a link is pasted, the
//! plugins at it, each with a checkbox. Download fetches the picked ones, each with its own progress
//! bar; once every picked plugin is downloaded, the button becomes Review…, which shows each one as
//! a picked file is shown. Picking another plugin after a download brings Download back, for it alone.

use std::path::PathBuf;
use std::time::Duration;

use delight_runtime::updates::{self, Link, Release, is_newer};
use delight_ui::{Button, Caption, Disableable as _, Group, Icon, IconName, StyledExt as _, Theme, ellipsize, h_flex, one_line, progress_bar, v_flex};
use futures::StreamExt as _;
use gpui::{AnyElement, Context, FontWeight, Hsla, IntoElement, ParentElement, Styled, Task, Window, div, prelude::*, px};

use super::{InstallWindow, LOGO, Step};
use crate::plugins;

/// How long the link has to stay as it is before it is looked at: a paste is one change, typing many.
const SETTLE: Duration = Duration::from_millis(300);
/// What the field shows while it is empty: the kind of link most people paste.
pub(super) const LINK_EXAMPLE: &str = "https://github.com/Meesho/delight-plugins/releases/latest/download/plugins.xml";

/// The link's page: what is at the link, as far as it has been looked at.
#[derive(Default)]
pub(super) struct LinkPage {
    found: Found,
    /// Looking at the link; replacing it (the link changed) drops the old look.
    _looking: Option<Task<()>>,
    /// Downloading the picked plugins, one after another; replacing it drops the downloads.
    _downloading: Option<Task<()>>,
}

#[derive(Default)]
enum Found {
    /// No link yet.
    #[default]
    Empty,
    /// Reading what the link points at.
    Looking,
    /// Why there is nothing to list.
    Problem(String),
    /// The plugins at the link, and where they are.
    Plugins { location: String, choices: Vec<Choice> },
}

/// One plugin at the link.
struct Choice {
    release: Release,
    /// The version installed now, if it is.
    installed: Option<String>,
    picked: bool,
    download: Download,
}

impl Choice {
    /// New, or newer than the one installed: the same or an older version can't be picked.
    fn pickable(&self) -> bool {
        self.installed.as_ref().is_none_or(|installed| is_newer(&self.release.version, installed))
    }
}

enum Download {
    Not,
    /// How much of it has come, 0 to 1 (0 while its size isn't known).
    Going(f32),
    Done(PathBuf),
    Failed(String),
}

/// What a download says while it runs.
enum Moved {
    Progress(u64, Option<u64>),
    Finished(Result<PathBuf, String>),
}

/// What the page's main button does now.
enum Next {
    /// Nothing picked, or nothing there: it is shown, and does nothing.
    Nothing,
    Download,
    Downloading,
    /// Every picked plugin is downloaded: review each, this many, to install it or not.
    Review(usize),
}

impl InstallWindow {
    /// The link changed: look at it once it settles. Downloads of the old link's plugins stop.
    pub(super) fn link_changed(&mut self, cx: &mut Context<Self>) {
        let Some(field) = &self.link else { return };
        let text = field.read(cx).text().trim().to_string();
        let Step::Link(page) = &mut self.step else { return };
        page._downloading = None;
        if text.is_empty() {
            page.found = Found::Empty;
            page._looking = None;
            cx.notify();
            return;
        }
        page._looking = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SETTLE).await;
            let link = match Link::parse(&text) {
                Ok(link) => link,
                Err(error) => {
                    this.update(cx, |this, cx| this.set_found(Found::Problem(format!("{error:#}")), cx)).ok();
                    return;
                }
            };
            this.update(cx, |this, cx| this.set_found(Found::Looking, cx)).ok();
            let location = link.location.clone();
            let fetched = cx.background_spawn(async move { updates::fetch_link(&link) }).await;
            this.update(cx, |this, cx| {
                let found = match fetched {
                    Ok(releases) if releases.is_empty() => Found::Problem("There are no plugins at this link.".into()),
                    Ok(releases) => Found::Plugins { location, choices: choices(releases, cx) },
                    Err(error) => Found::Problem(format!("{error:#}")),
                };
                this.set_found(found, cx);
            })
            .ok();
        }));
    }

    fn set_found(&mut self, found: Found, cx: &mut Context<Self>) {
        if let Step::Link(page) = &mut self.step {
            page.found = found;
            cx.notify();
        }
    }

    /// The plugins at the link, if they are listed, with where they are.
    fn listed(&mut self) -> Option<(&mut LinkPage, String)> {
        let Step::Link(page) = &mut self.step else { return None };
        let Found::Plugins { location, .. } = &page.found else { return None };
        let location = location.clone();
        Some((page, location))
    }

    fn choices_mut(&mut self) -> Option<&mut Vec<Choice>> {
        let Step::Link(page) = &mut self.step else { return None };
        let Found::Plugins { choices, .. } = &mut page.found else { return None };
        Some(choices)
    }

    /// Pick the plugin at `index`, or stop picking it.
    fn toggle(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(choice) = self.choices_mut().and_then(|choices| choices.get_mut(index)).filter(|choice| choice.pickable()) {
            choice.picked = !choice.picked;
            cx.notify();
        }
    }

    /// Pick every plugin that can be, or none if they all are.
    fn pick_all(&mut self, cx: &mut Context<Self>) {
        let Some(choices) = self.choices_mut() else { return };
        let all = choices.iter().filter(|choice| choice.pickable()).all(|choice| choice.picked);
        for choice in choices.iter_mut().filter(|choice| choice.pickable()) {
            choice.picked = !all;
        }
        cx.notify();
    }

    /// ↵, or the page's main button: download what is picked, or review what is downloaded.
    pub(super) fn link_next(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.next_step() {
            Next::Download => self.download_picked(cx),
            Next::Review(_) => self.install_downloaded(window, cx),
            Next::Nothing | Next::Downloading => {}
        }
    }

    fn next_step(&self) -> Next {
        let Step::Link(page) = &self.step else { return Next::Nothing };
        let Found::Plugins { choices, .. } = &page.found else { return Next::Nothing };
        let picked: Vec<&Choice> = choices.iter().filter(|choice| choice.picked).collect();
        if choices.iter().any(|choice| matches!(choice.download, Download::Going(_))) {
            Next::Downloading
        } else if picked.is_empty() {
            Next::Nothing
        } else if picked.iter().all(|choice| matches!(choice.download, Download::Done(_))) {
            Next::Review(picked.len())
        } else {
            Next::Download
        }
    }

    /// Download the picked plugins that aren't yet, one after another, each telling its row how far
    /// it has come.
    fn download_picked(&mut self, cx: &mut Context<Self>) {
        let Some((page, location)) = self.listed() else { return };
        let Found::Plugins { choices, .. } = &mut page.found else { return };
        let wanted: Vec<(usize, Release)> = choices
            .iter_mut()
            .enumerate()
            .filter(|(_, choice)| choice.picked && !matches!(choice.download, Download::Done(_)))
            .map(|(index, choice)| {
                choice.download = Download::Going(0.);
                (index, choice.release.clone())
            })
            .collect();
        if wanted.is_empty() {
            return;
        }
        page._downloading = Some(cx.spawn(async move |this, cx| {
            for (index, release) in wanted {
                let (sender, mut moved) = futures::channel::mpsc::unbounded();
                let location = location.clone();
                let _worker = cx.background_spawn(async move {
                    let progress = sender.clone();
                    let file = plugins::download(&location, &release, move |done, total| {
                        progress.unbounded_send(Moved::Progress(done, total)).ok();
                    });
                    sender.unbounded_send(Moved::Finished(file.map_err(|error| format!("{error:#}")))).ok();
                });
                while let Some(message) = moved.next().await {
                    let finished = matches!(message, Moved::Finished(_));
                    this.update(cx, |this, cx| this.download_moved(index, message, cx)).ok();
                    if finished {
                        break;
                    }
                }
            }
        }));
        cx.notify();
    }

    fn download_moved(&mut self, index: usize, moved: Moved, cx: &mut Context<Self>) {
        let Some(choice) = self.choices_mut().and_then(|choices| choices.get_mut(index)) else { return };
        choice.download = match moved {
            Moved::Progress(done, total) => Download::Going(total.filter(|total| *total > 0).map_or(0., |total| (done as f32 / total as f32).min(1.))),
            Moved::Finished(Ok(file)) => Download::Done(file),
            Moved::Finished(Err(why)) => Download::Failed(why),
        };
        cx.notify();
    }

    /// Every picked plugin is downloaded: look at each as at a picked file, then install it.
    fn install_downloaded(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(choices) = self.choices_mut() else { return };
        let files: Vec<PathBuf> = choices
            .iter()
            .filter(|choice| choice.picked)
            .filter_map(|choice| match &choice.download {
                Download::Done(file) => Some(file.clone()),
                _ => None,
            })
            .collect();
        if files.is_empty() {
            return;
        }
        self.files = files;
        self.at = 0;
        self.read(cx);
        // The field is gone: the keys go to the window, where ↵ installs.
        window.focus(&self.focus, cx);
    }

    /// The page: a header saying what it is for, the link's field, then what is at the link; the
    /// footer says how many are picked, with Cancel, and Download or Review….
    pub(super) fn link_page(&self, page: &LinkPage, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let problem = matches!(page.found, Found::Problem(_));
        let header = h_flex()
            .flex_shrink_0()
            .gap(px(12.))
            .items_center()
            .child(
                div()
                    .flex_shrink_0()
                    .size(px(LOGO))
                    .rounded(px(LOGO * 0.22))
                    .bg(t.tint(t.accent))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(Icon::new(IconName::Globe).size(px(22.)).color(t.accent)),
            )
            // As a plugin's header: two lines as tall as the tile (26 + 18).
            .child(
                v_flex()
                    .flex_1()
                    .min_w(px(0.))
                    .h(px(LOGO))
                    .justify_center()
                    .child(div().h(px(26.)).line_height(px(26.)).text_size(px(16.)).font_weight(FontWeight::SEMIBOLD).child("Install from a Link"))
                    .child(
                        div()
                            .h(px(18.))
                            .line_height(px(18.))
                            .text_size(px(11.5))
                            .text_color(t.text_muted)
                            .truncate()
                            .child("Plugins published on GitHub, or at any web address"),
                    ),
            );
        // As the Settings sidebar's search field.
        let field = h_flex()
            .flex_shrink_0()
            .h(px(32.))
            .px(px(10.))
            .rounded(px(7.))
            .bg(t.surface)
            .border_1()
            .border_color(if problem { t.error } else { t.border })
            .text_size(px(12.5))
            .child(div().flex_1().min_w(px(0.)).children(self.link.clone()));
        let body = match &page.found {
            Found::Empty => state_card(IconName::Puzzle, t.text_muted, "Paste a link to see its plugins", None, t),
            Found::Looking => state_card(IconName::RefreshCw, t.accent, "Looking at the link…", None, t),
            Found::Problem(why) => state_card(IconName::TriangleAlert, t.error, "Couldn’t use this link", Some(why), t),
            Found::Plugins { choices, .. } => self.plugin_list(choices, t, cx),
        };
        let (label, enabled) = match self.next_step() {
            Next::Nothing => ("Download".to_string(), false),
            Next::Download => ("Download".to_string(), true),
            Next::Downloading => ("Downloading…".to_string(), false),
            // Not "Install": each one is shown with what it asks for, and installed only on its Install.
            Next::Review(1) => ("Review Plugin…".to_string(), true),
            Next::Review(count) => (format!("Review {count} Plugins…"), true),
        };
        let picked = match &page.found {
            Found::Plugins { choices, .. } => match choices.iter().filter(|choice| choice.picked).count() {
                0 => "None selected".to_string(),
                count => format!("{count} selected"),
            },
            _ => String::new(),
        };
        // As the plugin's page: what the buttons are about on the left, then the buttons.
        let footer = h_flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(8.))
            .child(div().flex_1().text_size(t.text_size_small()).text_color(t.text_muted).child(picked))
            .child(Button::new("cancel-link", "Cancel").on_click(cx.listener(|this, _, window, cx| this.cancel(window, cx))))
            .child(Button::new("link-next", label).primary().disabled(!enabled).on_click(cx.listener(|this, _, window, cx| this.link_next(window, cx))));
        v_flex().size_full().p(px(20.)).gap(px(14.)).child(header).child(field).child(body).child(footer).into_any_element()
    }

    /// How many plugins there are, Select All, and a card with a row for each.
    fn plugin_list(&self, choices: &[Choice], t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let pickable: Vec<&Choice> = choices.iter().filter(|choice| choice.pickable()).collect();
        let all = !pickable.is_empty() && pickable.iter().all(|choice| choice.picked);
        let count = if choices.len() == 1 { "1 Plugin".to_string() } else { format!("{} Plugins", choices.len()) };
        let caption = h_flex()
            .flex_shrink_0()
            .px(px(4.))
            .items_center()
            .child(div().flex_1().child(Caption::new(count)))
            .when(pickable.len() > 1, |caption| {
                caption.child(Button::new("pick-all", if all { "Select None" } else { "Select All" }).text().on_click(cx.listener(|this, _, _, cx| this.pick_all(cx))))
            });
        let last = choices.len().saturating_sub(1);
        let rows: Vec<AnyElement> = choices.iter().enumerate().map(|(index, choice)| choice_row(index, index == last, choice, t, cx)).collect();
        v_flex()
            .flex_1()
            .min_h(px(0.))
            .gap(px(6.))
            .child(caption)
            .child(div().id("plugins-at-link").flex_1().min_h(px(0.)).overflow_y_scroll().child(Group::new().children(rows)))
            .into_any_element()
    }
}

/// Each plugin at the link, with the version installed of it, if any; new and newer ones picked.
fn choices(releases: Vec<Release>, cx: &mut Context<InstallWindow>) -> Vec<Choice> {
    let running = plugins::all(cx);
    releases
        .into_iter()
        .map(|release| {
            let installed = running.iter().find(|plugin| plugin.manifest().plugin.id == release.id).map(|plugin| plugin.manifest().plugin.version.clone());
            let mut choice = Choice { release, installed, picked: false, download: Download::Not };
            choice.picked = choice.pickable();
            choice
        })
        .collect()
}

/// What fills the page while there is no list: an icon in a soft circle of `color`, a line, and
/// maybe why, on a card.
fn state_card(icon: IconName, color: Hsla, title: &str, detail: Option<&str>, t: &Theme) -> AnyElement {
    v_flex()
        .flex_1()
        .min_h(px(0.))
        .overflow_hidden()
        .rounded(t.radius)
        .bg(t.card())
        .items_center()
        .justify_center()
        .gap(px(10.))
        .px(px(32.))
        .child(div().size(px(40.)).rounded_full().bg(t.tint(color)).flex().items_center().justify_center().child(Icon::new(icon).size(px(20.)).color(color)))
        .child(div().font_weight(FontWeight::MEDIUM).text_color(if detail.is_some() { t.text } else { t.text_muted }).child(title.to_string()))
        .children(detail.map(|detail| div().text_size(t.text_size_small()).text_color(t.text_muted).text_center().clamp_lines(6).child(ellipsize(detail, 600))))
        .into_any_element()
}

/// The height of a row's first line, the plugin's name: its checkbox and status are centred on it.
const NAME_LINE: f32 = 18.;

/// A small rounded label: "Update", "Installed".
fn pill(text: &'static str, color: Hsla, background: Hsla) -> impl IntoElement {
    div().flex_shrink_0().h(px(18.)).px(px(7.)).rounded(px(9.)).bg(background).flex().items_center().text_size(px(11.)).font_weight(FontWeight::SEMIBOLD).text_color(color).child(text)
}

/// One plugin at the link, as a line of a checklist (a link carries no logos, so no row has one): its
/// checkbox, its name with its version and what it does under them, and on the right whether it is
/// an update or installed (its percentage while it downloads, a red cross if that failed), with a
/// bar under the text while it downloads, green once it has. The checkbox and
/// the status line up with the name. The first and last rows round their hover at the card's corners.
fn choice_row(index: usize, last: bool, choice: &Choice, t: &Theme, cx: &mut Context<InstallWindow>) -> AnyElement {
    let release = &choice.release;
    let pickable = choice.pickable();
    let checkbox = div()
        .size(px(16.))
        .flex_shrink_0()
        .rounded(px(4.))
        .flex()
        .items_center()
        .justify_center()
        .when(choice.picked, |checkbox| checkbox.bg(t.accent).child(Icon::new(IconName::Check).size(px(12.)).color(t.accent_text)))
        .when(!choice.picked, |checkbox| checkbox.bg(t.surface).border_1().border_color(t.border));
    let version = match &choice.installed {
        Some(installed) if pickable => format!("{} → {}", ellipsize(&one_line(installed), 30), ellipsize(&one_line(&release.version), 30)),
        _ => ellipsize(&one_line(&release.version), 30),
    };
    let status = match (&choice.download, &choice.installed) {
        (Download::Going(fraction), _) => {
            Some(div().text_size(px(11.5)).text_color(t.text_muted).child(format!("{}%", (fraction * 100.).round() as u32)).into_any_element())
        }
        (Download::Failed(_), _) => Some(Icon::new(IconName::CircleX).size(px(16.)).color(t.error).into_any_element()),
        // Downloaded: its bar turns green, and the row says what it said before.
        (Download::Not | Download::Done(_), Some(_)) if pickable => Some(pill("Update", t.accent, t.tint(t.accent)).into_any_element()),
        (Download::Not | Download::Done(_), Some(_)) => Some(pill("Installed", t.text_muted, t.fill).into_any_element()),
        (Download::Not | Download::Done(_), None) => None,
    };
    let progress = match &choice.download {
        Download::Going(fraction) => Some(div().pt(px(6.)).child(progress_bar(*fraction, t.accent, t))),
        Download::Done(_) => Some(div().pt(px(6.)).child(progress_bar(1., t.success, t))),
        Download::Not | Download::Failed(_) => None,
    };
    let failed = match &choice.download {
        Download::Failed(why) => Some(div().pt(px(4.)).text_size(px(11.5)).text_color(t.error).clamp_lines(2).child(ellipsize(&one_line(why), 300))),
        _ => None,
    };
    let hover = t.fill_subtle();
    h_flex()
        .id(("choice", index))
        .items_start()
        .gap(px(10.))
        .px(px(14.))
        .py(px(10.))
        .when(index == 0, |row| row.rounded_t(t.radius))
        .when(last, |row| row.rounded_b(t.radius))
        .when(pickable, |row| row.cursor_pointer().hover(move |style| style.bg(hover)).on_click(cx.listener(move |this, _, _, cx| this.toggle(index, cx))))
        .when(!pickable, |row| row.opacity(0.6))
        .child(h_flex().flex_shrink_0().h(px(NAME_LINE)).items_center().child(checkbox))
        .child(
            v_flex()
                .flex_1()
                .min_w(px(0.))
                .child(
                    h_flex()
                        .gap(px(6.))
                        .items_center()
                        .child(div().min_w(px(0.)).line_height(px(NAME_LINE)).font_weight(FontWeight::SEMIBOLD).truncate().child(ellipsize(&one_line(release.title()), 100)))
                        .child(div().flex_shrink_0().text_size(px(11.5)).text_color(t.text_faint).child(version)),
                )
                .when(!release.description.trim().is_empty(), |text| {
                    text.child(div().mt(px(2.)).text_size(px(12.)).line_height(px(16.)).text_color(t.text_muted).clamp_lines(2).child(ellipsize(&release.description, 300)))
                })
                .children(progress)
                .children(failed),
        )
        .child(h_flex().flex_shrink_0().h(px(NAME_LINE)).items_center().children(status))
        .into_any_element()
}
