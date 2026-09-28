//! The editor's actions and the handler each one runs. Their keys are in
//! Delight's keymap (the app's `keymaps/default-macos.json`), bound in the
//! `Editor` key context; the editor's key context adds what the keymap can
//! test, e.g. `Editor && showing_completion`.

use gpui::{Context, Div, InteractiveElement};

use super::TextEditor;

/// The key context the editor's bindings apply in.
pub const CONTEXT: &str = "Editor";

macro_rules! editor_actions {
    ($($action:ident => $handler:ident),* $(,)?) => {
        gpui::actions!(editor, [$($action),*]);

        /// Routes every action to its handler on `editor`.
        pub(super) fn wire(element: Div, cx: &mut Context<TextEditor>) -> Div {
            element $(.on_action(cx.listener(TextEditor::$handler)))*
        }
    };
}

editor_actions! {
    Backspace => backspace,
    Delete => delete,
    DeleteWordLeft => delete_word_left,
    DeleteToLineStart => delete_to_line_start,
    Left => left,
    Right => right,
    Up => up,
    Down => down,
    WordLeft => word_left,
    WordRight => word_right,
    LineStart => line_start,
    LineEnd => line_end,
    DocStart => doc_start,
    DocEnd => doc_end,
    SelectLeft => select_left,
    SelectRight => select_right,
    SelectUp => select_up,
    SelectDown => select_down,
    SelectWordLeft => select_word_left,
    SelectWordRight => select_word_right,
    SelectLineStart => select_line_start,
    SelectLineEnd => select_line_end,
    SelectDocStart => select_doc_start,
    SelectDocEnd => select_doc_end,
    SelectAll => select_all,
    AcceptCompletion => accept_completion,
    Newline => newline,
    Copy => copy,
    Cut => cut,
    Paste => paste,
    Undo => undo,
    Redo => redo,
    ShowCharacterPalette => show_character_palette,
}
