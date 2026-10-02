//! The Base64 tool: the Encode and Decode tabs (the input picks one) with the result's
//! size at their right, the result, and its actions.

use delight_plugin_api::{Action, Actions, Input, Shortcut, Tool, host};
use delight_ui::conversion::Output;
use delight_ui::{ActiveTheme as _, SegmentedControl, h_flex, v_flex};
use gpui::{
    App, ClipboardItem, Context, FocusHandle, InteractiveElement, IntoElement, ParentElement, Render, ScrollHandle,
    SharedString, StatefulInteractiveElement, Styled, Task, Window, div, px,
};

use super::{Mode, convert, mode};

const MODES: [(Mode, &str); 2] = [(Mode::Encode, "Encode"), (Mode::Decode, "Decode")];

pub struct Base64View {
    /// The tabs' focus: a Tab stop, where ← / → switch them.
    modes_focus: FocusHandle,
    input: SharedString,
    /// The tab shown: the one the input fits, until another is picked for it.
    mode: Mode,
    output: Output,
    info: Option<SharedString>,
    copy: Option<String>,
    other: Option<String>,
    /// Where the pane is scrolled, kept while the tool is hidden.
    scroll: ScrollHandle,
    /// Replacing it cancels the conversion in progress.
    _task: Option<Task<()>>,
}

/// The footer's actions.
#[derive(Actions)]
pub enum Base64Action {
    /// The result as it is.
    Copy,
    /// Encoded: URL-safe, unpadded; decoded: the JSON in it, formatted.
    CopyOther,
}

impl Base64View {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            modes_focus: cx.focus_handle().tab_stop(true),
            input: SharedString::default(),
            mode: Mode::Encode,
            output: Output::default(),
            info: None,
            copy: None,
            other: None,
            scroll: ScrollHandle::new(),
            _task: None,
        }
    }

    fn labels(&self) -> (&'static str, &'static str) {
        match self.mode {
            Mode::Encode => ("Copy Base64", "Copy URL-safe"),
            Mode::Decode => ("Copy decoded", "Copy formatted JSON"),
        }
    }

    /// Converts the input in the background, then shows the result.
    fn convert(&mut self, cx: &mut Context<Self>) {
        let (input, mode) = (self.input.clone(), self.mode);
        self._task = Some(cx.spawn(async move |this, cx| {
            let (converted, output) = cx
                .background_executor()
                .spawn(async move {
                    let converted = convert(input.trim(), mode);
                    let output = Output::new(converted.conversion.clone(), input.trim());
                    (converted, output)
                })
                .await;
            this.update(cx, |this, cx| {
                this.output = output;
                this.info = converted.info.map(Into::into);
                this.copy = converted.copy;
                this.other = converted.other;
                cx.notify();
            })
            .ok();
        }));
    }
}

impl Tool for Base64View {
    type Action = Base64Action;

    fn on_input_changed(&mut self, input: &Input, cx: &mut Context<Self>) {
        self.input = input.text.clone().into();
        self.mode = mode(input.text.trim());
        self.convert(cx);
    }

    fn list_actions(&self, _: &App) -> Vec<Action<Base64Action>> {
        let (copy, other) = self.labels();
        let mut actions = Vec::new();
        if self.copy.is_some() {
            actions.push(Action::new(Base64Action::Copy, copy, Shortcut::Enter));
        }
        if self.other.is_some() {
            actions.push(Action::new(Base64Action::CopyOther, other, Shortcut::CmdEnter));
        }
        actions
    }

    fn perform_action(&mut self, action: Base64Action, cx: &mut Context<Self>) {
        let (copy, other) = self.labels();
        let (label, text) = match action {
            Base64Action::Copy => (copy, self.copy.clone()),
            Base64Action::CopyOther => (other, self.other.clone()),
        };
        let Some(text) = text else { return };
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        host(cx).toast(format!("{label} — copied to clipboard"), cx);
    }
}

impl Render for Base64View {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let modes = SegmentedControl::new("base64-mode")
            .focus(&self.modes_focus)
            .options(MODES.map(|(_, label)| label))
            .selected(MODES.iter().position(|(mode, _)| *mode == self.mode).unwrap_or(0))
            .on_change(cx.listener(|this, index: &usize, _, cx| {
                this.mode = MODES[*index].0;
                this.convert(cx);
            }));
        let t = cx.theme();
        let info = self.info.clone().map(|info| div().text_size(t.text_size_small()).text_color(t.text_faint).child(info));
        v_flex()
            .id("base64")
            .size_full()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .gap(px(14.))
            .child(h_flex().justify_between().child(modes).children(info))
            .child(self.output.render(cx))
    }
}
