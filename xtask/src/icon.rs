//! The app's icon: `packaging/macos/AppIcon.svg` drawn at each size the icon set needs,
//! then `iconutil` makes the `.icns`.

use std::path::Path;

use anyhow::{Context as _, Result};
use resvg::{tiny_skia, usvg};

use crate::shell::{run, tool};

/// The icon set's files: the pixels each is drawn at, and its name.
const SIZES: [(u32, &str); 10] = [
    (16, "icon_16x16"),
    (32, "icon_16x16@2x"),
    (32, "icon_32x32"),
    (64, "icon_32x32@2x"),
    (128, "icon_128x128"),
    (256, "icon_128x128@2x"),
    (256, "icon_256x256"),
    (512, "icon_256x256@2x"),
    (512, "icon_512x512"),
    (1024, "icon_512x512@2x"),
];

/// Write `output` (an `.icns`) from `svg`, using `work` as scratch space.
pub fn build(svg: &Path, work: &Path, output: &Path) -> Result<()> {
    let tree = usvg::Tree::from_data(&std::fs::read(svg)?, &usvg::Options::default()).with_context(|| format!("reading {}", svg.display()))?;
    let iconset = work.join("Delight.iconset");
    std::fs::create_dir_all(&iconset)?;
    for (pixels, name) in SIZES {
        let mut pixmap = tiny_skia::Pixmap::new(pixels, pixels).context("an empty icon")?;
        let (width, height) = (tree.size().width(), tree.size().height());
        let transform = tiny_skia::Transform::from_scale(pixels as f32 / width, pixels as f32 / height);
        resvg::render(&tree, transform, &mut pixmap.as_mut());
        pixmap.save_png(iconset.join(format!("{name}.png"))).with_context(|| format!("writing {name}"))?;
    }
    run(tool("iconutil", ["-c", "icns", "-o"].map(std::ffi::OsString::from).into_iter().chain([output.into(), iconset.into_os_string()])))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_icon_set_has_every_size_macos_asks_for() {
        let mut names: Vec<&str> = SIZES.iter().map(|(_, name)| *name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), 10, "each name once");
        assert!(SIZES.iter().any(|(pixels, name)| *pixels == 1024 && *name == "icon_512x512@2x"));
    }

    #[test]
    fn the_app_icon_draws_something_at_every_size() {
        let svg = Path::new(env!("CARGO_MANIFEST_DIR")).join("../packaging/macos/AppIcon.svg");
        let tree = usvg::Tree::from_data(&std::fs::read(svg).unwrap(), &usvg::Options::default()).unwrap();
        for (pixels, _) in SIZES {
            let mut pixmap = tiny_skia::Pixmap::new(pixels, pixels).unwrap();
            let scale = pixels as f32 / tree.size().width();
            resvg::render(&tree, tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());
            assert!(pixmap.pixels().iter().filter(|pixel| pixel.alpha() > 0).count() > (pixels * pixels / 4) as usize, "{pixels}px");
        }
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn an_icns_file_is_made() {
        let work = std::env::temp_dir().join(format!("delight-xtask-icon-{}", std::process::id()));
        std::fs::create_dir_all(&work).unwrap();
        let output = work.join("Delight.icns");
        build(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../packaging/macos/AppIcon.svg"), &work, &output).unwrap();
        let bytes = std::fs::read(&output).unwrap();
        assert_eq!(&bytes[..4], b"icns", "the .icns magic");
        assert!(bytes.len() > 50_000, "{} bytes", bytes.len());
        std::fs::remove_dir_all(&work).unwrap();
    }
}
