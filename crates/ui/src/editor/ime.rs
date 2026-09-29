//! The platform's text input: typed characters, input methods (composing
//! Japanese, Chinese …), dictation, the accent pop-up and the emoji palette.
//! macOS speaks UTF-16 offsets; the editor converts at this boundary only.
//!
//! A composition edits the text directly without recording undo steps; its
//! commit records one step from the text before composing to the result.

use std::ops::Range;

use gpui::{Bounds, Context, EntityInputHandler, Pixels, Point, UTF16Selection, Window, point};

use super::element::position_for_offset;
use super::history::{Edit, EditKind};
use super::{TextEditor, text};

impl TextEditor {
    /// The range an input-handler call targets: the given one, else the text
    /// being composed, else the selection.
    fn target(&self, range_utf16: Option<&Range<usize>>) -> Range<usize> {
        let range = range_utf16
            .map(|r| text::range_from_utf16(&self.content, r))
            .or_else(|| self.marked_range.clone())
            .unwrap_or_else(|| self.selected_range.clone());
        text::clamp(&self.content, range)
    }

    /// Ends a composition, recording it as one undo step.
    fn commit_composition(&mut self) {
        if let (Some(mut edit), Some(marked)) = (self.composing.take(), self.marked_range.take()) {
            edit.new = self.content[text::clamp(&self.content, marked)].to_string();
            self.history.record(edit, EditKind::Other);
        }
    }
}

impl EntityInputHandler for TextEditor {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let range = text::clamp(&self.content, text::range_from_utf16(&self.content, &range_utf16));
        actual_range.replace(text::range_to_utf16(&self.content, &range));
        Some(self.content[range].to_string())
    }

    fn selected_text_range(&mut self, _: bool, _: &mut Window, _: &mut Context<Self>) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: text::range_to_utf16(&self.content, &self.selected_range),
            reversed: self.selection_reversed,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked_range.as_ref().map(|r| text::range_to_utf16(&self.content, r))
    }

    fn unmark_text(&mut self, _: &mut Window, _: &mut Context<Self>) {
        self.commit_composition();
    }

    /// Typing, or committing a composition with its final text. A typed ↵
    /// adds nothing: new lines come from `Newline` (⇧↵) and pasting, and ↵
    /// is left to whatever binds it.
    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.read_only || (self.composing.is_none() && matches!(new_text, "\n" | "\r")) {
            return;
        }
        let range = self.target(range_utf16.as_ref());
        match self.composing.take() {
            Some(mut edit) => {
                self.content.replace_range(range.clone(), new_text);
                edit.new = new_text.to_string();
                self.history.record(edit, EditKind::Other);
                let cursor = range.start + new_text.len();
                self.selected_range = cursor..cursor;
                self.selection_reversed = false;
                self.marked_range = None;
                self.changed(cx);
            }
            None => self.replace(range, new_text, EditKind::Typing, cx),
        }
    }

    /// Composing: shows `new_text` underlined, not yet committed.
    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.read_only {
            return;
        }
        let range = self.target(range_utf16.as_ref());
        if self.composing.is_none() {
            self.composing = Some(Edit {
                start: range.start,
                old: self.content[range.clone()].to_string(),
                new: String::new(),
                selection_before: self.selected_range.clone(),
            });
        }
        self.content.replace_range(range.clone(), new_text);
        self.marked_range = (!new_text.is_empty()).then(|| range.start..range.start + new_text.len());
        self.selected_range = new_selected_range_utf16
            .map(|r| text::range_from_utf16(new_text, &r))
            .map(|r| range.start + r.start..range.start + r.end)
            .unwrap_or_else(|| range.start + new_text.len()..range.start + new_text.len());
        self.changed(cx);
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        _: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let layout = self.layout.as_ref()?;
        let range = text::range_from_utf16(&self.content, &range_utf16);
        let start = position_for_offset(layout, range.start)?;
        let end = position_for_offset(layout, range.end)?;
        Some(Bounds::from_corners(
            layout.bounds.origin + start,
            layout.bounds.origin + point(end.x, end.y + layout.line_height),
        ))
    }

    fn character_index_for_point(&mut self, p: Point<Pixels>, _: &mut Window, _: &mut Context<Self>) -> Option<usize> {
        let offset = self.offset_for_mouse(p)?;
        Some(text::to_utf16(&self.content, offset))
    }
}
