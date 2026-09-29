//! SVG: shows an SVG image, and copies it as a data URI.
//!
//! * this file: the plugin, and which inputs are SVG.
//! * `view`: the tool: the preview and its actions.
//! * `render`: the preview and the data URI from the input's SVG.
//! * `document`: an SVG document rendered at any scale.
//! * `checkerboard`: the squares behind the preview.

mod checkerboard;
mod document;
mod render;
mod view;

use delight_plugin_api::{AnyTool, Detection, Input, Operations, Plugin, plugin};
use gpui::{App, AppContext as _, AssetSource};

#[plugin(
    id = "delight.svg",
    name = "SVG",
    description = "Preview SVG images and copy them as a data URI.",
    author = "Delight",
    icon = "assets/icon.svg",
    tags = ["svg", "image", "preview"],
    tips = [
        "Paste an SVG to preview it, on a checkerboard or a light or dark backdrop",
        "↵ copies a pasted SVG as a data URI",
    ],
)]
struct Svg;

#[derive(Operations)]
enum SvgOperation {
    #[operation(
        id = "svg",
        title = "SVG Preview",
        description = "Show an SVG image; copy it as a data URI",
        tags = ["svg", "image", "preview", "vector"],
    )]
    Preview,
}

impl Plugin for Svg {
    type Operation = SvgOperation;

    fn new(cx: &mut App) -> Self {
        delight_ui::init_plugin(cx);
        Svg
    }

    fn detect(&mut self, input: &Input, _cx: &mut App) -> Vec<Detection<SvgOperation>> {
        confidence(&input.text).map(|confidence| Detection::new(SvgOperation::Preview, confidence)).into_iter().collect()
    }

    fn open_tool(&mut self, operation: SvgOperation, cx: &mut App) -> AnyTool {
        match operation {
            SvgOperation::Preview => cx.new(|_| view::SvgView::default()).into(),
        }
    }

    fn assets() -> Option<Box<dyn AssetSource>> {
        Some(Box::new(delight_ui::Assets))
    }
}

/// How much of the input is searched for `<svg`.
const DETECT_HEAD: usize = 1024;

/// How surely `text` is an SVG document.
fn confidence(text: &str) -> Option<f32> {
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
