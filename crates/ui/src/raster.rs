//! Pixels as images GPUI draws: the logo badges, and tools' rendered
//! pictures (e.g. the SVG tool's preview).

use std::sync::Arc;

use gpui::RenderImage;
use smallvec::SmallVec;

/// tiny-skia's premultiplied RGBA pixels as the straight-alpha BGRA image
/// GPUI draws.
pub fn render_image(mut pixels: Vec<u8>, width: u32, height: u32) -> Option<Arc<RenderImage>> {
    for px in pixels.chunks_exact_mut(4) {
        px.swap(0, 2);
        if px[3] > 0 {
            let alpha = px[3] as f32 / 255.;
            for c in &mut px[..3] {
                *c = (*c as f32 / alpha) as u8;
            }
        }
    }
    let frame = image::Frame::new(image::RgbaImage::from_raw(width, height, pixels)?);
    Some(Arc::new(RenderImage::new(SmallVec::from_elem(frame, 1))))
}
