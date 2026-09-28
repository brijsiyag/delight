//! Drawing the editor: a custom GPUI element that shapes and wraps the text,
//! paints selection, text and cursor, registers the input handler, and keeps
//! the cursor scrolled into view. It stores the layout it painted back on
//! the editor for mouse hit-testing and ↑/↓.

use gpui::{
    App, Bounds, Element, ElementId, ElementInputHandler, Entity, GlobalElementId, Hsla, IntoElement, LayoutId,
    PaintQuad, Pixels, Point, SharedString, Style, TextAlign, TextRun, Window, WrappedLine, fill, font, point, px,
    relative, size,
};

use super::TextEditor;

/// One logical (`\n`-separated) line after wrapping.
pub struct LineLayout {
    /// Byte offset of the line start in the content.
    pub start: usize,
    pub wrapped: WrappedLine,
    pub y: Pixels,
}

pub struct Layout {
    pub lines: Vec<LineLayout>,
    pub bounds: Bounds<Pixels>,
    pub line_height: Pixels,
}

/// Where `offset` is drawn, relative to the text's origin.
pub fn position_for_offset(layout: &Layout, offset: usize) -> Option<Point<Pixels>> {
    let line = layout.lines.iter().rev().find(|l| l.start <= offset)?;
    let p = line.wrapped.position_for_index(offset - line.start, layout.line_height)?;
    Some(point(p.x, p.y + line.y))
}

/// The offset closest to `p`, relative to the text's origin.
pub fn offset_for_point(layout: &Layout, p: Point<Pixels>) -> usize {
    let lh = layout.line_height;
    let Some(last) = layout.lines.last() else { return 0 };
    if p.y < px(0.) {
        return 0;
    }
    let line = layout.lines.iter().find(|l| p.y < l.y + l.wrapped.size(lh).height).unwrap_or(last);
    let local = point(p.x.max(px(0.)), (p.y - line.y).max(px(0.)));
    let index = match line.wrapped.closest_index_for_position(local, lh) {
        Ok(i) | Err(i) => i,
    };
    line.start + index.min(line.wrapped.len())
}

pub struct TextElement {
    pub editor: Entity<TextEditor>,
    pub color: Hsla,
    pub placeholder_color: Hsla,
    pub selection_color: Hsla,
    pub cursor_color: Hsla,
    pub font_family: SharedString,
}

pub struct Prepaint {
    lines: Vec<LineLayout>,
    /// The lines show the placeholder, not content.
    placeholder: bool,
    selections: Vec<PaintQuad>,
    cursor: Option<PaintQuad>,
    /// The completion's first line, greyed, and where it starts.
    ghost: Option<(Point<Pixels>, WrappedLine)>,
}

impl TextElement {
    /// Shapes every logical line of `text`; returns the lines and the height.
    fn shape(
        font_family: &SharedString,
        text: &str,
        color: Hsla,
        font_size: Pixels,
        lh: Pixels,
        wrap: Option<Pixels>,
        window: &mut Window,
    ) -> (Vec<LineLayout>, Pixels) {
        let mut lines = Vec::new();
        let (mut y, mut start) = (px(0.), 0);
        for line in text.split('\n') {
            let run = TextRun {
                len: line.len(),
                font: font(font_family.clone()),
                color,
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let wrapped = window
                .text_system()
                .shape_text(SharedString::from(line.to_string()), font_size, &[run], wrap, None)
                .ok()
                .and_then(|mut v| (!v.is_empty()).then(|| v.remove(0)))
                .unwrap_or_default();
            let height = wrapped.size(lh).height.max(lh);
            lines.push(LineLayout { start, wrapped, y });
            y += height;
            start += line.len() + 1;
        }
        (lines, y.max(lh))
    }

    /// Selection rectangles, one per visual row the selection covers.
    fn selection_quads(&self, layout: &Layout, selected: std::ops::Range<usize>) -> Vec<PaintQuad> {
        let (lh, bounds) = (layout.line_height, layout.bounds);
        let mut quads = Vec::new();
        for line in &layout.lines {
            let line_end = line.start + line.wrapped.len();
            if line_end < selected.start || line.start > selected.end {
                continue;
            }
            let s = selected.start.max(line.start) - line.start;
            let e = selected.end.min(line_end) - line.start;
            let (Some(sp), Some(ep)) = (line.wrapped.position_for_index(s, lh), line.wrapped.position_for_index(e, lh))
            else {
                continue;
            };
            // A selection that continues past the newline extends a little to show it.
            let tail = if selected.end > line_end { px(6.) } else { px(0.) };
            let width = line.wrapped.width().max(bounds.size.width.min(line.wrapped.width() + tail));
            let mut row_y = sp.y;
            while row_y <= ep.y {
                let x0 = if row_y == sp.y { sp.x } else { px(0.) };
                let x1 = if row_y == ep.y { ep.x + tail } else { width };
                let origin = bounds.origin + point(x0, line.y + row_y);
                quads.push(fill(Bounds::new(origin, size((x1 - x0).max(px(1.)), lh)), self.selection_color));
                row_y += lh;
            }
        }
        quads
    }
}

impl IntoElement for TextElement {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl Element for TextElement {
    type RequestLayoutState = ();
    type PrepaintState = Prepaint;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let editor = self.editor.read(cx);
        let (font_size, lh) = (editor.font_size, editor.line_height);
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        if !editor.multiline {
            style.size.height = lh.into();
            return (window.request_layout(style, [], cx), ());
        }
        // Multi-line: the height depends on how the text wraps at the width.
        let text = if editor.content.is_empty() { editor.placeholder.to_string() } else { editor.content.clone() };
        let (font_family, color) = (self.font_family.clone(), self.color);
        let id = window.request_measured_layout(style, move |known, available, window, _| {
            let wrap = known.width.or(match available.width {
                gpui::AvailableSpace::Definite(w) => Some(w),
                _ => None,
            });
            let (_, height) = Self::shape(&font_family, &text, color, font_size, lh, wrap, window);
            size(wrap.unwrap_or(px(400.)), height)
        });
        (id, ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> Prepaint {
        let editor = self.editor.read(cx);
        let (font_size, lh) = (editor.font_size, editor.line_height);
        let empty = editor.content.is_empty();
        let (text, color) =
            if empty { (editor.placeholder.to_string(), self.placeholder_color) } else { (editor.content.clone(), self.color) };
        let wrap = editor.multiline.then_some(bounds.size.width);
        let (selected, cursor_offset) = (editor.selected_range.clone(), editor.cursor());
        let (lines, _) = Self::shape(&self.font_family, &text, color, font_size, lh, wrap, window);
        let layout = Layout { lines, bounds, line_height: lh };

        let selections = if selected.is_empty() || empty { Vec::new() } else { self.selection_quads(&layout, selected.clone()) };
        let cursor = selected.is_empty().then(|| {
            let p = if empty { Some(point(px(0.), px(0.))) } else { position_for_offset(&layout, cursor_offset) }?;
            // Vertically centred on the text, not the full line height.
            let inset = (lh - font_size * 1.2).max(px(0.)) / 2.;
            Some(fill(Bounds::new(bounds.origin + point(p.x, p.y + inset), size(px(2.), lh - inset * 2.)), self.cursor_color))
        });
        // The completion continues the text where it ends (its first line; Tab
        // inserts all of it). It isn't part of the layout, so clicks and IME
        // offsets never land in it.
        let ghost = editor.visible_completion().and_then(|completion| {
            let first_line = completion.split('\n').next().unwrap_or_default().to_string();
            let origin = position_for_offset(&layout, editor.content.len())?;
            let (mut lines, _) = Self::shape(&self.font_family, &first_line, self.placeholder_color, font_size, lh, None, window);
            Some((origin, lines.remove(0).wrapped))
        });
        Prepaint { lines: layout.lines, placeholder: empty, selections, cursor: cursor.flatten(), ghost }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        prepaint: &mut Prepaint,
        window: &mut Window,
        cx: &mut App,
    ) {
        let (focus, lh, autoscroll, scroll, blink) = {
            let e = self.editor.read(cx);
            (e.focus_handle.clone(), e.line_height, e.autoscroll, e.scroll.clone(), e.blink.clone())
        };
        window.handle_input(&focus, ElementInputHandler::new(bounds, self.editor.clone()), cx);
        for quad in prepaint.selections.drain(..) {
            window.paint_quad(quad);
        }
        for line in &prepaint.lines {
            let _ = line.wrapped.paint(bounds.origin + point(px(0.), line.y), lh, TextAlign::Left, None, window, cx);
        }
        if let Some((origin, ghost)) = &prepaint.ghost {
            let _ = ghost.paint(bounds.origin + *origin, lh, TextAlign::Left, None, window, cx);
        }
        let cursor_bounds = prepaint.cursor.as_ref().map(|q| q.bounds);
        let show_cursor = focus.is_focused(window) && window.is_window_active() && blink.read(cx).visible();
        if show_cursor && let Some(cursor) = prepaint.cursor.take() {
            window.paint_quad(cursor);
        }

        // Keep the cursor in view inside the scroll container.
        if autoscroll && let Some(cb) = cursor_bounds {
            let viewport = scroll.bounds();
            let mut offset = scroll.offset();
            if cb.bottom() > viewport.bottom() {
                offset.y -= cb.bottom() - viewport.bottom();
                scroll.set_offset(offset);
                window.refresh();
            } else if cb.top() < viewport.top() {
                offset.y += viewport.top() - cb.top();
                scroll.set_offset(offset);
                window.refresh();
            }
        }

        // The placeholder's layout must not drive mouse or IME offsets: with no
        // content, every position is offset 0.
        let lines = if prepaint.placeholder { Vec::new() } else { std::mem::take(&mut prepaint.lines) };
        self.editor.update(cx, |e, _| {
            e.layout = Some(Layout { lines, bounds, line_height: lh });
            e.autoscroll = false;
        });
    }
}
