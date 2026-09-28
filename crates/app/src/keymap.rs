//! Delight's key bindings, compiled in; they aren't user-configurable. Each binds a
//! keystroke to an action within a key context (`Editor && multiline`). A key goes
//! to the innermost focused context that binds it, and within one context the later
//! binding wins:
//!
//! * `Editor`: a text input has focus.
//! * `Launcher`: anywhere in the launcher window.
//! * no context: anywhere in Delight.

use delight_ui::editor::actions::{self as editor, CONTEXT as EDITOR};
use gpui::{App, KeyBinding};

use crate::Quit;
use crate::launcher::{self, CONTEXT as LAUNCHER};

pub fn init(cx: &mut App) {
    let editor = Some(EDITOR);
    let multiline = Some("Editor && multiline");
    let completing = Some("Editor && showing_completion");
    let launcher = Some(LAUNCHER);
    cx.bind_keys([
        KeyBinding::new("cmd-q", Quit, None),
        // Text editing, wherever an input has focus.
        KeyBinding::new("backspace", editor::Backspace, editor),
        KeyBinding::new("shift-backspace", editor::Backspace, editor),
        KeyBinding::new("delete", editor::Delete, editor),
        KeyBinding::new("alt-backspace", editor::DeleteWordLeft, editor),
        KeyBinding::new("cmd-backspace", editor::DeleteToLineStart, editor),
        KeyBinding::new("left", editor::Left, editor),
        KeyBinding::new("right", editor::Right, editor),
        KeyBinding::new("up", editor::Up, editor),
        KeyBinding::new("down", editor::Down, editor),
        KeyBinding::new("ctrl-p", editor::Up, editor),
        KeyBinding::new("ctrl-n", editor::Down, editor),
        KeyBinding::new("alt-left", editor::WordLeft, editor),
        KeyBinding::new("alt-right", editor::WordRight, editor),
        KeyBinding::new("cmd-left", editor::LineStart, editor),
        KeyBinding::new("home", editor::LineStart, editor),
        KeyBinding::new("ctrl-a", editor::LineStart, editor),
        KeyBinding::new("cmd-right", editor::LineEnd, editor),
        KeyBinding::new("end", editor::LineEnd, editor),
        KeyBinding::new("ctrl-e", editor::LineEnd, editor),
        KeyBinding::new("cmd-up", editor::DocStart, editor),
        KeyBinding::new("cmd-down", editor::DocEnd, editor),
        KeyBinding::new("shift-left", editor::SelectLeft, editor),
        KeyBinding::new("shift-right", editor::SelectRight, editor),
        KeyBinding::new("shift-up", editor::SelectUp, editor),
        KeyBinding::new("shift-down", editor::SelectDown, editor),
        KeyBinding::new("alt-shift-left", editor::SelectWordLeft, editor),
        KeyBinding::new("alt-shift-right", editor::SelectWordRight, editor),
        KeyBinding::new("cmd-shift-left", editor::SelectLineStart, editor),
        KeyBinding::new("cmd-shift-right", editor::SelectLineEnd, editor),
        KeyBinding::new("cmd-shift-up", editor::SelectDocStart, editor),
        KeyBinding::new("cmd-shift-down", editor::SelectDocEnd, editor),
        KeyBinding::new("cmd-a", editor::SelectAll, editor),
        KeyBinding::new("cmd-c", editor::Copy, editor),
        KeyBinding::new("cmd-x", editor::Cut, editor),
        KeyBinding::new("cmd-v", editor::Paste, editor),
        KeyBinding::new("cmd-z", editor::Undo, editor),
        KeyBinding::new("cmd-shift-z", editor::Redo, editor),
        KeyBinding::new("ctrl-cmd-space", editor::ShowCharacterPalette, editor),
        KeyBinding::new("shift-enter", editor::Newline, multiline),
        KeyBinding::new("alt-enter", editor::Newline, multiline),
        KeyBinding::new("tab", editor::AcceptCompletion, completing),
        // The launcher.
        KeyBinding::new("escape", launcher::Dismiss, launcher),
        KeyBinding::new("cmd-k", launcher::ClearInput, launcher),
    ]);
}
