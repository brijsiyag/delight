//! Undo/redo. Each step records one edit — the replaced text and what
//! replaced it — not a copy of the whole content, so a large paste doesn't
//! multiply memory by the number of steps. Consecutive edits of the same kind
//! (typing a word, a run of backspaces) merge into one step.

use std::ops::Range;

/// One reversible edit: at `start`, `old` was replaced by `new`.
#[derive(Debug, Clone, PartialEq)]
pub struct Edit {
    pub start: usize,
    pub old: String,
    pub new: String,
    /// The selection before the edit, restored by undo.
    pub selection_before: Range<usize>,
}

impl Edit {
    /// The same edit, backwards: applying it undoes this one.
    fn inverse(&self) -> Edit {
        Edit {
            start: self.start,
            old: self.new.clone(),
            new: self.old.clone(),
            selection_before: self.start..self.start + self.new.len(),
        }
    }
}

/// What caused an edit; consecutive edits of the same kind merge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditKind {
    /// Characters typed at the cursor.
    Typing,
    /// ⌫: deleting backwards.
    Backspace,
    /// Delete: deleting forwards.
    DeleteForward,
    /// Anything else (paste, cut, replace-all, IME commit): its own step.
    Other,
}

/// At most this many steps, and at most this many bytes of recorded text.
const MAX_STEPS: usize = 200;
const MAX_BYTES: usize = 16 * 1024 * 1024;

#[derive(Default)]
pub struct History {
    undo: Vec<Edit>,
    redo: Vec<Edit>,
    /// The kind of the last recorded step, while it may still grow.
    open: Option<EditKind>,
}

fn bytes(edits: &[Edit]) -> usize {
    edits.iter().map(|e| e.old.len() + e.new.len()).sum()
}

impl History {
    /// Records a new edit — merged into the previous step when it continues
    /// it — and clears redo.
    pub fn record(&mut self, edit: Edit, kind: EditKind) {
        self.redo.clear();
        if self.open == Some(kind)
            && let Some(last) = self.undo.last_mut()
            && merge(last, &edit, kind)
        {
            return;
        }
        self.open = (kind != EditKind::Other).then_some(kind);
        self.undo.push(edit);
        while self.undo.len() > MAX_STEPS || (self.undo.len() > 1 && bytes(&self.undo) > MAX_BYTES) {
            self.undo.remove(0);
        }
    }

    /// The next edit starts a new step (e.g. after the cursor moved).
    pub fn break_group(&mut self) {
        self.open = None;
    }

    /// The edit that undoes the last step (apply it, then restore its
    /// `selection_before`).
    pub fn undo(&mut self) -> Option<Edit> {
        self.open = None;
        let edit = self.undo.pop()?;
        let inverse = edit.inverse();
        self.redo.push(edit);
        Some(Edit { selection_before: self.redo.last()?.selection_before.clone(), ..inverse })
    }

    /// The edit that redoes the last undone step.
    pub fn redo(&mut self) -> Option<Edit> {
        self.open = None;
        let edit = self.redo.pop()?;
        self.undo.push(edit.clone());
        let end = edit.start + edit.new.len();
        Some(Edit { selection_before: end..end, ..edit })
    }
}

/// Folds `next` into `last` if it directly continues it.
fn merge(last: &mut Edit, next: &Edit, kind: EditKind) -> bool {
    match kind {
        // Typing right after what was typed.
        EditKind::Typing if next.old.is_empty() && next.start == last.start + last.new.len() => {
            last.new.push_str(&next.new);
            true
        }
        // Deleting the character just before what was deleted.
        EditKind::Backspace if next.new.is_empty() && next.start + next.old.len() == last.start => {
            last.start = next.start;
            last.old.insert_str(0, &next.old);
            true
        }
        // Deleting forwards at the same spot.
        EditKind::DeleteForward if next.new.is_empty() && next.start == last.start => {
            last.old.push_str(&next.old);
            true
        }
        _ => false,
    }
}

/// Applies `edit` to `text`.
pub fn apply(text: &mut String, edit: &Edit) {
    text.replace_range(edit.start..edit.start + edit.old.len(), &edit.new);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edit_as(text: &mut String, history: &mut History, range: Range<usize>, new: &str, kind: EditKind) {
        let e = Edit { start: range.start, old: text[range.clone()].to_string(), new: new.into(), selection_before: range };
        apply(text, &e);
        history.record(e, kind);
    }

    fn edit(text: &mut String, history: &mut History, range: Range<usize>, new: &str) {
        edit_as(text, history, range, new, EditKind::Other);
    }

    fn undo(text: &mut String, history: &mut History) {
        let u = history.undo().unwrap();
        apply(text, &u);
    }

    #[test]
    fn typing_and_backspacing_merge_into_one_step() {
        let (mut text, mut h) = (String::from("x "), History::default());
        for (i, c) in "hello".chars().enumerate() {
            edit_as(&mut text, &mut h, 2 + i..2 + i, &c.to_string(), EditKind::Typing);
        }
        assert_eq!(text, "x hello");
        undo(&mut text, &mut h);
        assert_eq!(text, "x ", "one undo removes the word");

        let (mut text, mut h) = (String::from("abcdef"), History::default());
        for end in (4..=6).rev() {
            edit_as(&mut text, &mut h, end - 1..end, "", EditKind::Backspace);
        }
        edit_as(&mut text, &mut h, 0..1, "", EditKind::DeleteForward);
        edit_as(&mut text, &mut h, 0..1, "", EditKind::DeleteForward);
        assert_eq!(text, "c");
        undo(&mut text, &mut h);
        assert_eq!(text, "abc", "both forward deletes undo together");
        undo(&mut text, &mut h);
        assert_eq!(text, "abcdef", "the backspace run undoes together");
    }

    #[test]
    fn a_break_or_a_gap_starts_a_new_step() {
        let (mut text, mut h) = (String::new(), History::default());
        edit_as(&mut text, &mut h, 0..0, "a", EditKind::Typing);
        h.break_group(); // the cursor moved
        edit_as(&mut text, &mut h, 1..1, "b", EditKind::Typing);
        edit_as(&mut text, &mut h, 0..0, "c", EditKind::Typing); // not adjacent: new step
        assert_eq!(text, "cab");
        undo(&mut text, &mut h);
        assert_eq!(text, "ab");
        undo(&mut text, &mut h);
        assert_eq!(text, "a");
    }

    #[test]
    fn undo_and_redo_restore_text_and_selection() {
        let (mut text, mut h) = (String::from("hello"), History::default());
        edit(&mut text, &mut h, 5..5, " world");
        edit(&mut text, &mut h, 0..5, "HELLO");
        assert_eq!(text, "HELLO world");

        let u = h.undo().unwrap();
        apply(&mut text, &u);
        assert_eq!((text.as_str(), u.selection_before), ("hello world", 0..5));
        let u = h.undo().unwrap();
        apply(&mut text, &u);
        assert_eq!(text, "hello");
        assert!(h.undo().is_none());

        let r = h.redo().unwrap();
        apply(&mut text, &r);
        assert_eq!((text.as_str(), r.selection_before), ("hello world", 11..11));
    }

    #[test]
    fn a_new_edit_clears_redo() {
        let (mut text, mut h) = (String::from("a"), History::default());
        edit(&mut text, &mut h, 1..1, "b");
        let u = h.undo().unwrap();
        apply(&mut text, &u);
        edit(&mut text, &mut h, 1..1, "c");
        assert!(h.redo().is_none());
        assert_eq!(text, "ac");
    }

    #[test]
    fn memory_is_bounded() {
        let (mut text, mut h) = (String::new(), History::default());
        for _ in 0..MAX_STEPS + 50 {
            let end = text.len();
            edit(&mut text, &mut h, end..end, "x");
        }
        assert_eq!(h.undo.len(), MAX_STEPS);
    }
}
