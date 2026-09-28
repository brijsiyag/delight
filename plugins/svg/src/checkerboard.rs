//! [`Checkerboard`]: the grey-and-white squares image viewers draw behind a picture,
//! so its transparent areas show.

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};

use delight_ui::ActiveTheme;
use gpui::{
    App, ImageSource, IntoElement, ObjectFit, Pixels, RenderImage, RenderOnce, Size, Styled, StyledImage, Window, img,
    px,
};

/// A square's side: 8 pt, 16 px on a 2× screen.
const SQUARE_PIXELS: u32 = 16;

/// Fills its parent (give the parent `.relative()`), behind its content.
#[derive(IntoElement)]
pub struct Checkerboard {
    size: Size<Pixels>,
    radius: Pixels,
}

impl Checkerboard {
    /// `size`: about the parent's size. The squares are made for it and stretched to
    /// the parent's exact size, so they stay square.
    pub fn new(size: Size<Pixels>) -> Self {
        Self { size, radius: px(0.) }
    }

    /// The parent's corner radius: GPUI clips children to a rectangle, so the squares
    /// need their own rounded corners.
    pub fn rounded(mut self, radius: Pixels) -> Self {
        self.radius = radius;
        self
    }
}

impl RenderOnce for Checkerboard {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let dark = cx.theme().dark;
        // For a 2× screen.
        let (width, height) = ((f32::from(self.size.width) * 2.) as u32, (f32::from(self.size.height) * 2.) as u32);
        img(ImageSource::Render(squares(width.max(1), height.max(1), dark)))
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .rounded(self.radius)
            .object_fit(ObjectFit::Fill)
    }
}

/// The squares made so far, by (width, height, dark).
type Made = HashMap<(u32, u32, bool), Arc<RenderImage>>;

/// The squares, as the BGRA image GPUI draws; made once per size and theme.
fn squares(width: u32, height: u32, dark: bool) -> Arc<RenderImage> {
    static CACHE: LazyLock<Mutex<Made>> = LazyLock::new(Default::default);
    let mut cache = CACHE.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    cache
        .entry((width, height, dark))
        .or_insert_with(|| {
            let shades: [u8; 2] = if dark { [0x2C, 0x24] } else { [0xFF, 0xEE] };
            let mut pixels = Vec::with_capacity((width * height * 4) as usize);
            for y in 0..height {
                for x in 0..width {
                    let shade = shades[((x / SQUARE_PIXELS + y / SQUARE_PIXELS) % 2) as usize];
                    pixels.extend_from_slice(&[shade, shade, shade, 0xFF]);
                }
            }
            delight_ui::render_image(pixels, width, height).expect("the pixels fill the image")
        })
        .clone()
}
