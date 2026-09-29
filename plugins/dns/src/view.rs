//! The tool: a summary, then the records by section (value and TTL); looked up once
//! typing pauses.

use std::time::Duration;

use delight_plugin_api::{Action, Actions, Input, Shortcut, Tool, host};
use delight_ui::{ActiveTheme, Caption, Group, h_flex, v_flex};
use gpui::{
    App, ClipboardItem, Context, InteractiveElement, IntoElement, ParentElement, Render, StatefulInteractiveElement,
    Styled, Task, Window, div, px,
};

use crate::DnsOperation;
use crate::lookup::{Level, Notice, Report, Section, Target, target};
use crate::query;

/// A lookup asks a server several times: wait for typing to pause.
const DELAY: Duration = Duration::from_millis(400);

#[derive(Default)]
pub struct DnsView {
    target: Option<Target>,
    report: Option<Report>,
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
                        host(cx).remember_input(DnsOperation::Lookup, typed, cx);
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
            actions.push(Action::new(DnsAction::Addresses, "Copy addresses", Shortcut::Keystroke("enter".into())));
        }
        if report.names.is_some() {
            actions.push(Action::new(DnsAction::Names, "Copy names", Shortcut::Keystroke("enter".into())));
        }
        if report.dig_command.is_some() {
            actions.push(Action::new(DnsAction::DigCommand, "Copy dig command", Shortcut::Keystroke("cmd-enter".into())));
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
        let pane = v_flex().id("dns").size_full().overflow_y_scroll().gap(px(14.));
        let Some(report) = &self.report else {
            let looking = self.target.as_ref().map(|target| match target {
                Target::Host(host) => format!("Looking up {host}…"),
                Target::Ip(ip) => format!("Looking up {ip}…"),
            });
            return pane.children(looking.map(|text| div().text_color(t.text_faint).child(text)));
        };
        pane.children(report.notices.iter().map(|notice| notice_view(notice, cx)))
            .children(report.sections.iter().map(|section| section_view(section, cx)))
    }
}

fn notice_view(notice: &Notice, cx: &App) -> impl IntoElement {
    let t = cx.theme();
    let color = match notice.level {
        Level::Success => t.success,
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

/// A caption, then one row per record: its key, its value, and a hint (the TTL).
fn section_view(section: &Section, cx: &App) -> impl IntoElement {
    let t = cx.theme();
    let rows = section.rows.iter().map(|row| {
        h_flex()
            .items_start()
            .gap(px(12.))
            .px(px(12.))
            .py(px(7.))
            .text_size(t.text_size_small())
            .child(div().w(px(120.)).flex_shrink_0().text_color(t.text_muted).truncate().child(row.key.clone()))
            .child(div().flex_1().min_w(px(0.)).font_family(t.mono_font.clone()).child(row.value.clone()))
            .children(row.hint.clone().map(|hint| div().flex_shrink_0().text_color(t.text_faint).child(hint)))
            .into_any_element()
    });
    v_flex().gap(px(6.)).child(Caption::new(section.title)).child(Group::new().children(rows))
}
