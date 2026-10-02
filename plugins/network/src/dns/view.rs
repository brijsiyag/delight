//! The tool: what went wrong (if anything), then every record in one table (type, name,
//! value, TTL), and under it who answered; looked up once typing pauses.

use std::time::Duration;

use delight_plugin_api::{Action, Actions, Input, Shortcut, Tool, host};
use delight_ui::{ActiveTheme, Caption, Group, Theme, h_flex, v_flex};
use gpui::{
    AnyElement, App, ClipboardItem, Context, Hsla, InteractiveElement, IntoElement, ParentElement, Render, ScrollHandle,
    StatefulInteractiveElement, Styled, Task, Window, div, px,
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
        let t = cx.theme();
        let pane = v_flex().id("dns").size_full().overflow_y_scroll().track_scroll(&self.scroll).gap(px(14.));
        let Some(report) = &self.report else {
            let looking = self.target.as_ref().map(|target| match target {
                Target::Host(host) => format!("Looking up {host}…"),
                Target::Ip(ip) => format!("Looking up {ip}…"),
            });
            return pane.children(looking.map(|text| div().text_color(t.text_faint).child(text)));
        };
        let table = (!report.rows.is_empty()).then(|| {
            Group::new().child(header(t)).children(report.rows.iter().map(|row| row_view(row, t)))
        });
        let via = report.via.clone().map(|via| div().text_size(t.text_size_small()).text_color(t.text_faint).truncate().child(via));
        pane.children(report.notices.iter().map(|notice| notice_view(notice, cx))).children(table).children(via)
    }
}

fn notice_view(notice: &Notice, cx: &App) -> impl IntoElement {
    let t = cx.theme();
    let color = match notice.level {
        Level::Warning => t.warning,
        Level::Error => t.error,
    };
    div()
        .px(px(12.))
        .py(px(8.))
        .rounded(t.radius)
        .bg(t.tint(color))
        .text_color(color)
        .text_size(t.text_size_small())
        .child(notice.text.clone())
}

/// The table's columns: the type's badge, the name, the value (which wraps), the TTL.
const TYPE_WIDTH: f32 = 46.;
const NAME_WIDTH: f32 = 150.;
const TTL_WIDTH: f32 = 32.;

fn header(t: &Theme) -> AnyElement {
    h_flex()
        .h(px(26.))
        .gap(px(10.))
        .px(px(12.))
        .child(div().w(px(TYPE_WIDTH)).flex_shrink_0().child(Caption::new("Type")))
        .child(div().w(px(NAME_WIDTH)).flex_shrink_0().child(Caption::new("Name")))
        .child(div().flex_1().child(Caption::new("Value")))
        .child(h_flex().w(px(TTL_WIDTH)).flex_shrink_0().justify_end().child(Caption::new("TTL")))
        .text_color(t.text_muted)
        .into_any_element()
}

/// A record: its type as a badge (addresses in the accent, aliases and mail in their own
/// colours), its name, its value, its TTL.
fn row_view(row: &Row, t: &Theme) -> AnyElement {
    let (color, background): (Hsla, Hsla) = match row.kind {
        "A" | "AAAA" => (t.accent, t.tint(t.accent)),
        "CNAME" => (t.warning, t.tint(t.warning)),
        "MX" => (t.success, t.tint(t.success)),
        _ => (t.text_muted, t.fill),
    };
    let badge = div()
        .h(px(18.))
        .px(px(6.))
        .flex()
        .items_center()
        .rounded(px(4.))
        .bg(background)
        .text_color(color)
        .text_size(px(10.))
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .child(row.kind);
    h_flex()
        .items_start()
        .gap(px(10.))
        .px(px(12.))
        .py(px(5.))
        .text_size(t.text_size_small())
        .font_family(t.mono_font.clone())
        .child(div().w(px(TYPE_WIDTH)).flex_shrink_0().child(badge))
        .child(div().w(px(NAME_WIDTH)).flex_shrink_0().pt(px(2.)).text_color(t.text_muted).truncate().child(row.name.clone()))
        .child(div().flex_1().min_w(px(0.)).pt(px(2.)).child(row.value.clone()))
        .child(div().w(px(TTL_WIDTH)).flex_shrink_0().pt(px(2.)).text_right().text_color(t.text_faint).children(row.ttl.clone()))
        .into_any_element()
}
