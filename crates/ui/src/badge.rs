//! [`LogoBadge`]: a tool's logo (its manifest's icon SVG) as
//! a rounded square, in the logo's own colours.

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::{Arc, Mutex, OnceLock};

use gpui::{App, ImageSource, IntoElement, ParentElement, Pixels, RenderImage, RenderOnce, Styled, Window, div, img, px};
use resvg::{tiny_skia, usvg};

use crate::ActiveTheme;

/// A logo that isn't a valid SVG shows as an empty square.
#[derive(IntoElement)]
pub struct LogoBadge {
    svg: Arc<[u8]>,
    size: Pixels,
}

impl LogoBadge {
    pub fn new(svg: impl Into<Arc<[u8]>>) -> Self {
        Self { svg: svg.into(), size: px(20.) }
    }

    pub fn size(mut self, size: Pixels) -> Self {
        self.size = size;
        self
    }
}

impl RenderOnce for LogoBadge {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let radius = self.size * 0.22;
        let square = div().flex_shrink_0().size(self.size).rounded(radius);
        match logo(&self.svg) {
            Some(image) => square.child(img(ImageSource::Render(image)).size(self.size).rounded(radius)),
            None => square.bg(cx.theme().fill),
        }
    }
}

/// Logos are rasterised once, this many pixels square: sharp up to a 64pt
/// badge on a 2× screen. (GPUI would rasterise an SVG at its declared size,
/// e.g. 24 px, and scale that up.)
const LOGO_PIXELS: u32 = 128;

/// The rasterised logo, cached by the SVG's contents.
fn logo(svg: &[u8]) -> Option<Arc<RenderImage>> {
    static CACHE: OnceLock<Mutex<HashMap<u64, Option<Arc<RenderImage>>>>> = OnceLock::new();
    let mut hasher = DefaultHasher::new();
    svg.hash(&mut hasher);
    let mut cache = CACHE.get_or_init(Default::default).lock().ok()?;
    cache.entry(hasher.finish()).or_insert_with(|| rasterise(svg)).clone()
}

/// Renders `svg` centred in a `LOGO_PIXELS` square.
fn rasterise(svg: &[u8]) -> Option<Arc<RenderImage>> {
    let tree = usvg::Tree::from_data(svg, &usvg::Options::default()).ok()?;
    let (w, h) = (tree.size().width(), tree.size().height());
    let scale = LOGO_PIXELS as f32 / w.max(h);
    let offset = |side: f32| (LOGO_PIXELS as f32 - side * scale) / 2.;
    let transform = tiny_skia::Transform::from_scale(scale, scale).post_translate(offset(w), offset(h));
    let mut pixmap = tiny_skia::Pixmap::new(LOGO_PIXELS, LOGO_PIXELS)?;
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    crate::render_image(pixmap.take(), LOGO_PIXELS, LOGO_PIXELS)
}


#[cfg(test)]
mod tests {
    use super::*;

    /// BGRA pixel at (x, y) of a rasterised logo.
    fn pixel(image: &RenderImage, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * LOGO_PIXELS + x) * 4) as usize;
        image.as_bytes(0).unwrap()[i..i + 4].try_into().unwrap()
    }

    #[test]
    fn rasterises_logos_in_their_own_colours() {
        let red = rasterise(br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><rect width="24" height="24" fill="#ff0000"/></svg>"##).unwrap();
        assert_eq!(pixel(&red, 64, 64), [0, 0, 255, 255], "red, as BGRA");
        assert_eq!(pixel(&red, 127, 127), [0, 0, 255, 255], "fills the square");

        // A wide logo is centred: transparent above and below.
        let wide = rasterise(br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 24"><rect width="48" height="24" fill="#00ff00"/></svg>"##).unwrap();
        assert_eq!(pixel(&wide, 64, 64), [0, 255, 0, 255]);
        assert_eq!(pixel(&wide, 64, 5)[3], 0);

        assert!(rasterise(b"not an svg").is_none());
    }

    #[test]
    fn caches_each_logo() {
        static LOGO: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1 1"/>"#;
        assert!(Arc::ptr_eq(&logo(LOGO).unwrap(), &logo(LOGO).unwrap()));
    }
}
