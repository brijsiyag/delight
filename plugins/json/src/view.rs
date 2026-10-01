//! The JSON tool: the mode tabs, the formatting buttons, the result and its actions.

use delight_plugin_api::{Action, Actions, Input, Shortcut, Tool, host};
use delight_ui::conversion::Output;
use delight_ui::{IconButton, IconName, SegmentedControl, Selectable, h_flex, v_flex};
use gpui::{
    App, ClipboardItem, Context, FocusHandle, InteractiveElement, IntoElement, ParentElement, Render, ScrollHandle,
    SharedString, StatefulInteractiveElement, Styled, Task, Window, px,
};

use super::convert::{Indent, Mode, Options, convert};

const MODES: [(Mode, &str); 4] =
    [(Mode::Format, "Format"), (Mode::Minify, "Minify"), (Mode::Escape, "Escape"), (Mode::Unescape, "Unescape")];

impl Indent {
    fn toggled(self) -> Self {
        match self {
            Indent::Two => Indent::Four,
            Indent::Four => Indent::Two,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Indent::Two => "2",
            Indent::Four => "4",
        }
    }
}

pub struct JsonView {
    /// The mode tabs' focus: a Tab stop, where ← / → switch modes.
    modes_focus: FocusHandle,
    input: SharedString,
    options: Options,
    output: Output,
    /// Minified, for "Copy minified" next to a formatted result.
    minified: Option<String>,
    /// Where the pane is scrolled. Kept here, not in the window's element state, which goes while
    /// the tool is hidden: the tool comes back where it was.
    scroll: ScrollHandle,
    /// Replacing it cancels the conversion in progress.
    _task: Option<Task<()>>,
}

/// The footer's actions.
#[derive(Actions)]
pub enum JsonAction {
    /// The result, as the mode made it.
    Copy,
    /// A formatted result, minified.
    CopyMinified,
}

impl JsonView {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            modes_focus: cx.focus_handle().tab_stop(true),
            input: SharedString::default(),
            options: Options::default(),
            output: Output::default(),
            minified: None,
            scroll: ScrollHandle::new(),
            _task: None,
        }
    }

    fn copy_label(&self) -> &'static str {
        match self.options.mode {
            Mode::Format => "Copy formatted",
            Mode::Minify => "Copy minified",
            Mode::Escape => "Copy escaped",
            Mode::Unescape => "Copy unescaped",
        }
    }

    fn set_options(&mut self, options: Options, cx: &mut Context<Self>) {
        self.options = options;
        self.convert(cx);
    }

    /// Converts the input in the background, then shows the result.
    fn convert(&mut self, cx: &mut Context<Self>) {
        let (input, options) = (self.input.clone(), self.options);
        self._task = Some(cx.spawn(async move |this, cx| {
            let (output, minified) = cx
                .background_executor()
                .spawn(async move {
                    let (conversion, minified) = convert(&input, options);
                    (Output::new(conversion, input.trim()), minified)
                })
                .await;
            this.update(cx, |this, cx| {
                this.output = output;
                this.minified = minified;
                cx.notify();
            })
            .ok();
        }));
    }
}

impl Tool for JsonView {
    type Action = JsonAction;

    fn on_input_changed(&mut self, input: &Input, cx: &mut Context<Self>) {
        self.input = input.text.clone().into();
        self.convert(cx);
    }

    fn list_actions(&self, _: &App) -> Vec<Action<JsonAction>> {
        if self.output.text().is_none() {
            return Vec::new();
        }
        let mut actions = vec![Action::new(JsonAction::Copy, self.copy_label(), Shortcut::Enter)];
        if self.minified.is_some() {
            actions.push(Action::new(JsonAction::CopyMinified, "Copy minified", Shortcut::CmdEnter));
        }
        actions
    }

    fn perform_action(&mut self, action: JsonAction, cx: &mut Context<Self>) {
        let (label, text) = match action {
            JsonAction::Copy => match self.output.text() {
                Some(text) => (self.copy_label(), text.to_string()),
                None => return,
            },
            JsonAction::CopyMinified => match &self.minified {
                Some(minified) => ("Copy minified", minified.clone()),
                None => return,
            },
        };
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        host(cx).toast(format!("{label} — copied to clipboard"), cx);
    }
}

impl Render for JsonView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let options = self.options;
        let modes = SegmentedControl::new("json-mode")
            .focus(&self.modes_focus)
            .options(MODES.map(|(_, label)| label))
            .selected(MODES.iter().position(|(mode, _)| *mode == options.mode).unwrap_or(0))
            .on_change(cx.listener(move |this, index: &usize, _, cx| {
                this.set_options(Options { mode: MODES[*index].0, ..options }, cx)
            }));
        // Formatting buttons at the right of the result's title.
        let indent = IconButton::new("json-indent", IconName::IndentIncrease)
            .label(options.indent.label())
            .tooltip(format!("Indent: {} spaces", options.indent.label()))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.set_options(Options { indent: options.indent.toggled(), ..options }, cx)
            }));
        let sort_keys = IconButton::new("json-sort-keys", IconName::ArrowDownAZ)
            .selected(options.sort_keys)
            .tooltip(if options.sort_keys { "Keys sorted A→Z" } else { "Sort keys A→Z" })
            .on_click(cx.listener(move |this, _, _, cx| {
                this.set_options(Options { sort_keys: !options.sort_keys, ..options }, cx)
            }));
        let mut accessory = h_flex().gap(px(2.));
        if matches!(options.mode, Mode::Format | Mode::Unescape) {
            accessory = accessory.child(indent);
        }
        if options.mode == Mode::Format {
            accessory = accessory.child(sort_keys);
        }
        v_flex()
            .id("json")
            .size_full()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .gap(px(14.))
            .child(h_flex().child(modes))
            .child(self.output.render(Some(accessory.into_any_element()), cx))
    }
}
