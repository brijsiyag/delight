//! [`Field`]: a tool's one text field (the .env prefix, a JWT's secret). The tool takes
//! every change at once: there is nothing to save.
//!
//! It sits above the tool's scrolling pane, never in it: GPUI's branch panics
//! ("prepaint has not been performed on …", text.rs) when a surface with a text field
//! in it scrolls (see `docs/plan.md`, "Watch out for"). It has also panicked when text
//! changed next to a field (PagerDuty's settings), which a live result does; that is
//! not found in GPUI yet.

use delight_ui::{ActiveTheme as _, EditorEvent, EditorFont, TextEditor, h_flex};
use gpui::{AppContext as _, Context, Div, Entity, ParentElement, Styled, Subscription, Window, div, px};

#[derive(Default)]
pub struct Field {
    /// Made when the field first draws, as a text field needs a window.
    editor: Option<Entity<TextEditor>>,
    _changes: Option<Subscription>,
}

impl Field {
    /// The field, `placeholder` while it's empty; `on_change` gets the text (trimmed) as
    /// it changes.
    pub fn render<V: 'static>(
        &mut self,
        placeholder: &'static str,
        on_change: impl Fn(&mut V, String, &mut Context<V>) + 'static,
        window: &mut Window,
        cx: &mut Context<V>,
    ) -> Div {
        let editor = match &self.editor {
            Some(editor) => editor.clone(),
            None => {
                let editor = cx.new(|cx| TextEditor::new(window, cx).placeholder(placeholder).font(EditorFont::Mono).text_size(px(12.), px(18.)));
                self._changes = Some(cx.subscribe(&editor, move |view, editor, event: &EditorEvent, cx| {
                    if matches!(event, EditorEvent::Changed) {
                        let text = editor.read(cx).text().trim().to_string();
                        on_change(view, text, cx);
                        cx.notify();
                    }
                }));
                self.editor = Some(editor.clone());
                editor
            }
        };
        let t = cx.theme();
        h_flex()
            .flex_1()
            .min_w(px(0.))
            .h(px(26.))
            .px(px(8.))
            .rounded(px(7.))
            .bg(t.surface)
            .border_1()
            .border_color(t.border)
            .child(div().flex_1().min_w(px(0.)).child(editor))
    }
}
