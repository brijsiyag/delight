//! ⌃R: searching the input history. The bar's input becomes the search, and a list
//! below it (the window keeps its width) shows the remembered inputs containing its
//! words, newest first. ↵ puts the selected one in the input and brings up the tool
//! it was remembered for; Esc, or ⌃R again, goes back to the input as it was.

use delight_ui::theme::{INPUT_FONT_SIZE, INPUT_LINE_HEIGHT};
use delight_ui::{ActiveTheme, EditorEvent, EditorFont, Icon, IconName, LogoBadge, TextEditor, Theme, h_flex, v_flex};
use gpui::{
    AnyElement, App, AppContext as _, Context, Entity, Focusable as _, IntoElement, ParentElement, ScrollHandle,
    Styled, Subscription, Window, actions, div, prelude::*, px,
};

use super::Launcher;
use crate::history::{self, RememberedInput};
use crate::plugins;

/// The key context around the search input.
pub const CONTEXT: &str = "HistorySearch";

actions!(
    history,
    [
        /// Open the history search (from the launcher).
        Search,
        /// The next (older) match.
        SelectNext,
        /// The previous (newer) match.
        SelectPrevious,
        /// Put the selected input in the launcher.
        Confirm,
        /// Back to the launcher's input as it was.
        Cancel,
    ]
);

pub struct HistorySearch {
    pub query: Entity<TextEditor>,
    matches: Vec<RememberedInput>,
    selected: usize,
    scroll: ScrollHandle,
    _subscription: Subscription,
}

impl Launcher {
    /// Open the search, starting from the input if it's one line.
    pub(super) fn open_history(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.input.read(cx).text().to_string();
        let start = if text.contains('\n') { String::new() } else { text };
        let query = cx.new(|cx| {
            let mut query = TextEditor::new(window, cx)
                .font(EditorFont::Input)
                .text_size(px(INPUT_FONT_SIZE), px(INPUT_LINE_HEIGHT))
                .placeholder("Search the history");
            query.set_text(start, cx);
            query.select_all_text(cx);
            query
        });
        let subscription = cx.subscribe(&query, |this, _, event, cx| {
            if *event == EditorEvent::Changed {
                this.search_history(cx);
            }
        });
        window.focus(&query.focus_handle(cx), cx);
        self.history = Some(HistorySearch {
            query,
            matches: Vec::new(),
            selected: 0,
            scroll: ScrollHandle::new(),
            _subscription: subscription,
        });
        self.search_history(cx);
    }

    fn search_history(&mut self, cx: &mut Context<Self>) {
        let Some(search) = &self.history else { return };
        let query = search.query.read(cx).text().to_string();
        let matches = history::get(cx).search(&query).into_iter().cloned().collect();
        let Some(search) = &mut self.history else { return };
        search.matches = matches;
        search.selected = 0;
        search.scroll.scroll_to_item(0);
        cx.notify();
    }

    pub(super) fn move_in_history(&mut self, by: isize, cx: &mut Context<Self>) {
        let Some(search) = &mut self.history else { return };
        let last = search.matches.len().saturating_sub(1);
        search.selected = search.selected.saturating_add_signed(by).min(last);
        search.scroll.scroll_to_item(search.selected);
        cx.notify();
    }

    /// Put the `index`th match (the selected one if `None`) in the input, and prefer
    /// the tool it was remembered for.
    pub(super) fn confirm_history(&mut self, index: Option<usize>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(search) = self.history.take() else { return };
        window.focus(&self.input.focus_handle(cx), cx);
        if let Some(found) = search.matches.get(index.unwrap_or(search.selected)) {
            self.prefer_tool = Some((found.plugin_id.clone(), found.operation_id.clone()));
            let text = found.text.clone();
            self.input.update(cx, |input, cx| input.set_text(text, cx));
        }
        cx.notify();
    }

    pub(super) fn close_history(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.history.take().is_some() {
            window.focus(&self.input.focus_handle(cx), cx);
            cx.notify();
        }
    }

    /// The matches below the search, newest first, each with the plugin that
    /// remembered it.
    pub(super) fn render_history(&self, search: &HistorySearch, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let mut list =
            v_flex().id("history").flex_1().min_h(px(0.)).overflow_y_scroll().track_scroll(&search.scroll).py(px(6.));
        if search.matches.is_empty() {
            let message = if search.query.read(cx).text().trim().is_empty() {
                "Nothing remembered yet: tools remember inputs worth coming back to"
            } else {
                "No remembered input has these words"
            };
            list = list.child(div().px(px(20.)).py(px(8.)).text_color(t.text_faint).child(message));
        }
        let plugins = plugins::all(cx);
        for (i, found) in search.matches.iter().enumerate() {
            let selected = i == search.selected;
            let plugin = plugins.iter().find(|plugin| plugin.manifest().plugin.id == found.plugin_id);
            // A plugin that's gone shows as a puzzle piece, with its id for a name.
            let (icon, name) = match plugin {
                Some(plugin) => {
                    let manifest = &plugin.manifest().plugin;
                    let badge = LogoBadge::new(manifest.icon.as_bytes()).size(px(20.)).into_any_element();
                    (badge, manifest.name.clone())
                }
                None => {
                    let puzzle = Icon::new(IconName::Puzzle).size(px(20.)).color(t.text_faint).into_any_element();
                    (puzzle, found.plugin_id.clone())
                }
            };
            let hover = t.fill_subtle();
            let secondary = if selected { t.accent_text.opacity(0.8) } else { t.text_faint };
            list = list.child(
                h_flex()
                    .id(("remembered", i))
                    .mx(px(6.))
                    .px(px(12.))
                    .py(px(6.))
                    .gap(px(10.))
                    .rounded(t.radius_small())
                    .cursor_pointer()
                    .when(selected, |row| row.bg(t.accent).text_color(t.accent_text))
                    .when(!selected, |row| row.hover(move |style| style.bg(hover)))
                    .on_click(cx.listener(move |this, _, window, cx| this.confirm_history(Some(i), window, cx)))
                    .child(icon)
                    // One line: a multi-line input shows its line breaks as ⏎.
                    .child(div().flex_1().min_w(px(0.)).truncate().child(found.text.replace('\n', " ⏎ ")))
                    .child(div().flex_shrink_0().text_size(t.text_size_small()).text_color(secondary).child(name)),
            );
        }
        list.into_any_element()
    }
}

/// The search's icon, in place of the bolt.
pub fn icon(cx: &App) -> Icon {
    Icon::new(IconName::History).color(cx.theme().text_muted)
}
