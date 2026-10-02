//! The launcher: a floating input bar that grows into a panel once there's input,
//! with the tools that fit it on the left, the selected tool on the right, and its
//! actions in the footer.
//!
//! * `window`: opening, showing and hiding the window, and what the rest of the
//!   app asks of it.
//! * this file: the launcher's state and behaviour.
//! * `view`: drawing it.
//! * `tool_pane`: one tool, on the surface its plugin draws on.
//! * `footer`: which key runs which footer action.
//! * `history_search`: ⌃R, searching the input history.
//! * `tips`: the input's placeholder, a tip about Delight's keys.
//!
//! Keys come from the keymap (`crate::keymap`) by focus: the input
//! (`Launcher > Editor`) edits text, the tool list (`Launcher > ToolList`) moves
//! between tools, and `Launcher` keys work in both.

mod footer;
mod history_search;
mod tips;
mod tool_pane;
mod view;
mod window;

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use delight_protocol::{Action, Input};
use delight_runtime::{Plugin, RECOMMENDED, detect_all};
use delight_ui::theme::{INPUT_FONT_SIZE, INPUT_LINE_HEIGHT};
use delight_ui::{EditorEvent, EditorFont, TextEditor};
use gpui::{
    App, AppContext as _, Context, Entity, FocusHandle, Focusable as _, KeyDownEvent, Keystroke, ScrollHandle,
    SharedString, Subscription, Task, Window, actions, px,
};

pub use history_search::CONTEXT as HISTORY_SEARCH_CONTEXT;
pub use window::{
    bounds, focus_gained, focus_left, hide, hide_then, level, open, plugins_loaded, refresh, set_input, show, shows_plugin, toast,
    toggle,
};

use crate::macos::{self, NativeWindow};
use crate::{history, plugins, settings};
use history_search::HistorySearch;
use tool_pane::ToolPane;

/// The history search's actions, for the keymap.
pub mod history_actions {
    pub use super::history_search::{Cancel, Confirm, Search, SelectNext, SelectPrevious};
}

/// The empty bar, and the panel once there's input.
const BAR_WIDTH: f32 = 640.;
const PANEL_WIDTH: f32 = 800.;
/// macOS Spotlight's search field: 56pt tall, a pill (fully rounded ends).
const BAR_HEIGHT: f32 = 56.;
const BAR_RADIUS: f32 = BAR_HEIGHT / 2.;
/// Spotlight's padding before the icon, and between the icon and the text.
const BAR_PADDING_X: f32 = 20.;
const BAR_ICON_GAP: f32 = 16.;
const BAR_ICON_SIZE: f32 = 22.;
const PANEL_HEIGHT: f32 = 540.;
const PANEL_RADIUS: f32 = 24.;
/// Typing pauses this long before the tools are asked again.
const DETECT_DELAY: Duration = Duration::from_millis(30);
const TOAST: Duration = Duration::from_millis(1600);
/// Clipboard text larger than this isn't pasted on open.
const AUTO_PASTE_MAX_BYTES: usize = 1 << 20;
const STOPPED_TOAST: Duration = Duration::from_secs(8);

/// The key context of the launcher window.
pub const CONTEXT: &str = "Launcher";
/// The key context of the tool list, while it has focus.
pub const TOOL_LIST_CONTEXT: &str = "ToolList";
/// The key context around the selected tool's view: keys the tool doesn't use come
/// back out through it.
pub const TOOL_CONTEXT: &str = "Tool";

actions!(
    launcher,
    [
        Dismiss,
        ClearInput,
        /// Move focus to the next field: the input, then the tool list.
        FocusNext,
        FocusPrevious,
        /// Move focus to the tool list.
        FocusTools,
        /// Move focus into the selected tool, to its first control.
        FocusTool,
        /// In the tool list; on the first tool, back to the input.
        SelectPrevious,
        SelectNext,
        /// Complete the input with an older remembered input.
        OlderCompletion,
        /// Complete the input with a newer remembered input.
        NewerCompletion,
        OpenSettings,
    ]
);

/// Select the nth tool in the list, from 1.
#[derive(Clone, Debug, PartialEq, gpui::Action)]
#[action(namespace = launcher, no_json)]
pub struct SelectTool(pub usize);

/// A tool, by (plugin id, operation id): the name the input history knows it by, and the key of
/// everything the launcher keeps about it. Both ids are unique (a second plugin with an id is
/// refused at load, a manifest can't list an operation twice), and they stay the same when the
/// plugins start again, whatever order they come in.
type ToolIds = (String, String);

/// A tool that fits the input, and how well.
#[derive(Clone, Debug, PartialEq)]
struct Match {
    tool: ToolIds,
    /// Above 0, at most 1.
    confidence: f32,
}

impl Match {
    fn is_recommended(&self) -> bool {
        self.confidence >= RECOMMENDED
    }
}

/// The ids of the `operation`th tool in `plugin`'s manifest.
fn ids(plugin: &Plugin, operation: usize) -> Option<ToolIds> {
    let manifest = plugin.manifest();
    Some((manifest.plugin.id.clone(), manifest.operations.get(operation)?.id.clone()))
}

/// The plugin a tool is of, and the tool's index in that plugin's manifest; `None` once either is
/// gone.
fn find<'a>(plugins: &'a [Plugin], (plugin_id, operation_id): &ToolIds) -> Option<(&'a Plugin, usize)> {
    let plugin = plugins.iter().find(|plugin| plugin.manifest().plugin.id == *plugin_id)?;
    let operation = plugin.manifest().operations.iter().position(|operation| operation.id == *operation_id)?;
    Some((plugin, operation))
}

pub struct Launcher {
    focus_handle: FocusHandle,
    /// The window's AppKit side, changed outside GPUI updates (see `macos`).
    native: Option<NativeWindow>,
    input: Entity<TextEditor>,
    /// The tool list's focus (a Tab stop after the input).
    list_focus: FocusHandle,
    list_scroll: ScrollHandle,
    /// The tools that fit the input, best first.
    candidates: Vec<Match>,
    selected: Option<usize>,
    /// The tool the user picked: it stays selected while the input changes, as long
    /// as it still fits.
    picked: Option<ToolIds>,
    /// The tool the completion showing now was remembered for.
    completion_tool: Option<ToolIds>,
    /// Which remembered input the completion shows: 0 the newest that fits, then
    /// older ones (⌃N older, ⌃P newer). Back to 0 whenever the input changes.
    completion_index: usize,
    /// ⌃N or ⌃P was pressed in the empty input: it shows the remembered inputs as
    /// completions (it offers none by itself). Off whenever the input changes.
    browsing_history: bool,
    /// After a remembered input is taken (a completion, or from ⌃R): the tool it
    /// was for, to select once the plugins have been asked about it.
    prefer_tool: Option<ToolIds>,
    /// The history search (⌃R), while it's open.
    history: Option<HistorySearch>,
    /// The tip the input shows while it's empty.
    tip: String,
    /// The clipboard text pasted last on open, so an unchanged clipboard doesn't
    /// replace what was typed since.
    last_auto_paste: Option<String>,
    /// The tools opened so far, kept with their state while the plugins run; the
    /// launcher redraws when a tool's actions change.
    panes: HashMap<ToolIds, (Entity<ToolPane>, Subscription)>,
    /// The tool whose view is shown: the others are hidden (see [`Launcher::show_only`]).
    shown_tool: Option<ToolIds>,
    /// The launcher window is hidden: no tool's view is shown.
    window_hidden: bool,
    /// The footer's pin is on: the launcher stays up while another app is used, until it is next
    /// hidden (Esc, the shortcut). The "Hide when another app is used" setting is left as it is.
    pinned: bool,
    /// Replacing it cancels the detection before.
    detecting: Option<Task<()>>,
    toast: Option<SharedString>,
    toast_timer: Option<Task<()>>,
    /// Plugins whose stop was already announced, by id.
    announced_stops: HashSet<String>,
    /// The window's size (width, height), once set.
    size: Option<(f32, f32)>,
    _subscriptions: Vec<Subscription>,
}

impl Launcher {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        // The input as it was when the launcher last hid, if the history is on.
        let restored = if settings::get(cx).input_history {
            history::get(cx).input_to_restore().unwrap_or_default().to_string()
        } else {
            String::new()
        };
        let tip = tips::next("", cx);
        let input = cx.new(|cx| {
            let mut input = TextEditor::new(window, cx)
                .multiline(px(INPUT_LINE_HEIGHT * 4.))
                .font(EditorFont::Input)
                .text_size(px(INPUT_FONT_SIZE), px(INPUT_LINE_HEIGHT))
                .placeholder(tip.clone());
            input.set_text(restored, cx);
            input
        });
        let subscriptions = vec![
            cx.subscribe(&input, |this, _, event, cx| this.on_input_event(event, cx)),
            cx.observe_window_appearance(window, |_, _, cx| crate::theme::appearance_changed(cx)),
            // Losing the keyboard to something else than the launcher and the windows of plugins
            // hides them all, if that's on.
            cx.observe_window_activation(window, |_, window, cx| {
                if window.is_window_active() {
                    focus_gained(cx);
                } else if macos::is_window_visible(window) {
                    cx.defer(focus_left);
                }
            }),
        ];
        let mut launcher = Self {
            focus_handle: cx.focus_handle(),
            native: NativeWindow::of(window),
            input,
            list_focus: cx.focus_handle().tab_stop(true),
            list_scroll: ScrollHandle::new(),
            candidates: Vec::new(),
            selected: None,
            picked: None,
            completion_tool: None,
            completion_index: 0,
            browsing_history: false,
            prefer_tool: None,
            history: None,
            tip,
            last_auto_paste: None,
            panes: HashMap::new(),
            shown_tool: None,
            window_hidden: false,
            pinned: false,
            detecting: None,
            toast: None,
            toast_timer: None,
            announced_stops: HashSet::new(),
            size: None,
            _subscriptions: subscriptions,
        };
        launcher.input_changed(cx);
        launcher
    }

    /// What the tools get.
    fn input(&self, cx: &App) -> Input {
        Input { text: self.input.read(cx).text().to_string() }
    }

    fn is_expanded(&self, cx: &App) -> bool {
        !self.input.read(cx).text().trim().is_empty()
    }

    /// The bar while the input is empty, the panel otherwise. The history search
    /// keeps the width and drops down to the panel's height.
    fn wanted_size(&self, cx: &App) -> (f32, f32) {
        if self.history.is_some() {
            (self.size.map_or(BAR_WIDTH, |(width, _)| width), PANEL_HEIGHT)
        } else if self.is_expanded(cx) {
            (PANEL_WIDTH, PANEL_HEIGHT)
        } else {
            (BAR_WIDTH, BAR_HEIGHT)
        }
    }

    /// Resize the window to [`Self::wanted_size`]; it grows down.
    fn sync_window_size(&mut self, cx: &mut Context<Self>) {
        let size = self.wanted_size(cx);
        if self.size == Some(size) {
            return;
        }
        let animate = self.size.is_some();
        self.size = Some(size);
        let Some(native) = self.native.clone() else { return };
        let (width, height) = size;
        let radius = self.corner_radius();
        // Outside this update, where GPUI hears about the resize.
        cx.spawn(async move |_, _| {
            native.resize_keep_top(width.into(), height.into(), animate);
            native.set_corner_radius(radius.into());
        })
        .detach();
    }

    /// A pill for the bar, rounded corners once it's taller.
    fn corner_radius(&self) -> f32 {
        if self.size.is_some_and(|(_, height)| height > BAR_HEIGHT) { PANEL_RADIUS } else { BAR_RADIUS }
    }

    // -------------------------------------------------------------------------
    // Input
    // -------------------------------------------------------------------------

    fn on_input_event(&mut self, event: &EditorEvent, cx: &mut Context<Self>) {
        match event {
            EditorEvent::Changed => self.input_changed(cx),
            // The whole completion was taken (`Changed` follows): bring its tool up.
            EditorEvent::CompletionAccepted => self.prefer_tool = self.completion_tool.take(),
            EditorEvent::Focus | EditorEvent::Blur => {}
        }
    }

    fn input_changed(&mut self, cx: &mut Context<Self>) {
        if !self.is_expanded(cx) {
            self.picked = None;
        }
        self.completion_index = 0;
        self.browsing_history = false;
        self.show_completion(cx);
        self.detect(cx);
        cx.notify();
    }

    /// Offer how a remembered input would complete the text (Tab takes it), if the
    /// history is on.
    fn show_completion(&mut self, cx: &mut Context<Self>) {
        let text = self.input.read(cx).text().to_string();
        // An empty input offers a completion only once ⌃N or ⌃P asked for one.
        let offered = settings::get(cx).input_history && (!text.is_empty() || self.browsing_history);
        let completion = offered.then(|| history::get(cx).completion_for(&text, self.completion_index)).flatten();
        self.completion_tool = completion.map(|c| (c.plugin_id.to_string(), c.operation_id.to_string()));
        let remainder = completion.map(|c| SharedString::from(c.remainder.to_string()));
        self.input.update(cx, |input, cx| input.set_completion(remainder, cx));
    }

    /// Complete with an older (`by` 1) or newer (-1) remembered input, if there's
    /// one. In the empty input the first press shows the newest, whichever way.
    fn step_completion(&mut self, by: isize, cx: &mut Context<Self>) {
        let text = self.input.read(cx).text().to_string();
        if text.is_empty() && !self.browsing_history {
            if settings::get(cx).input_history && history::get(cx).completion_for("", 0).is_some() {
                self.browsing_history = true;
                self.completion_index = 0;
                self.show_completion(cx);
            }
            return;
        }
        let Some(index) = self.completion_index.checked_add_signed(by) else { return };
        if history::get(cx).completion_for(&text, index).is_some() {
            self.completion_index = index;
            self.show_completion(cx);
        }
    }

    /// The plugins started again: an open tool whose plugin went on keeps its pane, with what it
    /// shows. One whose plugin started again (its file changed, an update) or went is a view of an
    /// instance that no longer runs: its pane goes, which lets that instance go, and the tool opens
    /// afresh when it is next selected. Tools are named by ids, so nothing else changes.
    fn drop_stale_tools(&mut self, cx: &App) {
        let plugins = plugins::all(cx);
        self.panes.retain(|_, (pane, _)| plugins.iter().any(|running| running.same_instance(pane.read(cx).plugin())));
    }

    /// Show another tip in the empty input.
    fn show_next_tip(&mut self, cx: &mut Context<Self>) {
        self.tip = tips::next(&self.tip, cx);
        let tip = self.tip.clone();
        self.input.update(cx, |input, cx| input.set_placeholder(tip, cx));
    }

    /// Replace the input with the clipboard's text, if it changed since the last time
    /// (and isn't blank or huge).
    fn paste_clipboard(&mut self, cx: &mut Context<Self>) {
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return;
        };
        let unchanged = self.last_auto_paste.as_ref() == Some(&text);
        if text.trim().is_empty() || text.len() > AUTO_PASTE_MAX_BYTES || unchanged {
            return;
        }
        self.last_auto_paste = Some(text.clone());
        self.input.update(cx, |input, cx| input.set_text(text, cx));
    }

    /// Chaining tools: a plugin makes the input `text`, as an undoable edit that detects
    /// again. Only the plugin whose tool is selected may, so one working in the background
    /// can't rewrite what is being typed.
    fn set_input_from(&mut self, plugin_id: &str, text: String, cx: &mut Context<Self>) {
        if self.selected_candidate().is_none_or(|candidate| candidate.tool.0 != plugin_id) {
            log::warn!("{plugin_id} set the launcher's input without its tool selected: ignored");
            return;
        }
        self.input.update(cx, |input, cx| input.set_text(text, cx));
    }

    fn clear_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.input.update(cx, |input, cx| input.set_text("", cx));
        window.focus(&self.input.focus_handle(cx), cx);
    }

    // -------------------------------------------------------------------------
    // Tools
    // -------------------------------------------------------------------------

    /// Ask the plugins about the input once typing pauses: those that are on and have
    /// a tool that's on. Tools that are off aren't listed.
    fn detect(&mut self, cx: &mut Context<Self>) {
        let input = self.input(cx);
        self.detecting = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(DETECT_DELAY).await;
            let (asked, detected) = cx.update(|cx| {
                let settings = settings::get(cx);
                let asked: Vec<Plugin> = plugins::all(cx)
                    .iter()
                    .filter(|plugin| {
                        let manifest = plugin.manifest();
                        let id = &manifest.plugin.id;
                        manifest.operations.iter().any(|operation| settings.tool_runs(id, &operation.id))
                    })
                    .cloned()
                    .collect();
                let detected = detect_all(&asked, &input, cx);
                (asked, detected)
            });
            let candidates = detected.await;
            this.update(cx, |this, cx| {
                let settings = settings::get(cx);
                // Each answer names a plugin by its place among those asked: by ids from here on, without
                // the tools that are off.
                let candidates = candidates
                    .into_iter()
                    .filter_map(|candidate| {
                        let tool = ids(asked.get(candidate.plugin)?, candidate.operation)?;
                        settings.tool_runs(&tool.0, &tool.1).then_some(Match { tool, confidence: candidate.confidence })
                    })
                    .collect();
                this.show_candidates(candidates, cx);
            })
            .ok();
        }));
    }

    /// List the tools and keep the selection: the picked tool, else the best one if
    /// it's recommended, else nothing.
    fn show_candidates(&mut self, candidates: Vec<Match>, cx: &mut Context<Self>) {
        self.candidates = candidates;
        // The remembered input's tool, or its plugin's first one if that isn't
        // listed (or the input was remembered without its operation).
        if let Some(tool) = self.prefer_tool.take() {
            let exact = self.candidates.iter().find(|c| c.tool == tool);
            let same_plugin = || self.candidates.iter().find(|c| c.tool.0 == tool.0);
            if let Some(found) = exact.or_else(same_plugin).map(|c| c.tool.clone()) {
                self.picked = Some(found);
            }
        }
        let picked = self.picked.as_ref().and_then(|picked| self.candidates.iter().position(|c| &c.tool == picked));
        let best = self.candidates.first().filter(|c| c.is_recommended()).map(|_| 0);
        self.selected = picked.or(best);
        self.announce_stops(cx);
        self.update_selected_pane(cx);
        cx.notify();
    }

    fn selected_candidate(&self) -> Option<&Match> {
        self.candidates.get(self.selected?)
    }

    /// Select the `index`th tool: the user picked it.
    fn select(&mut self, index: usize, cx: &mut Context<Self>) {
        if index >= self.candidates.len() || self.selected == Some(index) {
            return;
        }
        self.selected = Some(index);
        self.picked = Some(self.candidates[index].tool.clone());
        self.list_scroll.scroll_to_item(index);
        self.update_selected_pane(cx);
        cx.notify();
    }

    /// On the first tool (or none), back to the input.
    fn select_previous(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.selected.and_then(|index| index.checked_sub(1)) {
            Some(index) => self.select(index, cx),
            None => window.focus(&self.input.focus_handle(cx), cx),
        }
    }

    fn select_next(&mut self, cx: &mut Context<Self>) {
        self.select(self.selected.map_or(0, |index| index + 1), cx);
    }

    /// Move focus to the tool list, selecting the first tool if none is.
    fn focus_tools(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.candidates.is_empty() {
            return;
        }
        if self.selected.is_none() {
            self.select(0, cx);
        }
        window.focus(&self.list_focus, cx);
        cx.notify();
    }

    /// Move focus into the selected tool: its view is the tab stop after the list, and
    /// focus arriving there goes on to the tool's first control.
    fn focus_tool(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_pane(cx).is_some() {
            window.focus_next(cx);
        }
    }

    /// Keys the tool left alone (it answers first): ← and ⇧Tab go back to the tool
    /// list, and Tab past the tool's last control on to the input.
    fn on_tool_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;
        let modifiers = keystroke.modifiers;
        let plain = !(modifiers.platform || modifiers.control || modifiers.alt || modifiers.function);
        match keystroke.key.as_str() {
            "left" if plain && !modifiers.shift => self.focus_tools(window, cx),
            "tab" if plain && modifiers.shift => self.focus_tools(window, cx),
            "tab" if plain => window.focus_next(cx),
            _ => return,
        }
        cx.stop_propagation();
    }

    /// Typing while the tool list has focus goes on in the input.
    fn on_list_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;
        let Some(text) = keystroke.key_char.clone() else {
            return;
        };
        if keystroke.modifiers.platform || keystroke.modifiers.control {
            return;
        }
        cx.stop_propagation();
        window.focus(&self.input.focus_handle(cx), cx);
        self.input.update(cx, |input, cx| input.insert(&text, cx));
    }

    /// The selected tool's pane, opened on first use; `None` if its plugin stopped.
    fn selected_pane(&mut self, cx: &mut Context<Self>) -> Option<Entity<ToolPane>> {
        let tool = self.selected_candidate()?.tool.clone();
        if let Some((pane, _)) = self.panes.get(&tool) {
            return Some(pane.clone());
        }
        let plugins = plugins::all(cx);
        let (plugin, _) = find(&plugins, &tool)?;
        if plugin.stopped().is_some() {
            return None;
        }
        let pane = cx.new(|cx| ToolPane::new(plugin, &tool.1, cx));
        let redraw = cx.observe(&pane, |_, _, cx| cx.notify());
        self.panes.insert(tool, (pane.clone(), redraw));
        Some(pane)
    }

    /// Show `tool`'s view, and only it. The tools of one plugin all sit in the same place of that
    /// plugin's own copy of this window, so a tool that stayed there while another was shown
    /// would take the other's clicks and scrolling, and be drawn for nothing. A hidden tool keeps
    /// its state and its last picture, and is shown again at once.
    fn show_only(&mut self, tool: Option<ToolIds>, cx: &mut Context<Self>) {
        if self.shown_tool == tool {
            return;
        }
        for (key, shown) in [(&self.shown_tool, false), (&tool, true)] {
            let Some(pane) = key.as_ref().and_then(|key| self.panes.get(key)).map(|(pane, _)| pane.clone()) else { continue };
            pane.update(cx, |pane, cx| pane.set_shown(shown, cx));
        }
        self.shown_tool = tool;
    }

    /// Whether the tool shown now is one of this plugin's.
    fn shows_plugin(&self, plugin_id: &str) -> bool {
        self.shown_tool.as_ref().is_some_and(|(plugin, _)| plugin == plugin_id)
    }

    /// The launcher window hid, or shows again: its tool's view goes, and comes back, with it.
    pub(super) fn window_shown(&mut self, shown: bool, cx: &mut Context<Self>) {
        self.window_hidden = !shown;
        // A pin lasts until the launcher hides.
        if !shown {
            self.pinned = false;
        }
        cx.notify();
    }

    /// The footer's pin: keep the launcher up while another app is used, or stop.
    fn toggle_pin(&mut self, cx: &mut Context<Self>) {
        self.pinned = !self.pinned;
        cx.notify();
    }

    /// Tell the selected tool what the input is now.
    fn update_selected_pane(&mut self, cx: &mut Context<Self>) {
        let input = self.input(cx);
        if let Some(pane) = self.selected_pane(cx) {
            pane.update(cx, |pane, cx| pane.on_input_changed(input, cx));
        }
    }

    /// A toast for each plugin that stopped since the last look (Delight keeps
    /// running without it).
    fn announce_stops(&mut self, cx: &mut Context<Self>) {
        for plugin in plugins::all(cx).iter() {
            let manifest = plugin.manifest();
            if let Some(reason) = plugin.stopped()
                && self.announced_stops.insert(manifest.plugin.id.clone())
            {
                let message = format!("{} stopped and is off until Delight restarts: {reason}", manifest.plugin.name);
                self.flash_for(message, STOPPED_TOAST, cx);
            }
        }
    }

    // -------------------------------------------------------------------------
    // Footer actions
    // -------------------------------------------------------------------------

    /// The selected tool's footer actions with their keys. An action's shortcut
    /// gives way to the keymap where the focus is.
    fn keyed_actions(&self, window: &Window, cx: &App) -> Vec<(Action, Option<Keystroke>)> {
        let Some((pane, _)) = self.selected_candidate().and_then(|c| self.panes.get(&c.tool)) else {
            return Vec::new();
        };
        let keymap = cx.key_bindings();
        let keymap = keymap.borrow();
        let context = window.context_stack();
        let is_bound = |keystroke: &Keystroke| !keymap.bindings_for_input(std::slice::from_ref(keystroke), &context).0.is_empty();
        footer::keyed(pane.read(cx).actions().to_vec(), is_bound)
    }

    /// Run the footer action whose shortcut was pressed (the keymap had no binding
    /// for it).
    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let pressed = &event.keystroke;
        let hit = self
            .keyed_actions(window, cx)
            .into_iter()
            .find(|(_, key)| key.as_ref().is_some_and(|own| footer::matches(own, pressed)));
        if let Some((action, _)) = hit {
            cx.stop_propagation();
            self.perform(&action, cx);
        }
    }

    /// Run a footer action: the tool does what it's for.
    fn perform(&mut self, action: &Action, cx: &mut Context<Self>) {
        let pane = self.selected_candidate().and_then(|c| self.panes.get(&c.tool)).map(|(pane, _)| pane.clone());
        if let Some(pane) = pane {
            pane.update(cx, |pane, cx| pane.perform(&action.id, cx));
        }
    }

    // -------------------------------------------------------------------------
    // Toasts
    // -------------------------------------------------------------------------

    fn flash(&mut self, message: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.flash_for(message, TOAST, cx);
    }

    /// Show `message` in the footer for `duration`.
    fn flash_for(&mut self, message: impl Into<SharedString>, duration: Duration, cx: &mut Context<Self>) {
        self.toast = Some(message.into());
        cx.notify();
        self.toast_timer = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(duration).await;
            this.update(cx, |this, cx| {
                this.toast = None;
                cx.notify();
            })
            .ok();
        }));
    }
}

