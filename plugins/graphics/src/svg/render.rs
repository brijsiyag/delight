//! Parsing and rendering the SVG, on a background task: the preview image and the
//! data URI. (Copying it as a PNG waits for the app's `copy_file`.)

use std::sync::Arc;

use base64::Engine as _;
use gpui::{RenderImage, SharedString};

use super::document::SvgDocument;

/// The largest the preview is drawn, in points; it fits inside.
pub const PREVIEW_WIDTH: f32 = 520.;
pub const PREVIEW_HEIGHT: f32 = 200.;
/// Small drawings (icons) are enlarged, but at most this much.
const MAX_ZOOM: f32 = 8.;
/// The preview is rendered for a 2× screen.
const SCALE: f32 = 2.;

#[derive(Clone, Default)]
pub enum Preview {
    #[default]
    Empty,
    Ready(Rendered),
    Failed(SharedString),
}

#[derive(Clone)]
pub struct Rendered {
    pub image: Arc<RenderImage>,
    /// The SVG's declared size.
    pub size: (f32, f32),
    /// The size the preview is drawn at, in points.
    pub shown: (f32, f32),
    pub data_uri: String,
}

/// Parse and render `svg`.
pub fn render(svg: &str) -> Preview {
    if svg.is_empty() {
        return Preview::Empty;
    }
    let document = match SvgDocument::parse(svg.as_bytes()) {
        Ok(document) => document,
        Err(error) => return Preview::Failed(format!("Can't show this SVG: {error}").into()),
    };
    let (width, height) = document.size();
    let zoom = (PREVIEW_WIDTH / width).min(PREVIEW_HEIGHT / height).min(MAX_ZOOM);
    let Some(image) = document.image(zoom * SCALE) else {
        return Preview::Failed("This SVG has nothing to draw".into());
    };
    Preview::Ready(Rendered {
        image,
        size: (width, height),
        shown: (width * zoom, height * zoom),
        data_uri: format!("data:image/svg+xml;base64,{}", base64::engine::general_purpose::STANDARD.encode(svg)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const ICON: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="12"><rect width="24" height="12" fill="#f00"/></svg>"##;

    #[test]
    fn enlarges_small_drawings_and_fits_big_ones() {
        let Preview::Ready(icon) = render(ICON) else { panic!("expected a preview") };
        assert_eq!(icon.size, (24., 12.));
        assert_eq!(icon.shown, (24. * MAX_ZOOM, 12. * MAX_ZOOM));
        assert!(icon.data_uri.starts_with("data:image/svg+xml;base64,"));

        let big = ICON.replace("width=\"24\" height=\"12\"", "width=\"2000\" height=\"1000\"");
        let Preview::Ready(big) = render(&big) else { panic!("expected a preview") };
        // 2:1, limited by the preview's height.
        assert_eq!(big.shown, (PREVIEW_HEIGHT * 2., PREVIEW_HEIGHT));
    }

    #[test]
    fn explains_invalid_svg() {
        assert!(matches!(render("<svg"), Preview::Failed(_)));
        assert!(matches!(render(""), Preview::Empty));
    }
}
