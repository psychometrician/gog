//! The marks — one drawing routine per mark, each a `pub(crate)` method on
//! `SvgRenderer` in its own file, dispatched by `SvgRenderer::render`. The
//! toolkit shared across marks (bar thickness, the `dodge` offset) lives here.
use std::borrow::Cow;
use std::fmt::Write;
use crate::data::DataFrame;
use crate::ir::{Channel, Layer, Transform};
use crate::render::polar::Polar;
use crate::render::svg::unit_norm;
use crate::render::Layout;

mod area;
mod bar;
mod boxplot;
mod interval;
mod line;
mod path;
mod point;
mod ribbon;
mod rule;
mod step;
mod surface;
mod text;
// Not a mark — the slot reading of `density`, drawn by `area` and `ribbon` alike
// (spec §5). It sits here because it *is* a drawing routine, and beside the two
// marks whose geometry it is rather than inside either, since neither owns it.
pub(crate) mod violin;
mod zone;

/// Where a data pair lands on the page, in whichever coordinate space the plot
/// is drawn in.
///
/// Flat, that is the two axes taken independently — the mapping `Layout` has
/// always done. Polar, it is the same two numbers read as an angle and a radius
/// (`render::polar`). One function rather than one per mark, so a point, the line
/// through it and a label beside it cannot disagree about where the datum is; the
/// marks that place a value at an (x, y) all call this and learn nothing about
/// the space they are in.
fn place(l: &Layout, polar: Option<&Polar>, x: f64, y: f64, xs: (f64, f64), ys: (f64, f64)) -> (f64, f64) {
    match polar {
        Some(p) => p.at(unit_norm(x, xs), unit_norm(y, ys)),
        None => (l.map_x(x, xs.0, xs.1), l.map_y(y, ys.0, ys.1)),
    }
}

/// Where a position column's values sit on its axis, whichever kind of axis it is.
///
/// A numeric column already *is* positions and is borrowed as it stands; a
/// categorical one becomes its category's index in the axis order — the order
/// `detect_categories` fixed, which `order()` may have sorted — so category *k*
/// sits at *k* and everything downstream (the sort, the slot width, the polar
/// angle) reads plain numbers and learns nothing about strings.
///
/// One function rather than one per mark, for the reason [`place`] is one
/// function: nine marks resolving "where is this category" separately is nine
/// chances to disagree, and a Law-2 gap is exactly what a per-mark copy of this
/// block produces. `None` means the column is neither — a string column on an
/// axis that resolved no categories — and the caller draws nothing.
fn positions<'a>(df: &'a DataFrame, field: &str, cats: Option<&[String]>) -> Option<Cow<'a, [f64]>> {
    if let Some(vals) = df.float_col(field) {
        return Some(Cow::Borrowed(vals));
    }
    let cats = cats?;
    let strs = df.str_col(field)?;
    Some(Cow::Owned(
        strs.iter()
            .map(|s| cats.iter().position(|c| c == s).map(|i| i as f64).unwrap_or(0.0))
            .collect(),
    ))
}

/// Bring a path's vertex list back to its first vertex when the angular domain
/// wraps — the closing segment of a radar.
///
/// Only ever fires in polar on a *categorical* angular axis, where the categories
/// exhaust the turn and nothing repeats the first one ([`Polar::wraps`]). Flat, and
/// on a measured angle, the list is returned untouched: there the curve either has
/// two ends because the domain has two ends, or closes itself because the data
/// supplied both. Shared by `line` and `area` so the two cannot disagree about
/// whether a radar is a closed shape.
fn close_if_wrapped(idxs: &mut Vec<usize>, polar: Option<&Polar>) {
    if idxs.len() >= 2 && polar.is_some_and(|p| p.wraps()) {
        idxs.push(idxs[0]);
    }
}

/// The two edges each row owns along one **floor** axis of the cube, in data units.
///
/// A cut mesh publishes them as columns; a category owns `[k-½, k+½]` and the scale
/// already holds it, so the slot is read back rather than stored. A *number* on an
/// axis no mesh cut is a point and owns no slot at all — `None`, and the caller draws
/// nothing, which is what makes a 3-D scatter of bars refuse rather than guess a width.
///
/// `fill` is how much of a category's slot the mark takes, and it is the mark's own
/// business rather than this function's: a `bar` takes four fifths and the empty fifth
/// is what *says* the categories are separate, where a `box` takes less again so its
/// whiskers have air. A cut axis ignores `fill` entirely — a histogram's bins are
/// adjacent intervals and must touch (Wilkinson: "there cannot be gaps between bars").
///
/// Lives here rather than inside `write_bars_3d` because all three **slot marks**
/// stand on the same floor (`legality::is_slot_mark`), and a footprint each of them
/// computed separately is three chances to disagree about where a cell is — the Law-2
/// gap a per-mark copy always produces.
fn cell_edges(
    df: &DataFrame, cut: (&str, &str), field: &str, cats: Option<&[String]>, fill: f64,
) -> Option<(Vec<f64>, Vec<f64>)> {
    if let (Some(lo), Some(hi)) = (df.float_col(cut.0), df.float_col(cut.1)) {
        return Some((lo.to_vec(), hi.to_vec()));
    }
    df.str_col(field)?;
    let p = positions(df, field, cats)?;
    let half = fill / 2.0;
    Some((p.iter().map(|v| v - half).collect(), p.iter().map(|v| v + half).collect()))
}

/// The order to paint solids standing on the cube's floor: far foot first.
///
/// **The sorted unit is the footprint, not the solid** (spec §15). Every slot mark
/// stands on the same floor, so which one occludes which is settled entirely by where
/// their feet are — a near column hides a far one however tall either is. Sorting by
/// a solid's center would let a tall far one claim to be nearer than a short near one
/// and paint over it, which is the picture being wrong in a way the reader cannot see.
/// Shared for the reason `cell_edges` is: one floor, one order.
fn floor_order(
    n: usize, x0s: &[f64], x1s: &[f64], y0s: &[f64], y1s: &[f64],
    xs: (f64, f64), ys: (f64, f64), base: f64, scene: &crate::render::project::Scene,
) -> Vec<usize> {
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| {
        let d = |i: usize| {
            let (cx, cy) = ((x0s[i] + x1s[i]) / 2.0, (y0s[i] + y1s[i]) / 2.0);
            scene.to_screen(unit_norm(cx, xs), unit_norm(cy, ys), base).depth
        };
        d(b).partial_cmp(&d(a)).unwrap_or(std::cmp::Ordering::Equal)
    });
    order
}

/// One solid standing on the cube's floor, from `lo` to `hi` on `z`.
///
/// A bar's column and a box's body are the **same shape**: an axis-aligned box on a
/// footprint, differing only in where its two ends come from — a baseline and a value
/// for one, two quartiles for the other. One routine, so they cannot part company
/// about face order, shading or the seam hairline.
///
/// The six faces take the shade their **data axis** earns: the top is the color
/// itself, the `x`-facing pair one step down, the `y`-facing pair two. Fixed to the
/// axes rather than to the light, so turning the scene rearranges the faces without
/// recoloring them (`palette::shade`).
#[allow(clippy::too_many_arguments)]
fn write_solid(
    svg: &mut String, scene: &crate::render::project::Scene,
    nx: (f64, f64), ny: (f64, f64), nz: (f64, f64),
    color: &str, opacity: f64,
) {
    let (lo, hi) = (nz.0.min(nz.1), nz.0.max(nz.1));
    // A solid with no height is no solid: an empty cell is left as floor rather than
    // drawn flat, the same refusal `zone` makes when it declines to paint an empty
    // cell the bottom of its ramp (spec §5).
    if (hi - lo).abs() < 1e-9 { return; }
    if ![nx.0, nx.1, ny.0, ny.1, lo, hi].iter().all(|v| v.is_finite()) { return; }

    // The 8 corners, `(x, y, z)` in the unit cube: floor first, then ceiling, each
    // counter-clockwise seen from above.
    let c = [
        (nx.0, ny.0, lo), (nx.1, ny.0, lo), (nx.1, ny.1, lo), (nx.0, ny.1, lo),
        (nx.0, ny.0, hi), (nx.1, ny.0, hi), (nx.1, ny.1, hi), (nx.0, ny.1, hi),
    ];
    let p: Vec<_> = c.iter().map(|&(a, b, d)| scene.to_screen(a, b, d)).collect();

    const FACES: [([usize; 4], f64); 6] = [
        ([4, 5, 6, 7], 0.00), // top    (z = high)
        ([0, 3, 2, 1], 0.34), // bottom (z = low) — culled from above, kept
                              //   so a tilt below the floor still draws a solid
        ([0, 1, 5, 4], 0.30), // y = y0
        ([3, 7, 6, 2], 0.30), // y = y1
        ([0, 4, 7, 3], 0.18), // x = x0
        ([1, 2, 6, 5], 0.18), // x = x1
    ];
    // Back-face culling by the projected winding, then painter's order among what is
    // left. A box is convex, so a face turned away from the camera is hidden by the
    // ones facing it and drawing it is wasted bytes — which is worth caring about
    // here, where a 30×30 mesh is 900 solids.
    let mut faces: Vec<(f64, usize)> = FACES.iter().enumerate()
        .filter_map(|(fi, (idx, _))| {
            let area: f64 = (0..4).map(|k| {
                let (a, b) = (&p[idx[k]], &p[idx[(k + 1) % 4]]);
                a.x * b.y - b.x * a.y
            }).sum();
            // SVG's y grows downward, so a counter-clockwise face in data space comes
            // out with negative signed area when it faces us.
            (area < 0.0).then(|| {
                let d = idx.iter().map(|&k| p[k].depth).sum::<f64>() / 4.0;
                (d, fi)
            })
        })
        .collect();
    faces.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    for (_, fi) in faces {
        let (idx, dim) = FACES[fi];
        let fill = crate::render::palette::shade(color, dim);
        let d: String = idx.iter().enumerate()
            .map(|(k, &v)| format!("{}{:.2},{:.2}", if k == 0 { "M" } else { "L" }, p[v].x, p[v].y))
            .collect::<Vec<_>>()
            .join(" ");
        // A hairline in the face's own shade closes the seam antialiasing leaves
        // between two abutting polygons — the same job the histogram's panel-color
        // separator does one dimension down, with the opposite sign: there the bars
        // must be parted, here the faces must not be.
        let _ = writeln!(svg,
            r#"    <path d="{d} Z" fill="{fill}" fill-opacity="{opacity:.3}" stroke="{fill}" stroke-width="0.5"/>"#);
    }
}

/// How thick a bar is, across the axis it sits on.
///
/// Takes the position axis's pixel length and scale rather than the whole
/// `Layout`, so it reads the same whether the bars stand up or lie down.
fn bar_thickness_svg(pos_vals: &[f64], n: usize, pos_px: f64, pos_scale: (f64, f64), contiguous: bool) -> f64 {
    if n == 1 { return pos_px * 0.20; }
    let mut sorted: Vec<f64> = pos_vals[..n].to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let min_gap = sorted.windows(2).map(|w| (w[1] - w[0]).abs()).filter(|&d| d > 1e-12).fold(f64::INFINITY, f64::min);
    if !min_gap.is_finite() { return pos_px * 0.20; }
    let span = (pos_scale.1 - pos_scale.0).max(1e-12);
    // A histogram's bins are adjacent intervals on a continuous axis, so a bar
    // fills its whole bin and touches its neighbors (factor 1.0). A categorical
    // bar leaves a fifth of its slot empty — that gap is what *says* the
    // categories are separate rather than a divided continuum.
    let fill = if contiguous { 1.0 } else { 0.80 };
    (min_gap / span) * pos_px * fill
}

/// Side-by-side placement for a `dodge`d, group-split mark (spec §5). The groups a
/// `color`/`group` split would stack at one shared position are tiled across that
/// position's slot instead: with `G` groups, group `g` (in canonical order) draws
/// at `1/G` of the slot's bar-thickness, its center offset `(g − (G−1)/2)·(slot/G)`
/// along the position axis. So the whole group occupies exactly the width one
/// undivided mark would — the between-category air is untouched — and, because the
/// offset resolves the overlap, dodged marks draw solid (the caller drops the
/// overlay's translucency).
///
/// Built only when the layer carries `Transform::Dodge` *and* the split has at
/// least two groups; otherwise the mark draws un-dodged, and a lone group is the
/// identity rather than a needless narrowing.
struct Dodge {
    /// The split values in canonical (legend) order — the offset index.
    groups: Vec<String>,
    /// Each original row's split value, so a writer iterating rows (or low/high
    /// pairs) looks its offset up by index.
    values: Vec<String>,
}

impl Dodge {
    /// Resolve a layer's dodge, or `None` when it is not dodged / has nothing to
    /// separate. Keys off `color`, else `group` — the precedence the statistics
    /// and the legend already use, so the offset order matches the swatch order.
    fn resolve(layer: &Layer, df: &DataFrame) -> Option<Dodge> {
        if !layer.transforms.iter().any(|t| matches!(t, Transform::Dodge)) {
            return None;
        }
        let field = layer
            .encodings
            .get(&Channel::Color)
            .or_else(|| layer.encodings.get(&Channel::Group))
            .map(|c| c.field.as_str())?;
        let values = df.str_col(field)?.to_vec();
        let groups = crate::data::categories_across(&[df], field);
        if groups.len() < 2 {
            return None; // one group: nothing to set beside anything
        }
        Some(Dodge { groups, values })
    }

    fn count(&self) -> f64 {
        self.groups.len() as f64
    }

    /// Each dodged mark is this fraction of the full slot width.
    fn width_frac(&self) -> f64 {
        1.0 / self.count()
    }

    /// The position-axis offset (same units as `slot`) for the mark on `row`. An
    /// unknown group sits at the slot center rather than off the end.
    fn offset_at(&self, row: usize, slot: f64) -> f64 {
        let g = self.values.get(row).map(String::as_str).unwrap_or("");
        match self.groups.iter().position(|x| x == g) {
            Some(i) => (i as f64 - (self.count() - 1.0) / 2.0) * (slot / self.count()),
            None => 0.0,
        }
    }
}


// ---------------------------------------------------------------------------
// A measure along a stroke — the `color` channel's other reading
//
// `color` on a stroke mark (`line`, `step`, `path`) answers one of two
// questions depending on the column it is handed, exactly as it does on a
// `point`: a *category* says which series this stroke is, and a *measure* says
// what the route was carrying as it went. The first splits the mark into one
// stroke per group; the second varies the color **along** a single stroke.
//
// The second reading is what makes these marks' `color` row read `Either`
// rather than `Discrete`. It costs a change of geometry: a ramped stroke cannot
// be one `<polyline>` (an element takes one `stroke`), so it is emitted as one
// element per segment. That switch is invisible in the grammar — the same
// sentence with a categorical column still emits the single polyline, byte for
// byte — and it is the same shape the 3-D depth sort needs, which is why the
// segment writer below is shared by both.
// ---------------------------------------------------------------------------

/// The color a stroke's segments take when `color` maps a measure.
pub(crate) struct StrokeRamp<'a> {
    vals: &'a [f64],
    scale: crate::scale::ChannelScale,
    stops: Vec<&'a str>,
}

impl<'a> StrokeRamp<'a> {
    /// Present only when `color` maps a **numeric** column. A categorical one is
    /// the series split, which the caller already does, so returning `None` here
    /// leaves that path exactly as it was.
    pub(crate) fn resolve(layer: &Layer, df: &'a DataFrame, ramp: &'a [String]) -> Option<Self> {
        let def = layer.encodings.get(&Channel::Color)?;
        // A set color wins over the channel everywhere else; `check_style`
        // refuses a layer that both maps and sets one, so there is nothing to
        // arbitrate here.
        Self::of(df, &def.field, ramp, Some(def))
    }

    /// The same ramp over a column named directly rather than bound.
    ///
    /// Needed because a **two-dimensional reading** measures itself by a column no
    /// binding named — a contour's `level` — and the rule the heatmap established is
    /// that when both positions are spoken for, the measurement goes to `color`
    /// whether or not anybody said so. Without this the rings drew in one color
    /// while a color-bar legend claimed they decoded the level, which is a key for
    /// an encoding that was not drawn.
    pub(crate) fn of(
        df: &'a DataFrame, field: &str, ramp: &'a [String],
        def: Option<&crate::ir::ChannelDef>,
    ) -> Option<Self> {
        let vals = df.float_col(field)?;
        Some(StrokeRamp {
            vals,
            scale: crate::scale::ChannelScale::of(vals, def),
            stops: ramp.iter().map(String::as_str).collect(),
        })
    }

    /// The color of the segment joining two rows: the ramp at the **mean** of
    /// their two fractions. A segment spans two values and has to show one
    /// color, and the midpoint is the only choice that treats its two ends
    /// alike — the same rule, and the same reason, as the mean depth that sorts
    /// a segment in 3-D.
    pub(crate) fn segment(&self, i: usize, j: usize) -> String {
        let f = |k: usize| self.scale.fraction(self.vals.get(k).copied().unwrap_or(f64::NAN));
        let t = (f(i) + f(j)) / 2.0;
        crate::render::palette::ramp_at(&self.stops, t)
    }
}

/// One segment of a stroke, as its own SVG element.
///
/// `run` is how far along the route this segment starts, in page pixels, and it
/// becomes the `stroke-dashoffset`. That is what keeps a dash a property of the
/// **route**: a dash restarts at the start of an element, so without the carried
/// phase a segmented stroke would show one identical dash per vertex — a texture
/// the data never asked for. Solid strokes add no attribute and so are
/// unaffected.
pub(crate) fn segment_svg(
    a: (f64, f64), b: (f64, f64), stroke: &str,
    width: f64, opacity: f64, dash: &str, run: f64,
) -> String {
    // Round caps on a slanted stroke: consecutive segments meet at a point, and
    // a round cap fills the wedge a butt cap would leave open at the join.
    segment_svg_capped(a, b, stroke, width, opacity, dash, run, "round")
}

/// [`segment_svg`] with the cap named, for the one mark whose corners are square.
pub(crate) fn segment_svg_capped(
    a: (f64, f64), b: (f64, f64), stroke: &str,
    width: f64, opacity: f64, dash: &str, run: f64, cap: &str,
) -> String {
    let off = if dash.is_empty() { String::new() } else { format!(r#" stroke-dashoffset="{run:.2}""#) };
    format!(
        "    <line x1=\"{:.2}\" y1=\"{:.2}\" x2=\"{:.2}\" y2=\"{:.2}\" stroke=\"{stroke}\" \
         stroke-width=\"{width}\" stroke-opacity=\"{opacity:.3}\"{dash}{off} \
         stroke-linecap=\"{cap}\"/>\n",
        a.0, a.1, b.0, b.1)
}

/// The page distance between two points — the arc length a dash phase advances by.
pub(crate) fn seg_len(a: (f64, f64), b: (f64, f64)) -> f64 {
    ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt()
}
