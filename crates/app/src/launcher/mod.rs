//! The launcher: a floating input bar that grows into a panel once there's input,
//! with the tools that fit it on the left, the selected tool on the right, and the
//! footer (the tool's actions) under them. Tools and the footer join it in the next
//! part; for now the panel says no tool fits.
//!
//! * `window`: opening, showing and hiding the window.
//! * this file: the launcher's state and behaviour.
//! * `view`: drawing it.

mod view;
mod window;

use delight_ui::theme::{INPUT_FONT_SIZE, INPUT_LINE_HEIGHT};
use delight_ui::{EditorEvent, EditorFont, TextEditor};
use gpui::{App, AppContext as _, Context, Entity, FocusHandle, Focusable as _, Subscription, Window, actions, px};

pub use window::{hide, open, show, toggle};

use crate::platform::{self, NativeWindow};

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

/// The key context of the launcher window.
pub const CONTEXT: &str = "Launcher";

actions!(launcher, [Dismiss, ClearInput]);

pub struct Launcher {
    focus_handle: FocusHandle,
    /// The window's AppKit side, changed outside GPUI updates (see `platform`).
    native: Option<NativeWindow>,
    input: Entity<TextEditor>,
    /// The window's size (width, height), once set.
    size: Option<(f32, f32)>,
    _subscriptions: Vec<Subscription>,
}

impl Launcher {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| {
            TextEditor::new(window, cx)
                .multiline(px(INPUT_LINE_HEIGHT * 4.))
                .font(EditorFont::Input)
                .text_size(px(INPUT_FONT_SIZE), px(INPUT_LINE_HEIGHT))
                .placeholder("What you got this time?")
        });
        let subscriptions = vec![
            cx.subscribe(&input, |this, _, event, cx| this.on_input_event(event, cx)),
            cx.observe_window_appearance(window, |_, _, cx| delight_ui::theme::appearance_changed(cx)),
            // Losing the keyboard to another app hides the launcher (a setting
            // later, on by default).
            cx.observe_window_activation(window, |_, window, cx| {
                if !window.is_window_active() && platform::is_window_visible(window) {
                    cx.defer(hide);
                }
            }),
        ];
        Self {
            focus_handle: cx.focus_handle(),
            native: NativeWindow::of(window),
            input,
            size: None,
            _subscriptions: subscriptions,
        }
    }

    fn is_expanded(&self, cx: &App) -> bool {
        !self.input.read(cx).text().trim().is_empty()
    }

    /// The bar while the input is empty, the panel otherwise.
    fn wanted_size(&self, cx: &App) -> (f32, f32) {
        if self.is_expanded(cx) { (PANEL_WIDTH, PANEL_HEIGHT) } else { (BAR_WIDTH, BAR_HEIGHT) }
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

    fn on_input_event(&mut self, event: &EditorEvent, cx: &mut Context<Self>) {
        match event {
            // The panel opens and closes with the text.
            EditorEvent::Changed => cx.notify(),
            EditorEvent::CompletionAccepted | EditorEvent::Focus | EditorEvent::Blur => {}
        }
    }

    fn clear_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.input.update(cx, |input, cx| input.set_text("", cx));
        window.focus(&self.input.focus_handle(cx), cx);
    }
}
