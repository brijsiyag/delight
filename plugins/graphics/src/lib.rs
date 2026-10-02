//! Graphics: tools for images. For now one, SVG Preview.
//!
//! * this file: the plugin and its tools.
//! * `svg`: shows an SVG image, and copies it as a data URI.

mod svg;

use delight_plugin_api::{AnyTool, Detection, Input, Operations, Plugin, plugin};
use gpui::{App, AppContext as _, AssetSource};

#[plugin(
    id = "delight_graphics",
    name = "Graphics",
    description = "Preview SVG images and copy them as a data URI.",
    author = "Delight",
    icon = "assets/icon.svg",
    tags = ["graphics", "svg", "image", "preview"],
    tips = ["Paste SVG code to preview the image"],
)]
struct Graphics;

#[derive(Operations)]
enum GraphicsOperation {
    #[operation(
        id = "svg",
        title = "SVG Preview",
        description = "Show an SVG image; copy it as a data URI",
        icon = "assets/svg.svg",
        tags = ["svg", "image", "preview", "vector"],
    )]
    Preview,
}

impl Plugin for Graphics {
    type Operation = GraphicsOperation;

    fn new(cx: &mut App) -> Self {
        delight_ui::init_plugin(cx);
        Graphics
    }

    fn detect(&mut self, input: &Input, _cx: &mut App) -> Vec<Detection<GraphicsOperation>> {
        svg::confidence(&input.text).map(|confidence| Detection::new(GraphicsOperation::Preview, confidence)).into_iter().collect()
    }

    fn open_tool(&mut self, operation: GraphicsOperation, cx: &mut App) -> AnyTool {
        match operation {
            GraphicsOperation::Preview => cx.new(|_| svg::SvgView::default()).into(),
        }
    }

    fn assets() -> Option<Box<dyn AssetSource>> {
        Some(Box::new(delight_ui::Assets))
    }
}
