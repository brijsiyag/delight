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

/// The text runs of the line of `len` bytes that starts at `start` in the text: one for each
/// stretch of a colour in `highlights`, and of the plain `color` between them.
fn runs_of(font_family: &SharedString, color: Hsla, start: usize, len: usize, highlights: &[(std::ops::Range<usize>, Hsla)]) -> Vec<TextRun> {
    let run = |len: usize, color: Hsla| TextRun {
        len,
        font: font(font_family.clone()),
        color,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let end = start + len;
    let mut runs = Vec::new();
    let mut at = start;
    for (range, colour) in highlights {
        if range.end <= at {
            continue;
        }
        if range.start >= end {
            break;
        }
        let (from, to) = (range.start.max(at), range.end.min(end));
        if from > at {
            runs.push(run(from - at, color));
        }
        runs.push(run(to - from, *colour));
        at = to;
    }
    if at < end || runs.is_empty() {
        runs.push(run(end - at, color));
    }
    runs
}

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
    /// A single-line editor's text is scrolled left by this much: the text starts at
    /// `bounds.origin` less this.
    scroll_x: Pixels,
}

/// How far a single-line field's text is scrolled left: as far as it was, moved just enough to
/// keep the cursor (at `cursor_x` in a text `text_width` wide) inside a field `view` wide, and
/// never further than the text's end (with room for the cursor there).
fn scrolled(current: Pixels, cursor_x: Pixels, text_width: Pixels, view: Pixels) -> Pixels {
    let room = px(2.);
    let mut x = current;
    if cursor_x - x > view - room {
        x = cursor_x - view + room;
    }
    if cursor_x < x {
        x = cursor_x;
    }
    x.clamp(px(0.), (text_width - view + room).max(px(0.)))
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
        highlights: &[(std::ops::Range<usize>, Hsla)],
        window: &mut Window,
    ) -> (Vec<LineLayout>, Pixels) {
        let mut lines = Vec::new();
        let (mut y, mut start) = (px(0.), 0);
        for line in text.split('\n') {
            let runs = runs_of(font_family, color, start, line.len(), highlights);
            let wrapped = window
                .text_system()
                .shape_text(SharedString::from(line.to_string()), font_size, &runs, wrap, None)
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
            let (_, height) = Self::shape(&font_family, &text, color, font_size, lh, wrap, &[], window);
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
        let colours: &[(std::ops::Range<usize>, Hsla)] = if empty { &[] } else { &editor.highlights };
        let (lines, _) = Self::shape(&self.font_family, &text, color, font_size, lh, wrap, colours, window);
        let mut layout = Layout { lines, bounds, line_height: lh };
        // A line longer than the field scrolls sideways so the cursor stays in view.
        let scroll_x = if editor.multiline || empty {
            px(0.)
        } else {
            let cursor_x = position_for_offset(&layout, cursor_offset).map_or(px(0.), |p| p.x);
            let text_width = layout.lines.first().map_or(px(0.), |line| line.wrapped.width());
            scrolled(editor.scroll_x, cursor_x, text_width, bounds.size.width)
        };
        let bounds = Bounds::new(bounds.origin - point(scroll_x, px(0.)), bounds.size);
        layout.bounds = bounds;

        let selections = if selected.is_empty() || empty { Vec::new() } else { self.selection_quads(&layout, selected.clone()) };
        let cursor = selected.is_empty().then(|| {
            let p = if empty { Some(point(px(0.), px(0.))) } else { position_for_offset(&layout, cursor_offset) }?;
            // Vertically centred on the text, not the full line height.
            let inset = (lh - font_size * 1.2).max(px(0.)) / 2.;
            Some(fill(Bounds::new(bounds.origin + point(p.x, p.y + inset), size(px(2.), lh - inset * 2.)), self.cursor_color))
        });
        // The completion continues the text where it ends (its first line; Tab
        // or → inserts all of it). It isn't part of the layout, so clicks and IME
        // offsets never land in it.
        let ghost = editor.visible_completion().and_then(|completion| {
            let first_line = completion.split('\n').next().unwrap_or_default().to_string();
            // In an empty editor the layout is the placeholder's: the ghost starts at 0.
            let origin = if empty { point(px(0.), px(0.)) } else { position_for_offset(&layout, editor.content.len())? };
            let (mut lines, _) = Self::shape(&self.font_family, &first_line, self.placeholder_color, font_size, lh, None, &[], window);
            Some((origin, lines.remove(0).wrapped))
        });
        // A completion of an empty editor is shown instead of the placeholder.
        let lines = if empty && ghost.is_some() { Vec::new() } else { layout.lines };
        Prepaint { lines, placeholder: empty, selections, cursor: cursor.flatten(), ghost, scroll_x }
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
        // The text starts left of the field when scrolled; what is scrolled out stays unseen.
        let text_bounds = Bounds::new(bounds.origin - point(prepaint.scroll_x, px(0.)), bounds.size);
        let cursor_bounds = prepaint.cursor.as_ref().map(|q| q.bounds);
        let show_cursor = focus.is_focused(window) && window.is_window_active() && blink.read(cx).visible();
        // A wrapped line's cursor at its end is at the edge: room for it.
        let mask = if self.editor.read(cx).multiline { Bounds::new(bounds.origin, size(bounds.size.width + px(3.), bounds.size.height)) } else { bounds };
        window.with_content_mask(Some(gpui::ContentMask { bounds: mask }), |window| {
            for quad in prepaint.selections.drain(..) {
                window.paint_quad(quad);
            }
            for line in &prepaint.lines {
                let _ = line.wrapped.paint(text_bounds.origin + point(px(0.), line.y), lh, TextAlign::Left, None, window, cx);
            }
            if let Some((origin, ghost)) = &prepaint.ghost {
                let _ = ghost.paint(text_bounds.origin + *origin, lh, TextAlign::Left, None, window, cx);
            }
            if show_cursor && let Some(cursor) = prepaint.cursor.take() {
                window.paint_quad(cursor);
            }
        });

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
        let scroll_x = prepaint.scroll_x;
        self.editor.update(cx, |e, _| {
            e.layout = Some(Layout { lines, bounds: text_bounds, line_height: lh });
            e.scroll_x = scroll_x;
            e.autoscroll = false;
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_lines_runs_cover_it_with_the_colours_between_the_plain_stretches() {
        let (plain, red, blue) = (gpui::hsla(0., 0., 0., 1.), gpui::hsla(0., 1., 0.5, 1.), gpui::hsla(0.6, 1., 0.5, 1.));
        let family: SharedString = "mono".into();
        let lens = |runs: Vec<TextRun>| runs.iter().map(|run| (run.len, run.color)).collect::<Vec<_>>();
        // The line is bytes 10..18; colours before it, across it and past its end.
        let highlights = [(0..5, red), (12..14, red), (15..30, blue)];
        assert_eq!(lens(runs_of(&family, plain, 10, 8, &highlights)), [(2, plain), (2, red), (1, plain), (3, blue)]);
        // No colours, or an empty line: one run of the whole line.
        assert_eq!(lens(runs_of(&family, plain, 0, 5, &[])), [(5, plain)]);
        assert_eq!(lens(runs_of(&family, plain, 40, 0, &highlights)), [(0, plain)]);
    }

    #[test]
    fn a_long_line_scrolls_to_keep_the_cursor_in_view() {
        let (view, text) = (px(100.), px(300.));
        // Short text: never scrolled.
        assert_eq!(scrolled(px(0.), px(30.), px(60.), view), px(0.));
        // Typing past the right edge scrolls just enough (the cursor a little inside it).
        assert_eq!(scrolled(px(0.), px(150.), text, view), px(52.));
        // The cursor at the end of the text: the end is in view, no further.
        assert_eq!(scrolled(px(0.), px(300.), text, view), px(202.));
        // Moving left, into what was scrolled away: scrolls back to it.
        assert_eq!(scrolled(px(202.), px(120.), text, view), px(120.));
        // Inside the view: stays.
        assert_eq!(scrolled(px(50.), px(90.), text, view), px(50.));
        // Deleting text so there is less than a view: back to 0.
        assert_eq!(scrolled(px(202.), px(40.), px(40.), view), px(0.));
    }
}
