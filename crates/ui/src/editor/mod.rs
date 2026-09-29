//! [`TextEditor`]: a soft-wrapping, single- or multi-line text field built on
//! GPUI's text system (GPUI has no text input of its own; this follows the
//! pattern of GPUI's `examples/input.rs`).
//!
//! Used for the launcher's input. Its keys come from Delight's keymap (the
//! `Editor` context), like every other key.
//!
//! * [`text`] — pure navigation: graphemes, words, lines, UTF-16.
//! * [`history`] — undo/redo of edits, merging consecutive typing.
//! * `blink` — the blinking cursor.
//! * `actions` — the editor's actions and their handlers (the keys are in
//!   the app's keymap).
//! * `state` — the editor entity and its editing primitives.
//! * `commands` — what each key and mouse gesture does.
//! * `ime` — the platform's text input.
//! * `element` — layout and painting.

mod blink;
mod commands;
mod element;
pub mod history;
mod keys;
mod ime;
pub mod actions;
mod state;
pub mod text;

pub use keys::key_bindings;
pub use state::{EditorEvent, EditorFont, TextEditor};
