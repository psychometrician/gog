//! Still SVG frames → one animated GIF.
//!
//! **This module converts; it never draws.** It is handed finished SVG by
//! `gog_core::plot::render_frames` and turns each one into pixels, so it holds
//! no opinion about a tick, a color or a layout and has none to disagree with
//! the engine about. That separation is the condition under which raster output
//! was allowed at all: a second writer that decides things drifts, and the last
//! one to do so ended up drawing untransformed rows under a transform's name.
//!
//! It lives in the bridge rather than in `gog-core` for the same reason the
//! bridge exists. A font stack and a rasterizer are a large dependency, and the
//! engine every binding's correctness rests on stays at `serde` — the crate that
//! decides what a plot *is* should not also carry the machinery for one of the
//! ways it can be written out.
//!
//! Where SVG is not read as motion — a message to a friend, a slide, a post —
//! this is the file that moves. The SVG remains the reference: it is resolution
//! independent, its text is still text, and it is what the parity harness
//! compares. A raster is none of those, and depends besides on which fonts the
//! machine writing it has installed, so it is a derived artifact and is treated
//! as one.

use resvg::{tiny_skia, usvg};

/// Encode `frames` into an infinitely looping GIF at `path`.
///
/// `scale` multiplies the plot's own canvas: a plot is 800 by 600 unless its
/// theme says otherwise, which is small for a post, and `scale = 2` is the
/// cheapest way to a sharp one. `delay_cs` is hundredths of a second per frame,
/// which is the resolution the format has.
///
/// Returns the pixel size written, for the caller to report.
pub fn write(
    frames: &[String],
    path: &str,
    scale: f32,
    delay_cs: u16,
) -> Result<(u32, u32), String> {
    // The system fonts are loaded **once**, not per frame. Twelve loads of a
    // font database costs more than the twelve rasterizations it would serve,
    // and every frame of one sequence resolves the same families anyway.
    let mut options = usvg::Options::default();
    options.fontdb_mut().load_system_fonts();

    // Opened on the first frame rather than up front, so a sequence that cannot
    // be drawn leaves no empty file behind claiming it was.
    let mut encoder: Option<gif::Encoder<std::fs::File>> = None;
    let mut size = (0, 0);

    for (i, svg) in frames.iter().enumerate() {
        let tree = usvg::Tree::from_str(svg, &options)
            .map_err(|e| format!("frame {i} could not be read back: {e}"))?;
        let canvas = tree.size();
        // Rounded rather than truncated, so a canvas whose scaled height lands a
        // hair under an integer does not lose its bottom row of pixels.
        let (w, h) = (
            (canvas.width() * scale).round() as u32,
            (canvas.height() * scale).round() as u32,
        );
        let mut pixmap = tiny_skia::Pixmap::new(w, h)
            .ok_or_else(|| format!("{w}x{h} is not a size that can be drawn"))?;
        // Filled opaque first. A GIF spends one palette entry on transparency,
        // and blending the plot's own background into it would fringe every
        // glyph — text is where a partly transparent edge shows worst.
        pixmap.fill(tiny_skia::Color::WHITE);
        resvg::render(
            &tree,
            tiny_skia::Transform::from_scale(scale, scale),
            &mut pixmap.as_mut(),
        );

        if encoder.is_none() {
            size = (w, h);
            let file = std::fs::File::create(path)
                .map_err(|e| format!("cannot write {path}: {e}"))?;
            let mut started = gif::Encoder::new(file, w as u16, h as u16, &[])
                .map_err(|e| format!("cannot start {path}: {e}"))?;
            started
                .set_repeat(gif::Repeat::Infinite)
                .map_err(|e| format!("cannot set the loop: {e}"))?;
            encoder = Some(started);
        }
        let encoder = encoder.as_mut().expect("opened on the first frame");

        let mut rgba = pixmap.data().to_vec();
        // Per frame rather than one palette for the sequence: a continuous ramp
        // like `plasma` spends most of 256 colors on itself, and a shared palette
        // would band the very gradient the plot exists to show.
        let mut frame = gif::Frame::from_rgba_speed(w as u16, h as u16, &mut rgba, 10);
        frame.delay = delay_cs;
        encoder
            .write_frame(&frame)
            .map_err(|e| format!("frame {i} could not be written: {e}"))?;
    }

    Ok(size)
}
