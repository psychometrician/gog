//! Still SVG frames → one animated GIF.
//!
//! The pixels are `raster`'s, which converts and never draws: each frame comes
//! out of `gog_core::plot::render_frames` finished, and this module only strings
//! them into a file that moves.
//!
//! Where SVG is not read as motion — a message to a friend, a slide, a post —
//! this is the file that moves.

use crate::raster;

/// Encode `frames` into an infinitely looping GIF at `path`.
///
/// `scale` multiplies the plot's own canvas: a plot is 800 by 600 unless its
/// theme says otherwise, which is small for a post, and `scale = 2` is the
/// cheapest way to a sharp one. `delays_cs` is each frame's hold in hundredths
/// of a second, which is the resolution the format has.
///
/// Returns the pixel size written, for the caller to report.
pub fn write(
    frames: &[String],
    path: &str,
    scale: f32,
    delays_cs: &[u16],
) -> Result<(u32, u32), String> {
    if delays_cs.len() != frames.len() {
        return Err(format!(
            "{} frames were drawn but {} holds were given",
            frames.len(),
            delays_cs.len()
        ));
    }
    let options = raster::options();

    // Opened on the first frame rather than up front, so a sequence that cannot
    // be drawn leaves no empty file behind claiming it was.
    let mut encoder: Option<gif::Encoder<std::fs::File>> = None;
    let mut size = (0, 0);

    for (i, svg) in frames.iter().enumerate() {
        let pixmap = raster::rasterize(svg, scale, &options, &format!("frame {i}"))?;
        let (w, h) = (pixmap.width(), pixmap.height());

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
        frame.delay = delays_cs[i];
        encoder
            .write_frame(&frame)
            .map_err(|e| format!("frame {i} could not be written: {e}"))?;
    }

    Ok(size)
}
