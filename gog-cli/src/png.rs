//! One still SVG → a PNG file.
//!
//! For the places a plot has to be a picture: a document, a slide, a message.
//! The drawing is the engine's and the conversion is `raster`'s; all this adds
//! is the file. A played plot never reaches here, because a PNG is one moment
//! and `gog_core::plot::render_still` refuses a plot that has several.

use crate::raster;

/// Write `svg` to `path` as a PNG at `scale` times its own size.
///
/// The file is created only once the picture is encoded, so a plot that cannot
/// be drawn leaves an existing file at `path` as it was. Returns the pixel size
/// written, for the caller to report.
pub fn write(svg: &str, path: &str, scale: f32) -> Result<(u32, u32), String> {
    let options = raster::options();
    let pixmap = raster::rasterize(svg, scale, &options, "the plot")?;
    let bytes = pixmap
        .encode_png()
        .map_err(|e| format!("the plot could not be encoded as a PNG: {e}"))?;
    std::fs::write(path, bytes).map_err(|e| format!("cannot write {path}: {e}"))?;
    Ok((pixmap.width(), pixmap.height()))
}
