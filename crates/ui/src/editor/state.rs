//! The editor entity: content, selection, options, the editing primitives
//! every command goes through, focus, and rendering.

use std::ops::Range;

use gpui::{
    AppContext, Context, CursorStyle, Entity, EventEmitter, FocusHandle, Focusable, InteractiveElement, IntoElement, KeyContext,
    Hsla, MouseButton, ParentElement, Pixels, Render, ScrollHandle, SharedString, StatefulInteractiveElement, Styled,
    Subscription, Window, div, point, prelude::FluentBuilder, px,
};

use super::blink::BlinkCursor;
use super::element::{Layout, TextElement, offset_for_point, position_for_offset};
use super::history::{Edit, EditKind, History, apply};
use super::{actions, text};
use crate::ActiveTheme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorEvent {
    /// The text changed.
    Changed,
    /// The user took the whole completion (Tab or →, or its last word with ⌥Tab or ⌥→); `Changed`
    /// follows.
    CompletionAccepted,
    Focus,
    Blur,
}

/// Which of the theme's fonts the editor uses.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum EditorFont {
    /// Interface text.
    #[default]
    Ui,
    /// Code.
    Mono,
    /// The launcher's input font (Lilex).
    Input,
}

/// What a drag after a click selects, macOS style.
pub(super) enum Drag {
    /// After a click: characters, from where it started.
    Chars,
    /// After a double-click: whole words, from this one.
    Words(Range<usize>),
    /// After a triple-click: whole lines, from this one.
    Lines(Range<usize>),
}

pub struct TextEditor {
    pub(super) focus_handle: FocusHandle,
    pub(super) content: String,
    pub(super) placeholder: SharedString,
    /// Greyed text after the cursor that Tab or → inserts (see `set_completion`).
    pub(super) completion: Option<SharedString>,
    pub(super) selected_range: Range<usize>,
    pub(super) selection_reversed: bool,
    /// Text an input method is still composing (underlined, not committed).
    pub(super) marked_range: Option<Range<usize>>,
    /// While composing: the edit the composition will record when committed.
    pub(super) composing: Option<Edit>,
    pub(super) multiline: bool,
    /// Text can be selected and copied, not changed.
    pub(super) read_only: bool,
    /// Colours for ranges of the text (byte ranges, in order, not overlapping); the rest is drawn in
    /// the text colour. Any change to the text drops them.
    pub(super) highlights: Vec<(Range<usize>, Hsla)>,
    pub(super) font: EditorFont,
    pub(super) font_size: Pixels,
    pub(super) line_height: Pixels,
    pub(super) max_height: Option<Pixels>,
    /// The last painted layout: for mouse hit-testing and ↑/↓.
    pub(super) layout: Option<Layout>,
    /// While the mouse button is down after a click in the text: what dragging
    /// extends the selection by.
    pub(super) drag: Option<Drag>,
    /// The column ↑/↓ keep while moving between rows.
    pub(super) goal_x: Option<Pixels>,
    pub(super) history: History,
    pub(super) blink: Entity<BlinkCursor>,
    pub(super) scroll: ScrollHandle,
    /// Scroll the cursor into view on the next paint.
    pub(super) autoscroll: bool,
    /// How far a single-line editor's text is scrolled left, to keep the cursor in view.
    pub(super) scroll_x: Pixels,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<EditorEvent> for TextEditor {}

impl TextEditor {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        // A Tab stop: Tab moves on to the next field, and back.
        let focus_handle = cx.focus_handle().tab_stop(true);
        let blink = cx.new(|_| BlinkCursor::new());
        let subscriptions = vec![
            cx.observe(&blink, |_, _, cx| cx.notify()),
            cx.on_focus(&focus_handle, window, |this, _, cx| {
                this.blink.update(cx, |b, cx| b.start(cx));
                cx.emit(EditorEvent::Focus);
            }),
            cx.on_blur(&focus_handle, window, |this, _, cx| {
                this.blink.update(cx, |b, cx| b.stop(cx));
                cx.emit(EditorEvent::Blur);
            }),
        ];
        Self {
            focus_handle,
            content: String::new(),
            placeholder: SharedString::default(),
            completion: None,
            selected_range: 0..0,
            selection_reversed: false,
            marked_range: None,
            composing: None,
            multiline: false,
            read_only: false,
            highlights: Vec::new(),
            font: EditorFont::default(),
            font_size: px(13.),
            line_height: px(18.),
            max_height: None,
            layout: None,
            drag: None,
            goal_x: None,
            history: History::default(),
            blink,
            scroll: ScrollHandle::new(),
            autoscroll: false,
            scroll_x: px(0.),
            _subscriptions: subscriptions,
        }
    }

    /// Text that is shown to be selected and copied: typing, pasting, cutting and undoing change
    /// nothing.
    pub fn read_only(mut self) -> Self {
        self.read_only = true;
        self
    }

    /// Several lines, soft-wrapped; scrolls past `max_height`.
    pub fn multiline(mut self, max_height: Pixels) -> Self {
        self.multiline = true;
        self.max_height = Some(max_height);
        self
    }

    pub fn font(mut self, font: EditorFont) -> Self {
        self.font = font;
        self
    }

    pub fn text_size(mut self, font_size: Pixels, line_height: Pixels) -> Self {
        self.font_size = font_size;
        self.line_height = line_height;
        self
    }

    pub fn placeholder(mut self, placeholder: impl Into<SharedString>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    /// Show another placeholder while the editor is empty.
    pub fn set_placeholder(&mut self, placeholder: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.placeholder = placeholder.into();
        cx.notify();
    }

    /// Colour ranges of the text, as `Range`s of byte offsets at character boundaries, in order and
    /// not overlapping (a syntax highlight). A change to the text drops them: set them after.
    pub fn set_highlights(&mut self, highlights: Vec<(Range<usize>, Hsla)>, cx: &mut Context<Self>) {
        if self.highlights != highlights {
            self.highlights = highlights;
            cx.notify();
        }
    }

    pub fn text(&self) -> &str {
        &self.content
    }

    /// Replaces everything (undoable), cursor at the end.
    pub fn set_text(&mut self, text: impl Into<String>, cx: &mut Context<Self>) {
        let text = text.into();
        if text != self.content {
            self.history.break_group();
            // The program sets the text of a read-only editor; the user can't change it.
            self.apply_replace(0..self.content.len(), &text, EditKind::Other, cx);
        }
    }

    /// Shows `completion` greyed after the text: how the input could go on
    /// (e.g. a remembered input). Tab or → inserts it, ⌥Tab or ⌥→ its next
    /// word. It shows only while the cursor is at the end with nothing
    /// selected, and any edit clears it: set a fresh one on
    /// [`EditorEvent::Changed`]. In an empty editor it takes the
    /// placeholder's place.
    pub fn set_completion(&mut self, completion: Option<SharedString>, cx: &mut Context<Self>) {
        let completion = completion.filter(|c| !c.is_empty());
        if completion != self.completion {
            self.completion = completion;
            cx.notify();
        }
    }

    /// The completion, if it's showing now.
    pub(super) fn visible_completion(&self) -> Option<SharedString> {
        let at_end = self.selected_range.is_empty() && self.cursor() == self.content.len();
        self.completion.clone().filter(|_| at_end && self.marked_range.is_none())
    }

    pub fn select_all_text(&mut self, cx: &mut Context<Self>) {
        self.selected_range = 0..self.content.len();
        self.selection_reversed = false;
        cx.notify();
    }

    /// Types `text` over the selection, as if it were typed.
    pub fn insert(&mut self, text: &str, cx: &mut Context<Self>) {
        self.replace(self.selected_range.clone(), text, EditKind::Typing, cx);
    }

    // ---------------------------------------------------------------------
    // Editing primitives
    // ---------------------------------------------------------------------

    /// The one place text changes (apart from IME composition): replaces
    /// `range` with `new_text`, records it for undo, puts the cursor after it.
    pub(super) fn replace(&mut self, range: Range<usize>, new_text: &str, kind: EditKind, cx: &mut Context<Self>) {
        if self.read_only {
            return;
        }
        self.apply_replace(range, new_text, kind, cx);
    }

    /// [`Self::replace`] without the read-only check: for the program's own changes.
    fn apply_replace(&mut self, range: Range<usize>, new_text: &str, kind: EditKind, cx: &mut Context<Self>) {
        let range = text::clamp(&self.content, range);
        let new_text =
            if self.multiline { new_text.replace("\r\n", "\n") } else { new_text.replace(['\r', '\n'], " ") };
        let edit = Edit {
            start: range.start,
            old: self.content[range.clone()].to_string(),
            new: new_text,
            selection_before: self.selected_range.clone(),
        };
        apply(&mut self.content, &edit);
        let cursor = edit.start + edit.new.len();
        self.history.record(edit, kind);
        self.selected_range = cursor..cursor;
        self.selection_reversed = false;
        self.marked_range = None;
        self.changed(cx);
    }

    /// Applies an undo or redo step.
    pub(super) fn apply_history(&mut self, edit: Edit, cx: &mut Context<Self>) {
        apply(&mut self.content, &edit);
        self.selected_range = text::clamp(&self.content, edit.selection_before);
        self.selection_reversed = false;
        self.marked_range = None;
        self.changed(cx);
    }

    /// After any change to the text.
    pub(super) fn changed(&mut self, cx: &mut Context<Self>) {
        self.highlights.clear();
        self.completion = None;
        self.goal_x = None;
        self.autoscroll = true;
        self.blink.update(cx, |b, cx| b.pause(cx));
        cx.emit(EditorEvent::Changed);
        cx.notify();
    }

    pub(super) fn cursor(&self) -> usize {
        if self.selection_reversed { self.selected_range.start } else { self.selected_range.end }
    }

    pub(super) fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.selected_range = offset..offset;
        self.selection_reversed = false;
        self.moved(cx);
    }

    pub(super) fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        if self.selection_reversed {
            self.selected_range.start = offset;
        } else {
            self.selected_range.end = offset;
        }
        if self.selected_range.end < self.selected_range.start {
            self.selection_reversed = !self.selection_reversed;
            self.selected_range = self.selected_range.end..self.selected_range.start;
        }
        self.moved(cx);
    }

    /// After the cursor or selection moved: the next edit is a new undo step.
    pub(super) fn moved(&mut self, cx: &mut Context<Self>) {
        self.history.break_group();
        self.autoscroll = true;
        self.blink.update(cx, |b, cx| b.pause(cx));
        cx.notify();
    }

    /// The offset one visual row up or down, or `None` at the first or last
    /// row.
    pub(super) fn vertical(&mut self, offset: usize, down: bool) -> Option<usize> {
        let layout = self.layout.as_ref()?;
        let pos = position_for_offset(layout, offset)?;
        let goal = *self.goal_x.get_or_insert(pos.x);
        let lh = layout.line_height;
        let y = if down { pos.y + lh } else { pos.y - lh };
        let total = layout.lines.last().map_or(lh, |l| l.y + l.wrapped.size(lh).height);
        if y < px(0.) || y >= total {
            return None;
        }
        Some(offset_for_point(layout, point(goal, y + lh / 2.)))
    }

    pub(super) fn offset_for_mouse(&self, position: gpui::Point<Pixels>) -> Option<usize> {
        let layout = self.layout.as_ref()?;
        Some(offset_for_point(layout, position - layout.bounds.origin))
    }
}

impl TextEditor {
    /// `Editor`, plus flags the keymap can test:
    /// * `multiline` — ↵ variants insert a line;
    /// * `showing_completion` — a greyed completion shows (Tab or → accepts it);
    /// * `empty_input` — there is no text;
    /// * `start_of_input` / `end_of_input` — the cursor is at the very start
    ///   / end with nothing selected (e.g. ↓ at the end moves to the tools).
    fn key_context(&self) -> KeyContext {
        let mut context = KeyContext::default();
        context.add(actions::CONTEXT);
        if self.multiline {
            context.add("multiline");
        }
        if self.visible_completion().is_some() {
            context.add("showing_completion");
        }
        if self.content.is_empty() {
            context.add("empty_input");
        }
        if self.selected_range.is_empty() {
            if self.cursor() == 0 {
                context.add("start_of_input");
            }
            if self.cursor() == self.content.len() {
                context.add("end_of_input");
            }
        }
        context
    }
}

impl Focusable for TextEditor {
    fn focus_handle(&self, _: &gpui::App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for TextEditor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme();
        let element = TextElement {
            editor: cx.entity(),
            color: t.text,
            placeholder_color: t.text_faint,
            selection_color: t.selection(),
            cursor_color: t.accent,
            font_family: match self.font {
                EditorFont::Ui => t.font.clone(),
                EditorFont::Mono => t.mono_font.clone(),
                EditorFont::Input => crate::theme::input_font(cx),
            },
        };
        let body = div()
            .id("editor-scroll")
            .w_full()
            .when_some(self.max_height, |d, h| d.max_h(h))
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .child(element);
        let root = div()
            .key_context(self.key_context())
            .track_focus(&self.focus_handle)
            .cursor(CursorStyle::IBeam)
            .w_full()
            .text_size(self.font_size)
            .line_height(self.line_height)
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .child(body);
        actions::wire(root, cx)
    }
}
