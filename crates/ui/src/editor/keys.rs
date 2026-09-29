//! The text editor's keys. The app binds them (its keymap) and so does every plugin
//! ([`init_plugin`](crate::init_plugin)), as a plugin doesn't get the app's: an input
//! there needs them to edit.

use gpui::KeyBinding;

use super::actions::*;

/// Bindings for text editing wherever an input has focus, in each context of the editor:
/// `Editor`, `Editor && multiline`, `Editor && showing_completion`.
pub fn key_bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("backspace", Backspace, Some(CONTEXT)),
        KeyBinding::new("shift-backspace", Backspace, Some(CONTEXT)),
        KeyBinding::new("delete", Delete, Some(CONTEXT)),
        KeyBinding::new("alt-backspace", DeleteWordLeft, Some(CONTEXT)),
        KeyBinding::new("cmd-backspace", DeleteToLineStart, Some(CONTEXT)),
        KeyBinding::new("left", Left, Some(CONTEXT)),
        KeyBinding::new("right", Right, Some(CONTEXT)),
        KeyBinding::new("up", Up, Some(CONTEXT)),
        KeyBinding::new("down", Down, Some(CONTEXT)),
        KeyBinding::new("ctrl-p", Up, Some(CONTEXT)),
        KeyBinding::new("ctrl-n", Down, Some(CONTEXT)),
        KeyBinding::new("alt-left", WordLeft, Some(CONTEXT)),
        KeyBinding::new("alt-right", WordRight, Some(CONTEXT)),
        KeyBinding::new("cmd-left", LineStart, Some(CONTEXT)),
        KeyBinding::new("home", LineStart, Some(CONTEXT)),
        KeyBinding::new("ctrl-a", LineStart, Some(CONTEXT)),
        KeyBinding::new("cmd-right", LineEnd, Some(CONTEXT)),
        KeyBinding::new("end", LineEnd, Some(CONTEXT)),
        KeyBinding::new("ctrl-e", LineEnd, Some(CONTEXT)),
        KeyBinding::new("cmd-up", DocStart, Some(CONTEXT)),
        KeyBinding::new("cmd-down", DocEnd, Some(CONTEXT)),
        KeyBinding::new("shift-left", SelectLeft, Some(CONTEXT)),
        KeyBinding::new("shift-right", SelectRight, Some(CONTEXT)),
        KeyBinding::new("shift-up", SelectUp, Some(CONTEXT)),
        KeyBinding::new("shift-down", SelectDown, Some(CONTEXT)),
        KeyBinding::new("alt-shift-left", SelectWordLeft, Some(CONTEXT)),
        KeyBinding::new("alt-shift-right", SelectWordRight, Some(CONTEXT)),
        KeyBinding::new("cmd-shift-left", SelectLineStart, Some(CONTEXT)),
        KeyBinding::new("cmd-shift-right", SelectLineEnd, Some(CONTEXT)),
        KeyBinding::new("cmd-shift-up", SelectDocStart, Some(CONTEXT)),
        KeyBinding::new("cmd-shift-down", SelectDocEnd, Some(CONTEXT)),
        KeyBinding::new("cmd-a", SelectAll, Some(CONTEXT)),
        KeyBinding::new("cmd-c", Copy, Some(CONTEXT)),
        KeyBinding::new("cmd-x", Cut, Some(CONTEXT)),
        KeyBinding::new("cmd-v", Paste, Some(CONTEXT)),
        KeyBinding::new("cmd-z", Undo, Some(CONTEXT)),
        KeyBinding::new("cmd-shift-z", Redo, Some(CONTEXT)),
        KeyBinding::new("ctrl-cmd-space", ShowCharacterPalette, Some(CONTEXT)),
        KeyBinding::new("shift-enter", Newline, Some("Editor && multiline")),
        KeyBinding::new("alt-enter", Newline, Some("Editor && multiline")),
        KeyBinding::new("tab", AcceptCompletion, Some("Editor && showing_completion")),
    ]
}
