//! The General page: Delight's own settings.

use delight_protocol::PLUGIN_API_VERSION;
use delight_ui::{Button, SegmentedControl, Switch, Theme, h_flex, row, section, v_flex};
use gpui::{AnyElement, Context, IntoElement, Keystroke, ParentElement, PromptLevel, Styled, div, px};

use super::shortcut_recorder::Recorded;
use super::SettingsWindow;
use crate::settings::{self, Appearance, DEFAULT_LAUNCHER_SHORTCUT, Settings};
use crate::history;

const APPEARANCES: [(Appearance, &str); 3] =
    [(Appearance::System, "Auto"), (Appearance::Light, "Light"), (Appearance::Dark, "Dark")];

impl SettingsWindow {
    pub(super) fn render_general(&self, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let current = settings::get(cx).clone();

        let shortcut = h_flex().gap(px(4.)).items_start().child(self.shortcut.clone()).child(
            Button::new("reset-shortcut", "Reset").text().on_click(cx.listener(|this, _, _, cx| {
                let default = Keystroke::parse(DEFAULT_LAUNCHER_SHORTCUT).expect("the default shortcut parses");
                this.shortcut.update(cx, |_, cx| cx.emit(Recorded(default)));
            })),
        );
        let launcher = vec![
            row("Open Delight", Some("The shortcut that shows the launcher, from any app"), shortcut, t),
            toggle(
                "hide-on-blur",
                "Hide when another app is used",
                Some("Esc and the shortcut always hide it"),
                current.hide_on_blur,
                |settings, on| settings.hide_on_blur = on,
                t,
            ),
            toggle(
                "paste-on-open",
                "Paste the clipboard on open",
                Some("Put the clipboard’s text into the input when Delight opens"),
                current.paste_clipboard_on_open,
                |settings, on| settings.paste_clipboard_on_open = on,
                t,
            ),
        ];

        let selected = APPEARANCES.iter().position(|(appearance, _)| *appearance == current.appearance).unwrap_or(0);
        let theme = SegmentedControl::new("appearance")
            .options(APPEARANCES.map(|(_, label)| label))
            .selected(selected)
            .on_change(|index, _, cx| settings::update(cx, |settings| settings.appearance = APPEARANCES[*index].0));

        let erase = Button::new("erase-history", "Erase…").on_click(|_, window, cx| {
            let answer = window.prompt(
                PromptLevel::Warning,
                "Erase the input history?",
                Some("Everything tools remembered goes, and the input kept for the next launch. This can’t be undone."),
                &["Erase", "Cancel"],
                cx,
            );
            cx.spawn(async move |cx| {
                if answer.await == Ok(0) {
                    cx.update(history::erase);
                }
            })
            .detach();
        });
        let history = vec![
            toggle(
                "input-history",
                "Remember inputs",
                Some("The input comes back at launch, and what tools remembered completes what you type"),
                current.input_history,
                |settings, on| settings.input_history = on,
                t,
            ),
            row(
                "Erase the history",
                Some("Everything tools remembered, and the input kept for the next launch"),
                erase,
                t,
            ),
        ];

        let system = vec![toggle(
            "open-at-login",
            "Open at login",
            None,
            current.open_at_login,
            |settings, on| settings.open_at_login = on,
            t,
        )];

        // Only a `.app` with Sparkle can update itself.
        let updates = crate::updater::available(cx).then(|| {
            let (detail, color) = match crate::updater::status(cx) {
                crate::updater::Status::Idle => ("Checked once a day".to_string(), t.text_muted),
                crate::updater::Status::Checking => ("Checking…".to_string(), t.text_muted),
                crate::updater::Status::Found(version) => (format!("Delight {version} is available"), t.accent),
                crate::updater::Status::UpToDate => ("Delight is up to date".to_string(), t.success),
                crate::updater::Status::Failed(why) => (why, t.error),
            };
            let check = Button::new("check-updates", "Check Now").on_click(|_, _, cx| crate::updater::check(cx));
            section("Updates", vec![delight_ui::row_with("Check for updates", detail, check, color)])
        });
        let versions = format!("Delight {} · Plugin API {PLUGIN_API_VERSION}", env!("CARGO_PKG_VERSION"));
        v_flex()
            .gap(px(18.))
            .child(section("Launcher", launcher))
            .child(section("Appearance", vec![row("Theme", None, theme, t)]))
            .child(section("Input history", history))
            .child(section("System", system))
            .children(updates)
            .child(div().flex().justify_center().text_size(px(11.)).text_color(t.text_muted).child(versions))
            .into_any_element()
    }
}

/// A row with a switch that sets one setting.
fn toggle(
    id: &'static str,
    title: &'static str,
    detail: Option<&'static str>,
    on: bool,
    set: fn(&mut Settings, bool),
    t: &Theme,
) -> AnyElement {
    let switch = Switch::new(id).checked(on).on_change(move |on, _, cx| settings::update(cx, |settings| set(settings, *on)));
    row(title, detail, switch, t)
}
