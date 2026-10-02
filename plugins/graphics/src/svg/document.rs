//! SVG documents rendered with resvg, for the preview. GPUI draws an SVG image at its
//! declared size, blurry on a 2× screen; this renders at any scale.

use std::sync::{Arc, LazyLock};

use delight_ui::render_image;
use gpui::RenderImage;
use resvg::{tiny_skia, usvg};

/// No fonts: a plugin's sandbox can't read the system's, so text in an SVG isn't
/// drawn.
static OPTIONS: LazyLock<usvg::Options<'static>> = LazyLock::new(usvg::Options::default);

/// A parsed SVG document. Parsing and rendering take a while for big documents: do
/// both in a task.
pub struct SvgDocument {
    tree: usvg::Tree,
}

impl SvgDocument {
    /// Parse `svg`; the error says what's wrong with it.
    pub fn parse(svg: &[u8]) -> Result<Self, String> {
        usvg::Tree::from_data(svg, &OPTIONS).map(|tree| Self { tree }).map_err(|error| error.to_string())
    }

    /// Its size in pixels, as the document declares it.
    pub fn size(&self) -> (f32, f32) {
        (self.tree.size().width(), self.tree.size().height())
    }

    /// Rendered at `scale` (2 for a 2× screen); `None` if that's empty or too big.
    pub fn image(&self, scale: f32) -> Option<Arc<RenderImage>> {
        let (width, height) = self.size();
        let mut pixmap = tiny_skia::Pixmap::new((width * scale).ceil() as u32, (height * scale).ceil() as u32)?;
        resvg::render(&self.tree, tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());
        let (width, height) = (pixmap.width(), pixmap.height());
        render_image(pixmap.take(), width, height)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RED_SQUARE: &[u8] =
        br##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="5"><rect width="10" height="5" fill="#ff0000"/></svg>"##;

    #[test]
    fn renders_at_any_scale() {
        let svg = SvgDocument::parse(RED_SQUARE).unwrap();
        assert_eq!(svg.size(), (10., 5.));
        let image = svg.image(2.).unwrap();
        assert_eq!((image.size(0).width.0, image.size(0).height.0), (20, 10));
        assert_eq!(&image.as_bytes(0).unwrap()[..4], [0, 0, 255, 255], "red, as BGRA");
    }

    #[test]
    fn says_what_is_wrong() {
        assert!(SvgDocument::parse(b"<svg").is_err());
        assert!(SvgDocument::parse(b"not an svg").is_err());
    }
}
