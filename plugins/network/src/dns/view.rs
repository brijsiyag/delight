//! The tool: what went wrong (if anything); every record, grouped by type under a heading, which is
//! what scrolls; and at the bottom who answered. Looked up once typing pauses. A click on a record
//! copies it.

use std::collections::HashSet;
use std::time::Duration;

use delight_plugin_api::{Action, Actions, Input, Shortcut, Tool, host};
use delight_ui::{ActiveTheme, Theme, ellipsize, h_flex, one_line, v_flex};
use gpui::{
    AnyElement, App, ClipboardItem, Context, FontWeight, Hsla, InteractiveElement, IntoElement, ParentElement, Render,
    ScrollHandle, StatefulInteractiveElement, Styled, Task, Window, div, px,
};

use crate::NetworkOperation;
use super::lookup::{Level, Notice, Report, Row, Target, target};
use super::query;

/// A lookup asks a server several times: wait for typing to pause.
const DELAY: Duration = Duration::from_millis(400);

#[derive(Default)]
pub struct DnsView {
    target: Option<Target>,
    report: Option<Report>,
    /// Where the pane is scrolled. Kept here, not in the window's element state, which goes while
    /// the tool is hidden: the tool comes back where it was.
    scroll: ScrollHandle,
    /// Replacing it cancels the lookup in progress.
    _task: Option<Task<()>>,
}

/// The footer's actions: each copies what it names.
#[derive(Actions)]
pub enum DnsAction {
    Addresses,
    Names,
    DigCommand,
}

impl Tool for DnsView {
    type Action = DnsAction;

    fn on_input_changed(&mut self, input: &Input, cx: &mut Context<Self>) {
        let wanted = target(&input.text).map(|(target, _)| target);
        if wanted == self.target {
            return;
        }
        self.target = wanted.clone();
        self.report = None;
        let typed = input.text.trim().to_string();
        self._task = wanted.map(|wanted| {
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(DELAY).await;
                let report = query::lookup(wanted, cx).await;
                this.update(cx, |this, cx| {
                    // A lookup that found something is worth coming back to.
                    if report.found() {
                        host(cx).remember_input(NetworkOperation::Lookup, typed, cx);
                    }
                    this.report = Some(report);
                    cx.notify();
                })
                .ok();
            })
        });
        cx.notify();
    }

    fn list_actions(&self, _: &App) -> Vec<Action<DnsAction>> {
        let Some(report) = &self.report else {
            return Vec::new();
        };
        let mut actions = Vec::new();
        if report.addresses.is_some() {
            actions.push(Action::new(DnsAction::Addresses, "Copy addresses", Shortcut::Enter));
        }
        if report.names.is_some() {
            actions.push(Action::new(DnsAction::Names, "Copy names", Shortcut::Enter));
        }
        if report.dig_command.is_some() {
            actions.push(Action::new(DnsAction::DigCommand, "Copy dig command", Shortcut::CmdEnter));
        }
        actions
    }

    fn perform_action(&mut self, action: DnsAction, cx: &mut Context<Self>) {
        let Some(report) = &self.report else { return };
        let (what, text) = match action {
            DnsAction::Addresses => ("Addresses", report.addresses.clone()),
            DnsAction::Names => ("Names", report.names.clone()),
            DnsAction::DigCommand => ("dig command", report.dig_command.clone()),
        };
        let Some(text) = text else { return };
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        host(cx).toast(format!("{what} copied"), cx);
    }
}

impl Render for DnsView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        let pane = v_flex().items_stretch().size_full().gap(px(12.));
        let Some(report) = &self.report else {
            let looking = self.target.as_ref().map(|target| match target {
                Target::Host(host) => format!("Looking up {host}…"),
                Target::Ip(ip) => format!("Looking up {ip}…"),
            });
            return pane.children(looking.map(|text| div().text_color(t.text_faint).child(text)));
        };
        let notices = report.notices.iter().map(|notice| notice_view(notice, &t));
        // Only the records scroll: who answered stays at the bottom.
        let records = (!report.rows.is_empty()).then(|| {
            let known = known_names(self.target.as_ref(), &report.rows);
            div()
                .id("dns-records")
                .flex_1()
                .min_h(px(0.))
                .overflow_y_scroll()
                .track_scroll(&self.scroll)
                .child(
                    v_flex()
                        .items_stretch()
                        .flex_shrink_0()
                        .gap(px(16.))
                        .pb(px(4.))
                        .children(grouped(&report.rows).into_iter().map(|(kind, rows)| group_view(kind, &rows, &known, &t))),
                )
        });
        let via = report.via.clone().map(|via| via_view(via, report, &t));
        pane.children(notices).children(records).children(via)
    }
}

/// Copy `value`, and say so in the footer.
fn copy(value: &str, cx: &mut App) {
    cx.write_to_clipboard(ClipboardItem::new_string(value.to_string()));
    host(cx).toast(format!("{} copied", ellipsize(&one_line(value), 48)), cx);
}

fn notice_view(notice: &Notice, t: &Theme) -> impl IntoElement {
    let color = match notice.level {
        Level::Warning => t.warning,
        Level::Error => t.error,
    };
    div()
        .flex_shrink_0()
        .px(px(12.))
        .py(px(8.))
        .rounded(t.radius)
        .bg(t.tint(color))
        .text_color(color)
        .text_size(t.text_size_small())
        .child(notice.text.clone())
}

/// The records of each type together, in the report's order: `(type, its rows with their place)`.
fn grouped(rows: &[Row]) -> Vec<(&'static str, Vec<(usize, &Row)>)> {
    let mut groups: Vec<(&'static str, Vec<(usize, &Row)>)> = Vec::new();
    for (index, row) in rows.iter().enumerate() {
        match groups.last_mut() {
            Some((kind, rows)) if *kind == row.kind => rows.push((index, row)),
            _ => groups.push((row.kind, vec![(index, row)])),
        }
    }
    groups
}

/// The names a record's owner can be without saying anything new: the one looked up and its
/// aliases' targets. Records owned by another name show it.
fn known_names(target: Option<&Target>, rows: &[Row]) -> HashSet<String> {
    let mut known: HashSet<String> = rows.iter().filter(|row| row.kind == "CNAME").map(|row| row.value.clone()).collect();
    if let Some(Target::Host(host)) = target {
        known.insert(host.clone());
    }
    known
}

fn kind_title(kind: &str) -> &str {
    match kind {
        "A" => "IPv4 addresses",
        "AAAA" => "IPv6 addresses",
        "CNAME" => "Aliases",
        "MX" => "Mail servers",
        "NS" => "Name servers",
        "TXT" => "Text records",
        "SOA" => "Zone authority",
        "PTR" => "Reverse lookup",
        other => other,
    }
}

/// A type's colour, from the theme: addresses in the accent, aliases and mail in their status
/// colours, name servers and text in code's.
fn kind_color(kind: &str, t: &Theme) -> Option<Hsla> {
    match kind {
        "A" | "AAAA" => Some(t.accent),
        "CNAME" => Some(t.warning),
        "MX" => Some(t.success),
        "NS" => Some(t.syntax().type_),
        "TXT" => Some(t.syntax().constant),
        _ => None,
    }
}

/// One type's records: a heading (its badge, title and count, and a hairline to the right edge),
/// then its rows, on the pane itself.
fn group_view(kind: &'static str, rows: &[(usize, &Row)], known: &HashSet<String>, t: &Theme) -> AnyElement {
    let (color, background) = match kind_color(kind, t) {
        Some(color) => (color, t.tint(color)),
        None => (t.text_muted, t.fill),
    };
    let badge = div()
        .flex_shrink_0()
        .h(px(18.))
        .px(px(6.))
        .flex()
        .items_center()
        .rounded(px(4.))
        .bg(background)
        .text_color(color)
        .text_size(px(10.))
        .font_weight(FontWeight::SEMIBOLD)
        .child(kind);
    let count = (rows.len() > 1).then(|| div().flex_shrink_0().text_color(t.text_faint).child(rows.len().to_string()));
    let heading = h_flex()
        .gap(px(8.))
        .px(px(8.))
        .text_size(t.text_size_small())
        .child(badge)
        .child(div().flex_shrink_0().font_weight(FontWeight::SEMIBOLD).text_color(t.text_muted).child(kind_title(kind)))
        .children(count)
        .child(div().flex_1().h(px(1.)).bg(t.separator()));
    let records = rows.iter().map(|(index, row)| {
        let show_owner = !known.contains(&row.name) || row.kind == "PTR";
        record_view(*index, row, show_owner, t)
    });
    v_flex().items_stretch().flex_shrink_0().gap(px(4.)).child(heading).children(records).into_any_element()
}

/// A record: its value, with its owner before it where that says something new (an alias's name,
/// the address a reverse lookup is of), and its TTL at the right. A click copies the value.
fn record_view(index: usize, row: &Row, show_owner: bool, t: &Theme) -> AnyElement {
    let hover = t.fill_subtle();
    let missing = row.value == "no PTR record";
    let owner = show_owner.then(|| {
        h_flex()
            .flex_shrink_0()
            .max_w(px(240.))
            .gap(px(6.))
            .child(div().min_w(px(0.)).truncate().text_color(t.text_muted).child(row.name.clone()))
            .child(div().text_color(t.text_faint).child("→"))
    });
    let value = row.value.clone();
    h_flex()
        .id(("dns-record", index))
        .items_start()
        .gap(px(10.))
        .px(px(8.))
        .py(px(5.))
        .rounded(t.radius_small())
        .font_family(t.mono_font.clone())
        .text_size(t.mono_size())
        .cursor_pointer()
        .hover(move |row| row.bg(hover))
        .on_click(move |_, _, cx| copy(&value, cx))
        .children(owner)
        .child(div().flex_1().min_w(px(0.)).text_color(if missing { t.text_faint } else { t.text }).child(row.value.clone()))
        .children(row.ttl.clone().map(|ttl| div().flex_shrink_0().text_size(t.text_size_small()).text_color(t.text_faint).child(ttl)))
        .into_any_element()
}

/// Who answered, pinned under the records: a dot (green when the lookup answered, orange or red
/// when it says something's wrong) and the resolver, how it was chosen and how fast.
fn via_view(via: String, report: &Report, t: &Theme) -> impl IntoElement {
    let dot = if report.notices.iter().any(|notice| notice.level == Level::Error) {
        t.error
    } else if !report.notices.is_empty() || !report.found() {
        t.warning
    } else {
        t.success
    };
    h_flex()
        .flex_shrink_0()
        .gap(px(8.))
        .px(px(8.))
        .pt(px(8.))
        .pb(px(12.))
        .border_t_1()
        .border_color(t.separator())
        .child(div().flex_shrink_0().size(px(7.)).rounded(px(4.)).bg(dot))
        .child(div().min_w(px(0.)).truncate().text_size(t.text_size_small()).text_color(t.text_muted).child(via))
}
