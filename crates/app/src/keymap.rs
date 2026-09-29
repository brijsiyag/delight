//! Delight's key bindings, compiled in; they aren't user-configurable. Each binds a
//! keystroke to an action within a key context (`Editor && multiline`). A key goes
//! to the innermost focused context that binds it, and within one context the later
//! binding wins:
//!
//! * `Editor`: a text input has focus.
//! * `HistorySearch`: the launcher's history search (⌃R) is open.
//! * `ToolList`: the launcher's tool list has focus.
//! * `Tool`: the selected tool has focus. The tool sees keys first; the ones it
//!   leaves alone come back out (`Launcher::on_tool_key_down`).
//! * `Launcher`: anywhere in the launcher window.
//! * `Settings`: the settings window.
//! * no context: anywhere in Delight.

use delight_ui::editor::actions::CONTEXT as EDITOR;
use gpui::{App, KeyBinding, NoAction};

use crate::Quit;
use crate::settings_window::{self, CONTEXT as SETTINGS};
use crate::launcher::{
    self, CONTEXT as LAUNCHER, HISTORY_SEARCH_CONTEXT, SelectTool, TOOL_CONTEXT as TOOL, TOOL_LIST_CONTEXT as TOOL_LIST,
    history_actions as history,
};

pub fn init(cx: &mut App) {
    let launcher = Some(LAUNCHER);
    let tool_list = Some(TOOL_LIST);
    let tool = Some(TOOL);
    // The UI kit's own keys: the text editor's, and ← / → in a segmented control.
    cx.bind_keys(delight_ui::key_bindings());
    cx.bind_keys([
        KeyBinding::new("cmd-q", Quit, None),
        // The launcher.
        KeyBinding::new("escape", launcher::Dismiss, launcher),
        KeyBinding::new("cmd-k", launcher::ClearInput, launcher),
        KeyBinding::new("tab", launcher::FocusNext, launcher),
        KeyBinding::new("shift-tab", launcher::FocusPrevious, launcher),
        // ↓ past the end of the input moves to the tool list.
        KeyBinding::new("down", launcher::FocusTools, Some("Launcher > Editor && end_of_input")),
        // ↑ on the first tool goes back to the input.
        KeyBinding::new("up", launcher::SelectPrevious, tool_list),
        KeyBinding::new("down", launcher::SelectNext, tool_list),
        // → moves into the tool; inside it, Tab and ⇧Tab are the tool's own (it
        // moves between its controls), not the launcher's.
        KeyBinding::new("right", launcher::FocusTool, tool_list),
        KeyBinding::new("tab", NoAction, tool),
        KeyBinding::new("shift-tab", NoAction, tool),
        KeyBinding::new("ctrl-p", launcher::SelectPrevious, tool_list),
        KeyBinding::new("ctrl-n", launcher::SelectNext, tool_list),
        KeyBinding::new("ctrl-r", history::Search, launcher),
        KeyBinding::new("cmd-,", launcher::OpenSettings, launcher),
        KeyBinding::new("cmd-w", settings_window::CloseSettings, Some(SETTINGS)),
        KeyBinding::new("escape", settings_window::CloseSettings, Some(SETTINGS)),
        // While the input shows a completion, or is empty: ⌃N and ⌃P complete with the
        // next (older) and previous (newer) remembered input.
        KeyBinding::new("ctrl-n", launcher::OlderCompletion, Some("Launcher > Editor && (showing_completion || empty_input)")),
        KeyBinding::new("ctrl-p", launcher::NewerCompletion, Some("Launcher > Editor && (showing_completion || empty_input)")),
    ]);
    // Searching the input history: ↑/↓ pick an input, ↵ or Tab uses it, Esc or ⌃R
    // again goes back. Last, so they win over the launcher's input keys.
    let searching = format!("{HISTORY_SEARCH_CONTEXT} > {EDITOR}");
    let searching = Some(searching.as_str());
    cx.bind_keys([
        KeyBinding::new("up", history::SelectPrevious, searching),
        KeyBinding::new("down", history::SelectNext, searching),
        KeyBinding::new("ctrl-p", history::SelectPrevious, searching),
        KeyBinding::new("ctrl-n", history::SelectNext, searching),
        KeyBinding::new("ctrl-r", history::Cancel, searching),
        KeyBinding::new("enter", history::Confirm, searching),
        KeyBinding::new("tab", history::Confirm, searching),
        KeyBinding::new("escape", history::Cancel, searching),
    ]);
    // The nth tool in the list.
    cx.bind_keys((1..=9).map(|n| KeyBinding::new(&format!("cmd-{n}"), SelectTool(n), launcher)));
}
