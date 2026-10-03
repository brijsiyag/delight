//! Drawing the launcher: the input bar, then (with input) the tool list, the
//! selected tool and the footer.

use delight_protocol::ActionStyle;
use delight_runtime::Plugin;
use delight_ui::theme::INPUT_LINE_HEIGHT;
use delight_ui::{
    ActiveTheme, Button, ButtonVariant, Caption, Divider, Icon, IconButton, IconName, Keycap, KeycapStyle, LogoBadge, Sizable as _,
    Theme, Tooltip, h_flex, keystroke_for, keystroke_label, v_flex,
};
use gpui::{
    AnyElement, App, Context, Div, FocusHandle, Focusable, FontWeight, IntoElement, KeyDownEvent, MouseButton, ParentElement,
    Render, Stateful, Styled, Window, div, prelude::*, px,
};

use super::{
    BAR_HEIGHT, BAR_ICON_GAP, BAR_ICON_SIZE, BAR_PADDING_X, CONTEXT, ClearInput, Dismiss, FocusNext, FocusPrevious,
    FocusTool, FocusTools, Launcher, NewerCompletion, OlderCompletion, OpenSettings, SelectNext, SelectPrevious,
    SelectTool, TOOL_CONTEXT, TOOL_LIST_CONTEXT, ToolIds, find, footer, hide, history_search,
};
use crate::{macos, plugin_windows, plugins, settings, settings_window};

const LIST_WIDTH: f32 = 200.;
const FOOTER_HEIGHT: f32 = 44.;
/// How many actions the footer shows; the others still answer their keys.
const FOOTER_ACTIONS: usize = 4;

impl Focusable for Launcher {
    fn focus_handle(&self, _: &gpui::App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for Launcher {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_window_size(cx);
        let t = cx.theme().clone();
        let root = v_flex()
            .key_context(CONTEXT)
            .track_focus(&self.focus_handle)
            .size_full()
            .overflow_hidden()
            .rounded(px(self.corner_radius()))
            // The tint decides the contrast; the native glass or blur underneath
            // adds the blur, and glass its shadow.
            .bg(t.window_tint())
            .border_1()
            .border_color(if macos::uses_liquid_glass() {
                // Spotlight's light rim.
                gpui::hsla(0., 0., 1., if t.dark { 0.2 } else { 0.6 })
            } else {
                t.border
            })
            .font_family(t.font.clone())
            .text_color(t.text)
            .text_size(t.text_size)
            .on_action(cx.listener(|_, _: &Dismiss, _, cx| cx.defer(hide)))
            .on_action(cx.listener(|this, _: &ClearInput, window, cx| this.clear_input(window, cx)))
            .on_action(cx.listener(|_, _: &FocusNext, window, cx| window.focus_next(cx)))
            .on_action(cx.listener(|_, _: &FocusPrevious, window, cx| window.focus_prev(cx)))
            .on_action(cx.listener(|this, _: &FocusTools, window, cx| this.focus_tools(window, cx)))
            .on_action(cx.listener(|this, _: &FocusTool, window, cx| this.focus_tool(window, cx)))
            .on_action(cx.listener(|this, _: &SelectPrevious, window, cx| this.select_previous(window, cx)))
            .on_action(cx.listener(|this, _: &SelectNext, _, cx| this.select_next(cx)))
            .on_action(cx.listener(|this, SelectTool(n): &SelectTool, _, cx| {
                if let Some(index) = n.checked_sub(1) {
                    this.select(index, cx);
                }
            }))
            .on_action(cx.listener(|_, _: &OpenSettings, _, cx| cx.defer(settings_window::open)))
            .on_action(cx.listener(|this, _: &OlderCompletion, _, cx| this.step_completion(1, cx)))
            .on_action(cx.listener(|this, _: &NewerCompletion, _, cx| this.step_completion(-1, cx)))
            .on_action(cx.listener(|this, _: &history_search::Search, window, cx| this.open_history(window, cx)))
            .on_action(cx.listener(|this, _: &history_search::SelectNext, _, cx| this.move_in_history(1, cx)))
            .on_action(cx.listener(|this, _: &history_search::SelectPrevious, _, cx| this.move_in_history(-1, cx)))
            .on_action(cx.listener(|this, _: &history_search::Confirm, window, cx| this.confirm_history(None, window, cx)))
            .on_action(cx.listener(|this, _: &history_search::Cancel, window, cx| this.close_history(window, cx)))
            .on_key_down(cx.listener(Self::on_key_down))
            // A click anywhere in it brings it in front of the plugins' windows, and gives it back the
            // keyboard if another app took it.
            .capture_any_mouse_down(|_, window, cx| {
                plugin_windows::raise(window, cx);
                super::window::clicked(window, cx);
            })
            .child(self.render_bar(&t, cx));
        if let Some(search) = &self.history {
            return root.child(Divider::horizontal()).child(self.render_history(search, &t, cx));
        }
        if !self.is_expanded(cx) {
            return root;
        }
        root.child(Divider::horizontal())
            .child(
                h_flex()
                    .flex_1()
                    .min_h(px(0.))
                    .child(self.render_list(&t, window, cx))
                    // Full height, between the list and the tool.
                    .child(div().w(px(1.)).h_full().flex_shrink_0().bg(t.separator()))
                    .child(self.render_detail(&t, cx)),
            )
            .child(Divider::horizontal())
            .child(self.render_footer(&t, window, cx))
    }
}

impl Launcher {
    fn render_bar(&self, t: &Theme, cx: &mut Context<Self>) -> impl IntoElement {
        // The first input line sits centred in the bar; more lines grow it down. The
        // icon and the clear button stay centred on that first line.
        let line = px(INPUT_LINE_HEIGHT);
        let on_first_line = |element: AnyElement| h_flex().h(line).flex_shrink_0().child(element);
        // While searching the history, its search input takes the input's place.
        let (icon, input) = match &self.history {
            Some(search) => (history_search::icon(cx), search.query.clone()),
            None => (Icon::new(IconName::Logo).color(t.text_muted), self.input.clone()),
        };
        let icon = icon.size(px(BAR_ICON_SIZE));
        let searching = self.history.is_some();

        h_flex()
            .when(searching, |bar| bar.key_context(history_search::CONTEXT))
            .flex_shrink_0()
            .items_start()
            .min_h(px(BAR_HEIGHT))
            .py((px(BAR_HEIGHT) - line) / 2.)
            .px(px(BAR_PADDING_X))
            .gap(px(BAR_ICON_GAP))
            // The logo is a handle for moving the window; the rest of it selects text.
            .child(
                on_first_line(icon.into_any_element())
                    .on_mouse_down(MouseButton::Left, |_, window, _| window.start_window_move()),
            )
            .child(div().flex_1().min_w(px(0.)).child(input))
            .when(!searching && self.is_expanded(cx), |bar| {
                let clear = IconButton::new("clear", IconName::CircleX)
                    .on_click(cx.listener(|this, _, window, cx| this.clear_input(window, cx)));
                bar.child(on_first_line(clear.into_any_element()))
            })
    }

    /// Recommended tools, then the others that fit less well. The selection is the
    /// accent colour while the list has focus, grey otherwise (as in macOS lists).
    fn render_list(&self, t: &Theme, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let plugins = plugins::all(cx);
        let focused = self.list_focus.is_focused(window);
        let section = |label: &'static str| div().px(px(14.)).py(px(4.)).child(Caption::new(label));
        let recommended = self.candidates.iter().take_while(|c| c.is_recommended()).count();
        let mut list = v_flex()
            .id("tools")
            .key_context(TOOL_LIST_CONTEXT)
            .track_focus(&self.list_focus)
            .on_key_down(cx.listener(Self::on_list_key_down))
            .w(px(LIST_WIDTH))
            .flex_shrink_0()
            .h_full()
            .overflow_y_scroll()
            .track_scroll(&self.list_scroll)
            .py(px(6.));
        list = if recommended == 0 {
            list.child(
                div()
                    .px(px(14.))
                    .py(px(6.))
                    .text_size(t.text_size_small())
                    .text_color(t.text_faint)
                    .child("No recommendations for this input"),
            )
        } else {
            list.child(section("Recommended"))
        };
        for (i, candidate) in self.candidates.iter().enumerate() {
            let Some((plugin, operation)) = find(&plugins, &candidate.tool) else { continue };
            if i == recommended {
                list = list
                    .child(div().mx(px(14.)).my(px(6.)).child(Divider::horizontal()))
                    .child(section("Other Matches"));
            }
            let selected = self.selected == Some(i);
            let on_accent = selected && focused;
            let hover = t.fill_subtle();
            // Only the first nine tools have a key (⌘1 to ⌘9): looking one up for each of dozens costs every frame.
            // They are the launcher's keys, wherever in it the focus is.
            let keystroke = if i < 9 { keystroke_for(&SelectTool(i + 1), &self.focus_handle, window) } else { None };
            list = list.child(
                h_flex()
                    .id(("tool", i))
                    .mx(px(6.))
                    .px(px(8.))
                    .py(px(5.))
                    .gap(px(8.))
                    .rounded(t.radius_small())
                    .cursor_pointer()
                    .when(on_accent, |row| row.bg(t.accent))
                    .when(selected && !focused, |row| row.bg(t.fill))
                    .when(!selected, |row| row.hover(move |style| style.bg(hover)))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.select(i, cx);
                        window.focus(&this.list_focus, cx);
                    }))
                    .child(LogoBadge::new(icon(plugin, operation)).size(px(20.)))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .text_color(if on_accent { t.accent_text } else { t.text })
                            .truncate()
                            .child(title(plugin, operation)),
                    )
                    .when_some(keystroke, |row, keystroke| {
                        let style = if on_accent { KeycapStyle::OnAccent } else { KeycapStyle::Plain };
                        row.child(Keycap::new(keystroke_label(&keystroke)).style(style))
                    }),
            );
        }
        list
    }

    /// The selected tool: its name, then its own view; with none selected, why. The tool whose view
    /// this draws is the one shown in its plugin, and every other is hidden (`Launcher::show_only`),
    /// after this frame: showing and hiding tell the plugins, which is no part of drawing.
    fn render_detail(&mut self, t: &Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let (detail, drawn) = self.render_tool(t, cx);
        let shown = drawn.filter(|_| !self.window_hidden);
        if shown != self.shown_tool {
            let launcher = cx.weak_entity();
            cx.defer(move |cx| {
                launcher.update(cx, |this, cx| this.show_only(shown, cx)).ok();
            });
        }
        detail
    }

    /// The selected tool's page, and which tool's view it draws (none: a notice instead).
    fn render_tool(&mut self, t: &Theme, cx: &mut Context<Self>) -> (Stateful<Div>, Option<ToolIds>) {
        // Opaque, unlike the launcher behind it: the tool is read on a solid page. No padding at the
        // bottom: the tool's view reaches the footer, so what it scrolls runs to the edge.
        let detail = v_flex().id("detail").flex_1().min_w(px(0.)).h_full().gap(px(14.)).px(px(18.)).pt(px(14.)).bg(t.tool_background());
        let plugins = plugins::all(cx);
        let Some(tool_ids) = self.selected_candidate().map(|candidate| candidate.tool.clone()) else {
            return (detail.child(self.render_empty(t)), None);
        };
        let Some((plugin, operation)) = find(&plugins, &tool_ids) else {
            return (detail, None);
        };
        let name = delight_ui::ellipsize(&delight_ui::one_line(&plugin.manifest().plugin.name), 100);
        let title = title(plugin, operation);
        // The plugin's name, unless the tool's title already says it: a link to its settings page.
        let plugin_name = (name != title).then(|| {
            let accent = t.accent;
            let id = plugin.manifest().plugin.id.clone();
            div()
                .id("plugin-name")
                .min_w(px(0.))
                .truncate()
                .cursor_pointer()
                .hover(move |style| style.text_color(accent))
                .tooltip(Tooltip::text("Open its settings"))
                .on_click(move |_, _, cx| {
                    let id = id.clone();
                    cx.defer(move |cx| settings_window::open_plugin(id, cx));
                })
                .child(name.clone())
        });
        let header = h_flex()
            .h(px(24.))
            .gap(px(8.))
            .child(LogoBadge::new(icon(plugin, operation)).size(px(20.)))
            // The title gives way to the plugin's name only when both don't fit: each is one line.
            .child(div().min_w(px(0.)).font_weight(FontWeight::SEMIBOLD).truncate().child(title))
            // Only the name is the link, not the space after it.
            .child(
                h_flex()
                    .flex_1()
                    .min_w(px(0.))
                    .text_size(t.text_size_small())
                    .text_color(t.text_faint)
                    .children(plugin_name),
            )
            .children(plugin_update(plugin, t, cx));
        let detail = detail.child(header);
        // A plugin that stopped isn't called again: say so instead.
        if let Some(reason) = plugin.stopped() {
            let id = &plugin.manifest().plugin.id;
            let buttons = [plugins::restart_button(id, cx), plugins::copy_details_button(id, &reason)];
            return (detail.child(stopped_notice(t, format!("{name} stopped: {}", plugins::stop_summary(&reason)), buttons)), None);
        }
        match self.selected_pane(cx) {
            // The rest of the height; the plugin draws its view there.
            Some(pane) => {
                let surface = pane.read(cx).surface.clone();
                let tool = v_flex()
                    .key_context(TOOL_CONTEXT)
                    .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| this.on_tool_key_down(event, window, cx)))
                    .flex_1()
                    .min_h(px(0.))
                    .overflow_hidden()
                    // A click elsewhere in the launcher never reaches the plugin's view: tell the tool,
                    // so it can close its menus.
                    .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                        if let Some(pane) = this.selected_pane(cx) {
                            let pane = pane.clone();
                            pane.update(cx, |pane, cx| pane.focus_lost(cx));
                        }
                    }))
                    .child(surface);
                (detail.child(tool), Some(tool_ids))
            }
            None => (detail, None),
        }
    }

    fn render_empty(&self, t: &Theme) -> impl IntoElement {
        let (title, hint) = if self.candidates.is_empty() {
            ("No tool fits this input", "Plugins in the plugins folder add tools.")
        } else {
            ("No strong match", "Pick a tool on the left, or keep typing.")
        };
        v_flex()
            .flex_1()
            .items_center()
            .justify_center()
            .gap(px(6.))
            .child(Icon::new(IconName::Sparkles).size(px(28.)).color(t.text_faint))
            .child(div().text_size(t.text_size_large()).font_weight(FontWeight::MEDIUM).child(title))
            .child(div().text_size(t.text_size_small()).text_color(t.text_muted).child(hint))
    }

    /// A toast, then the selected tool's actions.
    fn render_footer(&self, t: &Theme, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let toast = self.toast.clone().map(|message| {
            h_flex()
                .gap(px(6.))
                .text_color(t.success)
                .child(Icon::new(IconName::Check).size(px(12.)).color(t.success))
                .child(message)
        });
        // Buttons are clicked, not dragged: keep their mouse-downs from the footer.
        let mut actions = h_flex().gap(px(2.)).on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation());
        for (i, (action, key)) in footer::buttons(self.keyed_actions(window, cx), FOOTER_ACTIONS).into_iter().enumerate() {
            if i > 0 {
                actions = actions.child(Divider::vertical());
            }
            let style = match action.style {
                ActionStyle::Normal => ButtonVariant::Text,
                ActionStyle::Primary => ButtonVariant::Emphasis,
                ActionStyle::Attention => ButtonVariant::Attention,
            };
            let mut button = Button::new(("action", i), action.label.clone())
                .variant(style)
                .on_click(cx.listener(move |this, _, _, cx| this.perform(&action, cx)));
            if let Some(keystroke) = &key {
                button = button.shortcut(keystroke_label(keystroke));
            }
            actions = actions.child(button);
        }
        // Only while losing the keyboard would hide the launcher: with that setting off, it stays up anyway.
        let pin = settings::get(cx).hide_on_blur.then(|| {
            let tooltip = if self.pinned { "Pinned: stays open until it is hidden" } else { "Keep open while using other apps" };
            // On, it is filled: the same colour, no tint behind it.
            IconButton::new("pin", if self.pinned { IconName::PinFilled } else { IconName::Pin })
                .tooltip(tooltip)
                .on_click(cx.listener(|this, _, _, cx| this.toggle_pin(cx)))
        });
        let actions = actions.children(pin);
        // The footer is a handle for moving the window.
        h_flex()
            .on_mouse_down(MouseButton::Left, |_, window, _| window.start_window_move())
            .flex_shrink_0()
            .h(px(FOOTER_HEIGHT))
            .px(px(14.))
            .justify_between()
            .gap(px(12.))
            .text_size(t.text_size_small())
            .child(div().min_w(px(0.)).truncate().children(toast))
            .child(actions)
    }
}

/// A newer version of the tool's plugin, as its settings page offers it: a button that installs it,
/// or how far its download has come. A look for one that failed isn't shown here, where it would
/// read as the tool's own error; the plugin's page says so.
fn plugin_update(plugin: &Plugin, t: &Theme, cx: &App) -> Option<AnyElement> {
    use plugins::updates::{self, State};
    let id = plugin.manifest().plugin.id.clone();
    let offer = updates::offer(&id, cx)?;
    let label = match &offer.state {
        State::Downloading { done, total } => {
            let done = format!("Downloading… {}", plugins::download_size(*done, *total));
            return Some(div().flex_shrink_0().text_size(t.text_size_small()).text_color(t.text_muted).child(done).into_any_element());
        }
        State::Ready => format!("Update to {}", delight_ui::ellipsize(&delight_ui::one_line(&offer.release.version), 30)),
        State::AsksForMore => "Review Update…".to_string(),
        State::Failed(_) => "Retry Update".to_string(),
    };
    let button = Button::new("update-plugin", label).text().small().icon(IconName::RefreshCw).on_click(move |_, _, cx| updates::update(&id, cx));
    Some(button.into_any_element())
}

/// An operation's title.
fn title(plugin: &Plugin, operation: usize) -> String {
    plugin.manifest().operations.get(operation).map(|operation| delight_ui::ellipsize(&delight_ui::one_line(&operation.title), 100)).unwrap_or_default()
}

/// An operation's icon (SVG), or its plugin's if it has none.
fn icon(plugin: &Plugin, operation: usize) -> &[u8] {
    let manifest = plugin.manifest();
    let own = manifest.operations.get(operation).and_then(|operation| operation.icon.as_deref());
    own.unwrap_or(&manifest.plugin.icon).as_bytes()
}

/// What the tool pane shows for a plugin that stopped: why, in a few lines at most, and buttons
/// to start it again and to copy all of why.
fn stopped_notice(t: &Theme, text: String, buttons: [delight_ui::Button; 2]) -> impl IntoElement {
    h_flex()
        .items_start()
        .gap(px(8.))
        .px(px(12.))
        .py(px(9.))
        .rounded(t.radius)
        .bg(t.tint(t.error))
        .border_1()
        .border_color(t.error.opacity(0.25))
        .child(div().pt(px(1.)).child(Icon::new(IconName::CircleX).size(px(14.)).color(t.error)))
        .child(
            v_flex()
                .flex_1()
                .min_w(px(0.))
                .items_start()
                .gap(px(10.))
                .child(div().line_clamp(3).child(text))
                .child(h_flex().gap(px(8.)).children(buttons)),
        )
}
