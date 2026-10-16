//! A finished SVG → pixels, for the two raster files this bridge writes.
//!
//! **This module converts; it never draws.** It is handed finished SVG by
//! `gog_core::plot` and turns it into pixels, so it holds no opinion about a
//! tick, a color or a layout and has none to disagree with the engine about.
//! That separation is the condition under which raster output was allowed at
//! all: a second writer that decides things drifts, and the last one to do so
//! ended up drawing untransformed rows under a transform's name.
//!
//! It lives in the bridge rather than in `gog-core` for the same reason the
//! bridge exists. A font stack and a rasterizer are a large dependency, and the
//! engine every binding's correctness rests on stays at `serde` — the crate that
//! decides what a plot *is* should not also carry the machinery for one of the
//! ways it can be written out.
//!
//! The SVG remains the reference: it is resolution independent, its text is
//! still text, and it is what the parity harness compares. A raster is none of
//! those, and depends besides on which fonts the machine writing it has
//! installed, so it is a derived artifact and is treated as one.

use resvg::{tiny_skia, usvg};

/// What the SVG is read with: the machine's own fonts, loaded once.
///
/// Loaded by the caller and passed in, not per picture. Twelve loads of a font
/// database cost more than the twelve rasterizations of a GIF they would serve,
/// and every frame of one sequence resolves the same families anyway.
///
/// **A computer with no fonts gets a picture with no letters, and is told so.**
/// The rasterizer leaves out text it has no font for, and says nothing: a bare
/// server or a small container would write a plot with no title, no numbers on
/// its axes and an empty legend. That is a silent drop (§12), so the bridge says
/// what was left out and how to get it back.
pub fn options() -> usvg::Options<'static> {
    let mut options = usvg::Options::default();
    options.fontdb_mut().load_system_fonts();
    if options.fontdb.len() == 0 {
        eprintln!(
            "gog: no fonts were found on this computer, so the picture has no text: \
             no title, no numbers on the axes and no legend labels. Install a font, \
             such as the DejaVu fonts, and save it again, or save an SVG with \
             `save_svg()`, whose text the viewer's browser draws."
        );
    }
    options
}

/// One SVG drawn into pixels at `scale` times its own size, on white.
///
/// `what` names the picture in an error, "frame 3" or "the plot", so a failure
/// says which one it was.
pub fn rasterize(
    svg: &str,
    scale: f32,
    options: &usvg::Options<'_>,
    what: &str,
) -> Result<tiny_skia::Pixmap, String> {
    let tree = usvg::Tree::from_str(svg, options)
        .map_err(|e| format!("{what} could not be read back: {e}"))?;
    let canvas = tree.size();
    // Rounded rather than truncated, so a canvas whose scaled height lands a
    // hair under an integer does not lose its bottom row of pixels.
    let (w, h) = (
        (canvas.width() * scale).round() as u32,
        (canvas.height() * scale).round() as u32,
    );
    let mut pixmap = tiny_skia::Pixmap::new(w, h)
        .ok_or_else(|| format!("{w}x{h} is not a size that can be drawn"))?;
    // Filled opaque first. A GIF spends one palette entry on transparency, and
    // blending the plot's own background into it would fringe every glyph —
    // text is where a partly transparent edge shows worst. A PNG is filled the
    // same way, so the two files of one plot have the same white around it.
    pixmap.fill(tiny_skia::Color::WHITE);
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    Ok(pixmap)
}
