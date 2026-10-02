//! SVG: shows an SVG image, and copies it as a data URI.
//!
//! * this file: which inputs are SVG.
//! * `view`: the tool: the preview and its actions.
//! * `render`: the preview and the data URI from the input's SVG.
//! * `document`: an SVG document rendered at any scale.
//! * `checkerboard`: the squares behind the preview.

mod checkerboard;
mod document;
mod render;
mod view;

pub use view::SvgView;

/// How much of the input is searched for `<svg`.
const DETECT_HEAD: usize = 1024;

/// How surely `text` is an SVG document.
pub fn confidence(text: &str) -> Option<f32> {
    let text = text.trim_start();
    if text.starts_with("<svg") {
        Some(0.95)
    } else if text.starts_with('<') && text.chars().take(DETECT_HEAD).collect::<String>().contains("<svg") {
        // An XML declaration, a doctype or a comment before `<svg`.
        Some(0.9)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_svg() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1"/>"#;
        assert_eq!(confidence(svg), Some(0.95));
        assert_eq!(confidence(&format!("<?xml version=\"1.0\"?>\n{svg}")), Some(0.9));
        assert_eq!(confidence("<div>not svg</div>"), None);
        assert_eq!(confidence("hello"), None);
    }
}
