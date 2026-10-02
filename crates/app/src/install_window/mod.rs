//! Installing plugins: a window of its own (Settings stays closed) that asks, for each plugin file
//! given, whether to install it. It shows what the plugin is, whether it replaces one, which
//! permissions it asks for and why, and which tools it adds; Install copies it into the plugins folder.
//! Files come from the file picker in Settings, or are dropped on the menu bar icon; ones given while
//! the window is open join the end of its list. Or the window starts on a link to where plugins are
//! published (`link`): it lists them to pick from, downloads the picked ones, and goes on as with
//! files. Nothing is read by running it.
//!
//! One file is looked at at a time. A plugin starts as soon as it is installed, on its own: the
//! other plugins go on as they are.

use std::path::{Path, PathBuf};

use delight_protocol::Manifest;
use delight_ui::{
    ActiveTheme as _, Button, EditorEvent, Group, Icon, IconName, LogoBadge, StyledExt as _, TextEditor, Theme, ellipsize, h_flex, one_line, v_flex,
};
use gpui::{
    AnyElement, App, AppContext as _, Bounds, Context, Entity, FocusHandle, Focusable as _, FontWeight, Global, IntoElement, ParentElement, Render,
    Styled, Subscription, Task, TitlebarOptions, Window, WindowBackgroundAppearance, WindowBounds, WindowHandle, WindowKind, WindowOptions, actions,
    div, prelude::*, px, size,
};

mod link;

use link::{LINK_EXAMPLE, LinkPage};

use crate::plugins;
use crate::settings_window::{ITEM_ICON, OpenPermissions, content_color, file_name, item, permission_rows};

const WIDTH: f32 = 520.;
const HEIGHT: f32 = 540.;
/// The plugin's logo in the header, and the height of the two lines beside it.
const LOGO: f32 = 44.;

/// The key context of the window: ↵ confirms, Esc leaves the plugin being looked at, ⌘W closes.
pub const CONTEXT: &str = "InstallPlugin";

actions!(install_plugin, [Confirm, Cancel, Close]);

struct Open(WindowHandle<InstallWindow>);

impl Global for Open {}

/// Ask whether to install these plugin files: in the install window, opened, or brought forward with
/// the files added to the end of its list.
pub fn open_files(files: Vec<PathBuf>, cx: &mut App) {
    if files.is_empty() {
        return;
    }
    cx.activate(true);
    if let Some(open) = cx.try_global::<Open>().map(|open| open.0) {
        let added = files.clone();
        let joined = open.update(cx, |this, window, cx| {
            this.add(added, cx);
            window.activate_window();
        });
        if joined.is_ok() {
            return;
        }
    }
    open_window(Start::Files(files), cx);
}

/// Ask for a link to where a plugin is published, then whether to install it: in the install window,
/// opened, or brought forward as it is if it is open already.
pub fn open_link(cx: &mut App) {
    cx.activate(true);
    if let Some(open) = cx.try_global::<Open>().map(|open| open.0)
        && open.update(cx, |_, window, _| window.activate_window()).is_ok()
    {
        return;
    }
    open_window(Start::Link, cx);
}

/// What the window starts with.
enum Start {
    Files(Vec<PathBuf>),
    Link,
}

fn open_window(start: Start, cx: &mut App) {
    let title = match start {
        Start::Files(_) => "Install Plugin",
        Start::Link => "Install from a Link",
    };
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(None, size(px(WIDTH), px(HEIGHT)), cx))),
        titlebar: Some(TitlebarOptions { title: Some(title.into()), ..Default::default() }),
        focus: true,
        show: true,
        kind: WindowKind::Normal,
        is_resizable: false,
        is_minimizable: false,
        window_background: WindowBackgroundAppearance::Opaque,
        ..Default::default()
    };
    match cx.open_window(options, |window, cx| cx.new(|cx| InstallWindow::new(start, window, cx))) {
        Ok(handle) => {
            handle
                .update(cx, |this, window, cx| match &this.link {
                    Some(link) => window.focus(&link.focus_handle(cx), cx),
                    None => window.focus(&this.focus, cx),
                })
                .ok();
            cx.set_global(Open(handle));
        }
        Err(error) => log::error!("opening the install window: {error:#}"),
    }
}

/// What the window shows: the link's page, then the file it is on.
enum Step {
    /// Installing from a link: its field, and what is at it.
    Link(Box<LinkPage>),
    /// Reading the plugin's manifest.
    Reading,
    /// What it is and what it adds, waiting for Install or Cancel.
    Ask(Box<Manifest>),
    /// Why it isn't installed, waiting for OK.
    Failed(String),
}

struct InstallWindow {
    focus: FocusHandle,
    /// The files of this batch, in the order given. Files are only ever added at the end, so what the
    /// window said ("2 of 3") stays true of the ones before.
    files: Vec<PathBuf>,
    /// The file being looked at: its index in `files`.
    at: usize,
    step: Step,
    /// Which of the permission rows are open (none, at first).
    open: OpenPermissions,
    /// The link's field, when the window started by asking for one.
    link: Option<Entity<TextEditor>>,
    /// Reading the current file; replacing it drops the old read.
    _reading: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl InstallWindow {
    fn new(start: Start, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut subscriptions = vec![cx.observe_window_appearance(window, |_, _, cx| crate::theme::appearance_changed(cx))];
        let (files, link) = match start {
            Start::Files(files) => (files, None),
            Start::Link => {
                let link = cx.new(|cx| TextEditor::new(window, cx).placeholder(LINK_EXAMPLE));
                // A link pasted or typed is looked at once it settles.
                subscriptions.push(cx.subscribe(&link, |this, _, event: &EditorEvent, cx| {
                    if matches!(event, EditorEvent::Changed) {
                        this.link_changed(cx);
                    }
                }));
                (Vec::new(), Some(link))
            }
        };
        let mut this = Self {
            focus: cx.focus_handle(),
            files,
            at: 0,
            step: Step::Link(Box::default()),
            open: OpenPermissions::default(),
            link,
            _reading: None,
            _subscriptions: subscriptions,
        };
        if this.link.is_none() {
            this.read(cx);
        }
        this
    }

    /// More files for the end of the list; the first of them is looked at now if the window had
    /// none (it was asking for a link).
    fn add(&mut self, files: Vec<PathBuf>, cx: &mut Context<Self>) {
        let idle = self.files.is_empty();
        self.files.extend(files);
        if idle && !self.files.is_empty() {
            self.at = 0;
            self.read(cx);
        }
        cx.notify();
    }

    fn file(&self) -> &Path {
        &self.files[self.at]
    }

    /// Read the manifest of the current file.
    fn read(&mut self, cx: &mut Context<Self>) {
        self.step = Step::Reading;
        self.open = OpenPermissions::default();
        let file = self.file().to_path_buf();
        self._reading = Some(cx.spawn(async move |this, cx| {
            let Ok(read) = this.update(cx, |_, cx| plugins::inspect(file.clone(), cx)) else { return };
            let read = read.await;
            this.update(cx, |this, cx| {
                this.step = match read {
                    Ok(manifest) => Step::Ask(Box::new(manifest)),
                    Err(error) => Step::Failed(format!("{} isn’t a plugin this Delight can run.\n\n{error:#}", file_name(&file))),
                };
                cx.notify();
            })
            .ok();
        }));
        cx.notify();
    }

    /// Leave the current file: the next one, or done.
    fn next(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.at + 1 < self.files.len() {
            self.at += 1;
            self.read(cx);
        } else {
            window.remove_window();
        }
    }

    fn install(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Step::Ask(manifest) = &self.step else { return };
        match plugins::install(self.file(), manifest, cx) {
            Ok(()) => self.next(window, cx),
            Err(error) => {
                self.step = Step::Failed(format!("{} couldn’t be installed.\n\n{error:#}", file_name(self.file())));
                cx.notify();
            }
        }
    }

    /// ↵: Download or Review the plugins at the link, Install this one, or OK.
    fn confirm(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.step {
            Step::Link(_) => self.link_next(window, cx),
            Step::Ask(_) => self.install(window, cx),
            Step::Failed(_) => self.next(window, cx),
            Step::Reading => {}
        }
    }

    /// Esc, Cancel or OK: not this one. On the link's page, it closes the window.
    fn cancel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.step {
            Step::Link(_) => window.remove_window(),
            Step::Reading => {}
            Step::Ask(_) | Step::Failed(_) => self.next(window, cx),
        }
    }

    fn position(&self) -> String {
        position_label(self.at, self.files.len())
    }
}

/// "2 of 3" for the file at index 1 of 3; nothing when there is only one file.
fn position_label(at: usize, total: usize) -> String {
    if total <= 1 { String::new() } else { format!("{} of {total}", at + 1) }
}

impl Render for InstallWindow {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        let page = match &self.step {
            Step::Link(page) => self.link_page(page, &t, cx),
            Step::Reading => self.reading(&t),
            Step::Ask(manifest) => self.ask(manifest, &t, cx),
            Step::Failed(why) => self.failed(why, &t, cx),
        };
        v_flex()
            .key_context(CONTEXT)
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &Confirm, window, cx| this.confirm(window, cx)))
            .on_action(cx.listener(|this, _: &Cancel, window, cx| this.cancel(window, cx)))
            .on_action(cx.listener(|_, _: &Close, window, _| window.remove_window()))
            .size_full()
            .bg(content_color(&t))
            .font_family(t.font.clone())
            .text_color(t.text)
            .text_size(t.text_size)
            .child(page)
    }
}

impl InstallWindow {
    fn reading(&self, t: &Theme) -> AnyElement {
        div().size_full().flex().items_center().justify_center().text_color(t.text_muted).child(format!("Reading {}…", file_name(self.file()))).into_any_element()
    }

    /// The plugin: its header, whether it replaces one, its permissions and its tools; Install and Cancel.
    fn ask(&self, manifest: &Manifest, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let properties = &manifest.plugin;
        let operations = &manifest.operations;
        // What it replaces, if it has the id of a plugin there is.
        let replaces = plugins::all(cx)
            .iter()
            .zip(plugins::sources(cx).iter())
            .find(|(plugin, _)| plugin.manifest().plugin.id == properties.id)
            .map(|(plugin, source)| {
                let kind = if source.built_in { "built-in" } else { "installed" };
                let old = &plugin.manifest().plugin;
                format!("Replaces the {kind} “{}” {}.", old.name, old.version)
            });
        let (author, version) = (ellipsize(&one_line(&properties.author), 100), ellipsize(&one_line(&properties.version), 60));
        let by = if author.is_empty() { format!("Version {version}") } else { format!("{author} · Version {version}") };
        let header = h_flex()
            .flex_shrink_0()
            .gap(px(12.))
            .items_center()
            .child(LogoBadge::new(properties.icon.as_bytes()).size(px(LOGO)))
            // Two lines whose heights add up to the logo's (26 + 18), so text and logo start and end
            // together.
            .child(
                v_flex()
                    .flex_1()
                    .min_w(px(0.))
                    .h(px(LOGO))
                    .justify_center()
                    .child(div().h(px(26.)).line_height(px(26.)).text_size(px(16.)).font_weight(FontWeight::SEMIBOLD).truncate().child(ellipsize(&one_line(&properties.name), 160)))
                    .child(
                        h_flex()
                            .h(px(18.))
                            .items_center()
                            .gap(px(6.))
                            .text_size(px(11.5))
                            // Each ends in its own ellipsis when the line is too short for both.
                            .child(div().min_w(px(0.)).truncate().text_color(t.text_muted).child(by))
                            .child(div().flex_shrink_0().text_color(t.text_faint).child("·"))
                            .child(div().min_w(px(0.)).truncate().text_color(t.text_faint).child(properties.id.clone())),
                    ),
            );
        let notice = replaces.map(|text| {
            h_flex()
                .flex_shrink_0()
                .gap(px(8.))
                .items_center()
                .px(px(12.))
                .py(px(8.))
                .rounded(px(8.))
                .bg(t.tint(t.warning))
                .text_size(t.text_size_small())
                .child(Icon::new(IconName::Info).size(px(14.)).color(t.warning))
                .child(div().flex_1().min_w(px(0.)).clamp_lines(2).child(ellipsize(&text, 240)))
        });
        let tools = operations
            .iter()
            .map(|operation| {
                let icon = operation.icon.as_deref().unwrap_or(&properties.icon);
                item(LogoBadge::new(icon.as_bytes()).size(px(ITEM_ICON)).into_any_element(), operation.title.clone().into(), Some(operation.description.clone().into()), None, t)
            })
            .collect();
        // Scrolls when it is more than fits between the header and the buttons: the notice too.
        let details = div().id("install-details").flex_1().min_h(px(0.)).overflow_y_scroll().child(
            v_flex()
                .gap(px(14.))
                .children(notice)
                .when(!properties.description.is_empty(), |details| details.child(div().text_color(t.text_muted).clamp_lines(4).child(ellipsize(&properties.description, 700))))
                .child(counted("Permissions", properties.permissions.len(), self.permissions(&properties.permissions, t, cx), t))
                .child(counted("Tools", operations.len(), tools, t)),
        );
        let position = self.position();
        let footer = h_flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(8.))
            .child(div().flex_1().text_size(t.text_size_small()).text_color(t.text_muted).child(position))
            .child(Button::new("cancel-install", "Cancel").on_click(cx.listener(|this, _, window, cx| this.cancel(window, cx))))
            .child(Button::new("install", "Install").primary().on_click(cx.listener(|this, _, window, cx| this.install(window, cx))));
        v_flex().size_full().p(px(20.)).gap(px(12.)).child(header).child(details).child(footer).into_any_element()
    }

    /// The permission rows, each opening and closing on a click.
    fn permissions(&self, permissions: &[delight_protocol::PermissionRequest], t: &Theme, cx: &mut Context<Self>) -> Vec<AnyElement> {
        permission_rows(
            permissions,
            &self.open,
            |index| {
                Box::new(cx.listener(move |this, _: &gpui::ClickEvent, _, cx| {
                    this.open.toggle(index);
                    cx.notify();
                }))
            },
            t,
        )
    }

    /// Why this file isn't installed, with OK.
    fn failed(&self, why: &str, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let position = self.position();
        v_flex()
            .size_full()
            .p(px(20.))
            .gap(px(12.))
            .child(
                h_flex()
                    .flex_shrink_0()
                    .gap(px(10.))
                    .items_center()
                    .child(Icon::new(IconName::Info).size(px(22.)).color(t.error))
                    .child(div().text_size(px(18.)).font_weight(FontWeight::SEMIBOLD).child("Not installed")),
            )
            .child(div().id("install-failure").flex_1().min_h(px(0.)).overflow_y_scroll().text_color(t.text_muted).child(why.to_string()))
            .child(
                h_flex()
                    .flex_shrink_0()
                    .items_center()
                    .child(div().flex_1().text_size(t.text_size_small()).text_color(t.text_muted).child(position))
                    .child(Button::new("ok", "OK").primary().on_click(cx.listener(|this, _, window, cx| this.cancel(window, cx)))),
            )
            .into_any_element()
    }
}

/// A section titled with its name and, when there are any, how many it has.
fn counted(title: &'static str, count: usize, rows: Vec<AnyElement>, t: &Theme) -> impl IntoElement {
    let badge = (count > 0).then(|| {
        div()
            .min_w(px(18.))
            .h(px(18.))
            .px(px(6.))
            .rounded(px(9.))
            .bg(t.fill)
            .flex()
            .items_center()
            .justify_center()
            .text_size(px(11.))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(t.text_muted)
            .child(count.to_string())
    });
    v_flex()
        .gap(px(6.))
        .child(h_flex().gap(px(6.)).px(px(2.)).text_size(px(12.)).font_weight(FontWeight::SEMIBOLD).child(title).children(badge))
        .child(Group::new().children(rows))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_label_counts_the_files_of_the_batch_and_says_nothing_for_one() {
        assert_eq!(position_label(0, 1), "");
        assert_eq!(position_label(1, 3), "2 of 3");
        // Files added at the end keep the place: it was 2 of 3, and is 2 of 5.
        assert_eq!(position_label(1, 5), "2 of 5");
    }
}
