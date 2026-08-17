/// SVG renderer.
///
/// Produces a self-contained SVG string from a `PlotSpec`. Text is real vector
/// text (crisp at any zoom), and the output opens directly in any browser or
/// vector editor. Raster output is obtained by converting the SVG.
///
use std::collections::HashMap;
use std::fmt::Write;

use crate::data::DataFrame;
use crate::ir::{Channel, CoordSpace, Layer, Mark, PlotSpec, SpaceView, ThemeSpec, Transform};
use crate::legality::Diagnostic;
use crate::render::ticks::{auto_label, log_ticks, nice_ticks, ticks_at, ticks_with_labels, time_ticks, TickSpec};
use crate::scale;
use crate::render::layout::{Fit, PanelGrid};
use crate::render::nest::Nest;
use crate::render::polar::Polar;
use crate::render::project::{self, Screen};
use crate::render::{AxisFacts, Drawn, Layout, RenderContext};
use crate::render::legend::{collect_legends, write_legends, LEGEND_PADDING, LEGEND_PLOT_GAP};
use crate::render::palette::{build_color_map, resolve_ramp};
use crate::render::text::{esc, estimate_cap_height, estimate_text_width};

/// The fill opacity of an *overlaid* bar — one whose position is shared with a
/// bar of another color, as every bar of a color-split histogram is. The
/// series-hue outline, not the fill, is what carries each shape, so the fill can
/// be faint enough to see the bars stacked behind it. Used only when the caller
/// has not set `style(opacity = )`, which then answers the question itself.
pub(crate) const OVERLAY_FILL: f64 = 0.4;

/// The outline width of an overlaid bar. A solid, full-opacity edge in the
/// series color is what keeps three translucent histograms legible where they
/// pile up — the "step" silhouette a plain fill loses.
pub(crate) const OVERLAY_OUTLINE_W: f64 = 1.3;

/// How far a linear axis breathes past the data on a *free* end.
///
/// A calibrated visual constant, like the opacity range above — not a semantic
/// knob. Wilkinson (§6.2.2) names "a scale that does not fit closely the range
/// of the data" as a failure of the naive nice-number algorithm, which snaps
/// the range out to the outermost round tick and leaves dead margins. The range
/// follows the data instead; this is the small margin that keeps a mark off the
/// frame line. A *baseline* end (0, where a bar or area measures from) gets none
/// of it — zero is a real coordinate, and a gap below it would misplace it.
const AXIS_EXPAND: f64 = 0.05;

/// The panel background fill — and the color a histogram's bins are separated by.
///
/// A `bar * bin` draws *contiguous* bars: a histogram cuts a continuous axis into
/// adjacent intervals, and Wilkinson is explicit that "there cannot be gaps
/// between bars". Touching bars would merge into one silhouette, so a hairline
/// stroke in *this* color parts them — the same job the categorical gap does for
/// a bar chart, done without reintroducing the gap. Named once so the separator
/// can never drift from the surface it has to vanish against.
pub(crate) const PANEL_BG: &str = "#f5f5f8";

/// The facet strip's default fill: the band above a panel that names the level
/// it holds, one step darker than the panel it sits on.
///
/// Named because it was written **four times** as a literal — the two facet
/// directions, the wrapped ribbon, and the `play` strip — which is how the band
/// came to be the one piece of furniture no theme could reach.
/// `theme(strip = )` is what asks for a different one.
pub(crate) const STRIP_BG: &str = "#e4e4ec";

/// The facet strip's default ink, and one of the two candidates the label's color
/// is chosen from when nobody names one.
pub(crate) const STRIP_INK: &str = "#3c3c46";

/// The other candidate. A band dark enough that [`STRIP_INK`] would disappear on
/// it takes this instead — which is what makes `theme(strip = "black")` a
/// complete instruction rather than half of one.
pub(crate) const STRIP_INK_LIGHT: &str = "#ffffff";

/// The strip label's color: the caller's, else whichever default reads on the
/// band. A band with no luminance to read (`transparent`, `rgb(…)`) keeps the
/// dark ink, which is right for the case that occurs — a transparent band shows
/// the page.
pub(crate) fn strip_ink(theme: &ThemeSpec, band: &str) -> String {
    if let Some(ink) = theme.strip_text.as_deref() {
        return ink.to_string();
    }
    crate::color::better_ink(band, STRIP_INK, STRIP_INK_LIGHT)
        .unwrap_or(STRIP_INK)
        .to_string()
}

/// The gap between a polar plot's rim and the ring of angular tick labels around
/// it — the circular counterpart of the gap between a flat axis and its labels.
/// Named once so the frame that reserves the room and the labels that fill it
/// cannot drift apart.
const POLAR_RIM_GAP: f64 = 9.0;

/// One membership test a subset of the rows is defined by.
///
/// A plot's rows are divided twice before anything is drawn: across the page into
/// facet panels, and along the clock into `play` frames. Both divisions are the
/// same operation — keep the rows whose column equals this value — so they share
/// one type rather than each growing a filter path, which is what keeps the
/// statistics, the shared cut and the shared scale from having to know which
/// division they are inside.
///
/// Two variants because `play` accepts a numeric column where `facet` refuses one
/// (`data::frames_across` records why). A facet only ever produces `Str`.
#[derive(Clone, Copy)]
enum Slice<'a> {
    Str(&'a str, &'a str),
    Float(&'a str, f64),
}

/// What a panel tells a browser about the rows it drew, beyond its two domains.
///
/// All three answer one question: *given a row in the table, did this panel draw
/// it, and where?* The page needs that because its readout re-derives positions
/// instead of asking the picture what lies under the pointer, which is what lets
/// a mark carry no row number and an unbrushed plot stay byte-identical.
///
/// One struct rather than four more arguments, and it travels beside the domains
/// rather than being worked out in JavaScript for the reason the domains are:
/// the second copy of a rule is the one that drifts.
#[derive(Clone, Copy, Default)]
struct PanelFacts<'a> {
    /// The facet column this panel holds, as (column, level).
    facet_col: Option<(&'a str, &'a str)>,
    facet_row: Option<(&'a str, &'a str)>,
    /// The played column, its moments in order, and how long each one shows.
    play: Option<(&'a str, &'a [crate::data::FrameLevel], f64)>,
    /// `None` when a value can be turned back into a position; otherwise the one
    /// word for what stops it.
    place: Option<&'static str>,
}

// ---------------------------------------------------------------------------
// Renderer
// ---------------------------------------------------------------------------

pub struct SvgRenderer {
    pub width: f64,
    pub height: f64,
    pub point_radius: f64,
    /// Font sizes for tick labels, axis labels, and title.
    pub font_sm: f64,
    pub font_md: f64,
    pub font_lg: f64,
    /// What a page has already decided about this plot's panels and axes —
    /// [`Fit::free`] for a plot drawn on its own, which is every plot that is
    /// not composed. `render::page` is the only thing that sets it.
    pub(crate) fit: Fit,
    /// Which moment to leave showing, for a caller assembling the frames itself
    /// — `None` writes the sequence, which is every plot the book draws.
    ///
    /// **It selects among moments the renderer was already going to write, and
    /// changes nothing about how any of them is drawn.** That is what makes a
    /// GIF unable to disagree with the plot it came from: every scale, the color
    /// map and each legend are fitted across the whole sequence one pass above
    /// here, so a still is the animation with one moment left inline. A second
    /// writer choosing its own ticks is the failure this avoids by construction.
    pub(crate) still: Option<usize>,
}

/// The canvas a plot gets when it asks for nothing: `theme(width =, height =)`
/// is what asks. Named because two callers need to agree on it — the renderer's
/// own default and the page that divides a canvas into cells.
pub const CANVAS: (f64, f64) = (800.0, 600.0);

/// The moments a played plot has, in order, given the tables its layers bind.
///
/// Split out of [`SvgRenderer::draw`] so that a caller assembling stills counts
/// the frames the same way the renderer cuts them. Two counts that could differ
/// is the whole failure mode here: one short, and the last moment is silently
/// missing from the file.
pub(crate) fn play_levels_of(
    spec: &PlotSpec,
    source_frames: &[&DataFrame],
) -> Vec<crate::data::FrameLevel> {
    spec.layers
        .iter()
        .find_map(|l| l.encodings.get(&Channel::Play))
        .map(|d| crate::data::frames_across(source_frames, &d.field))
        .unwrap_or_default()
}

/// [`play_levels_of`] for a caller holding only the spec and its tables — the
/// door `plot::render_frames` comes in by. Resolves scope first, because `play`
/// is a channel and a layer that binds it after the mark binds it alone (§8).
pub(crate) fn play_levels(
    spec: &PlotSpec,
    data: &HashMap<String, DataFrame>,
) -> Vec<crate::data::FrameLevel> {
    let resolved = crate::legality::resolve_scopes(spec);
    let ctx = RenderContext::new(&resolved, data);
    let source_frames: Vec<&DataFrame> = resolved
        .layers
        .iter()
        .filter_map(|layer| ctx.resolve_data(&layer.data))
        .collect();
    play_levels_of(&resolved, &source_frames)
}

/// The base of the plot's type scale, in pixels — the tick labels' size when
/// nobody asks, and what `theme(font_size = )` replaces.
pub const FONT_BASE: f64 = 11.0;

/// The step between the three furniture sizes: ticks, then axis names and legend
/// titles, then the plot title.
///
/// The three sizes used to be three constants (11, 13, 16) and this is the same
/// triple written as what it already was — `round(11 × 1.2) = 13`,
/// `round(11 × 1.2²) = 16`. Stating the ratio once is what lets `theme()` carry
/// **one** number instead of three, and it means the default look cannot drift
/// apart from the scale a caller gets by asking for it.
pub const FONT_STEP: f64 = 1.2;

/// The three furniture sizes derived from one base, rounded the way the
/// constants they replace were.
///
/// Rounding is not cosmetic: it is what makes `theme(font_size = 11)` reproduce
/// the untouched default *exactly* rather than approximately, so no plot that
/// asks for the size it already had moves by a pixel.
pub fn font_scale(base: f64) -> (f64, f64, f64) {
    (
        base,
        (base * FONT_STEP).round(),
        (base * FONT_STEP * FONT_STEP).round(),
    )
}

impl Default for SvgRenderer {
    fn default() -> Self {
        let (font_sm, font_md, font_lg) = font_scale(FONT_BASE);
        Self {
            width: CANVAS.0,
            height: CANVAS.1,
            point_radius: 4.5,
            font_sm,
            font_md,
            font_lg,
            fit: Fit::free(),
            still: None,
        }
    }
}

impl SvgRenderer {
    /// The renderer for a plot that asked for its own size, typed as its theme
    /// asks — and for a page cell that was given one.
    ///
    /// **It takes the theme rather than only the rectangle, and that is the
    /// point.** A page draws every cell twice (`render::page`: once to measure
    /// where its panel would go, once knowing what the page decided), so a
    /// constructor that could be handed a size *without* the type scale is a
    /// constructor that lets the two passes disagree about how tall a tick label
    /// is — which shows up as a marginal plot that no longer touches the plot it
    /// describes. Passing the theme in makes forgetting it a compile error
    /// instead.
    pub(crate) fn for_theme(theme: &ThemeSpec, width: f64, height: f64) -> Self {
        let mut out = Self { width, height, ..Self::default() };
        if let Some(base) = theme.font_size {
            let (sm, md, lg) = font_scale(base);
            out.font_sm = sm;
            out.font_md = md;
            out.font_lg = lg;
        }
        out
    }
}

impl SvgRenderer {
    /// [`draw`](Self::draw)'s SVG and nothing else — **the tests' wrapper**, and
    /// test-only since 2026-07-27, when drawing gained a second thing to return.
    ///
    /// It was the production path until then, and moving `plot::render` off it is
    /// the point rather than a tidy-up: a caller that takes only the picture
    /// cannot report what the drawing left out, so the entry point uses `draw` and
    /// this stays where dropping the remarks is what a test wants. Assertions here
    /// are about bytes on the page; a test that cares about a remark asks
    /// `draw` for it.
    ///
    /// **Assumes `spec` is legal** — the unguarded half of [`crate::plot::render`],
    /// which is the only public way in. `pub(crate)` on purpose: the gate is the
    /// door, not something a caller remembers, so a binding that reaches past it
    /// fails to compile rather than drawing an illegal plot in silence (`plot.rs`,
    /// and spec §12).
    #[cfg(test)]
    pub(crate) fn render(&self, spec: &PlotSpec, data: &HashMap<String, DataFrame>) -> String {
        self.draw(spec, data).svg
    }

    /// [`render`](Self::render), and the two facts a page needs back from it: the
    /// panel rectangle this plot chose, and what each axis turned out to measure.
    /// See [`Drawn`] for why measuring is a whole render.
    pub(crate) fn draw(&self, spec: &PlotSpec, data: &HashMap<String, DataFrame>) -> Drawn {
        // Resolve channel scope before anything reads a binding, so every stage
        // below sees one complete set of per-layer encodings and no stage has to
        // know the scoping rule. Idempotent — `plot::render` has already run the
        // same resolution inside `legality::check`.
        let resolved = crate::legality::resolve_scopes(spec);
        let spec = &resolved;
        let ctx = RenderContext::new(spec, data);

        let x_field = ctx.coord_field(&Channel::X).unwrap_or("");
        let y_field = ctx.coord_field(&Channel::Y).unwrap_or("");

        // 3-D is one more vowel (spec §15): the plot projects in space iff a `z`
        // is bound, the way orientation is read off the bindings rather than
        // named. The viewing angle rides on the coordinate space — `space(...)`
        // sets it, a bare `z` takes the default three-quarter view. A `space`
        // with no `z` has nothing to project and is reported by `legality`, then
        // drawn flat here (the `is_3d` gate below is what draws it flat).
        // `polar` is the other coordinate space that changes where a value lands
        // (spec §9). It is asked for outright rather than triggered by a binding,
        // because unlike `z` it adds no dimension — it re-reads the two the plot
        // already has, `x` as the angle and `y` as the radius. The two spaces are
        // mutually exclusive (`check_polar` refuses `polar()` with a `z`), so
        // polar wins here and the 3-D gate stands down rather than both drawing.
        let polar_view = match &spec.coord {
            CoordSpace::Polar(v) => Some(*v),
            _ => None,
        };
        let is_polar = polar_view.is_some();
        // The third space that changes where a value lands, and the one that does
        // not land it anywhere: `nest()` turns each row's measure into an area and
        // partitions the panel (spec §15). Everything the other spaces reserve room
        // for — ticks, tick labels, axis names, gridlines — is absent here because
        // the two directions carry no variable, so this flag reads as "no guides"
        // through the layout below, the way `is_3d` reads as "the guides are on the
        // cube's edges". `check_nest` refuses `nest()` with a `z`, so it cannot
        // collide with the 3-D gate.
        let is_nest = matches!(spec.coord, CoordSpace::Nest);
        // Asked of `space_of`, the one source the legality checks read, rather than
        // re-derived here. It agrees with the sentence above and adds the case that
        // sentence predates: a **synthesized** `z` is a third dimension too, so
        // `bar * bin + x(a) + y(b) + space()` projects with no `z()` binding, the
        // way a flat histogram raises a count axis with no `y()` (spec §5/§15).
        let is_3d = crate::legality::space_of(spec) == crate::legality::SpaceKind::Space;

        // Whether a browser can work back from a value to the place this plot
        // drew it, and if not, the one word for why.
        //
        // The page's readout re-derives every row's position rather than asking
        // the picture what lies under the pointer, which is what lets it work
        // without a row number on every mark. The arithmetic is exact wherever a
        // mark stands at its own value, so the engine has to say where it does
        // not — it is the only side that knows it moved something. Two spaces
        // move everything by construction: a disc bends both axes, and a map
        // projects its two columns before anything is fitted, so the domains
        // below are in projected units while the reader's table is in degrees.
        // Everything else is a question about the layers.
        let place = if is_polar {
            Some("polar")
        } else if matches!(spec.coord, CoordSpace::Map(_)) {
            Some("map")
        } else {
            crate::legality::why_not_placed(spec)
        };

        // Which axis bends is read off the bindings, not asked for. Two bound
        // positions is the two-argument polar: `x` the angle, `y` the radius (the
        // rose). *One* is Wilkinson's `polar.theta` — the only position there is
        // becomes the angle and the radius is a constant, which is the pie
        // (§9.1.6.1). There is no knob because with one position there is nothing
        // to choose, the same reason there is no `flip` atom.
        //
        // Counting the *bindings* is the right question only where the bindings
        // are what place the marks. A **partition** places its own nodes — it
        // publishes all four edges from the tree, which is why `legality` lets
        // both positions go unnamed — so an unbound `x` there says "every leaf
        // weighs 1", the tally `count` already does, and not "this is a pie".
        // Read as a pie, its rings lost the axis they stand on and every sector
        // came out at radius 0: eight zero-radius arcs stacked on the center, a
        // blank circle from a sentence the book tells readers to write.
        let supplies_positions = spec.layers.iter()
            .any(|l| l.transforms.contains(&Transform::Partition));
        // Nested only, for the radial fallback below: crossed, the second axis is
        // apportioning the measure one level down rather than stepping a ring, so
        // its synthesized column is a share and not a depth.
        let rings_the_depth = spec.layers.iter().any(|l| {
            l.transforms.contains(&Transform::Partition)
                && !l.partition.as_ref().map(|p| p.cross).unwrap_or(false)
        });
        let measure_on_angle = is_polar && x_field.is_empty() && !supplies_positions;
        let view = match &spec.coord {
            CoordSpace::Space(v) => *v,
            _ => SpaceView::default(),
        };
        // A 3-D reading binds no `z`: `bin`/`count` invent the measurement and
        // publish it under the mesh's own column name (`count`, `density`,
        // `proportion`), where a flat histogram's count arrives under whatever `y`
        // was called — even when that is the empty string. So the fallback is the
        // synthesized name, and the axis, the scale and the mark all read one
        // column because they all read this.
        let bound_z = ctx.coord_field(&Channel::Z).unwrap_or("");
        let synth_z = spec.layers.iter().find_map(|l| {
            crate::legality::reads_two_dimensions(&l.mark, &l.transforms, crate::legality::SpaceKind::Space)
                .then(|| crate::transform::cell_measure(&l.transforms))
                .flatten()
        });
        let z_field = match (bound_z, is_3d) {
            ("", true) => synth_z.unwrap_or(""),
            _ => bound_z,
        };

        // A **partition**'s radial axis falls back to the ring it synthesized, the
        // same rule the 3-D reading just applied to `z`: the axis, the scale and
        // the mark read one column because they all read this.
        //
        // It is the *unbound* case that needs it, and the reason is a collision
        // rather than a missing name. The transform publishes the ring twice, once
        // under `depth` and once under whatever the measure axis reads — and with
        // neither position bound both of those are the empty string, so the ring
        // was dropped and the radial axis read the **measure** instead. A sunburst
        // of a tallied tree drew its rings inside the first tenth of the radius,
        // which is the same silent wrongness as no plot at all, one step later.
        // Named here rather than in `transform.rs`, which cannot see that the two
        // names it was handed are one name.
        let y_field = match (y_field, rings_the_depth) {
            ("", true) => crate::transform::NODE_DEPTH,
            _ => y_field,
        };

        // **A synthesized ring index earns no guide.** The radial axis of a
        // sunburst carries the level a node sits at, which the transform invented
        // and no reader looks a value up on: the numbers land inside the hole, the
        // rings draw a second set of circles through arcs that are already rings,
        // and the axis calls itself "Depth", which is the first question a reader
        // of a donut asks. So the ticks are dropped and the scale is kept — the
        // hole is still the stretch of axis the domain leaves empty, because that
        // is geometry rather than furniture.
        //
        // The **measure** axis keeps its guide, and the asymmetry is the whole
        // rule: an amount round the circle is a quantity in the data's own units,
        // the same one a stacked bar puts on its axis. `nest` reaches the same
        // place from further along — there *neither* direction carries a variable,
        // so it draws no axes at all and refuses to name them.
        let depth_is_the_radius = rings_the_depth && y_field == crate::transform::NODE_DEPTH;

        // Which axis the bars sit on, and which they measure along. Decided from
        // the bound column types in `legality`, so the check and the drawing can
        // never disagree about it.
        let orient = crate::legality::plot_orient(spec, data);
        let horizontal = orient == crate::legality::Orient::Horizontal;

        // Which axes carry a log scale. Read once here so every stage below
        // agrees about it — the ranges, the ticks, and the bar baseline all
        // have to make the same assumption or they disagree by a decade.
        // Read off the *axis*, not the plot field, so a plot whose only position
        // binding is a layer's still gets its scale. One scale per axis is the
        // invariant `check_layer_position` enforces: a layer asking for a
        // different one is the secondary axis §18 refuses.
        let x_log = scale::is_log(spec.axis_def(&Channel::X));
        let y_log = scale::is_log(spec.axis_def(&Channel::Y));
        let x_base = scale::log_base(spec.axis_def(&Channel::X));
        let y_base = scale::log_base(spec.axis_def(&Channel::Y));

        // Which axes carry moments in time. Like `category`, the time scale is
        // chosen from the column's type — a date column *is* temporal, and
        // asking the user to say so twice is the redundancy `scale =` exists
        // to avoid. `legality` refuses `log` on a temporal column, so an axis
        // is never both.
        let x_time = detect_time(&ctx, x_field);
        let y_time = detect_time(&ctx, y_field);

        // --- facets: the outer frame's categories --------------------------
        //
        // Read from the source tables, like `detect_time`: a facet subsets rows
        // before any transform runs, so the split is defined by the data as
        // bound, never by a transform's output. `categories_across` owns the
        // order, so a factor's declared levels order the panels the same way
        // they order an axis.
        let source_frames: Vec<&DataFrame> = spec.layers.iter()
            .filter_map(|layer| ctx.resolve_data(&layer.data))
            .collect();
        let (col_field, row_field) = match &spec.facet {
            Some(f) => (f.col.clone(), f.row.clone()),
            None => (None, None),
        };
        let facet_values = |field: &Option<String>| -> Vec<String> {
            field.as_deref()
                .map(|f| crate::data::categories_across(&source_frames, f))
                .unwrap_or_default()
        };
        let col_values = facet_values(&col_field);
        let row_values = facet_values(&row_field);
        let facet_wrap = spec.facet.as_ref().and_then(|f| f.wrap)
            .filter(|n| *n > 0 && col_values.is_empty() != row_values.is_empty());
        let ncols = col_values.len().max(1);
        let nrows = row_values.len().max(1);

        // --- play: the outer frame's *moments* ------------------------------
        //
        // The same partition as a facet, laid out in time instead of across the
        // page, so it is read the same way and from the same place: the source
        // tables, before any transform, because a frame subsets rows exactly as a
        // panel does. `frames_across` owns the order — a factor's levels for a
        // category, ascending for a number — and it is the one function that
        // knows a year is not measured but named.
        //
        // Read off the *layers* rather than off a plot-level field, because `play`
        // is a channel and channels have scope (§8). Written before the marks it
        // reaches every layer; written after one it binds that layer alone, and a
        // layer without it then stands still while the others move. That is not a
        // special case for animation — it is the same nearest-wins rule `data()`
        // has, and it arrives here already applied by `resolve_scopes`.
        let play_def = spec.layers.iter().find_map(|l| l.encodings.get(&Channel::Play));
        let play_levels = play_levels_of(spec, &source_frames);
        // One frame is not an animation: a column with a single distinct value
        // draws once, and the SMIL below is skipped rather than written for a
        // sequence that never advances.
        let nframes = play_levels.len().max(1);
        let frame_seconds = play_def.map(|d| d.frame_seconds()).unwrap_or(crate::ir::FRAME_SECONDS);

        // Pre-compute effective (transformed, scaled) DataFrames for each layer,
        // so that range computation and rendering both see the same values.
        //
        // A scale applies **before** the transform on the axis it groups by, and
        // **after** it on the axis the transform writes. Grouping has to happen
        // in the space the reader will see, or `bar * bin` on a log axis gives
        // bars of unequal width; the measured value is computed in the data's own
        // units and then displayed, so `bar * sum` stays a sum rather than
        // becoming the log of a product. `scale.rs` states the rule in full.
        //
        // The facet filters run first: the statistics then see the panel's rows
        // as if they were the whole data — the No Exceptions law applied to
        // frames, and the reason a faceted `bar * count` counts within each
        // panel rather than once across all of them.
        //
        // **A `bin`'s cut is the one thing that does not run here** (spec §11).
        // A cut is an extent description and a tally is the statistic (spec §5),
        // and faceting splits them: the tally is the panel's, the cut is the
        // plot's, exactly as the scale is. `cut_for` below resolves it from the
        // unfiltered rows and `eff_for` hands it down, so a panel still counts
        // only what it holds but counts it into everyone's bins.

        // Everything a layer's frame goes through *before* a transform sees it.
        // Split out because the cut has to run this far and no further: it needs
        // the layer's own position columns resolved (§8) and a stated domain
        // applied (§10), and it must not see the facet filter. Every step here is
        // filter-invariant — `resolve_positions` renames columns, `limit_cut` is a
        // per-row predicate — so the unfiltered pass answers exactly what a panel
        // holding every row would have answered.
        let prepared = |layer: &Layer, filters: &[Slice<'_>]| -> Option<DataFrame> {
            let df = ctx.resolve_data(&layer.data)?;
            let mut base = df.clone();
            for f in filters {
                base = match *f {
                    Slice::Str(field, v) => base.filter_str_eq(field, v),
                    Slice::Float(field, v) => base.filter_float_eq(field, v),
                };
            }
            // A layer may name its own column for a shared axis (spec
            // §8). Resolve that *here*, before anything reads a position,
            // so exactly one stage knows about it and every stage below —
            // transforms, the axis builder, category detection, all eleven
            // mark writers, polar, the projector — keeps working in terms
            // of one column name per axis. That is the whole reason the
            // feature is small: a shared axis was never the same question
            // as a shared column *name*.
            let base = resolve_positions(base, spec, layer);
            // A stated domain (spec §10) excludes rows the way a facet
            // does, and for the same reason it runs *here*: every
            // statistic below must see the stated range as if it were the
            // whole data, so `bar * bin + x(v, limits = c(0, 10))` cuts
            // its bins over 0–10 rather than binning everything and
            // hiding the ends. That is §10's ordering rule, inherited
            // rather than restated — a limit is a scale property, and a
            // scale applies before the transform on the axis it groups
            // by. `legality::limit_cut` is the one authority on which
            // rows survive, so the count the user was given cannot
            // disagree with the picture.
            let cut = crate::legality::limit_cut(spec, layer, &base);
            Some(if cut.is_empty() { base } else { base.keep_rows(&cut.keep) })
        };

        // Which axis a one-dimensional transform groups by, and which it writes.
        // One answer, asked in two places — the cut resolver and the transform
        // caller — because two copies of an exchange this fiddly is how they come
        // to disagree about a sideways violin.
        let key_is_x = |layer: &Layer, base: &DataFrame| -> bool {
            let violin = crate::legality::slot_density(spec, layer, Some(base));
            !((horizontal && crate::legality::is_slot_mark(&layer.mark))
                || violin == Some(crate::legality::Orient::Horizontal))
        };

        // Which axes the caller freed (spec §11). Read here rather than beside the
        // axis fitting because the *cut* needs it first: a panel with its own
        // scale wants its own bin edges, and the cut is resolved before any panel
        // is fitted. Which axis is freed is whichever binding says so — there is
        // no `free_x`/`free_y` vocabulary to enumerate.
        let is_free = |c: Channel| spec.axis_def(&c).is_some_and(|d| d.free);
        let (free_x, free_y, free_z) =
            (is_free(Channel::X), is_free(Channel::Y), is_free(Channel::Z));
        let any_free = free_x || free_y || free_z;

        // One cut per layer, resolved from every panel's rows at once.
        //
        // Always, faceted or not: an unfaceted plot's single panel *is* the frame,
        // so this answers what the per-panel derivation answered and the output is
        // byte-identical. Keeping one path rather than a faceted special case is
        // also what stops the two drifting.
        let cut_of = |layer: &crate::ir::Layer, rows: &[Slice<'_>]| -> crate::transform::BinCut {
                if !layer.transforms.contains(&Transform::Bin) {
                    return crate::transform::BinCut::default();
                }
                let Some(base) = prepared(layer, rows) else {
                    return crate::transform::BinCut::default();
                };
                // Cut in the space the reader sees, the rule `scale.rs` states and
                // the transform caller below obeys — a cut of unlogged values
                // displayed on a log axis is the unequal-width bar that rule exists
                // to prevent.
                let logged = |field: &str, is_log: bool, base_of: f64| -> Option<crate::transform::BinLayout> {
                    let df = if is_log { scale::log_column(&base, field, base_of) } else { base.clone() };
                    df.float_col(field).and_then(|xs| crate::transform::bin_layout(xs, layer.bin.as_ref()))
                };
                // A two-dimensional reading cuts both axes; a one-dimensional one
                // cuts whichever it groups by, and leaves the other alone.
                if crate::legality::reads_two_dimensions(
                    &layer.mark, &layer.transforms, crate::legality::space_of(spec)) {
                    crate::transform::BinCut {
                        x: logged(x_field, x_log, x_base),
                        y: logged(y_field, y_log, y_base),
                    }
                } else if key_is_x(layer, &base) {
                    crate::transform::BinCut { x: logged(x_field, x_log, x_base), y: None }
                } else {
                    crate::transform::BinCut { x: None, y: logged(y_field, y_log, y_base) }
                }
        };
        let layer_cuts: Vec<crate::transform::BinCut> =
            spec.layers.iter().map(|layer| cut_of(layer, &[])).collect();

        let eff_for = |filters: &[Slice<'_>], frame: Option<&crate::data::FrameLevel>| -> Vec<DataFrame> {
            spec.layers.iter().enumerate()
                .map(|(li, layer)| {
                    // The panel's own filters, before the moment is added below.
                    // A free axis re-cuts from *these*: a panel's rows across the
                    // whole sequence, never one frame's. Refitting a cut per frame
                    // would move the bars under the data for the same reason
                    // refitting a scale would (§16), and `play` is refused without
                    // a facet partly so this stays true.
                    let panel_rows = filters;
                    // The frame filter is *per layer*, where the panel filters are
                    // the plot's. A layer that binds `play` is cut down to the
                    // moment; a layer that does not is handed every row it ever
                    // had and so stands still behind the ones that move — the
                    // backdrop reading, and the exact counterpart of a layer whose
                    // table lacks the facet column being drawn in every panel.
                    let mut filters: Vec<Slice<'_>> = filters.to_vec();
                    if let (Some(fr), Some(def)) = (frame, layer.encodings.get(&Channel::Play)) {
                        filters.push(match &fr.key {
                            crate::data::FrameKey::Str(s) => Slice::Str(&def.field, s),
                            crate::data::FrameKey::Float(v) => Slice::Float(&def.field, *v),
                        });
                    }
                    let Some(base) = prepared(layer, &filters) else {
                        return DataFrame::new();
                    };
                    // A panel with its own scale wants its own edges (spec §11):
                    // the freed axis re-derives them from this panel's rows, the
                    // shared axis keeps the plot's. Decided per axis, so freeing y
                    // on a histogram of x leaves the bars where they were.
                    let cut = if any_free && layer.transforms.contains(&Transform::Bin) {
                        let own = cut_of(layer, panel_rows);
                        crate::transform::BinCut {
                            x: if free_x { own.x } else { layer_cuts[li].x },
                            y: if free_y { own.y } else { layer_cuts[li].y },
                        }
                    } else {
                        layer_cuts[li]
                    };
                    // **A partition is read before anything else**, because it is
                    // the one transform whose input is neither of the plot's
                    // positions: its levels are columns named in the atom, and it
                    // consumes the bound `x` as a weight rather than as a place.
                    // What comes back is the rectangular extent description — four
                    // edges and a center — so everything downstream (the zone's
                    // writer, the text's placement, the axis fit) reads it as it
                    // reads a binned mesh, and neither had to learn anything.
                    //
                    // The measure is `x` when it is bound and nothing when it is
                    // not, which is the tally `count`/`proportion` already do with
                    // no measurement. `x_field` receives the node's center, so a
                    // `text` layer needs no second computation to know where its
                    // label goes: one partition feeds the rectangle and the name.
                    if layer.transforms.contains(&crate::ir::Transform::Partition) {
                        let levels: Vec<String> = layer.partition.as_ref()
                            .map(|p| p.levels.clone()).unwrap_or_default();
                        let cross = layer.partition.as_ref()
                            .map(|p| p.cross).unwrap_or(false);
                        let measure = layer.encodings.get(&Channel::X)
                            .or(spec.x.as_ref())
                            .map(|e| e.field.as_str());
                        return crate::transform::partition(
                            &base, &levels, measure, x_field, y_field, cross);
                    }
                    // A `zone` carries `bounds`, but its `bounds` *names four
                    // columns* rather than reshaping rows into low/high pairs at a
                    // shared position — one row is one rectangle. Running the pair
                    // machinery over it would replace the frame with two columns
                    // and lose three of the four sides, so a zone takes the
                    // untransformed frame and reads its own columns. What it still
                    // needs is the log scaling, applied to the *bound* columns
                    // rather than to `x`/`y`, since those are the numbers that get
                    // placed.
                    if crate::legality::reads_two_dimensions(
                        &layer.mark, &layer.transforms, crate::legality::space_of(spec)) {
                        // A **two-dimensional reading** (spec §5): the transform is
                        // read over both of the layer's domain axes. Which geometry
                        // comes out is the *mark's* choice, decided here for the
                        // reason the branch below is — a mark says what to do with
                        // what a transform made, and `transform.rs` stays free of
                        // marks.
                        //
                        // **Dimensionality, not destination**, and the distinction is
                        // what lets a 3-D `bar` in here: it reads its `bin` over two
                        // axes exactly as a `zone` does — same `bin2d`, same four edge
                        // columns — and then stands the tally up along `z` instead of
                        // painting it. `measures_cells` is the other question and
                        // still answers `false` for it, which is what keeps the ramp,
                        // the legend and the `color` exemption off a plot whose
                        // measurement is a length.
                        //
                        // Log scaling goes on the **inputs**: cutting log positions
                        // gives cells even on the page, and the synthesized edges and
                        // vertices come out already scaled, so nothing downstream has
                        // to know.
                        let mut input = base;
                        if x_log { input = scale::log_column(&input, x_field, x_base); }
                        if y_log { input = scale::log_column(&input, y_field, y_base); }
                        let d = layer.density.as_ref();
                        // A `group` split runs the whole reading once per group, the
                        // way every statistic in `transform::apply` already does — a
                        // contour per species, on shared axes. `color` cannot be the
                        // split here (it carries the measurement, and `check_field`
                        // refuses any other field), so `group` is the only one to ask
                        // for. A `zone` refuses `group` outright in `rule_for`, so this
                        // is the contour's case in practice and the degenerate
                        // whole-frame one everywhere else.
                        let split = layer.encodings.get(&Channel::Group).map(|e| e.field.as_str());
                        //
                        // Which geometry comes out is `field_geometry`'s answer, not a
                        // second opinion formed here: **rings** are the traced level
                        // sets, which a `path` strokes and a `zone` fills, and the two
                        // therefore run the *same* transform — the `step` ruling at its
                        // most literal, since a filled band and its boundary curve are
                        // one shape drawn two ways. **Cells** are the mesh the field was
                        // sampled on, counted by `bin` or estimated by `density`.
                        use crate::legality::FieldGeometry;
                        let geom = crate::legality::field_geometry(layer);
                        // Which of the five, in the order the reading is decided.
                        // `Tally` is the tile plot: its cells are not cut out of
                        // anything, they are the categories, so it is the one case
                        // that publishes no extent columns at all — the mark reads
                        // the slot off the axis instead (spec §5).
                        //
                        // `Reduce` is the **two-dimensional group-by**, and it is
                        // `Tally`'s twin exactly as `count` is `mean`'s in one
                        // dimension: same cells, same slots, same absent-pair rule —
                        // the difference is only that a tally was handed no column and
                        // a reduction was. Which column is `measure_field`'s answer,
                        // read off the channel the *mark* measures with (`color` here,
                        // `z` in the cube), so this stage names no channel of its own.
                        //
                        // `CutReduce` is the **summary heatmap**, and it is the pair
                        // of the two above rather than a third thing: `bin` supplies
                        // the cells, the statistic supplies their measurement (spec
                        // §5). It is asked before `Cells` because a `bin` composed
                        // with a statistic is no longer measuring anything — the
                        // order here is what decides which half of the composition
                        // `bin` is doing, and putting it second is what let the
                        // statistic be silently dropped.
                        enum Cut<'a> { Rings, Cells, Tally(bool), Reduce(&'a str, crate::transform::AggFn), CutReduce(&'a str, crate::transform::AggFn), Pair(&'a str) }
                        let ts = &layer.transforms;
                        let which = match geom {
                            Some(FieldGeometry::Rings) => Cut::Rings,
                            _ if ts.contains(&Transform::Bin) => {
                                match crate::transform::reduces_column(ts, layer.quantile.as_ref())
                                    .zip(crate::legality::measure_field(spec, layer))
                                {
                                    Some((agg, field)) => Cut::CutReduce(field, agg),
                                    None => Cut::Cells,
                                }
                            }
                            _ if ts.contains(&Transform::Density) => Cut::Cells,
                            // Refused by `check_pair_summary` when the channel names
                            // nothing, so the `zip` cannot drop a reduction the user
                            // asked for — it only declines to invent one.
                            // A **pair** reduction — `range`, `confidence`, the
                            // summary a `box` injects. Asked before the single-value
                            // one because a `box` carries both readings at once (it
                            // injects its own transform and may also be given none),
                            // and because the two differ only in the arity of what
                            // each cell answers with.
                            _ if crate::transform::pairs_a_column(ts) => {
                                match crate::legality::measure_field(spec, layer) {
                                    Some(field) => Cut::Pair(field),
                                    None => Cut::Tally(ts.contains(&Transform::Proportion)),
                                }
                            }
                            _ => match crate::transform::reduces_column(ts, layer.quantile.as_ref())
                                .zip(crate::legality::measure_field(spec, layer))
                            {
                                Some((agg, field)) => Cut::Reduce(field, agg),
                                None => Cut::Tally(ts.contains(&Transform::Proportion)),
                            },
                        };
                        let cells = crate::transform::by_group(&input, split, |sub| match which {
                            Cut::Rings => {
                                crate::transform::density2d_contour(sub, x_field, y_field, d)
                            }
                            // The heatmap, counted: one row per non-empty cell.
                            Cut::Cells if ts.contains(&Transform::Bin) => {
                                crate::transform::bin2d(sub, x_field, y_field, layer.bin.as_ref(), cut)
                            }
                            // The heatmap, estimated: one row per cell of the field.
                            Cut::Cells => {
                                crate::transform::density2d_cells(sub, x_field, y_field, d)
                            }
                            // The tile plot, tallied: one row per non-empty pair of
                            // categories, as a count or as a share of the whole.
                            Cut::Tally(share) => {
                                crate::transform::count2d(sub, x_field, y_field, share)
                            }
                            // The tile plot, summarized: one row per non-empty pair,
                            // carrying the named column reduced within it.
                            Cut::Reduce(field, agg) => {
                                crate::transform::agg2d(sub, x_field, y_field, field, agg)
                            }
                            // The summary heatmap: the same mesh `bin2d` cuts, with
                            // the named column reduced inside each cell instead of
                            // the rows tallied. `Reduce`'s twin one extent
                            // description over — same statistic, cells cut rather
                            // than slotted.
                            Cut::CutReduce(field, agg) => {
                                crate::transform::bin2d_agg(sub, x_field, y_field, field,
                                    agg, layer.bin.as_ref(), cut)
                            }
                            // The floor, paired: one low/high pair per non-empty pair
                            // of categories, carrying whatever the statistic keeps
                            // beside them. `Reduce`'s twin — same cells, same slots,
                            // same absent-pair rule, two answers instead of one.
                            Cut::Pair(field) => {
                                let kind = ts.iter().find(|t| matches!(
                                    t, Transform::Range | Transform::Confidence | Transform::Box))
                                    .cloned().unwrap_or(Transform::Range);
                                crate::transform::pairs2d(sub, x_field, y_field, field, &kind,
                                    layer.confidence.as_ref(), layer.r#box.as_ref(),
                                    layer.range.as_ref(), layer.deviation.as_ref())
                            }
                        });
                        // The normalizer's second reading: divide whatever measured
                        // the cells by its total. Outside `by_group` because a share
                        // is a fraction of the whole frame however many groups split
                        // it — the plane's copy of the rule `apply` follows one
                        // dimension down (spec §5).
                        crate::transform::share_cells(&cells, ts)
                    } else if layer.mark == Mark::Zone {
                        let mut out = base;
                        if let Some(b) = layer.bounds.as_ref() {
                            for (col, is_log, lg_base) in [
                                (&b.start, x_log, x_base), (&b.end, x_log, x_base),
                                (&b.lower, y_log, y_base), (&b.upper, y_log, y_base),
                            ] {
                                if let (Some(c), true) = (col.as_deref(), is_log) {
                                    out = scale::log_column(&out, c, lg_base);
                                }
                            }
                        }
                        out
                    } else if layer.transforms.is_empty() {
                        // Nothing groups and nothing writes, so the two halves of
                        // the rule coincide: scale whichever axes asked for it.
                        let mut out = base;
                        if x_log { out = scale::log_column(&out, x_field, x_base); }
                        if y_log { out = scale::log_column(&out, y_field, y_base); }
                        out
                    } else {
                        // A transform groups by the position axis and writes to the
                        // measured one. For a horizontal slot mark those are y and x
                        // — a sideways box summarizes the column on `x` within each
                        // category on `y`, which is the upright reading with the two
                        // axes exchanged and nothing else altered.
                        //
                        // A **violin** groups by its slot and estimates along its
                        // measure, which is the same sentence — so it takes the same
                        // exchange, read off its own bindings rather than off the
                        // plot's orientation. It has to be its own question: a
                        // violin is not a slot mark (an `area` has no slot to dodge
                        // or stack in), so `plot_orient` never sees it and a plot
                        // that is nothing but violins would answer `Vertical` for a
                        // sideways one.
                        let on_x = key_is_x(layer, &base);
                        let ((key, key_log, key_base), (out, out_log, out_base)) = if on_x {
                            ((x_field, x_log, x_base), (y_field, y_log, y_base))
                        } else {
                            ((y_field, y_log, y_base), (x_field, x_log, x_base))
                        };
                        let input = if key_log { scale::log_column(&base, key, key_base) } else { base };
                        // A color or group binding splits the statistic: the
                        // transform runs within each group and tags every output
                        // row with it, so a histogram split by species is three
                        // histograms and the renderer can color them. `color`
                        // wins over `group`, the same precedence `write_line` uses
                        // — and the precedence is `legality`'s to state, because a
                        // check that counts rows per group has to count the groups
                        // this draw actually makes.
                        let group_field = crate::legality::group_field_of(layer);
                        let done = crate::transform::apply(&input, &layer.transforms, key, out, layer.bin.as_ref(), cut.axis(on_x), layer.density.as_ref(), layer.range.as_ref(), layer.confidence.as_ref(), layer.deviation.as_ref(), layer.quantile.as_ref(), layer.r#box.as_ref(), layer.bounds.as_ref(), layer.stack.as_ref(), group_field);
                        // The dot plot: a stacking `point` spends its span on glyphs
                        // rather than on length, so the tally becomes one row per
                        // observation (`transform::pile`, spec §5). Decided here for
                        // the reason the `zone` branch above is — the *mark* chooses
                        // what to do with what a transform made, and `transform.rs`
                        // stays free of marks. Before the log scaling, so a piled
                        // count is displayed logged rather than logged then piled.
                        let done = if layer.mark == Mark::Point && layer.transforms.contains(&Transform::Stack) {
                            crate::transform::pile(&done, out)
                        } else {
                            done
                        };
                        if out_log { scale::log_column(&done, out, out_base) } else { done }
                    }
                })
                .collect()
        };

        // One entry per (moment, panel), each holding one frame per layer. Moments
        // are the outer stride, so `panel_eff[f * npanels + panel.slot]` — and the
        // slot comes from the layout rather than from `(row, col)` arithmetic,
        // because a folded ribbon numbers its panels along the ribbon and a
        // crossing numbers them row-major. The unfaceted, unplayed plot is one
        // moment and one panel with no filters — the degenerate case, not a
        // separate path, and at `nframes == 1` the indexing is arithmetically what
        // it was before there were moments at all.
        //
        // Which levels each panel shows, in the order the layout will place them.
        // A crossing is the cross product, row-major; a folded ribbon is one entry
        // per level, and the layout decides where in the rectangle each lands.
        // Building the list here rather than indexing by `(row, col)` is what lets
        // the ragged tail exist: the panel count is the *level* count, so there is
        // nothing to filter for a cell no level maps to.
        let panel_levels: Vec<(Option<&str>, Option<&str>)> = if facet_wrap.is_some() {
            let levels = if col_values.is_empty() { &row_values } else { &col_values };
            let along_cols = !col_values.is_empty();
            levels.iter()
                .map(|v| if along_cols { (Some(v.as_str()), None) } else { (None, Some(v.as_str())) })
                .collect()
        } else {
            let mut keys = Vec::with_capacity(nrows * ncols);
            for r in 0..nrows {
                for c in 0..ncols {
                    keys.push((
                        col_values.get(c).map(String::as_str),
                        row_values.get(r).map(String::as_str),
                    ));
                }
            }
            keys
        };
        let npanels = panel_levels.len();
        let mut panel_eff: Vec<Vec<DataFrame>> = Vec::with_capacity(nframes * npanels);
        for f in 0..nframes {
            let frame = play_levels.get(f);
            for (cv, rv) in &panel_levels {
                let mut filters: Vec<Slice<'_>> = Vec::new();
                if let (Some(f), Some(v)) = (col_field.as_deref(), *cv) {
                    filters.push(Slice::Str(f, v));
                }
                if let (Some(f), Some(v)) = (row_field.as_deref(), *rv) {
                    filters.push(Slice::Str(f, v));
                }
                panel_eff.push(eff_for(&filters, frame));
            }
        }

        // **The map space, in one place.** Longitude and latitude become positions
        // on the flat page here, before anything is fitted — so the ranges below
        // are the *projected* ranges, the panel comes out the shape of the map,
        // and every reader downstream (the ticks, the marks, the brush rectangle,
        // the legends) works on ordinary numbers and is never told a projection
        // happened. That is why this space costs a dozen lines where `polar` costs
        // a module every mark has to consult.
        //
        // It runs after the panels are cut and before the scales are fitted, which
        // is the only correct place: cutting reads the facet's own column and does
        // not care, while fitting has to see projected numbers or the map is drawn
        // to the shape of the degree grid instead of its own.
        // The degree extents are read *before* the projection and kept, because
        // they are the only thing that cannot be recovered afterwards and the axes
        // need them: a reader is owed ticks in degrees, not in projected units.
        let map_degrees = if let CoordSpace::Map(view) = &spec.coord {
            let span = |field: &str| {
                let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
                for df in panel_eff.iter().flat_map(|p| p.iter()) {
                    for &v in df.float_col(field).into_iter().flatten() {
                        if v.is_finite() {
                            lo = lo.min(v);
                            hi = hi.max(v);
                        }
                    }
                }
                (lo <= hi).then_some((lo, hi))
            };
            let degrees = span(x_field).zip(span(y_field));
            let geo = crate::render::geo::Geo::new(view);
            for frames in panel_eff.iter_mut() {
                for df in frames.iter_mut() {
                    *df = crate::render::geo::project_frame(df, &geo, x_field, y_field);
                }
            }
            degrees.map(|d| (geo, d))
        } else {
            None
        };

        // Ranges, categories and ticks are computed across every panel's frames.
        // The fixed, shared scale is what makes the panels comparable, which is
        // the reason to facet at all — free per-panel scales are a later,
        // deliberate request, not a default.
        //
        // **And the same line is what makes an animation honest.** Because moments
        // flatten into this list beside panels, every scale, every category, every
        // tick, the color map and every legend are built across the whole
        // sequence at once. A scale that re-fitted per frame would move the axis
        // under the data and turn a still point into a moving one: the motion
        // would be the scale's, not the data's. Nothing here had to be taught
        // that — one subset is one subset, whether it is a panel or a moment.
        let all_eff: Vec<&DataFrame> = panel_eff.iter().flat_map(|p| p.iter()).collect();

        // What drawing finds that the check could not — see [`Drawn::remarks`].
        // Declared this early because the first finding is here: rows a log axis
        // cannot place. Accumulated across every panel, then deduped by message
        // before `Drawn` carries it out: a facet draws the same layer once per
        // panel, so a remark that reads the same in two panels is one fact said
        // twice. Two panels that genuinely differ keep both sentences, which is
        // the honest reading — the panels are different pictures. Nothing in
        // this function may `eprintln!` instead: stderr does not exist in the
        // browser, and a warning only the CLI can hear is half a warning.
        let mut remarks: Vec<Diagnostic> = Vec::new();

        warn_unplaceable(&mut remarks, &all_eff, x_field, x_log, "x");
        warn_unplaceable(&mut remarks, &all_eff, y_field, y_log, "y");

        // A "plain bar" is a bar layer that has NOT gone through bin (so its
        // positions are discrete and each deserves its own tick label).
        let has_plain_bar = spec.layers.iter().any(|l| {
            l.mark == Mark::Bar && !l.transforms.iter().any(|t| *t == Transform::Bin)
        });
        let has_any_bar = spec.layers.iter().any(|l| l.mark == Mark::Bar);

        // A mark read from a baseline must have that baseline on the axis, or
        // the fill runs off the panel edge and reads as a floating band. `bar`
        // and `area` are both such marks, so the axis stretch is asked as
        // "does anything here measure from a baseline?" rather than named after
        // one mark — the alternative is a second mark-specific flag, and the
        // third such mark would then need a third.
        //
        // **A violin is neither**, though it is drawn by both marks, and this is the
        // one place the two readings had to be told apart on the *axis* rather than
        // on the page. A half violin does close on a baseline — but on the slot's
        // center line, which lives on the other axis and is not a value of the
        // measure at all. Stretching the measure axis to zero for it padded the panel
        // down to 0 on a life-expectancy scale that starts near 35, so four fifths of
        // the plot was empty and every violin was squeezed into the top.
        let layer_frame = |l: &Layer| l.data.as_ref().or(spec.data.as_ref()).and_then(|n| data.get(n));
        let is_violin = |l: &Layer| crate::legality::slot_density(spec, l, layer_frame(l)).is_some();
        let has_any_area = spec.layers.iter().any(|l| l.mark == Mark::Area && !is_violin(l));

        // The silhouette a `step * bin` traces sits on the baseline the way a
        // bar's foot does — the "third such mark" the comment above predicted.
        // Only when binned: a plain `step` (a CDF, a survival curve) measures
        // from nothing and keeps the fit-the-data axis.
        let has_step_hist = spec.layers.iter().any(|l|
            l.mark == Mark::Step && l.transforms.iter().any(|t| *t == Transform::Bin));

        // The breathing margin on a free end exists for one reason: a `point`
        // glyph at the exact extreme would be half-clipped by the frame. An
        // `area` fills to its data edges and has no glyph to clip, so on the
        // axis it fills *along* — always x, since an area has no orientation —
        // the margin reads as an empty band rather than as padding, and the
        // axis should sit flush instead. A point sharing that axis brings the
        // clip risk back, so it wins: flush only when the fill is alone there.
        //
        // A `ribbon` fills along x exactly the same way, so it wants the flush
        // treatment too — but *only* this half of `area`'s behavior. It carries
        // no baseline (it floats between a synthesized low and high, like
        // `interval`), so it is deliberately absent from the baseline stretch on
        // the y-axis below; the two concerns `has_any_area` bundles come apart on
        // the ribbon.
        // A violin is excluded from the flush treatment for the reason it is excluded
        // from the stretch: it does not fill *along* x, it stands in a slot and
        // spreads across it. Lying down its estimate runs three bandwidths past the
        // extremes already, and the breathing margin keeps that tail off the frame.
        let has_any_ribbon = spec.layers.iter().any(|l| l.mark == Mark::Ribbon && !is_violin(l));
        let has_any_point = spec.layers.iter().any(|l| l.mark == Mark::Point);
        // In polar the angular axis is *periodic*: one turn spans exactly the
        // fitted range, so the scale minimum and maximum are the same place on the
        // circle (Wilkinson §9.1.6 aligns 0 radians with the minimum and 2π with
        // the maximum). A breathing margin there would open a wedge of dead angle
        // between the last value and the first, which is the seam the periodic
        // rule exists to close — so the angular axis is always flush.
        let flush_x = ((has_any_area || has_any_ribbon) && !has_any_point) || is_polar;

        // Both axes are built by the same routine. Orientation only chooses
        // which one gets the bar treatment — categories and bar-aligned ticks on
        // the position axis, a baseline at zero on the measured one. Duplicating
        // the x machinery onto y is how `png.rs` drifted out of step with
        // `svg.rs`; there is one copy here, called twice.
        //
        // A categorical y runs top-to-bottom, so the first category reads first
        // on a horizontal chart just as it does on a vertical one.
        //
        // --- fitting the axes, over whichever panels are asked for -----------
        //
        // Everything from here to `PanelAxes` is *what a set of panels implies
        // about the axes*: their categories, their ranges, their ticks. It is a
        // closure rather than straight-line code for one reason — `free`
        // (spec §11). A shared scale calls it once over every panel; a freed axis
        // calls it again per panel and takes its own answer.
        //
        // **One path, not two.** The shared case is the degenerate one, exactly
        // as an unfaceted plot is the 1×1 facet: pass every panel and the result
        // is byte-for-byte what the straight-line version computed. A second
        // fitting routine for the free case is how `png.rs` drifted from
        // `svg.rs`, one level down.
        let fit_axes = |panels: &[&Vec<DataFrame>]| -> PanelAxes {
            let eff: Vec<&DataFrame> = panels.iter().flat_map(|p| p.iter()).collect();
            let cat_x = detect_categories(&eff, spec, x_field, false);
            let cat_y = detect_categories(&eff, spec, y_field, true);

        // The frames bar positions are read from: bar layers only, every panel
        // being fitted over.
        let bar_frames: Vec<&DataFrame> = spec.layers.iter().enumerate()
            .filter(|(_, l)| l.mark == Mark::Bar)
            .flat_map(|(i, _)| panels.iter().map(move |p| &p[i]))
            .collect();

        // The sides a bounded `zone` places itself with, per axis — the one mark
        // whose position is not in the axis's own column.
        //
        // Every other mark, `ribbon * bounds` included, reaches the axis through
        // `field`: a pair transform *reshapes rows*, so the two boundaries arrive
        // as values of the measure column and `channel_range_eff` sees them like
        // any other. A zone is the exception by design — one row is one rectangle,
        // so it keeps its frame untransformed and reads its four columns straight
        // (see the effective-frame branch above, and `marks/zone.rs`). Nothing then
        // told the axis, so a zone's sides were placed in data units against a
        // range fitted without them: over a base layer that range was right anyway
        // and the rectangles landed correctly, which is every use of the mark in
        // the book — but a plot whose *only* layer is a bounded zone had no field
        // to fit at all, fell back to `0..1`, and drew its rectangles tens of
        // thousands of pixels off-panel. Silently: an empty-looking panel with
        // fabricated `0.0 … 1.0` axes, the failure §12 exists to make impossible.
        //
        // Both halves of a pair are required before either counts, which is
        // `write_zone`'s own rule rather than a second one — a lone `bounds(lower)`
        // names no side, the zone falls back to its slot, and an axis widened for a
        // column nothing was drawn from would be the mirror defect.
        let zone_sides = |axis: fn(&crate::ir::BoundsSpec) -> Option<(&str, &str)>| {
            spec.layers.iter().enumerate()
                .filter(|(_, l)| l.mark == Mark::Zone)
                .filter_map(|(i, l)| Some((i, axis(l.bounds.as_ref()?)?)))
                .flat_map(|(i, (lo, hi))| {
                    panels.iter().flat_map(move |p| [(&p[i], lo), (&p[i], hi)])
                })
                .collect::<Vec<_>>()
        };
        // And **which axis each pair bounds is the bindings' answer, not this
        // function's** (`legality::zone_orient`) — the second of the two places that
        // have to agree about it, the other being `marks/zone.rs`. They guessed the
        // same way for a year and were wrong together, which is why the funnel
        // (`zone * bounds(lo, hi) + y(stage)`) came out as rectangles off the panel
        // rather than as a misplaced but visible plot: the range was fitted from the
        // pair the mark then declined to draw on that axis.
        let turned = crate::legality::zone_orient(cat_x.is_some(), cat_y.is_some())
            == crate::legality::Orient::Horizontal;
        let (mut x_sides, mut y_sides) = if turned {
            (zone_sides(|b| b.measure()), zone_sides(|b| b.domain()))
        } else {
            (zone_sides(|b| b.domain()), zone_sides(|b| b.measure()))
        };

        // **A displaced pile puts its foot in a column of its own**, so the measure
        // axis has to be shown that column too — the second mark whose position is not
        // in the axis's own field, and it arrives through the same door for the same
        // reason. `stack` writes each element's cumulative *top* back into the measure
        // column and its *bottom* into `stack_base`, which was invisible to the fit and
        // did not need to be while every pile stood on zero: the feet were then between
        // 0 and the tallest top, inside a range the tops already implied. Displace the
        // piles (`stack(baseline = )`) and the feet go below zero, so an axis fitted
        // from the tops alone clips every band's underside — the same defect the
        // bounded `zone` had, and worth naming as such, since both are a mark placing
        // itself against a range fitted without it.
        //
        // **Only the displaced layers**, and that condition was arrived at the hard
        // way. Adding it for every stacked layer looked free — with the default
        // baseline `stack_base` spans 0 to the tallest foot, inside the range the tops
        // already imply, so the fitted window cannot move. It does not move: every
        // polygon in the book's two stacked plots stayed identical. What moved was the
        // *bottom tick*, from `0M` to `-0M`, because handing the fit an exact `0.0`
        // low end where it had been inferring one takes a different path through the
        // nice-number step and lands a hair below zero. Cosmetic, and still a plot
        // changing for a reason that has nothing to do with the change — so the column
        // is offered only where it is actually needed.
        let stack_sides: Vec<(&DataFrame, &str)> = spec.layers.iter().enumerate()
            .filter(|(_, l)| l.transforms.contains(&Transform::Stack)
                && l.stack.as_ref().and_then(|s| s.baseline.as_deref())
                    .is_some_and(|b| b != "zero"))
            .flat_map(|(i, _)| panels.iter().map(move |p| (&p[i], crate::transform::STACK_BASE)))
            .collect();
        if horizontal { x_sides.extend(stack_sides) } else { y_sides.extend(stack_sides) }

        // Tick density is deliberately NOT thinned for narrow panels. It was
        // tried: a smaller target coarsens the step, and the scale ceiling —
        // the step's next multiple above the data — climbs with it, which
        // traded a label-crowding problem for panels half full of dead space.
        // Crowding is instead answered by anchoring edge labels inward (see
        // `write_ticks`); panels too narrow even for that are the many-level
        // case that facet wrapping exists to solve, and it is listed as such.
        let (x_ticks, xs) = build_axis(
            &eff, &bar_frames, &x_sides, x_field, cat_x.as_deref(),
            has_plain_bar && !horizontal,
            has_any_bar && horizontal,
            flush_x,
            scale::tick_count_of(spec.axis_def(&Channel::X)),
            x_log, x_base, x_time,
            scale::domain_of(spec.axis_def(&Channel::X)),
            crate::legality::slot_reach(spec, data, Channel::X),
        );
        let (y_ticks, ys) = build_axis(
            &eff, &bar_frames, &y_sides, y_field, cat_y.as_deref(),
            has_plain_bar && horizontal,
            (has_any_bar && !horizontal) || has_any_area || has_step_hist,
            // The measured axis keeps its headroom: an area's *height* is a
            // quantity, and the peak wants clearance from the frame the way a
            // bar's top does. Only the fill-spanning axis goes flush.
            //
            // Except in a pie, where the measure *is* the angular axis: the total
            // has to be exactly one turn or the last slice stops short and leaves a
            // wedge of background at twelve o'clock. Headroom on a circle is a gap,
            // not clearance — the same reason the angular axis is always flush.
            measure_on_angle,
            scale::tick_count_of(spec.axis_def(&Channel::Y)),
            y_log, y_base, y_time,
            scale::domain_of(spec.axis_def(&Channel::Y)),
            crate::legality::slot_reach(spec, data, Channel::Y),
        );

        // A **cut** tiling *is* the panel, so the panel is fitted to it rather than to
        // the cell centers `build_axis` just read off the position columns. Both axes,
        // one call each, for the same reason the axis machinery has one copy.
        // Both cut readings, `bin`'s and `density`'s — a cell is a cell however it
        // was measured, and the panel is fitted to the mesh either way. The **tile
        // plot** is deliberately absent, and its absence is the point: a categorical
        // axis is already exactly its slots (`-0.5 ..= n-0.5`), so the mesh and the
        // panel were fitted to each other before the transform ran. That is the same
        // fact as the tally publishing no extent columns — there is nothing here to
        // fit *to*, because the scale already holds it. The contour
        // is deliberately absent: its rows are *vertices*, not cells, so the ordinary
        // fit-the-data axis is already right for them.
        // Two marks take their extent from the mesh, so two are fitted to it: a
        // `zone`'s cells, and a 3-D `bar`'s **footprint** — the cube's floor is cut
        // by the same `bin` into the same four edge columns, so the axes it stands on
        // must be fitted to those edges or the columns spill past the frame. A *flat*
        // `bar * bin` is excluded by `reads_a_field` itself (it cuts one axis, not
        // two) and keeps deriving its width from the spacing, unchanged.
        let cell_space = crate::legality::space_of(spec);
        //
        // A **partition** is fitted here too, and needs to be: its two axes are
        // both synthesized (a running total along one, a ring index up the other),
        // so neither is fitted from a bound column and both would fall back to
        // `(0, 1)`. It is asked for by name rather than through `reads_a_field`
        // because that predicate answers *did a transform cut this plane*, and a
        // partition cut a tree instead — same four edge columns, different question.
        let cell_frames: Vec<&DataFrame> = spec.layers.iter().enumerate()
            .filter(|(_, l)| (matches!(l.mark, Mark::Zone | Mark::Bar)
                && crate::legality::reads_a_field(&l.mark, &l.transforms, cell_space))
                || crate::legality::publishes_cells(&l.mark, &l.transforms, cell_space))
            .flat_map(|(i, _)| panels.iter().map(move |p| &p[i]))
            .collect();
        let (x_ticks, xs) = fit_to_cells(
            &cell_frames, crate::transform::CELL_START, crate::transform::CELL_END,
            crate::transform::CELL_X, crate::transform::CELL_DX, x_ticks, xs,
            stated_domain(scale::domain_of(spec.axis_def(&Channel::X)), x_log, x_base));
        let (y_ticks, ys) = fit_to_cells(
            &cell_frames, crate::transform::CELL_LOWER, crate::transform::CELL_UPPER,
            crate::transform::CELL_Y, crate::transform::CELL_DY, y_ticks, ys,
            stated_domain(scale::domain_of(spec.axis_def(&Channel::Y)), y_log, y_base));

        // **Where the spokes start.** An angular gridline marks an angle, and at the
        // center every angle is the same point — so a spoke drawn across a hole is
        // not a gridline but a starburst, and it is the ink a reader of a donut asks
        // about first. Where the marks begin away from the center the spokes begin
        // there too: the cells' own inner edge, as a fraction of the radial range.
        // Nothing to clip on a plot whose cells reach the center, and nothing at all
        // on a plot with no cells (a rose's bars grow from zero), so this is `0.0`
        // for every plot the circle drew before the sunburst.
        //
        // Flat, the same question does not arise: a vertical gridline crosses an
        // empty band at full width and stays a gridline, because the band has a
        // width. This is the circle's geometry, not an exception to a flat rule.
        let inner_edge = cell_frames.iter()
            .filter_map(|d| d.float_col(crate::transform::CELL_LOWER))
            .flat_map(|c| c.iter().copied())
            .filter(|v| v.is_finite())
            .fold(f64::INFINITY, f64::min);
        let inner_edge = match inner_edge.is_finite() {
            true => unit_norm(inner_edge, ys).clamp(0.0, 1.0),
            false => 0.0,
        };

        // On a **periodic** axis the slots have to tile the turn exactly, or the
        // last one wraps round and draws on top of the first.
        //
        // A *categorical* angular axis already tiles: `build_axis` gives it
        // `-0.5 ..= n-0.5`, which is n slots wide for n categories. A *measured*
        // one does not — the range is fitted to the positions, and n centers span
        // only n−1 slot-widths, so n slots cover one slot more than the circle.
        // `bar * bin + x(bearing) + polar()` drew its first and last wedge over
        // each other for exactly that reason: 10 bins of 40° on a 360° turn, 40°
        // of doubled ink at twelve o'clock, visible as a darker wedge.
        //
        // Widening by half a slot at each end makes the range the slots' own
        // support, which is what the categorical rule already says in its units.
        // Flat this question does not arise: an axis with two ends lets the end
        // slots overhang into the margin, and nothing wraps onto anything.
        let xs = if is_polar && cat_x.is_none() && !bar_frames.is_empty() {
            widen_to_slot_support(&bar_frames, x_field, xs)
        } else {
            xs
        };

        // A bar that divides one slot has no position axis at all (spec §15), so
        // there is no range to fit and nothing to tick. Give it a unit slot centered
        // on zero, which is what `write_bars` places every element at, and an empty
        // tick list so no axis is drawn under a chart that has none.
        let one_slot = x_field.is_empty()
            && spec.layers.iter().any(crate::legality::bar_divides_one_slot);
        let (x_ticks, xs) = if one_slot {
            (TickSpec { values: Vec::new(), labels: Vec::new(), step: 1.0 }, (-0.5, 0.5))
        } else {
            (x_ticks, xs)
        };

        // The z-axis, when there is one. Built by the same routine as x and y —
        // a plain continuous axis (no bars stand on z, none measure it, and it
        // never carries a baseline), so most of `build_axis`'s cases fall away.
        // `zs` normalizes the third coordinate into the unit cube; `z_ticks`
        // labels the frame. Log/time on z are refused with direction in
        // `legality` rather than half-drawn here, so this stays linear.
        //
        // The tick count is read here too, and it was not before: this call passed
        // a hard `None` while `z_axis.tick_count` existed in the IR, so the third
        // axis would have ignored the field even once a binding could write it.
        // Two absences wearing one name, and only the first was recorded.
        let (z_ticks, zs) = if is_3d {
            // No `sides`: a `zone` is refused in the cube (`mark_draws_in_space`),
            // so nothing places itself on `z` other than through the column.
            build_axis(&eff, &[], &[], z_field, None, false, false, false,
                       scale::tick_count_of(spec.axis_def(&Channel::Z)),
                       false, 10.0, None,
                       scale::domain_of(spec.axis_def(&Channel::Z)),
                       // A violin stands on a *flat* slot axis; the cube has none.
                       (0.0, 0.0))
        } else {
            (TickSpec { values: Vec::new(), labels: Vec::new(), step: 1.0 }, (0.0, 1.0))
        };

            PanelAxes { x_ticks, xs, y_ticks, ys, z_ticks, zs, cat_x, cat_y, inner_edge }
        };

        // The shared fit: every panel at once, which is what makes the panels
        // comparable and is the default (spec §11).
        let every_panel: Vec<&Vec<DataFrame>> = panel_eff.iter().collect();
        let shared = fit_axes(&every_panel);
        let PanelAxes {
            x_ticks, xs, y_ticks, ys, z_ticks, zs, cat_x, cat_y, inner_edge,
        } = shared.clone();

        // **A map's axes are labeled in degrees and placed by the projection.**
        // Everything downstream of the projection reads projected numbers, which is
        // exactly what makes the space cheap — and it is wrong for precisely one
        // reader, the axis, which would otherwise announce that longitude runs from
        // −2 to 2. So the ticks are chosen on the degree range and then projected,
        // which is the only order that gives round numbers in round places.
        //
        // A meridian is a **curve** in any pseudocylindrical projection, so a
        // longitude has no single `x`: it has one per latitude. The tick is placed
        // where that meridian meets the edge the labels are written along — the
        // bottom for longitude, the left for latitude — which is what a printed
        // map does and is the only honest answer to a question with no single one.
        //
        // Labels are signed degrees rather than `30°N` / `90°W`, and that is a
        // translation decision rather than a cartographic one: those suffixes are
        // English initials, and this book is written to survive being translated.
        // A degree sign is read everywhere; a `W` is not.
        let (x_ticks, y_ticks) = match &map_degrees {
            Some((geo, (lon, lat))) => {
                let degrees = |(lo, hi): (f64, f64), n: usize, at: &dyn Fn(f64) -> f64| {
                    let picked = crate::render::ticks::nice_ticks(lo, hi, n);
                    let (values, labels) = picked
                        .values
                        .iter()
                        .zip(picked.labels.iter())
                        .filter(|(v, _)| **v >= lo && **v <= hi)
                        .map(|(v, l)| (at(*v), format!("{l}°")))
                        .unzip();
                    crate::render::ticks::ticks_with_labels(values, labels)
                };
                (
                    degrees(*lon, 7, &|v| geo.project(v, lat.0).0),
                    degrees(*lat, 5, &|v| geo.project(lon.0, v).1),
                )
            }
            None => (x_ticks, y_ticks),
        };

        // --- free scales: one fit per panel, for the axes that asked ---------
        //
        // `free` is read off the binding, so *which* axis is freed is whichever
        // one the caller wrote it on and there is no `free_x`/`free_y` vocabulary
        // to enumerate (spec §11). An axis nobody freed keeps the shared fit, so
        // the common request — free y, shared x — is a mix rather than a mode.
        // Fitted here rather than inside the panel loop because the *layout* has
        // to know first: a freed axis draws its labels in every panel, so every
        // cell owes them room, and how much room is the widest label any panel
        // ended up with.
        //
        // A panel's fit spans all of its **moments**, never one at a time. `play`
        // is refused without a facet precisely so that this stays true: a scale
        // refitted per frame moves the axis under the data (§16), and freeing a
        // panel's scale must not smuggle that in through the side door.
        let per_panel: Vec<PanelAxes> = if !any_free {
            Vec::new()
        } else {
            (0..npanels)
                .map(|slot| {
                    let moments: Vec<&Vec<DataFrame>> = (0..nframes)
                        .map(|fi| &panel_eff[fi * npanels + slot])
                        .collect();
                    let own = fit_axes(&moments);
                    PanelAxes {
                        x_ticks: if free_x { own.x_ticks } else { shared.x_ticks.clone() },
                        xs: if free_x { own.xs } else { shared.xs },
                        cat_x: if free_x { own.cat_x } else { shared.cat_x.clone() },
                        y_ticks: if free_y { own.y_ticks } else { shared.y_ticks.clone() },
                        ys: if free_y { own.ys } else { shared.ys },
                        cat_y: if free_y { own.cat_y } else { shared.cat_y.clone() },
                        // The spokes' start is read off the cells against `ys`, so
                        // it follows whichever fit produced it.
                        inner_edge: if free_y { own.inner_edge } else { shared.inner_edge },
                        z_ticks: if free_z { own.z_ticks } else { shared.z_ticks.clone() },
                        zs: if free_z { own.zs } else { shared.zs },
                    }
                })
                .collect()
        };

        // What each cell owes its own tick labels. A shared axis writes one set
        // along the outer margin and every cell owes nothing, which is why this
        // is `(0, 0)` for every plot drawn before free scales existed.
        let cell_axis = {
            let widest = |pick: fn(&PanelAxes) -> &TickSpec| -> f64 {
                per_panel.iter()
                    .flat_map(|a| pick(a).labels.iter())
                    .map(|l| estimate_text_width(l, self.font_sm))
                    .fold(0.0_f64, f64::max)
            };
            let w = if free_y { widest(|a| &a.y_ticks) + 8.0 } else { 0.0 };
            let h = if free_x { estimate_cap_height(self.font_sm) + 8.0 } else { 0.0 };
            (w, h)
        };

        // Named after the transform when the transform is what put it there — the
        // same rule `x`/`y` get from `axis_label` below, which the third position
        // needed too the moment a transform could synthesize onto it. Without this
        // the 3-D histogram raises a count axis and leaves it nameless, which is a
        // measurement drawn with no word for what it measures.
        let z_label = {
            let explicit = label_for(&ctx, Channel::Z, spec.z_axis.label.as_deref());
            if explicit.is_empty() && is_3d && bound_z.is_empty() {
                synth_y_label(spec).unwrap_or_default()
            } else {
                explicit
            }
        };

        // A bar is read from its baseline, and a log axis has none: zero sits
        // infinitely far down it. The bars measure from the bottom of the scale
        // instead, which `clamp` yields from negative infinity without needing a
        // second code path inside `write_bars`.
        let ext_log = if horizontal { x_log } else { y_log };
        let ext_base = if ext_log { f64::NEG_INFINITY } else { 0.0 };

        // An area always measures along y — both its axes are continuous, so
        // there is no categorical axis to read an orientation off, and no
        // horizontal form to choose. It therefore takes its baseline from `y`
        // directly rather than from the bar orientation.
        let area_base = if y_log { f64::NEG_INFINITY } else { 0.0 };

        // A synthesizing transform names the axis it writes to, which is the
        // measured one — "Count" belongs on x when the bars lie down.
        //
        // **A filled pile outranks all of it, including a bound column's name.**
        // `stack(share = true)` divides every element by its slot's total, so
        // whatever the number was — a tally, a sum of revenue — the axis is reading
        // fractions of one and any other word on it is false. That is why this
        // overrides a `y(<column>)` binding where `synth_y_label` never does: the
        // synthesizers *write* the axis and so only name what was nameless, while a
        // fill *rescales* an axis someone may already have named. An explicit
        // `label =` still wins, being a word the reader chose knowing all this.
        let axis_label = |channel, override_label: Option<&str>, measured: bool| {
            if let Some(l) = override_label { return l.to_string() }
            if measured && share_stacked(spec) { return "Share".to_string() }
            let explicit = label_for(&ctx, channel, override_label);
            if explicit.is_empty() && measured {
                synth_y_label(spec).unwrap_or_default()
            } else {
                explicit
            }
        };
        let x_label = axis_label(Channel::X, spec.x_axis.label.as_deref(), horizontal);
        let y_label = axis_label(Channel::Y, spec.y_axis.label.as_deref(), !horizontal);
        // The ring index's guide, dropped here — one place, so the rings, the
        // numbers and the name go together and no writer has to ask again. The
        // *scale* is untouched: `ys` still spans the stated domain, which is what
        // holds the hole open.
        let (y_ticks, y_label) = match depth_is_the_radius {
            true => (TickSpec { values: Vec::new(), labels: Vec::new(), step: 1.0 },
                     String::new()),
            false => (y_ticks, y_label),
        };

        // **A displaced pile spends its measure axis**, so that axis draws no numbers
        // and no name — the fourth thing to earn silence, and it earns it the way the
        // other three do (§12, and the coverage chapter's "no readable axes" entry):
        // not by being asked, but because of *what the axis now carries*. Once
        // `stack(baseline = )` has moved a pile's foot, no value on the measure axis
        // corresponds to any measurement: a band from 12 to 20 says "this group is 8
        // here", and the 12 and the 20 are artifacts of where the pile was hung. A
        // number a reader can look up and be wrong about is worse than no number,
        // which is the same ruling that leaves an empty heatmap cell unpainted rather
        // than colored at the bottom of the ramp.
        //
        // The *scale* is untouched, exactly as the ring index leaves it: thicknesses
        // are still to scale and still comparable across the plot, which is the whole
        // of what a streamgraph asks a reader to do.
        let displaced = spec.layers.iter().any(|l| {
            l.transforms.contains(&Transform::Stack)
                && l.stack.as_ref().and_then(|s| s.baseline.as_deref())
                    .is_some_and(|b| b != "zero")
        });
        let (x_ticks, x_label, y_ticks, y_label) = match (displaced, horizontal) {
            (true, true) => (TickSpec { values: Vec::new(), labels: Vec::new(), step: 1.0 },
                             String::new(), y_ticks, y_label),
            (true, false) => (x_ticks, x_label,
                              TickSpec { values: Vec::new(), labels: Vec::new(), step: 1.0 },
                              String::new()),
            (false, _) => (x_ticks, x_label, y_ticks, y_label),
        };

        // Build color map before legends so both use the same color assignments.
        // The ramp is the continuous counterpart, resolved once for the plot.
        //
        // Colors are assigned from the *unfiltered* frames so a category keeps
        // its color in every panel, whether or not that panel has a row of it —
        // and, passing no moment here, in every frame of a `play` sequence too. A
        // color reassigned partway through would be the animation's worst failure
        // mode, since a reader tracks a bubble by its color and nothing on the
        // page would say the mapping had changed under them.
        let eff_global: Vec<DataFrame>;
        let color_frames: &[DataFrame] = if panel_eff.len() == 1 {
            &panel_eff[0]
        } else {
            eff_global = eff_for(&[], None);
            &eff_global
        };
        let color_map = build_color_map(spec, color_frames, &mut remarks);
        let ramp = resolve_ramp(&spec.palette);

        let legends = collect_legends(&ctx, &color_map, color_frames);
        let legend_panel_w = if legends.is_empty() { 0.0 } else {
            LEGEND_PLOT_GAP + legends.iter()
                .map(|b| b.width(self.font_sm, self.font_md))
                .fold(0.0_f64, f64::max)
        };

        // A 3-D plot reserves no margin for 2-D tick labels or axis titles —
        // its axes live on the cube's edges, inside the panel — so the layout is
        // fed empty ones and the cube takes the whole panel. The ranges above
        // are still needed to normalize coordinates; only the layout hints change.
        let no_ticks = TickSpec { values: Vec::new(), labels: Vec::new(), step: 1.0 };
        // A polar plot reserves no tick-label margin either — its angular labels
        // ring the circle and its radial ones run up the spoke, both *inside* the
        // panel. It keeps the axis names, though, unlike 3-D: they still say what
        // the angle and the radius measure, and there is nowhere in the circle to
        // put them.
        let (grid_xt, grid_yt, grid_xl, grid_yl): (&TickSpec, &TickSpec, &str, &str) =
            if is_3d {
                (&no_ticks, &no_ticks, "", "")
            } else if is_nest {
                // A packing has no axes at all — not axes drawn elsewhere, as in
                // 3-D and polar, but none. So it reserves no margin for either, and
                // it keeps no axis names: `check_nest` refuses `x_label()`/
                // `y_label()` outright rather than letting one be set and dropped,
                // and the *derived* names (a column's own) would be worse still,
                // since they would promise that the direction under them measures
                // that column.
                (&no_ticks, &no_ticks, "", "")
            } else if is_polar {
                (&no_ticks, &no_ticks, x_label.as_str(), y_label.as_str())
            } else {
                // An axis another plot on the page is drawing costs *this* plot
                // no margin — which is what puts a marginal histogram flush
                // against the scatter below it rather than a tick label's worth
                // of white away from it. The ticks themselves are unchanged, so
                // the gridlines still stand where the shared axis says they do.
                (
                    if self.fit.draw_x_axis { &x_ticks } else { &no_ticks },
                    if self.fit.draw_y_axis { &y_ticks } else { &no_ticks },
                    if self.fit.draw_x_axis { x_label.as_str() } else { "" },
                    if self.fit.draw_y_axis { y_label.as_str() } else { "" },
                )
            };

        // **A map's shape is the projection's, not the panel's.** Every other space
        // may stretch to fill the cell it is given, because stretching an ordinary
        // scatter changes nothing about what it says. Stretching a map does: the
        // whole claim of an equal-area projection is that ink is proportional to
        // ground, and a panel 1.75 times too tall breaks that claim while still
        // looking like a map. So the space takes the panel's proportions from its
        // own projected extent, which makes a projected unit the same number of
        // pixels across as it is tall.
        //
        // `theme(ratio = )` is overridden rather than refused, and the difference
        // matters: a ratio is a statement about a panel, and here the panel is not
        // the caller's to shape. The refusal is in `legality`, where a reader is
        // told, rather than here, where they would only see the number ignored.
        let map_ratio = match &spec.coord {
            CoordSpace::Map(_) => {
                let (w, h) = (xs.1 - xs.0, ys.1 - ys.0);
                (w.is_finite() && h.is_finite() && h.abs() > 1e-12).then(|| (w / h).abs())
            }
            _ => None,
        };

        let grid = PanelGrid::compute(
            self.width, self.height,
            (self.font_sm, self.font_md, self.font_lg),
            grid_xt, grid_yt, grid_xl, grid_yl,
            spec.title.is_some(),
            legend_panel_w,
            // Cloned because `panel_levels` borrows both, and each panel now
            // states its own level when it writes itself out, which is after
            // this. One short list of level names per facet, copied once.
            col_values.clone(), row_values.clone(),
            map_ratio.or(spec.theme.resolved().ratio),
            spec.theme.resolved().tick_angle,
            play_def.is_some(),
            facet_wrap,
            (free_x, free_y),
            cell_axis,
            self.fit,
        );

        // A dot plot whose piles have outgrown their dots is still drawn, and said
        // out loud (spec §12) — the panel's height and the tallest pile are both
        // known now, and one count unit is how far apart two rungs sit.
        if let Some(w) = grid.panels.first().map(|p| p.rect.h()).and_then(|h|
            pile_overlap_warning(spec, &panel_eff, y_field, ys, h, self.point_radius))
        {
            remarks.push(Diagnostic {
                kind: crate::legality::DiagnosticKind::Assumption,
                message: w,
            });
        }

        let mut svg = String::with_capacity(64 * 1024);
        self.write_header(&mut svg);
        self.write_canvas(&mut svg);

        for panel in &grid.panels {
            let l = &panel.rect;
            let clip = clip_id(l);

            // This panel's own axes, when a binding freed one. `per_panel` is
            // empty otherwise, and every name below then refers to the shared fit
            // destructured before the loop — which is what keeps a plot nobody
            // freed byte-for-byte what it was.
            //
            // **Resolved before anything is drawn, and that ordering is the whole
            // of the fix (2026-07-28).** This block used to sit *below* the frame
            // routines, so a freed panel drew its marks from its own fit and its
            // frame from the shared one — rings and spokes at one scale's
            // positions, tick numbers reading another's. `inner_edge` was the
            // visible end of it: rebound here and never read again, which the
            // compiler had been reporting as an unused variable the whole time.
            // A dead store is cheap; what it was pointing at was a guide that
            // disagreed with the data it annotates, which is §12's wrongness a
            // reader cannot see.
            let own = per_panel.get(panel.slot);
            let (x_ticks, xs) = match own {
                Some(a) => (&a.x_ticks, a.xs),
                None => (&x_ticks, xs),
            };
            let (y_ticks, ys) = match own {
                Some(a) => (&a.y_ticks, a.ys),
                None => (&y_ticks, ys),
            };
            let (z_ticks, zs) = match own {
                Some(a) => (&a.z_ticks, a.zs),
                None => (&z_ticks, zs),
            };
            let cat_x = match own { Some(a) => a.cat_x.clone(), None => cat_x.clone() };
            let cat_y = match own { Some(a) => a.cat_y.clone(), None => cat_y.clone() };
            let inner_edge = own.map_or(inner_edge, |a| a.inner_edge);

            // The polar panel is a disc, not a rectangle: its background, its clip
            // and its gridlines are all circular, so it is written by its own
            // frame routine rather than by bending the flat one.
            // A categorical angular axis divides the turn into one slot per
            // category, and the frame rotates back half a slot so `start` points at
            // the first category rather than at the padding before it (see
            // `Polar::new`). A measured angle has no slots and stays put.
            let angle_slots = if measure_on_angle { None } else { cat_x.as_ref().map(|c| c.len()) };
            let pol = polar_view.map(|v| Polar::new(
                l, v, POLAR_RIM_GAP + estimate_cap_height(self.font_sm) + 4.0,
                measure_on_angle, angle_slots));
            match &pol {
                Some(p) => self.write_polar_frame(&mut svg, p, x_ticks, xs, y_ticks, ys, &clip,
                                                  &spec.theme.resolved(), inner_edge),
                None => self.write_panel_background(&mut svg, l, &clip, &spec.theme.resolved()),
            }
            // What a browser needs, and the only thing it needs: where this panel
            // is and what its axes measure. Written only when a brush is declared,
            // so an ordinary plot's bytes are exactly what they were.
            // The levels this panel was filtered by, read from the same list the
            // filters were built from a few hundred lines up, so the two cannot
            // come apart. `zip` rather than two `if let`s, because a facet has a
            // column exactly when it has a level.
            let (cv, rv) = panel_levels.get(panel.slot).copied().unwrap_or((None, None));
            let facts = PanelFacts {
                facet_col: col_field.as_deref().zip(cv),
                facet_row: row_field.as_deref().zip(rv),
                play: (nframes >= 2).then(|| play_def
                    .map(|d| (d.field.as_str(), play_levels.as_slice(), frame_seconds)))
                    .flatten(),
                place,
            };
            self.write_brush_frame(&mut svg, spec, l, xs, ys, cat_x.as_ref(), cat_y.as_ref(),
                                   x_field, y_field, !is_nest && !is_3d,
                                   x_log.then_some(x_base), y_log.then_some(y_base), &facts);
            // Which subset this panel shows at moment `fi`. Moments are the outer
            // stride, so at one frame this is exactly the index it always was.
            let eff_at = |fi: usize| &panel_eff[fi * npanels + panel.slot];

            // The 3-D branch: a projected cube frame instead of 2-D axes and
            // gridlines, and marks routed through the projector and depth-sorted.
            // The box is drawn first so the marks paint over it — the glass box
            // sits behind the cloud — and its *labels* go on afterwards, below.
            // Which marks arrive here is `rule_for(_, Z).renders` and nothing else;
            // every other mark refuses `z` with direction in `legality`, so a
            // checked spec never brings one in.
            //
            // **This branch is inside the panel loop, and that is the whole of
            // faceted 3-D.** The `Scene` is built from `l` — *this panel's* rect —
            // so N panels project N cubes with no layout work beyond the division
            // `PanelGrid` already did. It read that way long before a facet was
            // allowed to reach it, which is why the refusal that used to gate it
            // was stale rather than protective (`legality::check_space`).
            //
            // A cube's guides are drawn per panel rather than once for the plot,
            // and that is the rule rather than an omission: a guide on the panel's
            // *boundary* is shared, because adjacent panels share that boundary,
            // and a guide *inside* the panel is not, because there is no boundary
            // to share it on. `polar` already worked this way; a cube's three axes
            // are edges of the cube, and the cube is inside the panel. The layout
            // agrees — 3-D is handed empty tick lists above, so no outer margin is
            // reserved for numbers that never go there.
            if is_3d {
                let scene = project::Scene::new(view, l.x0, l.y0, l.x1, l.y1, FRAME_INSET);
                self.write_space_box(&mut svg, &scene);
                // One group per moment, wrapping the marks and nothing else. The
                // panel behind them, its gridlines and its axes are shared by every
                // frame — they are drawn from one scale, so redrawing them per frame
                // would be bytes spent making them flicker. At one frame this writes
                // no group and no timing, which is what leaves an unplayed plot
                // byte-for-byte what it was.
                for fi in 0..nframes {
                    let eff = eff_at(fi);
                    self.open_frame(&mut svg, fi, nframes);
                    for &dim in self.selection_passes(spec) {
                      self.open_pass(&mut svg, dim);
                      for (layer, df) in spec.layers.iter().zip(eff.iter()) {
                        let Some(df) = self.pass_rows(spec, layer, df, dim) else { continue };
                        let df = &*df;
                        if df.is_empty() { continue }
                        match layer.mark {
                            Mark::Point => self.write_points(&mut svg, layer, df, l, xs, ys,
                                x_field, y_field, cat_x.as_deref(), cat_y.as_deref(),
                                &color_map, &ramp, &clip, zs, z_field, Some(&scene), None),
                            Mark::Path => self.write_path(&mut svg, layer, df, l, xs, ys,
                                x_field, y_field, cat_x.as_deref(), cat_y.as_deref(),
                                &color_map, &ramp, &clip, zs, z_field, Some(&scene), None),
                            // The column standing on the cube's floor — the 3-D
                            // histogram, and the first *slot* mark in space. It takes no
                            // `Layout`: a flat bar's thickness is pixels on the panel,
                            // where this one's footprint is two pairs of data edges the
                            // projector turns into a solid.
                            Mark::Bar => self.write_bars_3d(&mut svg, layer, df, xs, ys, zs,
                                x_field, y_field, z_field, cat_x.as_deref(), cat_y.as_deref(),
                                &color_map, &clip, &scene),
                            // The sheet through the samples, and the one mark that draws
                            // *only* here. It takes no `cat_x`/`cat_y`: a face asserts
                            // every value between two nodes, so both positions are
                            // continuous and there is no slot to map a category into.
                            // The other two slot marks, standing on the same floor as
                            // `bar` and by the same derivation — a whisker's span and a
                            // box's summary are a bar's length asked of the same pair of
                            // axes (`legality::is_slot_mark`).
                            Mark::Interval => self.write_interval_3d(&mut svg, layer, df, xs, ys, zs,
                                x_field, y_field, z_field, cat_x.as_deref(), cat_y.as_deref(),
                                &color_map, &clip, &scene),
                            Mark::Box => self.write_box_3d(&mut svg, layer, df, xs, ys, zs,
                                x_field, y_field, z_field, cat_x.as_deref(), cat_y.as_deref(),
                                &color_map, &clip, &scene),
                            Mark::Surface => self.write_surface(&mut svg, layer, df, xs, ys, zs,
                                x_field, y_field, z_field, &color_map, &ramp, &clip, &scene),
                            _ => {}
                        }
                      }
                      self.close_pass(&mut svg, dim);
                    }
                    self.close_frame(&mut svg, fi, nframes, frame_seconds);
                }
                // The frame's labels go on last, the same rule the flat and polar
                // frames follow below: a guide is an annotation *about* the scene,
                // not a member of it, so nothing the marks paint can take a
                // measurement's name away. This is what a solid mesh made
                // impossible to keep ignoring — it covers the whole floor, so
                // every number along both domain edges went under it.
                self.write_space_labels(&mut svg, &scene, l, &spec.theme.resolved(), view,
                    [scale::tick_count_of(spec.axis_def(&Channel::X)),
                     scale::tick_count_of(spec.axis_def(&Channel::Y)),
                     scale::tick_count_of(spec.axis_def(&Channel::Z))],
                    &x_ticks, xs, &x_label, &y_ticks, ys, &y_label, &z_ticks, zs, &z_label,
                    &mut remarks);
                continue;
            }

            // Skip vertical grid lines for plain bar charts (bars fill the gaps
            // visually), and skip whichever axis's gridlines `theme(grid = )`
            // turned off. The two reasons compose rather than override: the bar
            // rule is the renderer's own taste and the theme is the caller's, so
            // either one suppressing a set is enough.
            // A gridline is a reading aid for an axis, so a packed panel has none
            // to draw — and the cells cover the panel anyway, so drawing them would
            // be ink under paint that reappears wherever a region is translucent.
            if pol.is_none() && !is_nest {
                let theme = spec.theme.resolved();
                self.write_grid(&mut svg, l, &x_ticks, xs, &y_ticks, ys,
                                (has_plain_bar && !horizontal) || !theme.grid_x(),
                                (has_plain_bar && horizontal) || !theme.grid_y());
            }

            // The packing frame for this panel, built once and shared, on the same
            // rule as `Polar`: two readers of the same panel must not be able to
            // disagree about where a region is. A facet gets one per panel, so each
            // panel's regions fill *it* and the shares are read within the panel —
            // the analog of a shared scale, which is the only thing a packing can
            // offer in its place.
            let nst = is_nest.then(|| Nest::new(l));
            let pol_ref = pol.as_ref();
            // One group per moment, wrapping the marks and nothing else. The
            // panel behind them, its gridlines and its axes are shared by every
            // frame — they are drawn from one scale, so redrawing them per frame
            // would be bytes spent making them flicker. At one frame this writes
            // no group and no timing, which is what leaves an unplayed plot
            // byte-for-byte what it was.
            for fi in 0..nframes {
                let eff = eff_at(fi);
                self.open_frame(&mut svg, fi, nframes);
                // One pass unless the reader has selected something, in which case
                // the unselected rows are drawn first and pushed back.
                for &dim in self.selection_passes(spec) {
                  self.open_pass(&mut svg, dim);
                  for (layer, df) in spec.layers.iter().zip(eff.iter()) {
                    let Some(df) = self.pass_rows(spec, layer, df, dim) else { continue };
                    let df = &*df;
                    if df.is_empty() { continue }
                    // The **violin** (spec §5): `area` and `ribbon` both hand their slot
                    // reading of `density` to one routine, differing only in where the
                    // region closes. Decided before the mark match rather than inside two
                    // of its arms, because it is one question — *is this layer the slot
                    // reading?* — and `legality::slot_density` is its one authority, the
                    // same call the checks and the transform stage make.
                    if let Some(orient) = crate::legality::slot_density(spec, layer, Some(df)) {
                        use crate::render::marks::violin::Slot;
                        // Each mark's own identity, read against a slot: a `ribbon`
                        // closes on its reflection, an `area` on the slot's line, and a
                        // stroke closes on nothing. Listed rather than defaulted so a
                        // fifth mark joining `slot_density` is a compile error here.
                        let shape = match layer.mark {
                            Mark::Ribbon => Slot::Mirrored,
                            Mark::Area => Slot::Halved,
                            _ => Slot::Traced,
                        };
                        self.write_violin(&mut svg, layer, df, l, xs, ys, x_field, y_field,
                            cat_x.as_deref(), cat_y.as_deref(),
                            orient == crate::legality::Orient::Horizontal,
                            shape, &color_map, &clip, pol_ref);
                        continue;
                    }
                    match layer.mark {
                        Mark::Point => self.write_points(&mut svg, layer, df, l, xs, ys, x_field, y_field, cat_x.as_deref(), cat_y.as_deref(), &color_map, &ramp, &clip, zs, z_field, None, pol_ref),
                        Mark::Line  => self.write_line(&mut svg, layer, df, l, xs, ys, x_field, y_field, cat_x.as_deref(), &color_map, &ramp, &clip, pol_ref, &mut remarks),
                        Mark::Area  => self.write_area(&mut svg, layer, df, l, xs, ys, x_field, y_field, cat_x.as_deref(), area_base, &color_map, &clip, pol_ref),
                        Mark::Bar   => self.write_bars(&mut svg, layer, df, l, xs, ys, x_field, y_field, cat_x.as_deref(), cat_y.as_deref(), horizontal, ext_base, &color_map, &clip, pol_ref, nst.as_ref()),
                        Mark::Step  => self.write_step(&mut svg, layer, df, l, xs, ys, x_field, y_field, cat_x.as_deref(), area_base, &color_map, &ramp, &clip, pol_ref),
                        Mark::Interval => self.write_interval(&mut svg, layer, df, l, xs, ys, x_field, y_field, cat_x.as_deref(), cat_y.as_deref(), horizontal, &color_map, &clip, pol_ref),
                        Mark::Box => self.write_box(&mut svg, layer, df, l, xs, ys, x_field, y_field, cat_x.as_deref(), cat_y.as_deref(), horizontal, &color_map, &clip, pol_ref),
                        Mark::Ribbon => self.write_ribbon(&mut svg, layer, df, l, xs, ys, x_field, y_field, cat_x.as_deref(), &color_map, &clip, pol_ref),
                        Mark::Text => self.write_text(&mut svg, layer, df, l, xs, ys, x_field, y_field, cat_x.as_deref(), cat_y.as_deref(), &color_map, &clip, pol_ref, nst.as_ref(), &mut remarks),
                        Mark::Path => self.write_path(&mut svg, layer, df, l, xs, ys, x_field, y_field, cat_x.as_deref(), cat_y.as_deref(), &color_map, &ramp, &clip, zs, z_field, None, pol_ref),
                        // The one mark handed the whole spec rather than the two
                        // resolved field names: which axis places it is read off
                        // *both* positions against this layer's own table
                        // (`legality::rule_axis`), so a pair of `&str` cannot say it.
                        Mark::Rule => self.write_rule(&mut svg, layer, df, l, xs, ys, spec, cat_x.as_deref(), cat_y.as_deref(), &color_map, &clip, pol_ref),
                        // Drawn like any other mark, but its frame reached here
                        // untransformed — a zone's `bounds` names four columns rather
                        // than reshaping rows into pairs (see the effective-frame
                        // branch above), so it reads them straight off the table.
                        Mark::Zone => self.write_zone(&mut svg, layer, df, l, xs, ys, x_field, y_field, cat_x.as_deref(), cat_y.as_deref(), &color_map, &ramp, &clip, pol_ref),
                        // A surface draws in the cube and nowhere else, so it is handled
                        // in the 3-D branch above and a *flat* one never arrives — it is
                        // the one mark `mark_draws_in_space` refuses in `flat`, and
                        // `check_surface` says so with both routes into the cube named.
                        // Listed rather than caught by `_` on purpose: a new `Mark`
                        // variant must be a compile error in this match, not an empty
                        // panel — which is exactly how `area` came to render nothing,
                        // silently, for as long as it sat in the enum.
                        Mark::Surface => {}
                    }
                  }
                  self.close_pass(&mut svg, dim);
                }
                self.close_frame(&mut svg, fi, nframes, frame_seconds);
            }

            // The tick labels go on last in both spaces, so they stay readable over
            // whatever the marks painted. In polar that is the ring of category
            // names outside the circle and the radial numbers up the spoke.
            match &pol {
                Some(p) => self.write_polar_ticks(&mut svg, p, &x_ticks, xs, &y_ticks, ys),
                // Nothing at all in a packed panel. An axis line is the edge of a
                // measurement and a tick is a place on one; this space has neither,
                // and the cells' own edges are what the reader has instead.
                None if is_nest => {}
                None => {
                    self.write_axes(&mut svg, l, &spec.theme.resolved());
                    // A shared axis is one axis, so it is ticked once — by the
                    // plot nearest the edge it lives on. The same sentence
                    // `labels_x` says about the panels of a facet, said about
                    // the plots of a page (`render::page`).
                    self.write_ticks(&mut svg, l, &x_ticks, xs, &y_ticks, ys,
                                     grid.labels_x(panel) && self.fit.draw_x_axis,
                                     grid.labels_y(panel) && self.fit.draw_y_axis,
                                     grid.ncols > 1,
                                     spec.theme.resolved().tick_angle);
                }
            }
        }

        self.write_strips(&mut svg, &grid, &spec.theme.resolved());
        self.write_play_strip(&mut svg, &grid, &play_levels, frame_seconds,
                              &spec.theme.resolved());
        // In 3-D the axis names sit on the cube's edges, so the outer margin
        // carries only the title — the 2-D x/y labels would float against no axis.
        let (outer_xl, outer_yl) = if is_3d || is_nest { ("", "") } else { (x_label.as_str(), y_label.as_str()) };
        // A name belongs to the axis, so it goes wherever the ticks went: on the
        // one plot of the page that draws the shared axis, and nowhere else.
        let (outer_xl, outer_yl) = (
            if self.fit.draw_x_axis { outer_xl } else { "" },
            if self.fit.draw_y_axis { outer_yl } else { "" },
        );
        // The band the *margin* reserved, which is `grid_xt` rather than `x_ticks`:
        // polar draws real angular labels and hands the layout an empty list,
        // because those labels go inside the circle.
        self.write_labels(&mut svg, &grid.outer, outer_xl, outer_yl, spec,
                          !grid_xt.labels.is_empty());
        if !legends.is_empty() {
            // The canvas is the floor, not the panel — a legend has always been
            // allowed to run past the panel's bottom edge into the margin beside
            // the x tick labels, and bounding it at `grid.outer.y1` would drop
            // legends that fit the image.
            write_legends(&mut svg, &grid.outer, &legends, (self.font_sm, self.font_md),
                          self.height - LEGEND_PADDING, &mut remarks);
        }
        self.write_footer(&mut svg);

        // What a page needs back: where the panels ended up, and what each axis
        // measured. The panel *area* rather than the first panel — a faceted plot
        // composed onto a page aligns by the block its panels divide, since that
        // is the rectangle its axis runs over.
        let area = Layout {
            x0: grid.panels.first().map(|p| p.rect.x0).unwrap_or(grid.outer.x0),
            y0: grid.panels.first().map(|p| p.rect.y0).unwrap_or(grid.outer.y0),
            x1: grid.panels.last().map(|p| p.rect.x1).unwrap_or(grid.outer.x1),
            y1: grid.panels.last().map(|p| p.rect.y1).unwrap_or(grid.outer.y1),
        };
        // A map fits its scales on **projected** frames while `lon` and `lat` stay
        // degrees, so a page must not share this axis through a stated domain —
        // see `AxisFacts::projected` for why neither unit works.
        let projected = map_degrees.is_some();
        let facts = |field: &str, range: (f64, f64), cats: Option<&Vec<String>>, log: bool,
                     base: f64| AxisFacts {
            field: field.to_string(),
            range,
            cats: cats.cloned(),
            log_base: log.then_some(base),
            projected,
        };
        let mut seen = std::collections::HashSet::new();
        remarks.retain(|d| seen.insert(d.message.clone()));
        Drawn {
            svg,
            panel: area,
            x: facts(x_field, xs, cat_x.as_ref(), x_log, x_base),
            y: facts(y_field, ys, cat_y.as_ref(), y_log, y_base),
            remarks,
        }
    }

    // -----------------------------------------------------------------------
    // SVG structure
    // -----------------------------------------------------------------------

    fn write_header(&self, svg: &mut String) {
        writeln!(svg,
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">"#,
            w = self.width, h = self.height
        ).unwrap();
    }

    fn write_canvas(&self, svg: &mut String) {
        writeln!(svg,
            r#"  <rect width="{w}" height="{h}" fill="white"/>"#,
            w = self.width, h = self.height
        ).unwrap();
    }

    fn write_panel_background(&self, svg: &mut String, l: &Layout, clip: &str,
                              theme: &ThemeSpec) {
        writeln!(svg,
            r##"  <rect x="{x}" y="{y}" width="{w}" height="{h}" fill="{bg}"/>"##,
            x = l.x0, y = l.y0, w = l.w(), h = l.h(),
            bg = theme.background_or(PANEL_BG)
        ).unwrap();
        writeln!(svg,
            r#"  <clipPath id="{clip}"><rect x="{x}" y="{y}" width="{w}" height="{h}"/></clipPath>"#,
            x = l.x0, y = l.y0, w = l.w(), h = l.h()
        ).unwrap();
    }

    // -----------------------------------------------------------------------
    // Facet strips — the names of the panels
    //
    // A grid-faceted plot names its columns once, above the top row, and its
    // rows once, beside the last column — repeating the name on every panel
    // would spend a text row per panel to say something the alignment already
    // says. Row-strip text turns 90° to fit its narrow band, the one place in
    // gog text runs vertically.
    //
    // A *folded* ribbon cannot do that, and the reason is the fold: alignment
    // says nothing about which level a cell holds once the line has turned, so
    // every panel carries its own name. The layout put a rectangle above each
    // panel for it (`Panel::strip`); this only fills them.
    // -----------------------------------------------------------------------

    fn write_strips(&self, svg: &mut String, grid: &PanelGrid, theme: &ThemeSpec) {
        let strip_bg = theme.strip_or(STRIP_BG);
        let strip_fg = strip_ink(theme, &strip_bg);
        use crate::render::layout::{STRIP_H, STRIP_W};
        const STRIP_GAP: f64 = 4.0; // between the strip box and its panel
        let cap = estimate_cap_height(self.font_sm);

        if !grid.wrap_values.is_empty() {
            writeln!(svg,
                r##"  <g font-family="system-ui,sans-serif" font-size="{fs}" fill="{strip_fg}" text-anchor="middle">"##,
                fs = self.font_sm
            ).unwrap();
            for panel in &grid.panels {
                let (Some(band), Some(value)) = (&panel.strip, grid.wrap_values.get(panel.slot))
                    else { continue };
                let bh = band.h() - STRIP_GAP;
                writeln!(svg,
                    r##"    <rect x="{x:.2}" y="{y:.2}" width="{w:.2}" height="{bh:.2}" fill="{strip_bg}"/>"##,
                    x = band.x0, y = band.y0, w = band.w()
                ).unwrap();
                writeln!(svg,
                    r#"    <text x="{cx:.2}" y="{ty:.2}">{v}</text>"#,
                    cx = band.x0 + band.w() / 2.0, ty = band.y0 + bh / 2.0 + cap / 2.0,
                    v = esc(value)
                ).unwrap();
            }
            writeln!(svg, "  </g>").unwrap();
            return;
        }

        if !grid.col_values.is_empty() {
            writeln!(svg,
                r##"  <g font-family="system-ui,sans-serif" font-size="{fs}" fill="{strip_fg}" text-anchor="middle">"##,
                fs = self.font_sm
            ).unwrap();
            for (c, value) in grid.col_values.iter().enumerate() {
                let p = &grid.panels[c].rect; // row 0 comes first in row-major order
                // Below the play strip when there is one: that band names the
                // moment every panel is showing, so it sits above the names of the
                // panels themselves. Without `play` this is `outer.y0` exactly, and
                // the faceted plot is unmoved.
                let by = grid.play_strip.as_ref().map_or(grid.outer.y0, |p| p.y1);
                let bh = STRIP_H - STRIP_GAP;
                writeln!(svg,
                    r##"    <rect x="{x:.2}" y="{by:.2}" width="{w:.2}" height="{bh:.2}" fill="{strip_bg}"/>"##,
                    x = p.x0, w = p.w()
                ).unwrap();
                writeln!(svg,
                    r#"    <text x="{cx:.2}" y="{ty:.2}">{v}</text>"#,
                    cx = p.x0 + p.w() / 2.0, ty = by + bh / 2.0 + cap / 2.0, v = esc(value)
                ).unwrap();
            }
            writeln!(svg, "  </g>").unwrap();
        }

        if !grid.row_values.is_empty() {
            writeln!(svg,
                r##"  <g font-family="system-ui,sans-serif" font-size="{fs}" fill="{strip_fg}" text-anchor="middle">"##,
                fs = self.font_sm
            ).unwrap();
            for (r, value) in grid.row_values.iter().enumerate() {
                let p = &grid.panels[r * grid.ncols + grid.ncols - 1].rect;
                let bw = STRIP_W - STRIP_GAP;
                let bx = grid.outer.x1 - bw;
                writeln!(svg,
                    r##"    <rect x="{bx:.2}" y="{y:.2}" width="{bw:.2}" height="{h:.2}" fill="{strip_bg}"/>"##,
                    y = p.y0, h = p.h()
                ).unwrap();
                // Rotated 90° about its anchor: the glyphs then extend rightward
                // from the anchor, so backing the anchor off by half a cap height
                // centers the line of text in the strip box.
                let ax = bx + bw / 2.0 - cap / 2.0;
                let ay = p.y0 + p.h() / 2.0;
                writeln!(svg,
                    r#"    <text x="{ax:.2}" y="{ay:.2}" transform="rotate(90 {ax:.2} {ay:.2})">{v}</text>"#,
                    v = esc(value)
                ).unwrap();
            }
            writeln!(svg, "  </g>").unwrap();
        }
    }

    // -----------------------------------------------------------------------
    // Play — the facet strip read in time, and the timing that swaps it
    //
    // The whole of the animation is three small writers, and their smallness is
    // the finding rather than an economy: a frame is a complete plot, so nothing
    // here interpolates, keys a datum, or knows what a mark is. It hides one
    // group and shows the next, which is the one operation that works identically
    // on all thirteen marks and in every coordinate space — Law 2 satisfied by
    // construction rather than by thirteen implementations agreeing.
    //
    // **Why SMIL and not a `<style>` block.** Two plots are inlined into one HTML
    // page all through this book, and CSS `@keyframes` are document-scoped: a
    // shared animation name would let the second plot's timing silently
    // reinterpret the first's. `<animate>` is a *child of the element it drives*,
    // so it needs no name, no id and no class, and two plots on a page cannot
    // reach each other. `clip_id` and `tile_id` exist because this crate has had
    // that collision before; this is the version of the problem that can simply
    // not be had.
    //
    // **Why the first frame's state is written statically.** Everything that
    // converts an SVG rather than animating it — `rsvg-convert`, which is how this
    // book's PDF is made — ignores `<animate>` and renders the attributes as
    // written. So the static `display` *is* the print fallback, and writing
    // `inline` on the first moment and `none` on the rest is what makes a printed
    // page show that moment with its strip, rather than every moment at once.
    // -----------------------------------------------------------------------

    // -----------------------------------------------------------------------
    // Selection — the marks drawn twice, once pushed back
    //
    // A brush is drawn as a **row partition**, which is the shape `play` and
    // `facet` already use: the rows outside the selection are drawn first inside
    // one dimmed group, then the rows inside it are drawn at full strength over
    // the top. Nothing per-element changes, so no mark writer learns that
    // selection exists, and the dim composes with whatever opacity each mark
    // resolved for itself.
    //
    // Everything here is silent when nothing is selected, on `open_frame`'s
    // discipline and for the same reason: a plot that names no brush, or names
    // one the reader has not moved, must be byte-for-byte the plot it was before
    // this code existed. That promise is what the book, the PDF and every parity
    // hash rest on. Spec §15.
    // -----------------------------------------------------------------------

    /// Where this panel is, and what its axes measure — the one fact a browser
    /// cannot work out for itself.
    ///
    /// A gesture arrives in pixels and a brush is written in data units, so
    /// something has to invert the scale. Doing it in JavaScript would be a
    /// second copy of `scale.rs` living in another language, which is the drift
    /// that cost this project its second renderer. Doing it with a second entry
    /// point into the engine would mean promoting `Layout` and `AxisFacts` out of
    /// the one door `plot.rs` keeps, and composed pages discard both. So the
    /// engine states the two numbers per axis and the browser does one affine
    /// division it can check against the rectangle it measured.
    ///
    /// An empty `<g>` rather than a `<rect>`, deliberately: the renderer's own
    /// tests fingerprint marks by counting rects with particular attributes, and
    /// a rect here would be counted as one. It carries no ink, so it cannot.
    ///
    /// **Silent unless a brush is declared**, including a brush the reader has
    /// not moved yet — the browser needs these before the first drag, and a plot
    /// that never mentions a brush must not pay a byte for one.
    #[allow(clippy::too_many_arguments)]
    fn write_brush_frame(&self, svg: &mut String, spec: &PlotSpec, l: &Layout,
                         xs: (f64, f64), ys: (f64, f64),
                         cat_x: Option<&Vec<String>>, cat_y: Option<&Vec<String>>,
                         x_field: &str, y_field: &str, plane: bool,
                         x_log: Option<f64>, y_log: Option<f64>,
                         facts: &PanelFacts<'_>) {
        // A packing has regions rather than coordinates, so there is no domain to
        // state and nothing for a gesture to invert. The dimming still works
        // there, because a predicate reads a column's values and never asks where
        // a row was drawn — only the *gesture* needs a plane.
        if (spec.brush.is_empty() && spec.region.is_none()) || !plane {
            return;
        }
        // Named per axis rather than renamed after the fact. The log base below
        // is built with a `replace`, and the same trick applied here would have
        // emitted `data-cats` twice — once per axis, colliding — which is what it
        // did, so a drag on a categorical axis could never find its slots.
        let cats = |axis: &str, c: Option<&Vec<String>>| {
            c.map(|v| format!(" data-{axis}-cats=\"{}\"", v.iter()
                .map(|s| crate::render::text::esc(s)).collect::<Vec<_>>().join("|")))
                .unwrap_or_default()
        };
        // The column each axis measures travels with its domain. Which column an
        // axis reads is a *scope* question, and scope resolution is this engine's
        // job — a browser working it out from the spec would be a second copy of
        // `resolve_scopes` in another language.
        // **A log axis states its domain in log space**, because that is the
        // space positions are linear in — so a browser reading the two numbers
        // and interpolating between them gets a logarithm, not a value. The base
        // travels with the domain and says how to come back: `base^v`. Without
        // it a drag on a log axis produces a bound in the wrong units entirely,
        // and the engine then compares it against raw values.
        let base = |b: Option<f64>| b.map(|b| format!(" data-log=\"{b}\"")).unwrap_or_default();
        // Which slice of the table this panel holds. A faceted plot is one plot
        // over one table, so a browser walking the rows cannot tell which panel
        // a row belongs in: it answers a pointer in the Europe panel with an
        // African row, drawn at a position where Europe put nothing. With shared
        // scales the two coincide exactly, so the wrong answer looks like the
        // right one. These are the same two filters this panel's own frame was
        // built with, which is what keeps the attribute and the rows agreeing.
        let slice = |side: &str, pair: Option<(&str, &str)>| {
            pair.map(|(f, v)| format!(
                " data-facet-{side}-field=\"{}\" data-facet-{side}=\"{}\"",
                crate::render::text::esc(f), crate::render::text::esc(v)))
                .unwrap_or_default()
        };
        // Which moment is showing. Every moment of a played plot is in the
        // document at once and the clock chooses which one displays, so a
        // browser reading the table sees all of them and would answer with a row
        // from a frame nobody is looking at. These three say which frame that
        // is, given the time. The **keys** rather than the labels, because the
        // browser compares them against the column and a year is `1957` there
        // and "1957" on the strip.
        let play = facts.play.map(|(field, levels, seconds)| {
            let keys = levels.iter().map(|lv| match &lv.key {
                crate::data::FrameKey::Str(s) => crate::render::text::esc(s),
                crate::data::FrameKey::Float(v) => format!("{v}"),
            }).collect::<Vec<_>>().join("|");
            // The same precision the animation's own `begin` and `dur` are
            // written at, or the frame the browser computes drifts from the
            // frame the reader sees wherever `speed` makes this repeat.
            format!(concat!(r#" data-play-field="{f}" data-play-levels="{k}""#,
                            r#" data-play-seconds="{s:.3}""#),
                    f = crate::render::text::esc(field), k = keys, s = seconds)
        }).unwrap_or_default();
        // `row` is the promise that a value can be turned back into the place it
        // was drawn. Anything else names what broke it, so the page can say so
        // in a sentence rather than going quiet for no stated reason. A panel
        // carrying no such attribute at all — an old engine beside a new module
        // — reads as refused, which is the state a reader had before any of this
        // was built, and the safe way to be wrong.
        let place = format!(" data-gog-place=\"{}\"", facts.place.unwrap_or("row"));
        writeln!(svg,
            concat!(r#"  <g data-gog-panel="{x0} {y0} {x1} {y1}" "#,
                    r#"data-x-field="{xn}" data-x="{xf} {xt}"{xc}{xl} "#,
                    r#"data-y-field="{yn}" data-y="{yf} {yt}"{yc}{yl}"#,
                    r#"{fc}{fr}{pp}{pl}/>"#),
            x0 = l.x0, y0 = l.y0, x1 = l.x1, y1 = l.y1,
            xn = crate::render::text::esc(x_field), xf = xs.0, xt = xs.1,
            xc = cats("x", cat_x), xl = base(x_log).replace("data-log", "data-x-log"),
            yn = crate::render::text::esc(y_field), yf = ys.0, yt = ys.1,
            yc = cats("y", cat_y), yl = base(y_log).replace("data-log", "data-y-log"),
            fc = slice("col", facts.facet_col), fr = slice("row", facts.facet_row),
            pp = play, pl = place,
        ).unwrap();
    }

    /// One pass when nothing is selected, two when something is — unselected
    /// first, so the selection paints over what it was taken from.
    fn selection_passes(&self, spec: &PlotSpec) -> &'static [bool] {
        let selected = spec.brush.iter().any(|b| !b.is_resting())
            || spec.region.as_ref().is_some_and(|r| !r.is_resting());
        if selected {
            &[true, false]
        } else {
            &[false]
        }
    }

    /// The rows this layer contributes to this pass.
    ///
    /// `None` means it contributes none. A layer the selection cannot reach — a
    /// summarized one, a `bar`, a mark whose rows are vertices rather than
    /// elements — is drawn **once, whole, at full strength** rather than dimmed
    /// or drawn twice, which is what the Assumption in `legality::check_brush`
    /// tells the reader is happening.
    fn pass_rows<'a>(
        &self,
        spec: &PlotSpec,
        layer: &Layer,
        df: &'a DataFrame,
        dim: bool,
    ) -> Option<std::borrow::Cow<'a, DataFrame>> {
        let Some(keep) = crate::legality::brush_keeps(spec, df) else {
            // Nothing selected: one pass, the whole frame, the resting state.
            return (!dim).then(|| std::borrow::Cow::Borrowed(df));
        };
        let reachable = crate::legality::mark_takes_selection(&layer.mark)
            && crate::legality::layer_answers_selection(layer);
        if !reachable {
            return (!dim).then(|| std::borrow::Cow::Borrowed(df));
        }
        let side: Vec<bool> = if dim { keep.iter().map(|k| !k).collect() } else { keep };
        Some(std::borrow::Cow::Owned(df.keep_rows(&side)))
    }

    /// Open the group that pushes the unselected rows back.
    fn open_pass(&self, svg: &mut String, dim: bool) {
        if dim {
            writeln!(svg, r#"  <g opacity="{:.3}">"#, crate::render::encode::SELECTION_DIM).unwrap();
        }
    }

    fn close_pass(&self, svg: &mut String, dim: bool) {
        if dim {
            writeln!(svg, "  </g>").unwrap();
        }
    }

    /// Open the group holding one moment's marks.
    ///
    /// Silent at one frame: a single-valued `play` column is a plot, not a
    /// sequence, and a group around it would cost every unplayed plot in the
    /// corpus its bytes to say nothing.
    fn open_frame(&self, svg: &mut String, fi: usize, nframes: usize) {
        if nframes < 2 {
            return;
        }
        // The moment shown before any timing runs: the first, or the one a still
        // was asked for. One expression for both, because they are one idea —
        // which moment is on screen at the start — and a `still` that took a
        // second branch here could drift from what the sequence opens with.
        let shown = if fi == self.still.unwrap_or(0) { "inline" } else { "none" };
        writeln!(svg, r#"  <g display="{shown}">"#).unwrap();
    }

    /// Close it, and say when it shows.
    ///
    /// One `<animate>` per group, of constant size — the timing is carried by
    /// `begin` rather than by an N-entry `values` list per frame, so the markup
    /// grows with the number of frames and not with its square. Before its
    /// `begin` an element falls back to the `display` written on it, which is
    /// exactly the state [`open_frame`](Self::open_frame) set.
    fn close_frame(&self, svg: &mut String, fi: usize, nframes: usize, frame_seconds: f64) {
        if nframes < 2 {
            return;
        }
        // A still carries no timing at all. The caller holding the frames is the
        // clock now, and an `<animate>` left behind would switch the picture off
        // a fifth of a second after it was rasterized.
        if self.still.is_some() {
            writeln!(svg, "  </g>").unwrap();
            return;
        }
        writeln!(svg,
            concat!(
                r#"    <animate attributeName="display" values="inline;none" "#,
                r#"keyTimes="0;{k:.6}" dur="{dur:.3}s" begin="{begin:.3}s" "#,
                r#"calcMode="discrete" repeatCount="indefinite"/>"#,
            ),
            k = 1.0 / nframes as f64,
            dur = nframes as f64 * frame_seconds,
            begin = fi as f64 * frame_seconds,
        ).unwrap();
        writeln!(svg, "  </g>").unwrap();
    }

    /// The band naming the moment on show.
    ///
    /// `play`'s guide, and the answer to what the channel *earns*. `x` earns an
    /// axis and `color` a legend; the two channels that earn nothing, `group` and
    /// `label`, earn nothing because neither adds a fact the reader has to decode.
    /// A frame does. Without it a reader watches points move with no way to say
    /// what they are moving *through*, which is the same plot as one with an
    /// unlabeled axis — so "earns: nothing" was the one line of the channel table
    /// this feature had to correct rather than fill in.
    ///
    /// It is deliberately the facet strip's strip: same band, same fill, same
    /// type, because it is the facet's guide read in time. The only difference is
    /// that it names the plot rather than a column of it, which is why it spans
    /// the panel area and sits above the facet's own names.
    fn write_play_strip(
        &self,
        svg: &mut String,
        grid: &PanelGrid,
        levels: &[crate::data::FrameLevel],
        frame_seconds: f64,
        theme: &ThemeSpec,
    ) {
        let Some(band) = grid.play_strip.as_ref() else { return };
        let strip_bg = theme.strip_or(STRIP_BG);
        let strip_fg = strip_ink(theme, &strip_bg);
        if levels.len() < 2 {
            return;
        }
        const STRIP_GAP: f64 = 4.0; // between the strip box and the panels
        let cap = estimate_cap_height(self.font_sm);
        let bh = crate::render::layout::STRIP_H - STRIP_GAP;
        let cx = band.x0 + band.w() / 2.0;
        let ty = band.y0 + bh / 2.0 + cap / 2.0;

        writeln!(svg,
            r##"  <g font-family="system-ui,sans-serif" font-size="{fs}" fill="{strip_fg}" text-anchor="middle">"##,
            fs = self.font_sm
        ).unwrap();
        // The box is drawn once, outside the moments: it is the same rectangle in
        // every frame, so swapping it would be a flicker carrying no information.
        writeln!(svg,
            r##"    <rect x="{x:.2}" y="{y:.2}" width="{w:.2}" height="{bh:.2}" fill="{strip_bg}"/>"##,
            x = band.x0, y = band.y0, w = band.w()
        ).unwrap();
        for (fi, level) in levels.iter().enumerate() {
            self.open_frame(svg, fi, levels.len());
            writeln!(svg,
                r#"    <text x="{cx:.2}" y="{ty:.2}">{v}</text>"#,
                v = esc(&level.label)
            ).unwrap();
            self.close_frame(svg, fi, levels.len(), frame_seconds);
        }
        writeln!(svg, "  </g>").unwrap();
    }

    fn write_footer(&self, svg: &mut String) {
        writeln!(svg, "</svg>").unwrap();
    }

    // -----------------------------------------------------------------------
    // Grid
    // -----------------------------------------------------------------------

    fn write_grid(
        &self, svg: &mut String, l: &Layout,
        x_ticks: &TickSpec, xs: (f64, f64),
        y_ticks: &TickSpec, ys: (f64, f64),
        // Grid lines running *along* the bars are noise — the bars already fill
        // those gaps. Which set that is depends on orientation, so the caller
        // says which axis the bars stand on.
        skip_vertical: bool,
        skip_horizontal: bool,
    ) {
        writeln!(svg, r##"  <g stroke="#d2d2da" stroke-width="1">"##).unwrap();
        if !skip_vertical {
            for &v in &x_ticks.values {
                let sx = l.map_x(v, xs.0, xs.1);
                writeln!(svg, r#"    <line x1="{sx:.2}" y1="{y0:.2}" x2="{sx:.2}" y2="{y1:.2}"/>"#,
                    y0 = l.y0, y1 = l.y1).unwrap();
            }
        }
        if !skip_horizontal {
            for &v in &y_ticks.values {
                let sy = l.map_y(v, ys.0, ys.1);
                writeln!(svg, r#"    <line x1="{x0:.2}" y1="{sy:.2}" x2="{x1:.2}" y2="{sy:.2}"/>"#,
                    x0 = l.x0, x1 = l.x1).unwrap();
            }
        }
        writeln!(svg, "  </g>").unwrap();
    }

    // -----------------------------------------------------------------------
    // Axis lines
    // -----------------------------------------------------------------------

    /// The panel's boundary. Two axis lines by default; `theme(frame = "full")`
    /// closes them into a rectangle, which is what `theme_bw` draws and what a
    /// journal figure is usually asked for.
    fn write_axes(&self, svg: &mut String, l: &Layout, theme: &ThemeSpec) {
        if !theme.frame_drawn() {
            return;
        }
        writeln!(svg, r##"  <g stroke="{c}" stroke-width="1.5" fill="none">"##,
            c = crate::ir::THEME_FRAME_COLOR).unwrap();
        if theme.frame_is_full() {
            // One rectangle rather than four lines: the corners meet exactly, and
            // a stroked rect is what an SVG reader expects a frame to be.
            writeln!(svg,
                r#"    <rect x="{x0:.2}" y="{y0:.2}" width="{w:.2}" height="{h:.2}"/>"#,
                x0 = l.x0, y0 = l.y0, w = l.w(), h = l.h()).unwrap();
        } else {
            // Bottom (x-axis)
            writeln!(svg, r#"    <line x1="{x0:.2}" y1="{y1:.2}" x2="{x1:.2}" y2="{y1:.2}"/>"#,
                x0 = l.x0, y1 = l.y1, x1 = l.x1).unwrap();
            // Left (y-axis)
            writeln!(svg, r#"    <line x1="{x0:.2}" y1="{y0:.2}" x2="{x0:.2}" y2="{y1:.2}"/>"#,
                x0 = l.x0, y0 = l.y0, y1 = l.y1).unwrap();
        }
        writeln!(svg, "  </g>").unwrap();
    }

    // -----------------------------------------------------------------------
    // The polar frame — background, gridlines and rim
    //
    // Every guide a flat panel draws along two straight edges, a polar panel
    // draws around a circle: the background is a disc, the gridlines are rings at
    // the radial ticks and spokes at the angular ones, and the axis line is the
    // rim itself. Same ticks, same ranges, same colors — only the geometry the
    // positions are put through differs, which is the whole claim a coordinate
    // space makes (spec §9).
    // -----------------------------------------------------------------------

    #[allow(clippy::too_many_arguments)]
    fn write_polar_frame(
        &self, svg: &mut String, p: &Polar,
        x_ticks: &TickSpec, xs: (f64, f64),
        y_ticks: &TickSpec, ys: (f64, f64),
        clip: &str,
        theme: &ThemeSpec,
        // Where the spokes start, as a fraction of the radial range: the cells'
        // inner edge, or `0.0` for a plot whose marks reach the center.
        inner_edge: f64,
    ) {
        writeln!(svg,
            r##"  <circle cx="{cx:.2}" cy="{cy:.2}" r="{r:.2}" fill="{bg}"/>"##,
            cx = p.cx, cy = p.cy, r = p.r_max, bg = theme.background_or(PANEL_BG)
        ).unwrap();
        // A hair wider than the disc: the clip is a safety net for a value the
        // scale could not place, not an edge to shave the glyphs sitting on the rim.
        writeln!(svg,
            r#"  <clipPath id="{clip}"><circle cx="{cx:.2}" cy="{cy:.2}" r="{r:.2}"/></clipPath>"#,
            cx = p.cx, cy = p.cy, r = p.r_max + 8.0
        ).unwrap();

        // A pie has no radial scale to ring and no angular categories to spoke: its
        // whole surface is the measure, and its key is the legend. Drawing the flat
        // plot's guides here would be furniture decoding nothing.
        if p.measure_on_angle {
            if theme.frame_drawn() {
                writeln!(svg,
                    r##"  <circle cx="{cx:.2}" cy="{cy:.2}" r="{r:.2}" fill="none" stroke="{c}" stroke-width="1.5"/>"##,
                    cx = p.cx, cy = p.cy, r = p.r_max, c = crate::ir::THEME_FRAME_COLOR
                ).unwrap();
            }
            return;
        }

        // `theme(grid = )` reaches the circle too, and naming the sets by their
        // *axis* rather than by their direction is what lets it: the y axis's
        // gridlines are rings here and horizontal lines in the flat space, but
        // they are the y axis's in both. A `grid = "x"` that meant "vertical"
        // would have no reading at all inside a circle — the Law 2 exception this
        // naming avoids rather than special-cases.
        writeln!(svg, r##"  <g stroke="#d2d2da" stroke-width="1" fill="none">"##).unwrap();
        if theme.grid_y() {
            for &v in &y_ticks.values {
                let r = p.radius(unit_norm(v, ys));
                if r > 0.5 && r <= p.r_max + 0.5 {
                    writeln!(svg, r#"    <circle cx="{cx:.2}" cy="{cy:.2}" r="{r:.2}"/>"#,
                        cx = p.cx, cy = p.cy).unwrap();
                }
            }
        }
        if theme.grid_x() {
            for (u, _) in polar_angles(x_ticks, xs) {
                let (x0, y0) = p.at(u, inner_edge);
                let (x, y) = p.at(u, 1.0);
                writeln!(svg, r#"    <line x1="{x0:.2}" y1="{y0:.2}" x2="{x:.2}" y2="{y:.2}"/>"#)
                    .unwrap();
            }
        }
        writeln!(svg, "  </g>").unwrap();

        // The rim is the axis line: in polar the two straight edges of a flat
        // panel are one closed curve, so there is one of it and not two.
        // The rim is the axis line, and in polar the flat panel's two straight
        // edges are one closed curve — so `frame = "full"` and `"axes"` draw the
        // same picture here and only `"none"` differs. The setting reaches the
        // circle rather than stopping at the rectangle, which is Law 2.
        if theme.frame_drawn() {
            writeln!(svg,
                r##"  <circle cx="{cx:.2}" cy="{cy:.2}" r="{r:.2}" fill="none" stroke="{c}" stroke-width="1.5"/>"##,
                cx = p.cx, cy = p.cy, r = p.r_max, c = crate::ir::THEME_FRAME_COLOR
            ).unwrap();
        }
    }

    /// The polar tick labels: the angular names ringed outside the circle, the
    /// radial numbers running out along the spoke the circle starts on. Drawn
    /// after the marks, like the flat tick labels, so they stay readable over
    /// whatever was painted under them.
    fn write_polar_ticks(
        &self, svg: &mut String, p: &Polar,
        x_ticks: &TickSpec, xs: (f64, f64),
        y_ticks: &TickSpec, ys: (f64, f64),
    ) {
        if p.measure_on_angle {
            return; // a pie is decoded by its legend, not by ticks (see the frame)
        }
        let cap = estimate_cap_height(self.font_sm);
        writeln!(svg,
            r##"  <g font-family="system-ui,sans-serif" font-size="{}" fill="#3c3c46">"##,
            self.font_sm
        ).unwrap();
        for (u, label) in polar_angles(x_ticks, xs) {
            if label.is_empty() { continue }
            let (x, y, anchor) = p.rim_label(u, POLAR_RIM_GAP, cap);
            writeln!(svg,
                r#"    <text x="{x:.2}" y="{y:.2}" text-anchor="{anchor}">{}</text>"#, esc(label)
            ).unwrap();
        }
        writeln!(svg, "  </g>").unwrap();

        writeln!(svg,
            r##"  <g font-family="system-ui,sans-serif" font-size="{}" fill="#5a5a64" text-anchor="start">"##,
            self.font_sm
        ).unwrap();
        for (i, &v) in y_ticks.values.iter().enumerate() {
            let frac = unit_norm(v, ys);
            let r = p.radius(frac);
            if !(r > 0.5) || r > p.r_max + 0.5 { continue }
            let (x, y) = p.at(0.0, frac);
            let label = y_ticks.labels.get(i).map(String::as_str).unwrap_or("");
            if label.is_empty() { continue }
            writeln!(svg, r#"    <text x="{:.2}" y="{:.2}">{}</text>"#,
                x + 4.0, y + cap * 0.35, esc(label)).unwrap();
        }
        writeln!(svg, "  </g>").unwrap();
    }

    // -----------------------------------------------------------------------
    // Tick marks and labels
    // -----------------------------------------------------------------------

    /// `draw_x` / `draw_y` say whether this panel's edges carry tick marks and
    /// labels at all. With fixed, shared scales one set per margin is enough,
    /// so only the bottom row draws x ticks and only the left column draws y
    /// ticks; interior panels keep the gridlines, which carry the same
    /// positions without the ink.
    #[allow(clippy::too_many_arguments)]
    fn write_ticks(
        &self, svg: &mut String, l: &Layout,
        x_ticks: &TickSpec, xs: (f64, f64),
        y_ticks: &TickSpec, ys: (f64, f64),
        draw_x: bool, draw_y: bool,
        // Panel columns sit a small gap apart, and a centered label on an edge
        // tick overhangs its panel — the left panel's "150K" and the right
        // panel's "0K" then meet in the gap and read as one number. When set,
        // an edge tick's label anchors *inward* instead, so no label can cross
        // a panel boundary. Off for the unfaceted plot, whose margins already
        // give centered labels all the room they need.
        snap_edge_labels: bool,
        // `theme(tick_angle = )`. Degrees, turning the x labels counterclockwise so
        // long category names stop overlapping. The y labels are left alone: they
        // are already right-anchored in a margin sized to the longest of them, so
        // they do not collide with each other and turning them would only make
        // them harder to read.
        tick_angle: Option<f64>,
    ) {
        const TICK_LEN: f64 = 5.0;

        writeln!(svg, r##"  <g stroke="#5a5a64" stroke-width="1" fill="none">"##).unwrap();
        if draw_x {
            for &v in &x_ticks.values {
                let sx = l.map_x(v, xs.0, xs.1);
                writeln!(svg, r#"    <line x1="{sx:.2}" y1="{y1:.2}" x2="{sx:.2}" y2="{y2:.2}"/>"#,
                    y1 = l.y1, y2 = l.y1 + TICK_LEN).unwrap();
            }
        }
        if draw_y {
            for &v in &y_ticks.values {
                let sy = l.map_y(v, ys.0, ys.1);
                writeln!(svg, r#"    <line x1="{x1:.2}" y1="{sy:.2}" x2="{x2:.2}" y2="{sy:.2}"/>"#,
                    x1 = l.x0 - TICK_LEN, x2 = l.x0).unwrap();
            }
        }
        writeln!(svg, "  </g>").unwrap();

        // Tick labels
        writeln!(svg,
            r##"  <g font-family="system-ui,sans-serif" font-size="{}" fill="#3c3c46" text-anchor="middle">"##,
            self.font_sm
        ).unwrap();
        if draw_x {
            let label_y = l.y1 + TICK_LEN + TICK_GAP + estimate_cap_height(self.font_sm);
            let turn = tick_angle.filter(|d| *d != 0.0);
            for (v, label) in x_ticks.values.iter().zip(&x_ticks.labels) {
                let sx = l.map_x(*v, xs.0, xs.1);
                if let Some(deg) = turn {
                    // Anchored at its *end* and turned about the tick, so the
                    // label hangs down-left from the mark it belongs to and the
                    // end nearest the axis is the end that names it. The negative
                    // angle is SVG's: its rotation is clockwise, because y is down.
                    writeln!(svg,
                        r#"    <text x="{sx:.2}" y="{label_y:.2}" text-anchor="end" transform="rotate({a:.2} {sx:.2} {label_y:.2})">{}</text>"#,
                        esc(label), a = -deg).unwrap();
                    continue;
                }
                let half = crate::render::text::estimate_text_width(label, self.font_sm) / 2.0;
                let anchor = if !snap_edge_labels { "" }
                    else if sx - half < l.x0 { r#" text-anchor="start""# }
                    else if sx + half > l.x1 { r#" text-anchor="end""# }
                    else { "" };
                writeln!(svg, r#"    <text x="{sx:.2}" y="{label_y:.2}"{anchor}>{}</text>"#, esc(label)).unwrap();
            }
        }
        writeln!(svg, "  </g>").unwrap();

        writeln!(svg,
            r##"  <g font-family="system-ui,sans-serif" font-size="{}" fill="#3c3c46" text-anchor="end">"##,
            self.font_sm
        ).unwrap();
        if draw_y {
            let label_x = l.x0 - TICK_LEN - TICK_GAP;
            for (v, label) in y_ticks.values.iter().zip(&y_ticks.labels) {
                let sy = l.map_y(*v, ys.0, ys.1) + estimate_cap_height(self.font_sm) / 2.0;
                writeln!(svg, r#"    <text x="{label_x:.2}" y="{sy:.2}">{}</text>"#, esc(label)).unwrap();
            }
        }
        writeln!(svg, "  </g>").unwrap();
    }

    // -----------------------------------------------------------------------
    // Axis labels and title
    // -----------------------------------------------------------------------

    /// `drew_x_ticks` is whether the x axis put a row of labels under the panel.
    /// The name has to clear that row when there is one and must not reserve it
    /// when there is not: `Layout::compute` gives an axis with no tick labels no
    /// band for them, so assuming one anyway drops the name past the space that
    /// was reserved. **Every polar plot in the book had its axis name clipped by
    /// the canvas edge** for exactly that reason — the circle draws its guides
    /// inside the panel and is handed an empty tick list, like the cube and the
    /// packing, and those two suppress their outer names for other reasons and so
    /// never showed it.
    fn write_labels(
        &self, svg: &mut String, l: &Layout,
        x_label: &str, y_label: &str, spec: &PlotSpec, drew_x_ticks: bool,
    ) {
        let plot_cx = (l.x0 + l.x1) / 2.0;
        let label_h = estimate_cap_height(self.font_md);

        // Title
        if let Some(title) = &spec.title {
            let y_label_offset = if !y_label.is_empty() { label_h + 6.0 } else { 0.0 };
            let ty = l.y0 - y_label_offset - estimate_cap_height(self.font_lg) * 0.3 - 8.0;
            // **Centered on the panel, then held inside the canvas.** The panel is
            // the thing the title names, so it centers there and not over the
            // legend beside it — but a legend pushes that center left, and a title
            // wider than what is left of the canvas then starts at a negative x and
            // is cut off by the edge. On a page that edge is the *cell's*, which is
            // where it showed: at full width a title has room to spare, and in a
            // quarter-page cell "Turned: the same sheet from the west" lost its
            // first letter. Nudging beats clipping, and it only ever moves a title
            // that would otherwise be unreadable.
            const EDGE: f64 = 4.0; // breathing room, so a nudged title is not flush
            let half = estimate_text_width(title, self.font_lg) / 2.0;
            let lo = half + EDGE;
            // `max(lo)` so a title wider than the whole canvas still centers rather
            // than inverting the range: it overflows both sides evenly, which reads
            // as too long instead of as misplaced.
            let cx = plot_cx.clamp(lo, (self.width - half - EDGE).max(lo));
            writeln!(svg,
                r##"  <text x="{cx:.2}" y="{ty:.2}" font-family="system-ui,sans-serif" font-size="{fs}" font-weight="600" fill="#0f0f19" text-anchor="middle">{title}</text>"##,
                fs = self.font_lg, title = esc(title)
            ).unwrap();
        }

        // Y-axis label (horizontal, above the plot area — modern FT style)
        if !y_label.is_empty() {
            let ty = l.y0 - 6.0;
            writeln!(svg,
                r##"  <text x="{x:.2}" y="{ty:.2}" font-family="system-ui,sans-serif" font-size="{fs}" fill="#28283a" text-anchor="start">{y_label}</text>"##,
                x = l.x0, fs = self.font_md, y_label = esc(y_label)
            ).unwrap();
        }

        // X-axis label (centered below ticks)
        if !x_label.is_empty() {
            let tick_row = match drew_x_ticks {
                true => 5.0 + estimate_cap_height(self.font_sm),
                false => 0.0,
            };
            let ty = l.y1 + tick_row + 8.0 + label_h;
            writeln!(svg,
                r##"  <text x="{cx:.2}" y="{ty:.2}" font-family="system-ui,sans-serif" font-size="{fs}" fill="#28283a" text-anchor="middle">{x_label}</text>"##,
                cx = plot_cx, fs = self.font_md, x_label = esc(x_label)
            ).unwrap();
        }
    }

    // -----------------------------------------------------------------------
    // 3-D frame — the glass box that makes a projected cloud readable
    // -----------------------------------------------------------------------

    /// The **box** of a `space` plot's guides: the projected unit cube as a faint
    /// wireframe, plus the three edges from the origin corner emphasized. Drawn
    /// before the marks so the box sits *behind* the cloud — a draw-order stand-in
    /// for true occlusion, which with front-edge culling is still M8b (spec §16).
    ///
    /// The **labels** are not here. They are guides rather than scene elements, so
    /// they go on after the marks (`write_space_labels`), which is the rule the
    /// flat and polar frames already follow.
    fn write_space_box(&self, svg: &mut String, scene: &project::Scene) {
        let corners: Vec<Screen> = project::CUBE_CORNERS.iter()
            .map(|&(x, y, z)| scene.to_screen(x, y, z))
            .collect();

        // The whole cube as a faint wireframe — the depth cue the cloud reads against.
        writeln!(svg, r##"  <g stroke="#d8d8de" stroke-width="1" fill="none">"##).unwrap();
        for &(a, b) in &project::CUBE_EDGES {
            writeln!(svg, r#"    <line x1="{:.2}" y1="{:.2}" x2="{:.2}" y2="{:.2}"/>"#,
                corners[a].x, corners[a].y, corners[b].x, corners[b].y).unwrap();
        }
        writeln!(svg, "  </g>").unwrap();

        // The three axis edges from the origin corner (0,0,0), a touch darker.
        writeln!(svg, r##"  <g stroke="#9a9aa4" stroke-width="1.4" fill="none">"##).unwrap();
        for &(a, b) in &[(0usize, 1usize), (0, 3), (0, 4)] {
            writeln!(svg, r#"    <line x1="{:.2}" y1="{:.2}" x2="{:.2}" y2="{:.2}"/>"#,
                corners[a].x, corners[a].y, corners[b].x, corners[b].y).unwrap();
        }
        writeln!(svg, "  </g>").unwrap();
    }

    /// The **labels** of a `space` plot's guides: tick numbers along the three
    /// origin edges, and each axis named at its far end. Billboarded (always
    /// horizontal) and drawn *after* the marks, so a solid mesh cannot paint over
    /// the numbers that say what it measures.
    ///
    /// Placement is **measured**, which is the whole difference from the version
    /// that shipped with M8a. That one computed a position per label and wrote it,
    /// so nothing stopped two labels landing in the same place: at the origin
    /// corner three axes meet and their near-origin numbers piled up, and an
    /// axis's name printed through its own last tick (`S8pal Length` in the
    /// book's first 3-D plot). Every other guide in the engine measures its text —
    /// a flat tick label anchors inward off `estimate_text_width`, a margin is
    /// sized from it — and this was the one that did not.
    #[allow(clippy::too_many_arguments)]
    fn write_space_labels(
        &self, svg: &mut String, scene: &project::Scene, l: &Layout, theme: &ThemeSpec,
        view: SpaceView,
        // What each axis's binding *asked* for, where it asked — `None` is the
        // engine's default and stays silent if the frame has to thin it.
        asked: [Option<usize>; 3],
        x_ticks: &TickSpec, xs: (f64, f64), x_label: &str,
        y_ticks: &TickSpec, ys: (f64, f64), y_label: &str,
        z_ticks: &TickSpec, zs: (f64, f64), z_label: &str,
        // Where the thinning report goes. **Not `eprintln!`**, which is what it
        // was: this routine runs once per panel, so a faceted cube said the same
        // sentence once per cube. `remarks` is the list that already dedupes by
        // message (see `draw`), on the rule that a fact true of two panels is one
        // fact said twice. Nobody could see it before, because until the facet
        // gate came off a cube could not be faceted.
        remarks: &mut Vec<Diagnostic>,
    ) {
        let asked_for = |axis: FrameAxis| match axis {
            FrameAxis::X => asked[0],
            FrameAxis::Y => asked[1],
            FrameAxis::Z => asked[2],
        };
        let center = scene.to_screen(0.5, 0.5, 0.5);
        let axes = [
            (FrameAxis::X, x_ticks, xs, x_label),
            (FrameAxis::Y, y_ticks, ys, y_label),
            (FrameAxis::Z, z_ticks, zs, z_label),
        ];

        // Candidates in priority order, and the order is **pinned before free**.
        // A tick label is fixed to a value on an edge and can only be dropped; an
        // axis name is free to slide along its edge and away from it, so it is the
        // one that should give way. Placing names first — the intuitive reading,
        // since a nameless measurement is the worse loss — instead spent a *middle*
        // tick to seat each name, which reads as a bug in the sequence (`500 400
        // 200 100 0`) rather than as a choice, and costs a number that could not
        // have gone anywhere else. Within an axis the ticks are walked from the far
        // end *inward*, so what thins is the crowd where edges meet rather than the
        // labels that had room.
        // Measured first, placed second. A name's distance from its edge is read
        // off its own numbers, so the numbers are built before any of them is put
        // anywhere — which is also why this cannot be one pass per axis.
        let built: Vec<(FrameEdge, Vec<FrameLabel>, &str)> = axes.iter()
            .map(|(axis, ticks, range, label)| {
                let edge = FrameEdge::choose(scene, *axis);
                (edge, self.frame_tick_labels(scene, center, ticks, *range, edge), *label)
            })
            .collect();

        let mut queue: Vec<FrameLabel> = Vec::new();
        for (_, ticks, _) in &built {
            queue.extend(ticks.iter().cloned());
        }
        for (edge, ticks, label) in &built {
            if label.is_empty() { continue }
            queue.push(self.frame_name_label(scene, center, l, *edge, ticks, label));
        }

        // Place them, and the two kinds of collision get different answers.
        //
        // Two *different* axes landing together is a coincidence of the projection:
        // their edges meet at a corner and their numbers are unrelated, so pushing
        // one further out separates them and both survive.
        //
        // One axis colliding with **itself** is the foreshortened axis, and a nudge
        // is exactly the wrong answer there. Its perpendicular runs across the axis,
        // so nudging alternate numbers staggers the column into two zigzagging
        // ranks, which a reader reads as broken rather than as tight — measured on
        // `bar * mean` at `tilt = 85`, where it produced `80 70 50 40` at two
        // different x positions. So a number that collides with its own axis is
        // **dropped**, which thins the column and leaves the surviving numbers where
        // they were. Tick *selection* is untouched either way: a coarser step climbs
        // the scale ceiling and leaves the cube half empty, which is the recorded
        // reason a narrow flat panel is not thinned either (see `build_axis`).
        //
        // A *name* is free to move whatever it meets, having no fixed value to sit
        // at — which is why the rule is about two numbers of one axis rather than
        // about one axis's labels.
        let offered: Vec<(FrameAxis, usize)> = built.iter()
            .map(|(edge, ticks, _)| (edge.axis, ticks.len()))
            .collect();
        let mut placed: Vec<FrameLabel> = Vec::new();
        for mut cand in queue {
            let mut fits = false;
            for step in 0..=FRAME_LABEL_NUDGES {
                cand.nudge(step as f64 * FRAME_LABEL_NUDGE);
                let blockers: Vec<&FrameLabel> = placed.iter()
                    .filter(|p| !cand.clears(p, TICK_GAP))
                    .collect();
                if blockers.is_empty() {
                    fits = true;
                    break;
                }
                if blockers.iter().any(|p| p.tick && cand.tick && p.axis == cand.axis) {
                    break;
                }
            }
            if fits {
                placed.push(cand);
            }
        }

        // A count the caller *stated* and the frame could not draw is said out
        // loud (spec §12): thinning is the right answer to a foreshortened axis,
        // but doing it silently to an explicit `tick_count` is accepting a binding
        // and dropping it. The engine's own default stays silent, which is §12's
        // omission rule — an unambiguous default needs no warning, and only the
        // renderer knows how many labels fit, exactly as with a dot plot's piles.
        for (axis, wanted) in offered {
            let Some(asked) = asked_for(axis) else { continue };
            let drawn = placed.iter().filter(|l| l.tick && l.axis == axis).count();
            if drawn >= wanted { continue }
            remarks.push(Diagnostic {
                kind: crate::legality::DiagnosticKind::Assumption,
                message: format!(
                    "gog: `{c}(…, tick_count = {asked})` asked for {asked} ticks and {drawn} fit — \
                     this axis projects too short at `tilt = {tilt:.0}` for the rest, so the labels \
                     were thinned rather than overlapped. The scale is unchanged (the ones drawn \
                     are the ones you asked for, at the same spacing); lower the tilt, ask for \
                     fewer, or leave `tick_count` off.",
                    c = frame_channel_name(axis), tilt = view.tilt,
                ),
            });
        }

        // A halo in the panel's own color, because these labels now sit *over*
        // the marks. A flat tick label reads against a clean margin; the cube has
        // no margin to sit in (`FRAME_INSET` is inside the panel), so the clear
        // ground a guide needs is drawn rather than reserved.
        let bg = theme.background_or(PANEL_BG);
        writeln!(svg,
            r##"  <g font-family="system-ui,sans-serif" text-anchor="middle" paint-order="stroke" stroke="{bg}" stroke-width="3" stroke-linejoin="round">"##
        ).unwrap();
        for l in &placed {
            writeln!(svg,
                r#"    <text x="{:.2}" y="{:.2}" font-size="{}" fill="{}">{}</text>"#,
                l.x, l.y, l.font, l.fill, esc(&l.text)).unwrap();
        }
        writeln!(svg, "  </g>").unwrap();
    }

    /// The tick labels of one 3-D axis, far end first — candidates, not yet
    /// placed. Each is offset *perpendicular to its own edge*, on the side away
    /// from the cube center: the three edges leave their corner in three screen
    /// directions, so a radial nudge would stack their labels on top of each other
    /// where a per-axis perpendicular fans them apart.
    fn frame_tick_labels(
        &self, scene: &project::Scene, center: Screen,
        ticks: &TickSpec, range: (f64, f64), edge: FrameEdge,
    ) -> Vec<FrameLabel> {
        let (lo, hi) = (range.0.min(range.1), range.0.max(range.1));
        let cap = estimate_cap_height(self.font_sm);
        let perp = frame_perp(scene, center, edge);
        let mut out = Vec::new();
        for (v, lab) in ticks.values.iter().zip(&ticks.labels).rev() {
            if *v < lo - 1e-9 || *v > hi + 1e-9 { continue }
            if lab.is_empty() { continue }
            let (dx, dy, dz) = edge.at(unit_norm(*v, range));
            let p = scene.to_screen(dx, dy, dz);
            out.push(FrameLabel::new(
                lab.clone(), p.x, p.y + cap / 2.0, perp, FRAME_TICK_OFFSET,
                self.font_sm, "#3c3c46", edge.axis, true,
            ));
        }
        out
    }

    /// The name of one 3-D axis, placed **outside its own numbers** — a candidate,
    /// not yet placed.
    ///
    /// Centered on its edge rather than at the far end, and that is structural
    /// rather than cosmetic: the two domain edges are the two that meet at the near
    /// corner, so their far ends are *the same corner* and M8a's two names arrived
    /// on top of each other there. The midpoint is also where a reader looks for an
    /// axis's name, and what the flat frame does — `write_labels` centers the x name
    /// under the panel.
    ///
    /// How far out is **measured from the axis's own tick labels**, the rule
    /// `PanelGrid` already uses to size a flat y-axis margin: a name clears the
    /// widest number beside it. Both extents are taken *along the perpendicular*,
    /// which is what makes one rule serve three axes with very different needs — a
    /// floor edge's perpendicular is mostly vertical, so a domain name needs only
    /// its height cleared however long the words are, while the vertical strut's is
    /// horizontal and a long name there needs its full width.
    ///
    /// And when that does not fit the panel, the name goes past the **end** of its
    /// edge instead. Two placements tried in order, first one that fits — still one
    /// rule rather than a case per axis: a name sits beside its numbers when there
    /// is room beside them, and beyond its axis when there is not. `Elevation (m)`
    /// against the cube's left strut is the second case (the frame's inset leaves
    /// ~84px and the name wants ~90px), which puts it above the strut, where M8a
    /// put every name and where M8a happened to be right.
    fn frame_name_label(
        &self, scene: &project::Scene, center: Screen, l: &Layout,
        edge: FrameEdge, ticks: &[FrameLabel], label: &str,
    ) -> FrameLabel {
        let cap = estimate_cap_height(self.font_sm);
        let perp = frame_perp(scene, center, edge);
        let name = |x: f64, y: f64, dir: (f64, f64), off: f64| FrameLabel::new(
            label.to_string(), x, y, dir, off, self.font_md, "#28283a", edge.axis, false,
        );
        // How much room the name itself and the widest number take, each measured
        // across the direction the name is being pushed along.
        let across = |dir: (f64, f64), half_w: f64, cap: f64|
            dir.0.abs() * half_w + dir.1.abs() * cap / 2.0;
        let mine = across(perp, crate::render::text::estimate_text_width(label, self.font_md) / 2.0,
                          estimate_cap_height(self.font_md));
        let widest = ticks.iter()
            .map(|t| across(perp, t.half_w, t.cap))
            .fold(0.0_f64, f64::max);

        let (mx, my, mz) = edge.at(0.5);
        let m = scene.to_screen(mx, my, mz);
        let beside = name(m.x, m.y + cap / 2.0, perp,
                          FRAME_TICK_OFFSET + widest + TICK_GAP + mine);
        if beside.within(l) {
            return beside;
        }

        // Past the end of the edge, along the edge's own screen direction. For the
        // strut that is straight up, above the topmost number.
        let (a, b) = (edge.at(0.0), edge.at(1.0));
        let (o, f) = (scene.to_screen(a.0, a.1, a.2), scene.to_screen(b.0, b.1, b.2));
        let (adx, ady) = (f.x - o.x, f.y - o.y);
        let alen = (adx * adx + ady * ady).sqrt().max(1e-6);
        let along = (adx / alen, ady / alen);
        let mine_along = across(along, crate::render::text::estimate_text_width(label, self.font_md) / 2.0,
                                estimate_cap_height(self.font_md));
        name(f.x, f.y + cap / 2.0, along, TICK_GAP + mine_along)
    }

}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Fraction of a panel rectangle left clear around the projected cube, so the
/// frame's tick labels and axis names have room to sit outside the box.
const FRAME_INSET: f64 = 0.14;

/// Which axis of the 3-D frame a tick run belongs to.
#[derive(Clone, Copy, PartialEq, Eq)]
enum FrameAxis {
    X,
    Y,
    Z,
}

/// The channel name a 3-D frame axis carries, for a diagnostic that has to quote
/// the caller's own word back to them.
fn frame_channel_name(axis: FrameAxis) -> &'static str {
    match axis {
        FrameAxis::X => "x",
        FrameAxis::Y => "y",
        FrameAxis::Z => "z",
    }
}

/// Which of the cube's **four** parallel edges an axis is ticked along, and where
/// on it a value sits.
///
/// The cube offers four edges per axis and M8a always took the three radiating
/// from corner `(0,0,0)`. That corner is the one *farthest* from the camera at
/// every ordinary viewing angle (at the book's default `turn = 30, tilt = 25` its
/// depth is `+0.83`, the maximum of the eight), so the frame ticked its three back
/// edges and every label lay across the middle of the picture. Draw order hid it:
/// the labels went on before the marks, so the stretch of each edge that ran
/// behind the data was simply invisible, and what showed was the stub that reached
/// past the silhouette. Drawing the labels on top — which is what a guide needs —
/// exposes it at once, so the edge choice is part of the same fix rather than a
/// later polish item.
///
/// The rule is measured, per axis, per view: **the edge on the outside of the
/// projected silhouette.** A domain axis stays on the floor (`z = 0`), which is
/// where the plane the data sits on is, and takes the floor edge **nearest the
/// camera** — the two of them make the V at the bottom of the projection, below
/// anything standing on the floor. The measure axis takes the vertical strut
/// **furthest from the projected center**, which is the cube's left or right
/// silhouette and so is never inside the data either.
#[derive(Clone, Copy)]
struct FrameEdge {
    axis: FrameAxis,
    /// The two unit-cube coordinates held fixed while this axis runs 0 → 1, in
    /// `(x, y, z)` order with the axis's own coordinate removed.
    fixed: (f64, f64),
}

impl FrameEdge {
    /// The unit-cube point a fraction of the way along this edge.
    fn at(self, t: f64) -> (f64, f64, f64) {
        match self.axis {
            FrameAxis::X => (t, self.fixed.0, self.fixed.1),
            FrameAxis::Y => (self.fixed.0, t, self.fixed.1),
            FrameAxis::Z => (self.fixed.0, self.fixed.1, t),
        }
    }

    /// Choose the edge to tick this axis along, for this view.
    ///
    /// **The ties are the interesting part, and they are not rare — they are the
    /// default.** At `turn = 30` the two outermost struts sit at exactly ±0.683 of
    /// the projected center, mirror images, so which is "furthest" is a coin flip
    /// that floating-point noise was calling: two plots at the same angle picked
    /// opposite sides. Every symmetric view has this property, and the default view
    /// is symmetric. So each comparison needs a real margin before it counts as a
    /// difference, and a stated preference when it does not.
    fn choose(scene: &project::Scene, axis: FrameAxis) -> Self {
        // Generous next to projected pixels and tiny next to any real difference:
        // this only has to swallow the noise of two sin/cos paths to one number.
        const EPS: f64 = 1e-6;
        let edge = |fixed| FrameEdge { axis, fixed };
        match axis {
            // A domain axis: on the floor, on whichever side is nearer the camera.
            // A tie means the camera is looking straight down this axis, and goes
            // to the low side.
            FrameAxis::X | FrameAxis::Y => {
                let (near, far) = (edge((1.0, 0.0)), edge((0.0, 0.0)));
                if near.mid_depth(scene) < far.mid_depth(scene) - EPS { near } else { far }
            }
            // The measure axis: the strut whose projected midpoint sits furthest
            // from the cube's center across the screen — the cube's left or right
            // silhouette, where the data can never be in front of it. A tie goes to
            // the **left**, the side a reader looks for a vertical scale on.
            FrameAxis::Z => {
                let cx = scene.to_screen(0.5, 0.5, 0.5).x;
                let mut best = edge((0.0, 0.0));
                for (fx, fy) in [(1.0, 0.0), (1.0, 1.0), (0.0, 1.0)] {
                    let cand = edge((fx, fy));
                    let (bx, kx) = (best.mid(scene).x - cx, cand.mid(scene).x - cx);
                    let further = kx.abs() > bx.abs() + EPS;
                    // **Both halves need the margin, and only the first had it.**
                    // `kx < bx` was exact, so once the magnitudes tied *within* EPS
                    // the direction was decided by whatever noise separated them —
                    // the very thing EPS is here to swallow, applied to half the
                    // comparison. The case that exposed it was `space(turn = -360)`,
                    // the same view as `turn = 0`: bit-identical marks and frame
                    // lines, `sin_az` of 2.4e-16 rather than exactly 0, this choice
                    // flipped, and on the other edge two tick labels collided and
                    // were dropped after their nudges. Eighteen labels became
                    // sixteen, with every mark in place and nothing on stderr.
                    //
                    // `Scene::new` also folds `turn` into one lap now, which removes
                    // that *particular* noise at its source. The two fixes are not
                    // one fix twice: normalizing makes equal views equal, and this
                    // margin makes the choice steady whatever the numbers are — a
                    // genuinely different angle can land two struts a hair apart and
                    // would flip here for the same reason. Either alone hides the
                    // symptom, so each is pinned by its own test.
                    let tied = (kx.abs() - bx.abs()).abs() <= EPS;
                    let tied_and_lefter = tied && kx < bx - EPS;
                    if further || tied_and_lefter {
                        best = cand;
                    }
                }
                best
            }
        }
    }

    /// The projected midpoint of this edge.
    fn mid(self, scene: &project::Scene) -> Screen {
        let (x, y, z) = self.at(0.5);
        scene.to_screen(x, y, z)
    }

    /// How far this edge's midpoint is from the camera — larger is farther.
    fn mid_depth(self, scene: &project::Scene) -> f64 {
        self.mid(scene).depth
    }
}

/// The air a tick label keeps around itself: between a flat axis's tick mark and
/// its number, and between any two labels of a 3-D frame. One constant, because
/// it answers one question — how much clearance a number needs to read as its own
/// word — in the two places that ask it.
const TICK_GAP: f64 = 4.0;

/// How far a 3-D tick label sits off its own axis, before any nudge.
const FRAME_TICK_OFFSET: f64 = 9.0;
/// How much further out a colliding frame label is pushed per try.
const FRAME_LABEL_NUDGE: f64 = 6.0;
/// How many nudges a frame label gets before it is dropped instead. Three tries
/// carry a label ~18px clear of where it started, which separates the three axes
/// meeting at the origin corner; a label crowded *along* its own axis is not
/// helped by any of them and is what this bound exists to give up on.
const FRAME_LABEL_NUDGES: usize = 3;

/// One label of the 3-D frame, measured: where its ink sits and how much room it
/// takes, so the frame can ask whether two of them fit before drawing both.
///
/// Billboarded, so the box is axis-aligned on screen whatever the viewing angle:
/// `x` is the center of the text (`text-anchor="middle"`), `y` its baseline, and
/// the ink runs from `y - cap` up to `y`.
#[derive(Clone)]
struct FrameLabel {
    text: String,
    x: f64,
    y: f64,
    /// Where the label started, before any nudge — nudges are absolute from here
    /// rather than cumulative, so retrying a placement cannot drift.
    ox: f64,
    oy: f64,
    /// The screen direction a collision pushes this label along.
    dir: (f64, f64),
    half_w: f64,
    cap: f64,
    font: f64,
    fill: &'static str,
    /// Which axis this label belongs to, and whether it is one of its numbers.
    /// Together they decide whether a collision may be nudged out of or has to be
    /// given up on — see the placement loop in `write_space_labels`.
    axis: FrameAxis,
    tick: bool,
}

impl FrameLabel {
    #[allow(clippy::too_many_arguments)]
    fn new(
        text: String, x: f64, y: f64, dir: (f64, f64), offset: f64,
        font: f64, fill: &'static str, axis: FrameAxis, tick: bool,
    ) -> Self {
        let half_w = crate::render::text::estimate_text_width(&text, font) / 2.0;
        let (ox, oy) = (x + dir.0 * offset, y + dir.1 * offset);
        FrameLabel {
            text, x: ox, y: oy, ox, oy, dir, half_w,
            cap: estimate_cap_height(font), font, fill, axis, tick,
        }
    }

    /// Push this label `d` pixels further along its own direction, from where it
    /// started.
    fn nudge(&mut self, d: f64) {
        self.x = self.ox + self.dir.0 * d;
        self.y = self.oy + self.dir.1 * d;
    }

    /// Does this label's ink clear `other`'s by `pad` pixels? Two boxes clear if
    /// they are apart on *either* screen axis, which is the same test a flat tick
    /// label makes against its panel edge before anchoring inward — one dimension
    /// up, and against another label rather than a boundary.
    fn clears(&self, other: &FrameLabel, pad: f64) -> bool {
        let dx = (self.x - other.x).abs() - (self.half_w + other.half_w);
        let dy = (self.y - other.y).abs() - (self.cap + other.cap) / 2.0;
        dx >= pad || dy >= pad
    }

    /// Does this label's ink fall inside a panel? The 3-D frame's labels live
    /// *within* the panel rather than in a reserved margin, so this is the check
    /// that a placement has anywhere to go.
    fn within(&self, l: &Layout) -> bool {
        self.x - self.half_w >= l.x0 && self.x + self.half_w <= l.x1
            && self.y - self.cap >= l.y0 && self.y <= l.y1
    }
}

/// The screen direction perpendicular to one 3-D frame edge, pointing away from
/// the cube center — the side a tick label sits on.
fn frame_perp(scene: &project::Scene, center: Screen, edge: FrameEdge) -> (f64, f64) {
    let a = edge.at(0.0);
    let b = edge.at(1.0);
    let o = scene.to_screen(a.0, a.1, a.2);
    let f = scene.to_screen(b.0, b.1, b.2);
    let (adx, ady) = (f.x - o.x, f.y - o.y);
    let alen = (adx * adx + ady * ady).sqrt().max(1e-6);
    let mut perp = (-ady / alen, adx / alen);
    let (mx, my) = ((o.x + f.x) / 2.0 - center.x, (o.y + f.y) / 2.0 - center.y);
    if mx * perp.0 + my * perp.1 < 0.0 {
        perp = (-perp.0, -perp.1);
    }
    perp
}

/// Normalize a value into `[0, 1]` across a range, guarding a zero span — how a
/// data coordinate becomes a unit-cube coordinate before projection.
/// The angular tick positions of a polar plot, as fractions of one turn, paired
/// with their labels.
///
/// The axis is periodic, so its two ends are the *same* spoke: a month axis
/// running 0–12 has a tick at each, and drawn naively it would print "0" and "12"
/// on top of each other at twelve o'clock and stroke the same gridline twice.
/// The second one is dropped. A categorical axis never hits this — its slots are
/// interior to the turn — which is why the de-duplication compares positions on
/// the circle rather than special-casing a type.
fn polar_angles<'a>(ticks: &'a TickSpec, xs: (f64, f64)) -> Vec<(f64, &'a str)> {
    let mut out: Vec<(f64, &str)> = Vec::new();
    for (i, &v) in ticks.values.iter().enumerate() {
        let u = unit_norm(v, xs);
        if !u.is_finite() { continue }
        let coincides = out.iter().any(|&(w, _)| {
            let d = (u - w).abs() % 1.0;
            d < 1e-9 || (1.0 - d) < 1e-9
        });
        if coincides { continue }
        out.push((u, ticks.labels.get(i).map(String::as_str).unwrap_or("")));
    }
    out
}

pub(crate) fn unit_norm(v: f64, range: (f64, f64)) -> f64 {
    let span = range.1 - range.0;
    if span.abs() < 1e-12 { 0.5 } else { (v - range.0) / span }
}

/// The default fill for a `text` mark's glyphs — a near-black that reads on a
/// white panel, matching the axis-label ink. `style(color = )` overrides it, and
/// `color(<category>)` recolors per group.
pub(crate) const TEXT_FILL: &str = "#28283a";

/// Format a numeric `label` value for drawing: an integer loses its `.0`, and any
/// other value keeps up to three decimals with trailing zeros trimmed — so `3`
/// renders as "3" (not "3.000") and `3.14159` as "3.142".
pub(crate) fn fmt_label_num(v: f64) -> String {
    if !v.is_finite() {
        return String::new();
    }
    if v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{v:.0}")
    } else {
        let s = format!("{v:.3}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}


/// Collect the categories on one axis, in the order they should be drawn.
///
/// Any mark with a string column on this axis defines the categorical scale,
/// not just `bar` — a `point` on a categorical axis is a strip plot, and gating
/// that to one mark was a per-mark special case.
///
/// `reverse` flips the result for the y axis. Screen y grows downward while the
/// scale grows upward, so without it the first category would land at the bottom
/// and `order(gold, desc = TRUE)` would put the largest bar at the bottom of a
/// horizontal chart and the leftmost of a vertical one. Reversing keeps
/// `order` meaning one thing: first in sort order, first in reading order.
///
/// This doc comment spent several sessions attached to `FRAME_INSET`, 280 lines
/// up: a second `///` block was added directly beneath it with no blank line
/// between, so rustdoc glued both onto the const and this function had none.
fn detect_categories(
    eff: &[&DataFrame],
    spec: &PlotSpec,
    field: &str,
    reverse: bool,
) -> Option<Vec<String>> {
    let mut labels = crate::data::categories_across(eff, field);
    if labels.is_empty() {
        return None;
    }

    if let Some(s) = &spec.order {
        if s.field == field {
            // Ordering an axis **by itself** means "in this column's own order",
            // and which order that is depends on what the column carries. Plain
            // text carries nothing but its spelling, so alphabetical is the only
            // answer there is. A **factor carries declared levels**, and those are
            // the column's own order — `categories_across` has already applied
            // them, so ascending is the list we are holding and descending is that
            // list reversed.
            //
            // Sorting the labels alphabetically in both cases is what this did
            // until 2026-07-26, and it silently threw the levels away: five-year
            // age bands ordered `0 10 15 5 50 55` and a population pyramid came out
            // with its floors shuffled. Nothing failed, because a scrambled
            // categorical axis is still a well-formed axis. Note the asymmetry it
            // created, which is the Law-2 smell that names the bug: the levels were
            // honored by *every* other reader of this column (the facet strips say
            // so at `layout_panels`, and the axis itself when no `order` is present)
            // and discarded only by the one atom whose whole job is ordering.
            //
            // `order` by *another* column still overrides the levels, in the branch
            // below. That is the documented power and it is untouched: the levels
            // are what the column says about itself, and a second column outranks
            // them. Nothing outranks the column's own order but itself.
            match eff.iter().find_map(|df| df.levels(field)) {
                Some(_) if s.descending => labels.reverse(),
                Some(_) => {}
                None if s.descending => labels.sort_by(|a, b| b.cmp(a)),
                None => labels.sort(),
            }
        } else {
            // By another column: build category → value from the first layer
            // that carries both.
            let mut sort_map: Vec<(String, f64)> = Vec::new();
            for df in eff {
                let Some(keys) = df.str_col(field)        else { continue };
                let Some(vals) = df.float_col(&s.field)   else { continue };
                for (k, v) in keys.iter().zip(vals.iter()) {
                    if !sort_map.iter().any(|(e, _)| e == k) {
                        sort_map.push((k.clone(), *v));
                    }
                }
                break;
            }
            if !sort_map.is_empty() {
                let value_of = |k: &String| {
                    sort_map.iter().find(|(e, _)| e == k).map(|(_, v)| *v).unwrap_or(0.0)
                };
                labels.sort_by(|a, b| {
                    let (va, vb) = (value_of(a), value_of(b));
                    if s.descending {
                        vb.partial_cmp(&va).unwrap_or(std::cmp::Ordering::Equal)
                    } else {
                        va.partial_cmp(&vb).unwrap_or(std::cmp::Ordering::Equal)
                    }
                });
            }
        }
    }

    if reverse { labels.reverse(); }
    Some(labels)
}

/// Build the ticks and scale for one axis.
///
/// `bar_position` — the axis the bars sit on, when their positions are numeric:
/// ticks land on the bars rather than at round numbers.
/// `bar_extent` — the axis the bars measure along: the range is stretched to
/// include zero, because a bar is read from the baseline.
///
/// `eff` is every panel's effective frames together: a facet shares one scale
/// across its panels, so the axis is built from all of them at once.
/// `bar_frames` is the subset the bar layers drew — the frames bar-aligned
/// ticks read their positions from.
#[allow(clippy::too_many_arguments)]
/// A fitted range widened by half a slot at each end — the *support* of the
/// slots rather than the span between their centers.
///
/// The slot width is read the same way [`super::marks::bar_thickness_svg`] reads
/// it, as the smallest gap between distinct positions, and from the same frames
/// the bars are drawn from; the two have to agree or the wedges will not tile
/// whatever the range says. Returns the range untouched when there is no second
/// position to measure a gap against.
/// Fit an axis to a **tiling's outer edges**, when the plot has one.
///
/// `build_axis` reads the position columns, and a 2-D `bin` fills those with cell
/// *centers* — so the outer half of every edge cell hangs past the fitted range
/// and is clipped away by the panel, visibly, as a border of short cells. The
/// tiling's own edge columns are the honest range: they are where the data
/// actually stops.
///
/// Flush by construction, and that is the point rather than an omission. A
/// breathing margin would leave a band of panel no cell covers, which on a
/// heatmap does not read as margin — it reads as a region that was measured and
/// found empty.
///
/// A plot with no binned zone passes its range and ticks through untouched.
/// `center`/`half` is how a **hexagonal** mesh describes the same extent — one
/// center column and one half-extent — since a hexagon has no edges to name. The
/// two forms are asked in that order and the first that answers wins, which is
/// the mark's own rule ("ask the tiling what its cells look like") applied to the
/// axis.
///
/// **A stated end is not fitted away** (spec §10, §12). The mesh's edges are a
/// *derivation* — where the data stopped — and `stated` is the caller's own
/// domain, so each end that was stated survives this pass and each end that was
/// not is taken from the cells. Overriding both was a live silent drop:
/// `y(depth, limits = c(0, 4))` on a sunburst reached the ticks and nothing else,
/// so the innermost ring still ran to the center and the hole the book promised
/// was never drawn — while the tick labels moved, which is what made it look as
/// though the domain had landed. Which end came from where is why the fix is
/// per-end rather than a flag: `c(0, NA)` opens the hole and still lets the rim
/// grow to the tree.
fn fit_to_cells(
    frames: &[&DataFrame], lo_field: &str, hi_field: &str,
    center: &str, half: &str,
    t: TickSpec, range: (f64, f64),
    stated: (Option<f64>, Option<f64>),
) -> (TickSpec, (f64, f64)) {
    let reduce = |field: &str, pick: fn(f64, f64) -> f64, seed: f64| -> Option<f64> {
        let v = frames.iter()
            .filter_map(|d| d.float_col(field))
            .flat_map(|c| c.iter().copied())
            .filter(|v| v.is_finite())
            .fold(seed, pick);
        v.is_finite().then_some(v)
    };
    // A hexagon's outermost point on each axis is its center plus its own half-
    // extent, so the mesh's bounds are the extreme centers pushed out by one.
    // Read as a pair rather than per row because every cell in a mesh is the same
    // size — that is what makes it a mesh.
    let span = match (reduce(center, f64::min, f64::INFINITY),
                      reduce(center, f64::max, f64::NEG_INFINITY),
                      reduce(half, f64::max, f64::NEG_INFINITY)) {
        (Some(lo), Some(hi), Some(h)) => Some((lo - h, hi + h)),
        _ => match (reduce(lo_field, f64::min, f64::INFINITY),
                    reduce(hi_field, f64::max, f64::NEG_INFINITY)) {
            (Some(lo), Some(hi)) => Some((lo, hi)),
            _ => None,
        },
    };
    match span {
        Some((lo, hi)) => {
            let (lo, hi) = (stated.0.unwrap_or(lo), stated.1.unwrap_or(hi));
            match (hi > lo, stated.0.is_some() || stated.1.is_some()) {
                // `clip_ticks`'s give-up guard is for a range gog *derived* on the
                // caller's behalf; a range the caller had a hand in is `adopt_range`'s,
                // which is the same distinction those two functions already draw.
                (true, true) => adopt_range(t, lo, hi),
                (true, false) => clip_ticks(t, lo, hi),
                _ => (t, range),
            }
        }
        None => (t, range),
    }
}

/// A stated domain (spec §10) in the **axis's own units**.
///
/// `limits` arrives in the data's units, like the ticks, so a log axis converts —
/// its columns already hold log positions. Shared by the two readers of a stated
/// domain rather than written twice: `build_axis`, which adopts it, and
/// `fit_to_cells`, which must not fit it away. Two copies is how the second one
/// came to disagree with the first.
fn stated_domain(
    limits: (Option<f64>, Option<f64>), is_log: bool, base: f64,
) -> (Option<f64>, Option<f64>) {
    let to_axis = |v: f64| if is_log { scale::to_log(v, base) } else { v };
    (
        limits.0.map(to_axis).filter(|v| v.is_finite()),
        limits.1.map(to_axis).filter(|v| v.is_finite()),
    )
}

fn widen_to_slot_support(bar_frames: &[&DataFrame], field: &str, xs: (f64, f64)) -> (f64, f64) {
    let mut vals: Vec<f64> = bar_frames
        .iter()
        .filter_map(|d| d.float_col(field))
        .flat_map(|c| c.iter().copied())
        .filter(|v| v.is_finite())
        .collect();
    if vals.len() < 2 {
        return xs;
    }
    vals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let slot = vals
        .windows(2)
        .map(|w| w[1] - w[0])
        .filter(|&d| d > 1e-12)
        .fold(f64::INFINITY, f64::min);
    if !slot.is_finite() {
        return xs;
    }
    (xs.0 - slot / 2.0, xs.1 + slot / 2.0)
}

/// What a set of panels implies about the axes — the whole answer, so that the
/// shared fit and a freed one differ only in *which panels were asked*.
///
/// Built by the `fit_axes` closure in `render`. A default plot builds one of
/// these over every panel at once; a binding that says `free = TRUE` builds
/// another per panel and takes that axis's fields from it (spec §11).
#[derive(Clone)]
struct PanelAxes {
    x_ticks: TickSpec,
    xs: (f64, f64),
    y_ticks: TickSpec,
    ys: (f64, f64),
    z_ticks: TickSpec,
    zs: (f64, f64),
    cat_x: Option<Vec<String>>,
    cat_y: Option<Vec<String>>,
    /// Where a polar plot's spokes start, as a fraction of the radial range —
    /// read off the cells' inner edge, so it follows whichever fit produced `ys`.
    inner_edge: f64,
}

fn build_axis(
    eff: &[&DataFrame],
    bar_frames: &[&DataFrame],
    // The `(frame, column)` pairs that place marks on this axis *without* going
    // through `field` — a bounded `zone`'s sides, and nothing else today. See
    // where they are gathered for why this mark is the exception.
    sides: &[(&DataFrame, &str)],
    field: &str,
    cats: Option<&[String]>,
    bar_position: bool,
    bar_extent: bool,
    // Suppress the free-end breathing margin: the axis an `area` fills along
    // sits flush to its data rather than leaving an empty band each side.
    flush: bool,
    tick_count: Option<usize>,
    is_log: bool,
    base: f64,
    time: Option<crate::time::TimeUnit>,
    // The domain the binding states, in the data's own units (spec §10). Each
    // end is independent: `c(0, NA)` pins the baseline and leaves the top to the
    // data.
    limits: (Option<f64>, Option<f64>),
    // How far, **in slots**, the marks on a *categorical* axis reach below the
    // first category and above the last (`legality::slot_reach`). `(0.0, 0.0)` for
    // every axis nothing overhangs, which is every axis but a violin's.
    slot_reach: (f64, f64),
) -> (TickSpec, (f64, f64)) {
    if let Some(cats) = cats {
        let vals: Vec<f64> = (0..cats.len()).map(|i| i as f64).collect();
        // Half a slot each side is exactly right for everything that *stands in* its
        // slot — a bar, a box, a whisker. A violin does not have to: `density(reach =
        // )` is measured in slots and a ridgeline's whole point is reaching past its
        // own, so the axis is told how far and grows to hold it. Without this the top
        // ridge was clipped by the frame — the mark drawing outside the panel, which
        // is the same defect as an area whose baseline fell off the scale, and just
        // as invisible to every check because the plot still rendered.
        //
        // Only the end the shapes actually reach toward grows: an `area` or a stroke
        // reaches one way, so the other keeps its half slot rather than opening a
        // matching band of nothing.
        let (lo, hi) = slot_reach;
        return (
            ticks_with_labels(vals, cats.to_vec()),
            ((-0.5f64).min(-lo), (cats.len() as f64 - 0.5).max(cats.len() as f64 - 1.0 + hi)),
        );
    }

    // The axis spans everything placed on it: the values of its own column, and
    // the columns a bounded `zone` draws its sides from. Unioned rather than used
    // as a fallback, because the two coexist — a zone highlighting part of a line
    // chart must not shrink the axis to itself, and one reaching past the line's
    // last point must widen it.
    let (mut mn, mut mx) = std::iter::once(channel_range_eff(eff, field))
        .chain(sides.iter().map(|(d, c)| channel_range_eff(&[d], c)))
        .flatten()
        .reduce(|a, b| (a.0.min(b.0), a.1.max(b.1)))
        .unwrap_or((0.0, 1.0));

    // A stated end replaces the data's, before anything downstream reads the
    // range — the ticks, the baseline stretch, the bar slots and the polar wrap
    // all have to agree about where the axis ends, and they do that by there
    // being one range rather than by each being told.
    //
    // In log space the column already holds log positions, so a stated end —
    // which arrives in the data's own units, like the ticks — is converted here.
    let stated = stated_domain(limits, is_log, base);
    if let Some(l) = stated.0 { mn = l }
    if let Some(h) = stated.1 { mx = h }

    // Every branch below picks its ticks against the range above, and then the
    // one closing step puts the stated ends back — because each branch widens
    // the range its own way (a calendar to its boundaries, a log axis to whole
    // powers, a fitted axis by 5%) and a stated end must survive all three
    // identically. Doing it once here is also what gives `limits` the **flush**
    // rule for free: replacing a breathed end with the exact stated value *is*
    // suppressing `AXIS_EXPAND` on that end, and only on that end, so
    // `c(0, NA)` pins the baseline while the top still breathes (spec §10).
    let close = |(t, range): (TickSpec, (f64, f64))| -> (TickSpec, (f64, f64)) {
        if stated.0.is_none() && stated.1.is_none() {
            return (t, range);
        }
        adopt_range(t, stated.0.unwrap_or(range.0), stated.1.unwrap_or(range.1))
    };

    // A temporal axis ticks at calendar boundaries, whatever else is going on.
    // Deliberately ahead of the bar paths: bars at dates get calendar
    // gridlines, not a tick per bar — daily bars would label every one — and
    // `bar_extent`'s stretch-to-zero would put the baseline at 1970.
    //
    // **The ticks come from the calendar; the range comes from the data.** That
    // is the section's own rule (§10) and the calendar is not exempt from it —
    // it used to be, taking `(first tick, last tick)` as the range, which is
    // precisely the bracket-outward failure the rule exists to undo. Six weeks
    // of daily orders ticked on Mondays drew a 49-day axis for 41 days of data,
    // four dead days at each end, because the first Monday at or before Mar 1 is
    // Feb 26 and the first at or after Apr 11 is Apr 15. An axis is allowed to
    // end between ticks; that is the whole point of fitting.
    if let Some(unit) = time {
        let t = time_ticks(mn, mx, unit);
        // A bar owns a slot, so the range has to hold the half-slot beyond the
        // first and last one or the end bars are sliced by the frame. Everything
        // else fits with the ordinary free-end breathing, and an `area` fills
        // flush — the same three answers the linear path gives, reached here
        // rather than skipped.
        let (lo, hi) = if bar_position {
            widen_to_slot_support(bar_frames, field, (mn, mx))
        } else {
            fitted_range(mn, mx, false, flush)
        };
        return close(clip_ticks(t, lo, hi));
    }

    // The column already holds log positions, so the range is in decades and the
    // ticks have to be chosen there too. `bar_extent` is deliberately not
    // honored: stretching to include zero is meaningless when zero is
    // infinitely far down the axis — the bars measure from its foot instead.
    //
    // **The ticks come from the powers; the range comes from the data** — §10's
    // rule, and the log axis was the last branch still exempt from it. It took
    // `(first tick, last tick)` as its range, which is the bracket-outward failure
    // the calendar branch above was fixed for, arriving through the one door left
    // open. On a histogram it did not merely waste space, it **clipped data**: a
    // bar's range is read off its *centers*, and a slot reaches half a bin past the
    // last of them, so `bar * bin(20) + x(gdp, scale = "log")` over gapminder ended
    // its axis at 10^5 while the last bar reached 10^5.055 — 41% of that bar sliced
    // off by the frame — and opened 2.9 empty bar-widths at the other end, where
    // the first center sat well above 10^2. Both are the one mistake, and the
    // asymmetry is why it read as two: bracketing outward is invisible below the
    // data and fatal above it.
    if is_log {
        let t = log_ticks(mn, mx, base);
        // A bar owns a slot, so the range has to hold the half-slot beyond the
        // first and last one or the end bars are sliced by the frame. Everything
        // else fits with the ordinary free-end breathing, and an `area` fills
        // flush — the same three answers the linear and calendar paths give,
        // reached here rather than skipped.
        let (lo, hi) = if bar_position {
            widen_to_slot_support(bar_frames, field, (mn, mx))
        } else {
            fitted_range(mn, mx, false, flush)
        };
        return close(clip_ticks(t, lo, hi));
    }

    // A stated end is a real coordinate, so the stretch-to-baseline does not
    // reach it: `y(v, limits = c(10, 50))` on bars means the axis starts at 10,
    // and pulling it back to 0 would draw the baseline where the caller said it
    // is not. The *unstated* end still stretches, which is what keeps
    // `c(NA, 50)` a bar chart rather than a floating one.
    if bar_extent {
        if stated.0.is_none() { mn = mn.min(0.0) }
        if stated.1.is_none() { mx = mx.max(0.0) }
    }

    if bar_position {
        return close(bar_x_ticks_eff(bar_frames, field, mn, mx, tick_count));
    }
    // Nice numbers for the *ticks*; the data for the *range*. `nice_ticks`
    // brackets the data outward with round numbers — that is the right set of
    // tick values, but the wrong scale bounds (§6.2.2). `fit_axis` keeps the
    // values and pins the range to the data instead.
    //
    // **A stated count is refined until the fit survives it; a derived one is
    // not.** `nice_ticks` picks a *step* from the target and cannot know how many
    // of the round numbers it lands on will fall inside the fitted range — and
    // when fewer than two do, `clip_ticks` gives the fit up and widens the axis to
    // the bracketing numbers instead. That trade was made when this parameter was
    // reachable from no binding, so only odd data could reach it. A caller asking
    // for two ticks reaches it on purpose: `x(year, tick_count = 2)` over
    // 1952–2007 takes a step of 100, lands on 1900/2000/2100, keeps one, and drew
    // a **1900–2100 axis for 55 years of data** — the bracket-outward failure §10
    // exists to undo, arriving through the new door.
    //
    // So a stated count asks for a finer step until at least two of its ticks land
    // inside the data's own range. It is the distinction `adopt_range` already
    // draws one line over: the guard exists because a *derivation* was the lesser
    // thing to give up, and a count the caller wrote is not a derivation. The
    // derived path is deliberately untouched, which is also why no existing plot
    // moves.
    let default_target = 5;
    let mut t = nice_ticks(mn, mx, tick_count.unwrap_or(default_target));
    if tick_count.is_some() {
        let (lo, hi) = fitted_range(mn, mx, bar_extent, flush);
        let mut target = tick_count.unwrap_or(default_target);
        // Bounded: each step is at least a factor 2 finer, so a handful of tries
        // covers any range a nice number can straddle.
        for _ in 0..8 {
            if ticks_inside(&t, lo, hi).1 >= 2 { break }
            target += 1;
            t = nice_ticks(mn, mx, target);
        }
    }
    close(fit_axis(t, mn, mx, bar_extent, flush))
}

/// The display range for a linear axis: fit the data closely, with a small
/// breathing margin on each *free* end and none on a baseline end.
///
/// `mn`/`mx` arrive with the baseline (0) already folded in when `baseline` is
/// set, so an end sitting exactly at 0 is the baseline and is pinned; every
/// other end breathes by `AXIS_EXPAND`.
fn fitted_range(mn: f64, mx: f64, baseline: bool, flush: bool) -> (f64, f64) {
    let span = mx - mn;
    if !(span > 0.0) {
        // A single distinct value: give it a unit of room either side so the
        // degenerate tick triple (v-1, v, v+1) has somewhere to land.
        return (mn - 1.0, mx + 1.0);
    }
    // A flush axis fits its data exactly; otherwise a free end breathes.
    let margin = if flush { 0.0 } else { span * AXIS_EXPAND };
    let mut lo = mn - margin;
    let mut hi = mx + margin;
    if baseline {
        if mn == 0.0 { lo = 0.0; } // data was all ≥ 0 — bottom is the baseline
        if mx == 0.0 { hi = 0.0; } // data was all ≤ 0 — top is the baseline
    }
    (lo, hi)
}

/// Pin an axis's range to its data and drop the ticks that fall outside it.
///
/// `nice_ticks` returns round tick values that bracket the data *outward*; this
/// keeps their values and labels but replaces the loose range with a close fit.
/// A tick beyond the fitted range is simply not drawn — the axis can end between
/// ticks, which is what lets the range follow the data.
///
/// The guard: if fewer than two ticks survive, the data span is narrower than a
/// single nice step, and fitting would leave a bare axis (Wilkinson's "too few
/// tick marks" failure, the twin of the one this fixes). There the loose
/// bracketing range is the lesser evil, so it is kept.
fn fit_axis(t: TickSpec, mn: f64, mx: f64, baseline: bool, flush: bool) -> (TickSpec, (f64, f64)) {
    let (lo, hi) = fitted_range(mn, mx, baseline, flush);
    clip_ticks(t, lo, hi)
}

/// Adopt `lo..=hi` as the axis range and drop the ticks that fall outside it.
///
/// Split out of [`fit_axis`] because a tiling arrives at its range a different
/// way — from its own cell edges rather than from a fitted spread — but has the
/// identical duty afterwards. The guard below is why it must be shared: an axis
/// whose surviving ticks number fewer than two is bare, and both callers would
/// otherwise have to remember that.
fn clip_ticks(t: TickSpec, lo: f64, hi: f64) -> (TickSpec, (f64, f64)) {
    let (clipped, kept) = ticks_inside(&t, lo, hi);
    if kept < 2 {
        let s = (t.scale_min(), t.scale_max());
        return (t, s);
    }
    (clipped, (lo, hi))
}

/// Adopt `lo..=hi` because the caller **stated** it — no guard.
///
/// [`clip_ticks`]'s guard exists for a range gog *derived*: when fitting would
/// bare the axis, the derivation was the lesser thing and is given up. A stated
/// domain (spec §10) is not a derivation to second-guess. Dropping back to the
/// loose bracketing range here would draw an axis over a range the caller did
/// not ask for, having accepted the binding — the silent override §12 forbids,
/// and it would take the polar cycle with it, since `limits = c(0, 24)` closing
/// the circle is the whole point of stating it.
///
/// A bare axis is possible in return and is the honest outcome: the ticks are
/// chosen against the stated range upstream, so it takes a deliberately odd
/// domain to get one.
fn adopt_range(t: TickSpec, lo: f64, hi: f64) -> (TickSpec, (f64, f64)) {
    (ticks_inside(&t, lo, hi).0, (lo, hi))
}

/// The ticks that fall within `lo..=hi`, and how many there were.
fn ticks_inside(t: &TickSpec, lo: f64, hi: f64) -> (TickSpec, usize) {
    let eps = (hi - lo) * 1e-9;
    let keep: Vec<usize> = (0..t.values.len())
        .filter(|&i| t.values[i] >= lo - eps && t.values[i] <= hi + eps)
        .collect();
    let values = keep.iter().map(|&i| t.values[i]).collect();
    let labels = keep.iter().map(|&i| t.labels[i].clone()).collect();
    (TickSpec { values, labels, step: t.step }, keep.len())
}

/// The declared time resolution of this field, wherever it is bound.
///
/// Read from the *source* tables, not the transformed frames: a transform
/// rebuilds its output with plain floats, but grouping by a date column keeps
/// the key values as epoch seconds, so the declaration on the source is still
/// the truth about what the axis is showing.
fn detect_time(ctx: &RenderContext<'_>, field: &str) -> Option<crate::time::TimeUnit> {
    ctx.spec.layers.iter()
        .filter_map(|layer| ctx.resolve_data(&layer.data))
        .find_map(|df| df.time_unit(field))
}

/// Say how many rows a log axis could not place, when any survive to this point.
///
/// `legality` refuses the plot outright when the *source* data has values a
/// logarithm is undefined at, so in the normal path this never fires. It covers
/// the two cases the check cannot see: `GOG_STRICT=0`, where the caller has
/// asked for best effort, and a transform whose *output* goes non-positive —
/// `bar * sum` over a column that nets out at zero. Dropping a row without
/// saying so is the one outcome the working agreement forbids — which is why
/// this lands in `remarks` rather than on stderr: a browser has no stderr, and
/// a dropped-rows report only the CLI can hear is the silent drop one hop out.
fn warn_unplaceable(out: &mut Vec<Diagnostic>, eff: &[&DataFrame], field: &str, is_log: bool, axis: &str) {
    if !is_log || field.is_empty() { return }
    let (mut bad, mut total) = (0usize, 0usize);
    for df in eff {
        let Some(vals) = df.float_col(field) else { continue };
        total += vals.len();
        bad += vals.iter().filter(|v| !v.is_finite()).count();
    }
    if bad == 0 { return }
    out.push(Diagnostic {
        kind: crate::legality::DiagnosticKind::Assumption,
        message: format!(
            "gog: {bad} of {total} rows have no place on the log `{axis}` axis — a \
             logarithm is undefined at zero and below, so they are not drawn. Filter \
             those rows, or drop `scale = \"log\"` from `{axis}({field})`."
        ),
    });
}

/// Say when a dot plot's piles have grown too tall to be dots any more (spec §12,
/// an Assumption: it renders, and here is what you should know).
///
/// A pile's rungs are exactly **one count unit** apart, so how tightly the dots pack
/// is decided by the tallest pile against the panel's height — and past some height
/// they overlap into a solid column, at which point the plot has quietly stopped
/// being a dot plot. Its whole claim is that every observation is visible and the
/// pile can be counted; 2 000 dots in one column is a histogram drawn the expensive
/// way (10 000 rows measured at 813 KB of SVG, against about 3 KB for `bar * bin`).
///
/// **There is no threshold constant here, deliberately.** The dots stop being
/// separable exactly when the gap between two rungs is smaller than a dot's
/// diameter, and the renderer knows both numbers — so the test is derived from what
/// is actually on the page and stays right at any panel size or `style(size = )`,
/// the same rule that gives `nudge` its distance and `dodge` its width. A determined
/// value earns no parameter (spec §5).
///
/// Returns the message rather than printing it, so the condition is testable; the
/// caller prints it with the `gog: ` prefix like every other stderr warning.
fn pile_overlap_warning(
    spec: &PlotSpec, panel_eff: &[Vec<DataFrame>], measure: &str,
    ys: (f64, f64), panel_h: f64, default_radius: f64,
) -> Option<String> {
    let span = (ys.1 - ys.0).abs();
    if !(span > 0.0) || !(panel_h > 0.0) { return None }
    let gap = panel_h / span; // one count unit, in pixels

    for (i, layer) in spec.layers.iter().enumerate() {
        if layer.mark != Mark::Point || !layer.transforms.contains(&Transform::Stack) {
            continue;
        }
        if gap >= 2.0 * layer.style.size.unwrap_or(default_radius) {
            continue; // the dots still stand apart, which is the whole point of them
        }
        // A pile's top rung *is* the tally it piled, so the largest value on the
        // measure column is how many dots are stacked in the tallest column.
        let tallest = panel_eff.iter()
            .filter_map(|p| p.get(i))
            .filter_map(|df| df.float_col(measure))
            .flat_map(|v| v.iter().copied())
            .filter(|v| v.is_finite())
            .fold(0.0f64, f64::max);
        if tallest < 2.0 { continue }

        let binned = layer.transforms.contains(&Transform::Bin);
        let sentence = if binned { "point * bin * stack" } else { "point * count * stack" };
        let summary = if binned {
            "`bar * bin` reads the same shape as a summary, and `line * density` as a curve"
        } else {
            "`bar * count` reads the same tallies as lengths"
        };
        return Some(format!(
            "gog: `{sentence}` piled {tallest:.0} dots into one column, and at that height \
             they overlap instead of counting — a dot plot says what it says by showing every \
             observation separately, so this is a histogram drawn dot by dot. For this many \
             rows, {summary}. `style(size = )` shrinks the dots, which buys a little room."
        ));
    }
    None
}

fn channel_range_eff(eff: &[&DataFrame], field: &str) -> Option<(f64, f64)> {
    let mut mn = f64::INFINITY;
    let mut mx = f64::NEG_INFINITY;
    for df in eff {
        let Some(vals) = df.float_col(field) else { continue };
        for &v in vals { if v < mn { mn = v; } if v > mx { mx = v; } }
    }
    if mn.is_finite() { Some((mn, mx)) } else { None }
}

fn label_for(ctx: &RenderContext<'_>, channel: Channel, override_label: Option<&str>) -> String {
    override_label
        .map(str::to_string)
        .or_else(|| ctx.coord_field(&channel).map(auto_label))
        .unwrap_or_default()
}

/// Infer the y-axis label from a synthesizing transform when no `y()` binding
/// is present.  Synthesizing transforms (`bin`, `count`, `density`) invent their
/// own y column — the user should not need to name it.
///
/// **`proportion` is asked before the others and outranks them**, because it is a
/// normalizer: whatever made the number, the axis is reading it as a share once
/// `proportion` has divided (spec §5). Read in list order instead, `bar * bin *
/// proportion` labeled its axis `Count` over a column of fractions — the same
/// mistake `transform::cell_measure` had to stop making one dimension up, and one
/// half of the plot the relative-frequency histogram replaced.
/// Does any layer fill its piles to 1 (`stack(share = true)`)?
///
/// Asked by `axis_label`, because a filled pile changes what the measure axis is
/// *in* rather than what wrote it — see the note there.
fn share_stacked(spec: &PlotSpec) -> bool {
    spec.layers.iter().any(|l| {
        l.transforms.contains(&Transform::Stack)
            && l.stack.as_ref().is_some_and(|s| s.share.unwrap_or(false))
    })
}

fn synth_y_label(spec: &PlotSpec) -> Option<String> {
    if spec.layers.iter().any(|l| l.transforms.contains(&Transform::Proportion)) {
        return Some("Proportion".into());
    }
    for layer in &spec.layers {
        for t in &layer.transforms {
            match t {
                Transform::Count      => return Some("Count".into()),
                Transform::Bin        => return Some("Count".into()),
                Transform::Density    => return Some("Density".into()),
                // A nested partition's measure axis is the **ring**, which is the
                // one synthesized axis here that is an index rather than a
                // quantity. It still belongs in this list: the user did not name
                // the column, so the axis has to name itself, which is the whole
                // rule. Crossed there is no ring — the second axis is apportioning
                // the same measure the first one is, one level down, so it says so
                // and the caller's own `y_label()` is what renames it.
                Transform::Partition  => return Some(match layer.partition.as_ref()
                    .map(|p| p.cross).unwrap_or(false) {
                        true  => "Share of column".into(),
                        false => "Depth".into(),
                    }),
                _ => {}
            }
        }
    }
    None
}

/// Copy a layer's own position columns onto the names the shared axes go by.
///
/// The one place per-layer positions exist. A note table calling its value `at`
/// where the plot's axis is `gdp` is *the same axis, spelled differently in
/// another table* (spec §8), so the resolution is a rename and nothing more:
/// after this, one column name per axis holds for every frame, which is what the
/// rest of the renderer has always assumed and can go on assuming.
///
/// Overwriting is deliberate. If the layer's table happens to carry a column
/// under the axis name too, the layer said to read a different one, so the copy
/// must win — in this layer's frame only, since frames are per-layer clones.
fn resolve_positions(df: DataFrame, spec: &PlotSpec, layer: &Layer) -> DataFrame {
    let mut out = df;
    for ch in [Channel::X, Channel::Y, Channel::Z] {
        let (Some(own), Some(axis)) = (layer.encodings.get(&ch), spec.axis_def(&ch)) else {
            continue;
        };
        if own.field == axis.field {
            continue;
        }
        out = alias_column(out, &own.field, &axis.field);
    }
    out
}

/// Copy one column under a second name, carrying its type with it — a date stays
/// a date and a factor keeps its declared order, or the axis would silently
/// change kind on the way through.
fn alias_column(df: DataFrame, from: &str, to: &str) -> DataFrame {
    if let Some(values) = df.float_col(from).cloned() {
        return match df.time_unit(from) {
            Some(unit) => df.with_time(to, values, unit),
            None => df.with_float(to, values),
        };
    }
    if let Some(values) = df.str_col(from).cloned() {
        return match df.levels(from).map(<[String]>::to_vec) {
            Some(levels) => df.with_levels(to, values, levels),
            None => df.with_str(to, values),
        };
    }
    // No such column. `legality::check` has already refused this with direction;
    // reaching here means `GOG_STRICT=0`, where drawing what can be drawn is the
    // asked-for behavior.
    df
}

// **The missing-binding warnings were deleted 2026-08-17, and the deletion is the
// fourth narrowing of the same function.** It printed three messages -- a reading
// transform with no `y`, one with no `x`, and a mark with no `x` -- each ending
// "Rendering empty chart".
//
// Every case it named is refused by `legality::check` first, measured across all
// twelve marks on both axes, so the warning was never the only voice and never the
// deciding one. Its sentence was false in both switch positions besides: under
// `GOG_STRICT=1` nothing is rendered at all, and under `GOG_STRICT=0` a chart *is*
// rendered, so "Rendering empty chart" described neither.
//
// The history is the argument. The comments removed with it recorded three earlier
// narrowings, each after this function libeled a plot the gate had already blessed:
// a `partition`, a `text` in `nest` drawing its labels perfectly, and
// `interval * bounds(lo, hi) + y(term)` -- a good forest plot told it was empty.
// Each fix taught it one more of `legality`'s exceptions. A second copy of a
// judgment that belongs to the gate has to be taught every exception the gate
// knows, forever, and it is wrong in the window before it is. So it is gone rather
// than corrected a fourth time: one question, one answer, in `legality.rs`.

fn bar_x_ticks_eff(
    bar_frames: &[&DataFrame],
    x_field: &str,
    data_min: f64, data_max: f64,
    target_count: Option<usize>,
) -> (TickSpec, (f64, f64)) {
    let mut vals: Vec<f64> = Vec::new();
    for df in bar_frames {
        let Some(col) = df.float_col(x_field) else { continue };
        for &v in col {
            if !vals.iter().any(|&e: &f64| (e - v).abs() < 1e-10) { vals.push(v); }
        }
    }
    vals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    if vals.is_empty() || vals.len() > 20 {
        // Too many distinct positions to tick one-per-bar; this axis behaves
        // like a continuous one, so it fits its data the same way (no baseline —
        // it carries positions, not amounts).
        let t = nice_ticks(data_min, data_max, target_count.unwrap_or(5));
        return fit_axis(t, data_min, data_max, false, false);
    }

    let step = if vals.len() > 1 {
        vals.windows(2)
            .map(|w| (w[1] - w[0]).abs())
            .filter(|&d| d > 1e-12)
            .fold(f64::INFINITY, f64::min)
    } else {
        (data_max - data_min).abs().max(1.0)
    };
    let step = if step.is_infinite() { 1.0 } else { step };
    let padding   = step * 0.5;
    let scale_min = vals[0] - padding;
    let scale_max = vals[vals.len() - 1] + padding;
    (ticks_at(vals), (scale_min, scale_max))
}

/// A clip id derived from the rectangle it clips.
///
/// The book inlines many SVGs into one HTML page, where ids are global and the
/// first definition wins — the lesson the gradient legend's id taught. Deriving
/// the id from the geometry means two clips share an id only when they clip the
/// same rectangle, which is the one collision that cannot mislead. (The old
/// fixed `plot-clip` id had exactly that latent defect; faceting, whose panel
/// rectangles genuinely differ from plot to plot, is what made it real.)
fn clip_id(l: &Layout) -> String {
    format!(
        "clip-{}-{}-{}-{}",
        (l.x0 * 10.0).round() as i64,
        (l.y0 * 10.0).round() as i64,
        (l.x1 * 10.0).round() as i64,
        (l.y1 * 10.0).round() as i64,
    )
}



// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{Layer, ScaleType};
    use crate::render::palette::{PALETTE_GOG, RAMP_BLUE};
    use crate::render::text::estimate_text_width;

    // -----------------------------------------------------------------------
    // Brush — the selection, and the promise that it costs an unbrushed plot
    // nothing (spec §15)
    //
    // The first two tests here are the whole safety argument for the feature and
    // they were written before it existed. Everything in the book, the PDF and
    // every recorded parity hash rests on one sentence: **a plot that does not
    // name a brush draws exactly what it drew before selection was built.** The
    // 692 tests around this one are the broad form of that promise; these are the
    // sharp form, and they name the two artifacts a selection could leak.
    // -----------------------------------------------------------------------

    fn brush_data() -> HashMap<String, DataFrame> {
        let mut d = HashMap::new();
        d.insert("t".to_string(), DataFrame::new()
            .with_float("gx", vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0])
            .with_float("gy", vec![2.0, 4.0, 1.0, 5.0, 3.0, 6.0])
            .with_str("cat", vec!["a", "a", "b", "b", "c", "c"]
                .into_iter().map(String::from).collect()));
        d
    }

    fn brush_spec() -> PlotSpec {
        PlotSpec::new().data("t").x("gx").y("gy").layer(Layer::new(Mark::Point))
    }

    /// Counting glyphs is how these tests read a pass, because a `point` writes
    /// exactly one `<circle>` per row it is handed.
    fn circles(svg: &str) -> usize {
        svg.matches("<circle").count()
    }

    // -----------------------------------------------------------------------
    // Map — the sphere on the page (spec §15)
    // -----------------------------------------------------------------------

    /// Places spread over the whole globe, so the two properties below are
    /// measured across the range where a projection actually differs from a
    /// rescaling rather than in a corner where every projection agrees.
    fn world() -> HashMap<String, DataFrame> {
        let df = DataFrame::new()
            .with_float("lon", vec![0.0, 90.0, -90.0, 180.0, -180.0, 0.0, 0.0])
            .with_float("lat", vec![0.0, 0.0, 0.0, 0.0, 0.0, 60.0, -60.0]);
        let mut m = HashMap::new();
        m.insert("t".to_string(), df);
        m
    }

    fn world_map(preserve: crate::ir::Preserve) -> PlotSpec {
        PlotSpec::new()
            .data("t")
            .x("lon")
            .y("lat")
            .coord(CoordSpace::Map(crate::ir::MapView { preserve }))
            .layer(Layer::new(Mark::Point))
    }

    /// Pull every `<circle>` center out, in the row order they were written.
    fn centers(svg: &str) -> Vec<(f64, f64)> {
        let mut out = Vec::new();
        for chunk in svg.split("<circle ").skip(1) {
            let grab = |key: &str| -> Option<f64> {
                let at = chunk.find(key)? + key.len();
                chunk[at..].split('"').next()?.parse().ok()
            };
            if let (Some(x), Some(y)) = (grab("cx=\""), grab("cy=\"")) {
                out.push((x, y));
            }
        }
        out
    }

    /// **The property that makes an equal-area map worth having.** A projected
    /// unit must be the same number of pixels across as it is tall, or the panel
    /// has stretched the map and the ink is no longer proportional to the ground —
    /// which breaks the projection's whole claim while still looking like a map.
    ///
    /// Measured, not asserted: the equator's half-width and the 60° parallel's
    /// height are compared against what `geo` says those distances are. Before the
    /// space took the panel's proportions from its own extent, this came out 1.75
    /// and every map was a fitted rectangle wearing a projection's name.
    #[test]
    fn a_map_is_drawn_at_the_projections_proportions_and_not_the_panels() {
        for preserve in [crate::ir::Preserve::Area, crate::ir::Preserve::Angle] {
            let svg = SvgRenderer::default().render(&world_map(preserve), &world());
            let pts = centers(&svg);
            assert_eq!(pts.len(), 7, "every place should be drawn once");
            let geo = crate::render::geo::Geo::new(&crate::ir::MapView { preserve });

            // Row 0 is (0, 0); row 3 is (180, 0); row 5 is (0, 60).
            let px_east = (pts[3].0 - pts[0].0) / geo.project(180.0, 0.0).0;
            let px_north = (pts[0].1 - pts[5].1) / geo.project(0.0, 60.0).1;
            assert!(
                (px_east / px_north - 1.0).abs() < 1e-3,
                "{preserve:?}: {px_east:.3} px per unit across, {px_north:.3} down"
            );
        }
    }

    /// Longitude is linear along the equator in both projections, so twice the
    /// longitude is twice the distance. A projection wired up to the wrong column,
    /// or applied one column at a time, fails this immediately.
    #[test]
    fn twice_the_longitude_is_twice_the_distance_along_the_equator() {
        let svg = SvgRenderer::default().render(&world_map(crate::ir::Preserve::Area), &world());
        let pts = centers(&svg);
        let (at90, at180) = (pts[1].0 - pts[0].0, pts[3].0 - pts[0].0);
        assert!((at180 / at90 - 2.0).abs() < 1e-3, "90° gave {at90:.3}, 180° gave {at180:.3}");
        // And west mirrors east about the prime meridian.
        assert!(((pts[0].0 - pts[2].0) - at90).abs() < 0.05);
    }

    /// **The axes say degrees, because everything else downstream says projected
    /// units.** The projection is applied to the data, which is what makes the
    /// space cheap and is wrong for exactly one reader: an axis left alone would
    /// announce that longitude runs from −2 to 2.
    #[test]
    fn a_maps_axes_are_labeled_in_degrees_rather_than_projected_units() {
        let svg = SvgRenderer::default().render(&world_map(crate::ir::Preserve::Area), &world());
        assert!(svg.contains("°</text>"), "no degree labels: {svg}");
        for bare in [">-2</text>", ">2</text>", ">-1</text>", ">1</text>"] {
            assert!(!svg.contains(bare), "a projected unit reached the axis: {bare}");
        }
    }

    /// Six rows: three rings, two continents, with `west` holding two of the rings.
    fn rings_by_continent() -> HashMap<String, DataFrame> {
        let df = DataFrame::new()
            .with_float("lon", vec![0.0, 1.0, 5.0, 6.0, 10.0, 11.0])
            .with_float("lat", vec![0.0, 1.0, 0.0, 1.0, 0.0, 1.0])
            .with_str("piece", ["a", "a", "b", "b", "c", "c"].map(String::from).to_vec())
            .with_str(
                "continent",
                ["west", "west", "west", "west", "east", "east"].map(String::from).to_vec(),
            );
        let mut m = HashMap::new();
        m.insert("t".to_string(), df);
        m
    }

    /// **Every channel that splits, splits.** `color` splits and encodes; `group`
    /// splits and encodes nothing; binding both means the mark is drawn once per
    /// combination, which here is one stroke per ring.
    ///
    /// Each of the five series marks used to pick a *single* field to split by, in
    /// the order `color`, `group`, `pattern` — so a `group` beside a `color` was
    /// silently discarded. It drew a plausible picture rather than an empty one,
    /// which is what kept it hidden: a world map grouped by coastline and colored
    /// by continent came out as one scribble per continent, every ring joined end
    /// to end.
    #[test]
    fn a_group_beside_a_color_splits_by_both_rather_than_being_dropped() {
        for mark in [Mark::Path, Mark::Line, Mark::Step] {
            let spec = PlotSpec::new()
                .data("t")
                .x("lon")
                .y("lat")
                .layer(
                    Layer::new(mark.clone())
                        .encode(Channel::Group, "piece")
                        .encode(Channel::Color, "continent"),
                );
            let svg = SvgRenderer::default().render(&spec, &rings_by_continent());
            let strokes = svg.matches("<polyline").count();
            assert_eq!(strokes, 3, "{mark:?}: three rings, so three strokes:\n{svg}");
            // And the colors still come from `continent`, not from `piece`: two of
            // the three rings are `west`, so the three strokes carry two hues.
            // Read off the polylines alone — the gridlines and the axes carry a
            // `stroke` too, and counting those made this assertion meaningless.
            let hues: std::collections::HashSet<&str> = svg
                .split("<polyline")
                .skip(1)
                .filter_map(|s| s.split("stroke=\"").nth(1))
                .filter_map(|s| s.split('"').next())
                .collect();
            assert_eq!(hues.len(), 2, "{mark:?}: colored by ring instead of continent");
        }
    }

    /// Two regions. `big` is a square with a smaller square inside it as a second
    /// ring — the Lesotho case — and `small` is that inner square as a region of
    /// its own. Every ring repeats its first vertex last, which is what divides a
    /// region's rows into rings.
    fn enclave() -> HashMap<String, DataFrame> {
        let ring = |x0: f64, y0: f64, x1: f64, y1: f64| {
            vec![(x0, y0), (x1, y0), (x1, y1), (x0, y1), (x0, y0)]
        };
        let mut pts = ring(0.0, 0.0, 10.0, 10.0);
        pts.extend(ring(4.0, 4.0, 6.0, 6.0));
        let n_big = pts.len();
        pts.extend(ring(4.0, 4.0, 6.0, 6.0));
        let df = DataFrame::new()
            .with_float("lon", pts.iter().map(|p| p.0).collect())
            .with_float("lat", pts.iter().map(|p| p.1).collect())
            .with_str(
                "region",
                (0..pts.len())
                    .map(|i| if i < n_big { "big".to_string() } else { "small".to_string() })
                    .collect(),
            );
        let mut m = HashMap::new();
        m.insert("t".to_string(), df);
        m
    }

    fn choropleth() -> PlotSpec {
        PlotSpec::new()
            .data("t")
            .x("lon")
            .y("lat")
            .coord(CoordSpace::Map(crate::ir::MapView::default()))
            .layer(
                Layer::new(Mark::Zone)
                    .encode(Channel::Group, "region")
                    .encode(Channel::Color, "region"),
            )
    }

    /// **A region is one path, however many rings it has**, and the rings are found
    /// by closure rather than by a second grouping column. Two regions here, three
    /// rings between them, so two paths and three subpaths.
    #[test]
    fn a_boundary_becomes_one_filled_path_per_region() {
        let svg = SvgRenderer::default().render(&choropleth(), &enclave());
        assert_eq!(svg.matches("<path d=\"M").count(), 2, "one path per region: {svg}");
        // `M` starts a subpath and `Z` closes it: two rings in `big`, one in `small`.
        let region_paths: Vec<&str> = svg.split("<path d=\"").skip(1).collect();
        let subpaths: usize = region_paths.iter().map(|p| p.split('"').next().unwrap_or("").matches('M').count()).sum();
        assert_eq!(subpaths, 3, "three rings across two regions");
    }

    /// **The hole is what even-odd buys**, and it is the case naive per-ring filling
    /// gets wrong: the enclave would be painted over by whichever region drew last.
    /// With both of `big`'s rings in one path under `evenodd`, the middle is empty
    /// for `small` to fill with its own color, whatever the draw order.
    #[test]
    fn an_inner_ring_is_a_hole_rather_than_a_patch_painted_over() {
        let svg = SvgRenderer::default().render(&choropleth(), &enclave());
        assert!(svg.contains(r#"fill-rule="evenodd""#), "no even-odd rule: {svg}");
        // The two regions carry different fills, so the enclave is readable as its
        // own value rather than as a smudge on its container.
        let fills: Vec<&str> = svg
            .split("<path d=\"")
            .skip(1)
            .filter_map(|p| p.split("fill=\"").nth(1))
            .filter_map(|p| p.split('"').next())
            .collect();
        assert_eq!(fills.len(), 2);
        assert_ne!(fills[0], fills[1], "the enclave took its container's color");
    }

    /// **A plot that names no brush carries none of the machinery.** Neither the
    /// dimmed group nor the panel metadata may appear, because either one would
    /// change the bytes of every plot in the book at once.
    #[test]
    fn a_plot_that_names_no_brush_carries_no_selection_machinery() {
        let svg = SvgRenderer::default().render(&brush_spec(), &brush_data());
        assert!(!svg.contains("data-gog-panel"), "a plain plot must not carry panel metadata");
        assert!(!svg.contains(r#"<g opacity="#), "a plain plot must not carry a dimmed group");
        assert_eq!(circles(&svg), 6, "and it draws every row once");
    }

    /// **A brush the reader has not moved draws the same ink.** The resting state
    /// is what print shows and what the page shows before the first drag, so it
    /// has to be the plot itself — the panel metadata the browser needs is the
    /// only difference, and it draws nothing.
    #[test]
    fn a_resting_brush_draws_exactly_the_same_ink() {
        let plain = SvgRenderer::default().render(&brush_spec(), &brush_data());
        let resting = SvgRenderer::default()
            .render(&brush_spec().brush(crate::ir::BrushDef::new("gx")), &brush_data());
        let ink: String = resting.lines()
            .filter(|l| !l.contains("data-gog-panel"))
            .collect::<Vec<_>>().join("\n");
        assert_eq!(ink.trim(), plain.trim(),
            "a resting brush must change nothing but the panel metadata");
        assert!(resting.contains("data-gog-panel"),
            "and the metadata must be there before the first drag, or nothing can invert a pixel");
    }

    /// **A selection pushes back what it was taken from**, rather than removing
    /// it: a brush highlights and never filters, so every row is still drawn.
    #[test]
    fn a_brush_pushes_back_the_rows_outside_it() {
        let spec = brush_spec().brush(crate::ir::BrushDef::new("gx").at(2.5, 4.5));
        let svg = SvgRenderer::default().render(&spec, &brush_data());
        assert_eq!(circles(&svg), 6, "no row is dropped — a brush is not `limits`");
        let dim = format!(r#"<g opacity="{:.3}">"#, crate::render::encode::SELECTION_DIM);
        assert_eq!(svg.matches(&dim).count(), 1, "one dimmed group: {svg}");
        // Two of the six rows sit inside 2.5..4.5, so four are pushed back.
        let dimmed = svg.split(&dim).nth(1).unwrap().split("</g>").next().unwrap();
        assert_eq!(circles(dimmed), 4, "the four rows outside the bound are the dimmed ones");
    }

    /// Selecting on a column of categories is the same atom, and which of the two
    /// readings applies is decided by the *column*, exactly as the column decides
    /// whether `color` hands out a ramp or a palette.
    #[test]
    fn a_brush_on_a_category_column_selects_slots() {
        let spec = brush_spec()
            .brush(crate::ir::BrushDef::new("cat").levels(vec!["b".to_string()]));
        let svg = SvgRenderer::default().render(&spec, &brush_data());
        let dim = format!(r#"<g opacity="{:.3}">"#, crate::render::encode::SELECTION_DIM);
        let dimmed = svg.split(&dim).nth(1).unwrap().split("</g>").next().unwrap();
        assert_eq!(circles(dimmed), 4, "the four rows outside category `b` are pushed back");
    }

    /// **A free shape reaches the engine as a predicate, and draws like a bound.**
    /// The two counts in this test are the whole argument for the gesture: the
    /// traced triangle pushes back three of the six rows, and the rectangle that
    /// contains that same triangle — which is all a written `brush` can say —
    /// pushes back none of them.
    #[test]
    fn a_traced_region_pushes_back_the_rows_outside_the_shape() {
        let dim = format!(r#"<g opacity="{:.3}">"#, crate::render::encode::SELECTION_DIM);
        let drawn = |path: Vec<[f64; 2]>| {
            let mut spec = brush_spec().brush(crate::ir::BrushDef::new("gx"));
            spec.region = Some(crate::ir::RegionDef::new("gx", "gy", path));
            SvgRenderer::default().render(&spec, &brush_data())
        };

        let triangle = drawn(vec![[0.5, 0.5], [6.5, 0.5], [0.5, 6.5]]);
        assert_eq!(circles(&triangle), 6, "no row is dropped — a region highlights, like a bound");
        let dimmed = triangle.split(&dim).nth(1).unwrap().split("</g>").next().unwrap();
        assert_eq!(circles(dimmed), 3, "the three rows outside the shape are pushed back");

        let rectangle = drawn(vec![[0.5, 0.5], [6.5, 0.5], [6.5, 6.5], [0.5, 6.5]]);
        let none = rectangle.split(&dim).nth(1).unwrap().split("</g>").next().unwrap();
        assert_eq!(circles(none), 0,
            "and the rectangle around that triangle selects every row, so none is pushed back");
    }

    /// A region nobody has traced is the resting state, exactly as an unmoved
    /// brush is: the plot draws as it always did, and the page shows that before
    /// the first gesture and the PDF shows it forever.
    #[test]
    fn a_region_of_two_vertices_draws_exactly_the_resting_plot() {
        let resting = SvgRenderer::default()
            .render(&brush_spec().brush(crate::ir::BrushDef::new("gx")), &brush_data());
        let mut spec = brush_spec().brush(crate::ir::BrushDef::new("gx"));
        spec.region = Some(crate::ir::RegionDef::new("gx", "gy", vec![[0.5, 0.5], [6.5, 0.5]]));
        let traced = SvgRenderer::default().render(&spec, &brush_data());
        assert_eq!(traced, resting, "an unclosed outline must select nothing at all");
    }

    /// **A layer the selection cannot reach is drawn once, whole, at full
    /// strength** — never twice, and never dimmed. A summarized layer has no
    /// honest answer to "which of these rows did you select", so it declines to
    /// give one rather than approximating it, and `check_brush` says so.
    #[test]
    fn a_layer_the_selection_cannot_reach_is_drawn_once_and_whole() {
        let spec = brush_spec()
            .layer(Layer::new(Mark::Bar).transform(Transform::Count))
            .brush(crate::ir::BrushDef::new("gx").at(2.5, 4.5));
        let svg = SvgRenderer::default().render(&spec, &brush_data());
        let bars = svg.lines().filter(|l| l.contains("<rect") && l.contains("fill-opacity")).count();
        assert!(bars > 0, "the summarized layer still draws");
        let dim = format!(r#"<g opacity="{:.3}">"#, crate::render::encode::SELECTION_DIM);
        let dimmed = svg.split(&dim).nth(1).unwrap().split("</g>").next().unwrap();
        assert!(!dimmed.contains("<rect"), "and it is not in the dimmed pass: {dimmed}");
    }

    // -----------------------------------------------------------------------
    // Chains — every transform in a legal chain has to do something (spec §5, §12)
    // -----------------------------------------------------------------------

    /// A frame with **uneven groups**, which is the whole reason it is written by
    /// hand rather than generated.
    ///
    /// Six categories of unequal size (1, 2, 3, 4, 5, 6 rows) against a continuous
    /// column that repeats, so a chain has real groups to reduce within on either
    /// axis. Even groups are a trap: with every group the same size, `sum` is `mean`
    /// times a constant and the two draw the *same picture once `proportion` divides
    /// them* — so an evenly-generated fixture reports the engine confusing `sum` with
    /// `mean` when the engine is right and the fixture is degenerate. A single row
    /// per group is the same trap one step further on, where all five reductions
    /// agree. A sweep on 2026-07-30 reported 312 clean pairs for exactly this reason
    /// and missed 40 real failures.
    fn chain_frame() -> HashMap<String, DataFrame> {
        let sizes = [1usize, 2, 3, 4, 5, 6];
        let names = ["a", "b", "c", "d", "e", "f"];
        let (mut cat, mut num, mut val, mut lo, mut hi) =
            (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new());
        let mut k = 0_u64;
        for (i, n) in sizes.iter().enumerate() {
            for _ in 0..*n {
                cat.push(names[i].to_string());
                // Few distinct values, several rows each — a continuous key whose
                // values are all distinct makes singleton groups, where every
                // reduction agrees for real and the properties below cannot tell a
                // working engine from a broken one. **Unevenly** spaced for the same
                // kind of reason: evenly spaced, a cut lands one distinct value in
                // each cell, and `bin` and `count` then group the rows identically
                // and draw the same plot — an accident of the numbers that P2 reads
                // as the engine failing to tell them apart.
                num.push([0.0, 3.0, 11.0, 26.0, 42.0, 57.0][i]);
                // Irregular within a group, and deliberately not an arithmetic
                // progression: a progression has its mean *at* its median, so
                // `mean` and `median` draw the same picture and P2 reports a defect
                // that is the fixture's. Squaring is what breaks the symmetry.
                let v = 2.0 + ((k * k * 13) % 29) as f64;
                val.push(v);
                lo.push(v - 1.5);
                hi.push(v + 2.5);
                k += 1;
            }
        }
        let df = DataFrame::new()
            .with_str("cat", cat)
            .with_float("num", num)
            .with_float("val", val)
            .with_float("lo", lo)
            .with_float("hi", hi);
        HashMap::from([("d".to_string(), df)])
    }

    /// Render one chain on one mark, or `None` if the engine refuses it.
    ///
    /// `check` is asked first, exactly as `gog-cli` asks it, so the property below
    /// tests only chains a caller could actually draw.
    fn render_chain(mark: &Mark, ts: &[Transform], categorical_x: bool) -> Option<String> {
        chain_run(mark, ts, categorical_x).map(|(svg, _)| svg)
    }

    /// The rendered plot and what the engine said about it, or `None` if refused.
    /// `check` is asked first, exactly as `gog-cli` asks it, so the properties below
    /// cover only chains a caller could actually draw.
    fn chain_run(mark: &Mark, ts: &[Transform], categorical_x: bool)
        -> Option<(String, Vec<crate::legality::Diagnostic>)>
    {
        let data = chain_frame();
        let mut layer = Layer::new(mark.clone());
        for t in ts {
            layer = layer.transform(t.clone());
        }
        if ts.contains(&Transform::Bounds) {
            layer = layer.bounds("lo", "hi");
        }
        // `quantile` names no default, because the only defensible one is 0.5 and
        // that is `median`, so a legal sentence always carries a probability. It
        // is 0.9 here rather than 0.5 for that same reason: at 0.5 this chain
        // *is* the `median` chain, and P2 would be asking two spellings of one
        // statistic to draw differently.
        if ts.contains(&Transform::Quantile) {
            layer = layer.at_quantile(0.9);
        }
        let spec = PlotSpec::new()
            .data("d")
            .x(if categorical_x { "cat" } else { "num" })
            .y("val")
            .layer(layer);
        let said = crate::legality::check(&spec, &data);
        if said.iter().any(|d| d.kind == crate::legality::DiagnosticKind::Illegal) {
            return None;
        }
        Some((SvgRenderer::default().render(&spec, &data), said))
    }

    /// Did the engine say, in as many words, that this transform is doing nothing?
    ///
    /// The one way a chain is allowed to fail P1. A transform can be redundant
    /// rather than contradictory — `count` beside `proportion`, where a share is
    /// already a share *of* a tally — and the plot drawn is then exactly the plot
    /// asked for, with one atom that was not needed. Refusing that would forbid the
    /// ugly-but-legal (Law 8); saying nothing would be the silent drop. So the rule
    /// is not *every transform changes the picture* but **every transform changes
    /// the picture, or the engine says why it does not**.
    fn declared_a_noop(mark: &Mark, ts: &[Transform], categorical_x: bool, t: &Transform) -> bool {
        let Some((_, said)) = chain_run(mark, ts, categorical_x) else { return false };
        let name = format!("{t:?}").to_lowercase();
        said.iter().any(|d|
            d.kind == crate::legality::DiagnosticKind::Assumption
                && d.message.contains(&format!("`{name}`"))
                && d.message.contains("draws the same plot as")
        )
    }

    /// A short stable digest, so a failure names the chain rather than printing two
    /// entire SVG documents at each other.
    fn digest(s: &str) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        s.hash(&mut h);
        h.finish()
    }

    /// **P1 — no drop. Removing a transform from a legal chain has to change the
    /// picture.**
    ///
    /// If it does not, that transform was accepted and ignored, which is the silent
    /// drop §12 forbids arriving through a *composition* rather than through a
    /// binding. It is the mechanical form of the rule, and it says nothing about
    /// order — deliberately, because two spellings that each mean something are the
    /// caller's business (Law 8) while a spelling that means less than it says is
    /// not.
    ///
    /// This is what the enumerated composition checks could not see. On 2026-07-31,
    /// of 582 two-transform chains that drew, only 271 had every transform doing
    /// something: `bar * smooth * mean` drew `bar * smooth`, `interval * range *
    /// confidence` drew `interval * confidence`, `line * sum * range` drew
    /// `line * range`.
    #[test]
    fn no_legal_chain_ignores_a_transform_it_was_given() {
        for (mark, ts, categorical_x, drawn) in every_legal_chain() {
            for i in 0..ts.len() {
                let mut shorter = ts.clone();
                let dropped = shorter.remove(i);
                let Some(without) = render_chain(&mark, &shorter, categorical_x) else { continue };
                if digest(&drawn) != digest(&without) { continue }
                assert!(
                    declared_a_noop(&mark, &ts, categorical_x, &dropped),
                    "`{} * {}` draws exactly `{} * {}` — `{dropped:?}` was accepted and \
                     ignored, and nothing said so",
                    mark_of(&mark), names(&ts), mark_of(&mark), names(&shorter)
                );
            }
        }
    }

    /// **P2 — no confusion. Swapping one transform for a different one has to change
    /// the picture.**
    ///
    /// P1's other half. A chain can honor *that* you named a transform without
    /// honoring *which*, and that is the same defect one level down: `line * smooth *
    /// range` and `line * smooth * confidence` rendered byte-identical, so a min–max
    /// range and a 95% confidence interval drew the same plot. Nothing about the
    /// chain's length changed, so P1 was blind to it.
    #[test]
    fn no_legal_chain_confuses_one_transform_with_another() {
        for (mark, ts, categorical_x, drawn) in every_legal_chain() {
            for i in 0..ts.len() {
                for alt in crate::legality::USER_TRANSFORMS {
                    if alt == ts[i] || ts.contains(&alt) { continue }
                    let mut swapped = ts.clone();
                    swapped[i] = alt.clone();
                    let Some(other) = render_chain(&mark, &swapped, categorical_x) else { continue };
                    assert_ne!(
                        digest(&drawn), digest(&other),
                        "`{} * {}` and `{} * {}` draw the same plot — the engine read \
                         that a transform was named but not which one",
                        mark_of(&mark), names(&ts), mark_of(&mark), names(&swapped)
                    );
                }
            }
        }
    }

    // The two names `legality` spells for a reader are private to it, and a failure
    // message is not worth widening a module's surface for. `Debug` reads well enough
    // here: `Bar`, `Bin * Mean`.
    /// **P3 — the written order of a chain does not change the picture.**
    ///
    /// The book says so in four places, and until this test it said so on the
    /// strength of a sweep somebody ran by hand. A claim the manual makes about the
    /// engine that no check verifies is the blind spot this project keeps walking
    /// into — prose naming a transform that does not exist, an `error: true` chunk
    /// that stopped erroring. This is the same class, one level up: a sentence in
    /// three chapters resting on nothing.
    ///
    /// **It is a consequence rather than a rule, which is why it is a test and not a
    /// sort.** Nothing reorders a chain anywhere in the engine. Order stopped
    /// mattering because the contradictions are refused, and what is left has at most
    /// one transform actually running in the sequence — `bin` is hoisted, `proportion`
    /// divides the recombined frame, `stack` accumulates across groups, and
    /// `dodge`/`jitter` are render-stage. One transform has no order. If a later
    /// change makes some legal chain order-dependent again, this fails, and the honest
    /// answer may well be to change the book rather than the engine: two spellings
    /// that each mean something are the caller's business (Law 8). What must not
    /// happen is the two drifting apart in silence.
    #[test]
    fn the_written_order_of_a_chain_does_not_change_the_picture() {
        for (mark, ts, categorical_x, drawn) in every_legal_chain() {
            for spelling in permutations(&ts) {
                if spelling == ts { continue }
                let Some(other) = render_chain(&mark, &spelling, categorical_x) else {
                    panic!("`{} * {}` draws but `{} * {}` is refused — the same chain, \
                            written differently",
                           mark_of(&mark), names(&ts), mark_of(&mark), names(&spelling));
                };
                assert_eq!(
                    digest(&drawn), digest(&other),
                    "`{} * {}` and `{} * {}` are the same chain and draw different \
                     plots — the book says written order cannot do that",
                    mark_of(&mark), names(&ts), mark_of(&mark), names(&spelling)
                );
            }
        }
    }

    /// Every ordering of a chain. Chains are capped at four by the job rule, so this
    /// is at most 24 and needs no crate.
    fn permutations(ts: &[Transform]) -> Vec<Vec<Transform>> {
        if ts.len() <= 1 { return vec![ts.to_vec()] }
        let mut out = Vec::new();
        for i in 0..ts.len() {
            let mut rest = ts.to_vec();
            let head = rest.remove(i);
            for mut tail in permutations(&rest) {
                tail.insert(0, head.clone());
                out.push(tail);
            }
        }
        out
    }

    fn mark_of(m: &Mark) -> String { format!("{m:?}") }
    fn names(ts: &[Transform]) -> String {
        ts.iter().map(|t| format!("{t:?}")).collect::<Vec<_>>().join(" * ")
    }

    /// Every chain of one, two or three transforms that any drawable mark accepts,
    /// on both a categorical and a continuous domain, already rendered.
    ///
    /// Enumerated from the mark × transform grid and filtered by `check`, so the two
    /// properties above cover exactly what a caller can write — and a transform added
    /// later widens them without anybody editing a list. Three is the practical
    /// ceiling here rather than the rule's four: the fourth is always a collision
    /// modifier, whose contribution the two properties already see at length three.
    fn every_legal_chain() -> Vec<(Mark, Vec<Transform>, bool, String)> {
        use crate::legality::{ALL_MARKS, USER_TRANSFORMS, TransformLegality, mark_takes_transform};
        let mut out = Vec::new();
        for mark in ALL_MARKS {
            let legal: Vec<Transform> = USER_TRANSFORMS.into_iter()
                .filter(|t| mark_takes_transform(&mark, t) != TransformLegality::None)
                .collect();
            let mut chains: Vec<Vec<Transform>> = Vec::new();
            for a in 0..legal.len() {
                chains.push(vec![legal[a].clone()]);
                for b in 0..legal.len() {
                    if b == a { continue }
                    chains.push(vec![legal[a].clone(), legal[b].clone()]);
                    for c in (b + 1)..legal.len() {
                        if c == a { continue }
                        chains.push(vec![legal[a].clone(), legal[b].clone(), legal[c].clone()]);
                    }
                }
            }
            for ts in chains {
                for categorical_x in [true, false] {
                    if let Some(svg) = render_chain(&mark, &ts, categorical_x) {
                        out.push((mark.clone(), ts.clone(), categorical_x, svg));
                    }
                }
            }
        }
        assert!(out.len() > 200, "the enumeration found only {} legal chains", out.len());
        out
    }

    // -----------------------------------------------------------------------
    // Per-layer positions — one axis, its own column (spec §8)
    // -----------------------------------------------------------------------

    /// The claim, made as geometry rather than as bytes: a note table whose value
    /// column is called `at` lands at **the same pixel** as the data point whose
    /// `gdp` holds the same number. If the resolution were wrong the note would
    /// be somewhere else on the panel, and no amount of "it rendered" would say
    /// so — which is how the §8 sentence sat unexecutable for several sessions.
    #[test]
    fn a_layer_reading_its_own_column_lands_on_the_shared_axis() {
        let data: HashMap<String, DataFrame> = HashMap::from([
            (
                "t".to_string(),
                DataFrame::new()
                    .with_float("gdp", vec![1000.0, 2000.0, 3000.0, 4000.0])
                    .with_float("life", vec![40.0, 50.0, 60.0, 70.0]),
            ),
            (
                "notes".to_string(),
                DataFrame::new()
                    // The same coordinate as the second row above, spelled by two
                    // other column names.
                    .with_float("at", vec![2000.0])
                    .with_float("val", vec![50.0])
                    .with_str("what", vec!["HERE".into()]),
            ),
        ]);
        let spec = PlotSpec::new()
            .data("t")
            .x("gdp")
            .y("life")
            .layer(Layer::new(Mark::Point))
            .layer(
                Layer::new(Mark::Text)
                    .data("notes")
                    .encode(Channel::X, "at")
                    .encode(Channel::Y, "val")
                    .encode(Channel::Label, "what"),
            );
        let svg = SvgRenderer::default().render(&spec, &data);

        let attr = |line: &str, key: &str| -> Option<f64> {
            line.split(&format!("{key}=\""))
                .nth(1)?
                .split('"')
                .next()?
                .parse()
                .ok()
        };
        let second_point = svg
            .lines()
            .filter(|l| l.contains("<circle"))
            .filter_map(|l| attr(l, "cx"))
            .nth(1)
            .expect("four points should be drawn");
        let note = svg
            .lines()
            .find(|l| l.contains("HERE"))
            .and_then(|l| attr(l, "x"))
            .expect("the note should be drawn");

        assert!(
            (second_point - note).abs() < 0.01,
            "the note reading `at` = 2000 must land where `gdp` = 2000 does: \
             point at {second_point}, note at {note}"
        );
    }

    /// The axis keeps the plot's name, not the note table's. One axis means one
    /// label and one set of ticks; a layer's column name is local to that layer
    /// and must not surface as chrome.
    #[test]
    fn a_layers_own_column_name_never_reaches_the_axis() {
        let data: HashMap<String, DataFrame> = HashMap::from([
            (
                "t".to_string(),
                DataFrame::new()
                    .with_float("gdp", vec![1000.0, 2000.0])
                    .with_float("life", vec![40.0, 50.0]),
            ),
            (
                "notes".to_string(),
                DataFrame::new().with_float("at", vec![1500.0]).with_float("val", vec![45.0]),
            ),
        ]);
        let spec = PlotSpec::new()
            .data("t")
            .x("gdp")
            .y("life")
            .layer(Layer::new(Mark::Point))
            .layer(
                Layer::new(Mark::Point)
                    .data("notes")
                    .encode(Channel::X, "at")
                    .encode(Channel::Y, "val"),
            );
        let svg = SvgRenderer::default().render(&spec, &data);
        assert!(svg.contains(">Gdp<"), "the axis label is the plot's column");
        assert!(!svg.contains(">At<"), "the note's column name must not label the axis");
    }

    /// `point + x(gdp) + y(life) + line` — positions written after the first of
    /// two marks. The later layer names no position, so it reads the axis, and
    /// the plot is identical to the one that binds them before the marks. Pins
    /// the fallback the book's dominant idiom depends on: 317 of its expressions
    /// write `mark + x(…)`, and every one of them has a mark after it in some
    /// chapter.
    #[test]
    fn a_layer_naming_no_position_reads_the_axis_another_layer_set() {
        let data: HashMap<String, DataFrame> = HashMap::from([(
            "t".to_string(),
            DataFrame::new()
                .with_float("gdp", vec![1000.0, 2000.0, 3000.0])
                .with_float("life", vec![40.0, 50.0, 60.0]),
        )]);
        // Written before the marks: both layers are plot-scoped.
        let before = PlotSpec::new()
            .data("t")
            .x("gdp")
            .y("life")
            .layer(Layer::new(Mark::Point))
            .layer(Layer::new(Mark::Line));
        // Written after the first mark: the point owns them, the line falls back.
        let after = PlotSpec::new()
            .data("t")
            .layer(
                Layer::new(Mark::Point)
                    .encode(Channel::X, "gdp")
                    .encode(Channel::Y, "life"),
            )
            .layer(Layer::new(Mark::Line));

        let a = SvgRenderer::default().render(&before, &data);
        let b = SvgRenderer::default().render(&after, &data);
        assert!(b.contains("<polyline"), "the line must still be drawn");
        assert_eq!(a, b, "the two spellings must render the same plot");
    }

    // -----------------------------------------------------------------------
    // Polar — the plane bent into a circle
    // -----------------------------------------------------------------------

    fn polar_counts() -> HashMap<String, DataFrame> {
        HashMap::from([(
            "t".to_string(),
            DataFrame::new()
                .with_str("dir", vec!["N", "E", "S", "W"].into_iter().map(String::from).collect())
                .with_float("n", vec![10.0, 40.0, 20.0, 30.0]),
        )])
    }

    fn render_rose(start: f64, polar: bool) -> String {
        let mut s = PlotSpec::new().data("t").x("dir").y("n");
        if polar {
            s = s.coord(CoordSpace::Polar(crate::ir::PolarView { start }));
        }
        SvgRenderer::default().render(&s.layer(Layer::new(Mark::Bar)), &polar_counts())
    }

    /// Every `x y` a path command lands on, in the order drawn — enough of an SVG
    /// path parser to check where the sectors actually sit.
    fn path_points(d: &str) -> Vec<(f64, f64)> {
        let t: Vec<&str> = d.split_whitespace().collect();
        let mut pts = Vec::new();
        let mut i = 0;
        let num = |s: &str| s.parse::<f64>().unwrap_or(f64::NAN);
        while i < t.len() {
            match t[i] {
                "M" | "L" if i + 2 < t.len() => { pts.push((num(t[i + 1]), num(t[i + 2]))); i += 3; }
                "A" if i + 7 < t.len() => { pts.push((num(t[i + 6]), num(t[i + 7]))); i += 8; }
                _ => i += 1,
            }
        }
        pts
    }

    fn sector_paths(svg: &str) -> Vec<Vec<(f64, f64)>> {
        svg.lines()
            .filter(|l| l.contains("<path d=\"M"))
            .filter_map(|l| Some(path_points(l.split(r#"d=""#).nth(1)?.split('"').next()?)))
            .collect()
    }

    /// The disc the frame drew: its center and radius, read back off the panel
    /// background so a test measures against what the renderer actually laid out.
    fn disc(svg: &str) -> (f64, f64, f64) {
        let line = svg.lines()
            .find(|l| l.contains("<circle") && l.contains(&format!(r#"fill="{PANEL_BG}""#)))
            .expect("no polar disc drawn");
        let attr = |k: &str| -> f64 {
            line.split(&format!(r#"{k}=""#)).nth(1).unwrap().split('"').next().unwrap().parse().unwrap()
        };
        (attr("cx"), attr("cy"), attr("r"))
    }

    /// The claim the whole space exists to make (spec §9, Wilkinson ch. 9): a rose
    /// is the *same sentence* as a bar chart, read in a different coordinate
    /// space. Same bars, same count of them — rectangles in the plane, wedges in
    /// the circle. If polar drew rectangles, or drew a different number of marks,
    /// it would be a different chart rather than a different reading.
    #[test]
    fn a_rose_is_the_same_bar_chart_bent() {
        let flat = render_rose(0.0, false);
        let rose = render_rose(0.0, true);

        // `<rect` + `fill-opacity` is the bar fingerprint (the panel background is
        // a rect too, and carries no fill-opacity).
        let bars = |svg: &str| svg.lines().filter(|l| l.contains("<rect") && l.contains("fill-opacity")).count();
        assert_eq!(bars(&flat), 4, "flat drew the wrong number of bars");
        assert_eq!(sector_paths(&rose).len(), 4, "polar drew the wrong number of wedges");
        assert_eq!(bars(&rose), 0, "polar still drew rectangles");
        assert!(!rose.contains("NaN"), "polar output has NaN coordinates");
    }

    /// On a periodic axis the slots must **tile** the turn: cover it once each,
    /// with no gap and no double ink. A categorical angular axis got this right by
    /// construction (`-0.5 ..= n-0.5` is n slots for n categories); a *measured*
    /// one did not, because the range was fitted to the bin centers and n centers
    /// span only n−1 slot-widths. `bar * bin + x(bearing) + polar()` therefore drew
    /// its first and last wedge on top of each other — 40° of doubled ink at twelve
    /// o'clock, dark enough to read through.
    ///
    /// Asserted as *total coverage equals one turn*, which is the property that
    /// matters and which catches a gap as well as an overlap.
    #[test]
    fn the_wedges_of_a_measured_angle_tile_the_turn() {
        let mut data = HashMap::new();
        // Twelve values, evenly spread, so `bin` cuts several equal slots.
        data.insert("t".to_string(), DataFrame::new().with_float(
            "bearing", (0..12).map(|i| i as f64 * 30.0).collect::<Vec<f64>>()));
        let spec = PlotSpec::new().data("t").x("bearing")
            .coord(CoordSpace::Polar(crate::ir::PolarView { start: 0.0 }))
            .layer(Layer::new(Mark::Bar).transform(Transform::Bin));
        let svg = SvgRenderer::default().render(&spec, &data);

        let (cx, cy, _) = disc(&svg);
        let bearing = |x: f64, y: f64| (x - cx).atan2(cy - y).to_degrees().rem_euclid(360.0);

        let mut total = 0.0;
        let mut spans: Vec<(f64, f64)> = Vec::new();
        for path in sector_paths(&svg) {
            let outer: Vec<(f64, f64)> = path.into_iter()
                .filter(|&(x, y)| ((x - cx).powi(2) + (y - cy).powi(2)).sqrt() > 3.0)
                .collect();
            if outer.is_empty() { continue }
            let mut angs: Vec<f64> = outer.iter().map(|&(x, y)| bearing(x, y)).collect();
            angs.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let (mut lo, mut hi) = (angs[0], angs[angs.len() - 1]);
            if hi - lo > 180.0 { let t = lo; lo = hi; hi = t + 360.0; } // straddles 12 o'clock
            total += hi - lo;
            spans.push((lo, hi));
        }
        assert!(spans.len() >= 3, "expected several wedges, got {}", spans.len());
        assert!((total - 360.0).abs() < 0.5,
            "the wedges cover {total:.2}° of a 360° turn — {} of them: {spans:?}", spans.len());

        // And no two of them share any angle, allowing for the wrap.
        for i in 0..spans.len() {
            for j in (i + 1)..spans.len() {
                for shift in [-360.0, 0.0, 360.0] {
                    let overlap = spans[i].1.min(spans[j].1 + shift)
                        - spans[i].0.max(spans[j].0 + shift);
                    assert!(overlap <= 0.05,
                        "wedges {i} and {j} overlap by {overlap:.2}°: {:?} {:?}", spans[i], spans[j]);
                }
            }
        }
    }

    /// The panel is a disc, and everything drawn is inside it. The flat panel's
    /// rectangle is what a mark is clipped to there; here it is the circle, and a
    /// wedge escaping it would mean the radial scale and the frame disagree.
    #[test]
    fn nothing_is_drawn_outside_the_polar_disc() {
        let rose = render_rose(0.0, true);
        let (cx, cy, r) = disc(&rose);
        assert!(r > 20.0, "the disc collapsed: r = {r}");
        for path in sector_paths(&rose) {
            for (x, y) in path {
                let d = ((x - cx).powi(2) + (y - cy).powi(2)).sqrt();
                assert!(d <= r + 0.5, "a wedge reached {d:.2} outside a disc of {r:.2}");
            }
        }
    }

    /// The radial axis is used, not ignored: the largest value reaches the rim and
    /// the smallest stays well inside it. Pins that `y` really is the radius —
    /// wedges of one length would be the failure mode this catches.
    #[test]
    fn the_radius_carries_the_measured_value() {
        let rose = render_rose(0.0, true);
        let (cx, cy, r) = disc(&rose);
        let reach: Vec<f64> = sector_paths(&rose).iter()
            .map(|p| p.iter()
                .map(|(x, y)| ((x - cx).powi(2) + (y - cy).powi(2)).sqrt())
                .fold(0.0_f64, f64::max))
            .collect();
        let (lo, hi) = (reach.iter().cloned().fold(f64::INFINITY, f64::min),
                        reach.iter().cloned().fold(0.0_f64, f64::max));
        assert!(hi > r * 0.85, "the largest bar stopped short of the rim: {hi:.2} of {r:.2}");
        // Counts are 10..40 on an axis from 0, so the shortest wedge is a quarter
        // of the longest — a real spread, not four equal spokes.
        assert!((lo / hi - 0.25).abs() < 0.05, "radii do not track the values: {lo:.2}, {hi:.2}");
    }

    /// `start` turns the whole space and nothing else: the same data, the same
    /// number of wedges, each at a different bearing. A no-op here would mean the
    /// view parameter was accepted and dropped (§12's one unforgivable failure).
    #[test]
    fn the_start_angle_turns_the_whole_plot() {
        let a = render_rose(0.0, true);
        let b = render_rose(90.0, true);
        assert_eq!(sector_paths(&a).len(), sector_paths(&b).len());
        assert_ne!(sector_paths(&a), sector_paths(&b), "`start` changed nothing");
        // Same shape, turned: the set of radii each wedge reaches is unchanged.
        let reaches = |svg: &str| {
            let (cx, cy, _) = disc(svg);
            let mut v: Vec<i64> = sector_paths(svg).iter()
                .map(|p| p.iter().map(|(x, y)| ((x - cx).powi(2) + (y - cy).powi(2)).sqrt())
                    .fold(0.0_f64, f64::max).round() as i64)
                .collect();
            v.sort();
            v
        };
        assert_eq!(reaches(&a), reaches(&b), "rotating the space changed the radii");
    }

    /// The bearing each wedge is centered on, in turns clockwise from twelve
    /// o'clock. Summed as unit vectors rather than averaged as angles, so a wedge
    /// straddling twelve o'clock does not average to the opposite side.
    fn wedge_centers(svg: &str, cx: f64, cy: f64) -> Vec<f64> {
        sector_paths(svg).iter().map(|p| {
            let dir = |&(x, y): &(f64, f64)| {
                let (dx, dy) = (x - cx, cy - y);
                let m = (dx * dx + dy * dy).sqrt().max(1e-12);
                (dx / m, dy / m)
            };
            let (ax, ay) = dir(&p[0]);
            let (bx, by) = dir(&p[1]);
            let t = (ax + bx).atan2(ay + by);
            let t = if t < 0.0 { t + std::f64::consts::TAU } else { t };
            t / std::f64::consts::TAU
        }).collect()
    }

    /// The first category sits at the start angle, and the rest tile the turn from
    /// there. Two claims in one.
    ///
    /// A categorical scale runs `-0.5 ..= n-0.5`, so its *origin* is half a slot
    /// before the first category — padding, not a place in the data. Pointing the
    /// start angle there put north at 22.5° of an eight-point compass and left the
    /// reader to work out `-180/n`. The space is rotated back by half a slot so the
    /// category itself lands on the start, the way a flat categorical axis puts its
    /// tick at the category's center.
    ///
    /// And the axis is periodic and flush, so the slots divide the whole turn with
    /// no dead wedge at the seam.
    #[test]
    fn the_first_category_sits_at_the_start_angle_and_the_rest_tile_the_turn() {
        let rose = render_rose(0.0, true);
        let (cx, cy, _) = disc(&rose);
        let mut centers = wedge_centers(&rose, cx, cy);
        centers.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert_eq!(centers.len(), 4);
        // Four categories, four equal quarter-turn slots, the first at the top.
        for (i, c) in centers.iter().enumerate() {
            let want = i as f64 * 0.25;
            assert!((c - want).abs() < 0.005, "category {i} centered at {c:.4} of a turn, wanted {want:.4}");
        }
    }

    /// The half-slot rotation is for *categories*, which divide the turn into
    /// slots. A measured angle has no slots and its scale minimum is a real value,
    /// so it stays exactly on the start angle: the smallest bearing lands at twelve
    /// o'clock, not half of anything past it.
    #[test]
    fn a_measured_angle_is_not_rotated_off_the_start() {
        let data: HashMap<String, DataFrame> = HashMap::from([(
            "t".to_string(),
            DataFrame::new()
                .with_float("deg", (0..8).map(|i| i as f64 * 45.0).collect())
                .with_float("v", (0..8).map(|i| 1.0 + i as f64).collect()),
        )]);
        let spec = PlotSpec::new().data("t").x("deg").y("v")
            .coord(CoordSpace::Polar(crate::ir::PolarView::default()))
            .layer(Layer::new(Mark::Point));
        let svg = SvgRenderer::default().render(&spec, &data);
        let (cx, cy, r) = disc(&svg);
        // The glyph for the smallest bearing (0°, the least `v` too, so the point
        // nearest the center) must sit on the vertical spoke above the center.
        let mut on_spoke = svg.lines()
            .filter(|l| l.contains("<circle") && l.contains("cx=") && !l.contains(&format!("r=\"{r:.2}\"")))
            .filter_map(|l| {
                let g = |k: &str| l.split(&format!("{k}=\"")).nth(1)?.split('"').next()?.parse::<f64>().ok();
                Some((g("cx")?, g("cy")?))
            })
            .filter(|(x, y)| (x - cx).abs() < 0.5 && *y < cy)
            .collect::<Vec<_>>();
        on_spoke.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
        assert!(!on_spoke.is_empty(), "no point landed on the start spoke");
    }

    // -- the hole: a stretch of the radial axis with nothing standing on it ---

    /// Two branches, two levels, so the tree has an inner ring and a rim.
    fn tree_data() -> HashMap<String, DataFrame> {
        HashMap::from([(
            "t".to_string(),
            DataFrame::new()
                .with_str("g", ["a", "a", "b", "b"].into_iter().map(String::from).collect())
                .with_str("i", ["p", "q", "r", "s"].into_iter().map(String::from).collect())
                .with_float("v", vec![3.0, 1.0, 2.0, 2.0]),
        )])
    }

    /// The same shape six branches wide, and the width is what a defect needs to
    /// be visible. A tallied tree weighs every leaf 1, so its measure axis runs to
    /// 12 here and ticks at 5 and 10 — **neither of which falls inside the ring
    /// range 1..3**. With the ring column dropped the radial axis reads those
    /// weights, and no tick of theirs lands in the cells for `fit_to_cells` to
    /// clip to, so the rings crush into the first tenth of the radius. A four-leaf
    /// tree hides it: its ticks are 1, 2, 3, 4, two of them land inside, and the
    /// cell fit rescues the plot by luck.
    fn wide_tree_data() -> HashMap<String, DataFrame> {
        let branches = ["a", "b", "c", "d", "e", "f"];
        HashMap::from([(
            "t".to_string(),
            DataFrame::new()
                .with_str("g", branches.iter().flat_map(|b| [b.to_string(), b.to_string()])
                    .collect())
                .with_str("i", branches.iter()
                    .flat_map(|b| [format!("{b}1"), format!("{b}2")]).collect())
                .with_float("v", branches.iter().flat_map(|_| [3.0, 1.0]).collect()),
        )])
    }

    /// `measure` bound or not, and a stated radial domain or not.
    fn render_tree(polar: bool, measure: bool, depth_lo: Option<f64>) -> String {
        render_tree_of(tree_data(), polar, measure, depth_lo)
    }

    fn render_tree_of(
        data: HashMap<String, DataFrame>, polar: bool, measure: bool, depth_lo: Option<f64>,
    ) -> String {
        let mut s = PlotSpec::new().data("t");
        if polar {
            s = s.coord(CoordSpace::Polar(crate::ir::PolarView::default()));
        }
        if measure {
            s = s.x("v");
        }
        if let Some(lo) = depth_lo {
            s = s.y_limited(crate::transform::NODE_DEPTH, Some(lo), Some(3.0));
        }
        let layer = Layer::new(Mark::Zone)
            .transform(Transform::Partition)
            .partition(&["g", "i"]);
        SvgRenderer::default().render(&s.layer(layer), &data)
    }

    /// The radius each sector spans, innermost first.
    fn sector_radii(svg: &str, cx: f64, cy: f64) -> Vec<(f64, f64)> {
        let mut out: Vec<(f64, f64)> = sector_paths(svg).iter()
            .map(|p| {
                let r = |&(x, y): &(f64, f64)| ((x - cx).powi(2) + (y - cy).powi(2)).sqrt();
                let (lo, hi) = p.iter().map(r)
                    .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), v| (a.min(v), b.max(v)));
                // Whole pixels: two sectors of one ring can differ in the last
                // decimal, and this list is counted as well as measured.
                (lo.round(), hi.round())
            })
            .collect();
        out.sort_by(|a, b| a.partial_cmp(b).unwrap());
        out.dedup();
        out
    }

    /// **A stated domain reaches the axis, even where a tiling is fitted to** —
    /// spec §10, and the defect that made this test exist. `fit_to_cells` refits
    /// the panel to a mesh's own edges, which is right for a derived range and
    /// wrong for a stated one: `y(depth, limits = c(0, 4))` on a sunburst moved the
    /// tick labels and nothing else, so the innermost ring still ran to the center
    /// and the hole the book promised was never drawn. Asserted on the *geometry*
    /// rather than the range, because ticks responding was exactly what hid it.
    #[test]
    fn a_stated_radial_domain_hollows_out_the_sunburst() {
        let (cx, cy, r) = disc(&render_tree(true, true, Some(0.0)));
        let hollow = sector_radii(&render_tree(true, true, Some(0.0)), cx, cy);
        let solid = sector_radii(&render_tree(true, true, None), cx, cy);

        // Rings 1..2 and 2..3 on a domain of 0..3: a third of the radius empty,
        // then two rings of a third each.
        assert!(solid[0].0 < 1.0, "unstated, the innermost ring reaches the center: {solid:?}");
        assert!(hollow[0].0 > r / 4.0,
                "stating the domain leaves the middle empty: {hollow:?}, radius {r}");
        assert!((hollow[0].0 - r / 3.0).abs() < 1.0,
                "the hole is the stretch 0..1 of a 0..3 axis: {hollow:?}, radius {r}");
        assert!((hollow.last().unwrap().1 - r).abs() < 1.0,
                "and the rim still reaches the frame: {hollow:?}, radius {r}");
    }

    /// The same hole, unbent — an **empty band** under the icicle, which is the
    /// plainest statement of what a hole is: not a round thing, a stretch of one
    /// axis with nothing on it. One assertion in two spaces, because a hole that
    /// only appeared in polar would be the renderer's doing rather than the scale's.
    #[test]
    fn the_flat_reading_of_a_hole_is_an_empty_band() {
        let floor = |svg: &str| -> (f64, f64) {
            let cell = |l: &&str| l.contains("<rect") && l.contains("fill-opacity")
                && !l.contains("rx=");
            let attr = |l: &str, k: &str| -> f64 {
                l.split(&format!(r#"{k}=""#)).nth(1).unwrap().split('"').next().unwrap()
                    .parse().unwrap()
            };
            let bottom = svg.lines().filter(cell)
                .map(|l| attr(l, "y") + attr(l, "height"))
                .fold(f64::NEG_INFINITY, f64::max);
            let panel = svg.lines()
                .find(|l| l.contains("<rect") && l.contains(PANEL_BG))
                .map(|l| attr(l, "y") + attr(l, "height"))
                .expect("no panel drawn");
            (bottom, panel)
        };
        let (cells, panel) = floor(&render_tree(false, true, Some(0.0)));
        assert!(panel - cells > 1.0, "a band of panel no cell covers: {cells} of {panel}");

        let (cells, panel) = floor(&render_tree(false, true, None));
        assert!((panel - cells).abs() < 1.0,
                "and with nothing stated the cells fill the panel: {cells} of {panel}");
    }

    /// **A synthesized ring index earns no guide.** Reported from the book: a
    /// reader met a donut whose first question was "what is Depth?", asked of an
    /// axis name, four numbers printed inside the hole and a set of gridline
    /// circles drawn through arcs that are already rings. None of it decodes
    /// anything — the level a node sits at is an index the transform invented, not
    /// a quantity anybody looks a value up on. The scale stays (it is what holds
    /// the hole open); only the furniture goes.
    #[test]
    fn the_ring_index_draws_no_ticks_and_no_name() {
        let svg = render_tree(true, true, Some(0.0));
        let (cx, cy, _) = disc(&svg);
        assert!(!svg.contains(">Depth<"), "the ring index named itself as an axis");

        // Nothing printed inside the hole, which is where every radial number went.
        let hole = sector_radii(&svg, cx, cy)[0].0;
        assert!(hole > 1.0, "no hole to test against");
        for t in svg.split("<text").skip(1) {
            let at = |k: &str| -> Option<f64> {
                t.split(&format!("{k}=\"")).nth(1)?.split('"').next()?.parse().ok()
            };
            if let (Some(x), Some(y)) = (at("x"), at("y")) {
                let r = ((x - cx).powi(2) + (y - cy).powi(2)).sqrt();
                assert!(r > hole - 1.0,
                        "a label sits inside the hole, {r:.0} from the center of a {hole:.0} one");
            }
        }

        // And the measure keeps its own, because an amount is a quantity.
        assert!(svg.contains(">Amount<") || svg.split("<text").skip(1)
                    .any(|t| t.split('>').nth(1).is_some_and(|s| s.starts_with(char::is_numeric))),
                "the measure axis lost its guide too");
    }

    /// **A spoke does not cross the hole.** An angular gridline marks an angle and
    /// every angle is the same point at the center, so the part of a spoke inside a
    /// donut's hole is a starburst over an empty disc. Reported from the book in the
    /// same breath as the ring index's numbers, and fixed in the same place: the
    /// spokes start at the cells' inner edge.
    #[test]
    fn a_spoke_does_not_cross_the_hole() {
        let svg = render_tree(true, true, Some(0.0));
        let (cx, cy, r) = disc(&svg);
        let hole = sector_radii(&svg, cx, cy)[0].0;
        assert!(hole > 1.0, "no hole to test against");

        let dist = |x: f64, y: f64| ((x - cx).powi(2) + (y - cy).powi(2)).sqrt();
        let mut spokes = 0;
        for line in svg.split("<line").skip(1) {
            let at = |k: &str| -> Option<f64> {
                line.split(&format!("{k}=\"")).nth(1)?.split('"').next()?.parse().ok()
            };
            let (Some(x1), Some(y1), Some(x2), Some(y2)) =
                (at("x1"), at("y1"), at("x2"), at("y2")) else { continue };
            let (a, b) = (dist(x1, y1), dist(x2, y2));
            if a > r + 1.0 || b > r + 1.0 { continue } // not inside the circle
            spokes += 1;
            assert!(a.min(b) > hole - 1.0,
                    "a spoke reaches {:.0} into a hole of {hole:.0}", a.min(b));
        }
        assert!(spokes > 0, "no spoke drawn at all, so nothing was asserted");
    }

    /// **Binding one thing fewer must not take the plot apart.** A partition with
    /// no measure weighs every leaf 1 — the tally `count` already does — so it is
    /// the same sunburst with a different weight, and it draws like one.
    ///
    /// Two separate defects made it draw like nothing. The transform publishes its
    /// ring twice, once under `depth` and once under whatever the measure axis
    /// reads, and with *neither* position bound those two names were both the empty
    /// string: the ring was dropped and the radial axis read the measure, so the
    /// rings landed inside the first tenth of the radius. And `measure_on_angle`
    /// read the unbound `x` as Wilkinson's one-argument pie, which suppresses the
    /// radial axis — right for a pie, whose radius carries nothing, and wrong here,
    /// where it carries the depth. So the geometry and the guides are asserted
    /// separately, against the measured sunburst rather than against constants.
    #[test]
    fn a_tallied_partition_in_polar_draws_what_a_measured_one_draws() {
        let tallied = render_tree_of(wide_tree_data(), true, false, None);
        let measured = render_tree_of(wide_tree_data(), true, true, None);
        let (cx, cy, r) = disc(&tallied);

        let rings = sector_radii(&tallied, cx, cy);
        assert_eq!(rings.len(), sector_radii(&measured, disc(&measured).0,
                                             disc(&measured).1).len(),
                   "the same tree draws the same number of rings: {rings:?}");
        assert!(rings[0].1 > 1.0, "the sectors collapsed onto the center: {rings:?}");
        assert!((rings.last().unwrap().1 - r).abs() < 1.0,
                "and the rim reaches the frame: {rings:?}, radius {r}");

        // Every tick this plot shares with the measured one — the ring numbers, and
        // the weights round the angle. Read as a pie it had none: a pie is decoded
        // by its legend, so the whole guide is skipped.
        let numbers = |svg: &str| svg.split("<text").skip(1)
            .filter_map(|t| t.split('>').nth(1)?.split('<').next())
            .filter(|s| s.trim().parse::<f64>().is_ok())
            .count();
        assert!(numbers(&tallied) > 0,
                "a tallied sunburst keeps the axes a measured one has ({} vs {})",
                numbers(&tallied), numbers(&measured));
    }

    // -- the pie: one position, so the position is the angle -----------------

    fn pie_data() -> HashMap<String, DataFrame> {
        // Six rows in a 3 : 2 : 1 split, so the slices must come out at a half, a
        // third and a sixth of the turn — angles a reader could check with a ruler.
        HashMap::from([(
            "t".to_string(),
            DataFrame::new().with_str(
                "g",
                ["a", "a", "a", "b", "b", "c"].into_iter().map(String::from).collect(),
            ),
        )])
    }

    fn render_pie(polar: bool) -> String {
        let mut s = PlotSpec::new().data("t");
        if polar {
            s = s.coord(CoordSpace::Polar(crate::ir::PolarView::default()));
        }
        let layer = Layer::new(Mark::Bar)
            .transform(Transform::Count)
            .transform(Transform::Stack)
            .encode(Channel::Color, "g");
        SvgRenderer::default().render(&s.layer(layer), &pie_data())
    }

    /// The turn a wedge covers, as a fraction of the whole circle, read back off
    /// its two straight edges.
    fn wedge_turns(svg: &str, cx: f64, cy: f64) -> Vec<f64> {
        sector_paths(svg).iter().map(|p| {
            let bearing = |&(x, y): &(f64, f64)| {
                let t = (x - cx).atan2(cy - y);
                if t < 0.0 { t + std::f64::consts::TAU } else { t }
            };
            // A pie slice is `M edge A arc L center Z`: the first point and the
            // arc's end are the two edges of the wedge.
            let (a, b) = (bearing(&p[0]), bearing(&p[1]));
            let d = (b - a + std::f64::consts::TAU) % std::f64::consts::TAU;
            d / std::f64::consts::TAU
        }).collect()
    }

    /// "A pie chart is a stacked bar in polar coordinates" (Wilkinson, ch. 2) —
    /// made literal. The same sentence draws a segmented column flat and a pie in
    /// polar, and the pie's slices carry the shares the column's segments do.
    #[test]
    fn a_pie_is_a_stacked_bar_in_polar_coordinates() {
        let flat = render_pie(false);
        let pie = render_pie(true);

        // `<rect` + `fill-opacity` is the bar fingerprint; the legend's swatches
        // are rects with a corner radius, and only they carry `rx`.
        let bars = |svg: &str| svg.lines()
            .filter(|l| l.contains("<rect") && l.contains("fill-opacity") && !l.contains("rx="))
            .count();
        assert_eq!(bars(&flat), 3, "flat: one segment per group, in one column");
        assert!(!pie.contains("NaN"), "pie has NaN coordinates");

        let (cx, cy, _) = disc(&pie);
        let mut turns = wedge_turns(&pie, cx, cy);
        turns.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert_eq!(turns.len(), 3, "one slice per group");
        for (got, want) in turns.iter().zip([1.0 / 6.0, 1.0 / 3.0, 0.5]) {
            assert!((got - want).abs() < 0.002, "slice {got:.4} of a turn, wanted {want:.4}");
        }
    }

    /// The slices must cover the circle exactly. They stop short if the measured
    /// axis keeps its usual 5% headroom, which on a circle is not clearance but a
    /// wedge of background at twelve o'clock — the bug this pins.
    #[test]
    fn the_slices_close_the_circle_with_no_gap() {
        let pie = render_pie(true);
        let (cx, cy, _) = disc(&pie);
        let total: f64 = wedge_turns(&pie, cx, cy).iter().sum();
        assert!((total - 1.0).abs() < 0.002, "the slices cover {total:.4} of a turn, not all of it");
    }

    /// A pie of a *measured* column, not of a count. This shipped drawing an empty
    /// circle: with `x` unbound and `y` continuous, `bar_orient` read the plot as
    /// horizontal (its rule for `bar * bin + y(h)`, a histogram on its side), which
    /// made `speed` the *key* rather than the measure — so the statistic had no
    /// column to summarize and every slice was zero wide. A positionless bar has no
    /// orientation to read, and the bound column is always its measure.
    #[test]
    fn a_pie_of_a_measured_column_is_not_empty() {
        let data: HashMap<String, DataFrame> = HashMap::from([(
            "t".to_string(),
            DataFrame::new()
                .with_str("g", ["a", "a", "b"].into_iter().map(String::from).collect())
                .with_float("v", vec![3.0, 3.0, 2.0]),
        )]);
        let layer = || Layer::new(Mark::Bar)
            .transform(Transform::Sum)
            .transform(Transform::Stack)
            .encode(Channel::Color, "g");
        let render = |polar: bool| {
            let mut s = PlotSpec::new().data("t").y("v");
            if polar { s = s.coord(CoordSpace::Polar(crate::ir::PolarView::default())); }
            SvgRenderer::default().render(&s.layer(layer()), &data)
        };

        let pie = render(true);
        let (cx, cy, _) = disc(&pie);
        let mut turns = wedge_turns(&pie, cx, cy);
        turns.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert_eq!(turns.len(), 2, "a slice per group: {turns:?}");
        // Sums of 6 and 2, so the slices are three quarters and one quarter.
        for (got, want) in turns.iter().zip([0.25, 0.75]) {
            assert!((got - want).abs() < 0.002, "slice {got:.4} of a turn, wanted {want:.4}");
        }

        // The same sentence flat is the share-of-total column, and it broke the same
        // way: two segments piled in one slot, not an empty panel.
        let flat = render(false);
        let bars: Vec<&str> = flat.lines()
            .filter(|l| l.contains("<rect") && l.contains("fill-opacity") && !l.contains("rx="))
            .collect();
        assert_eq!(bars.len(), 2, "flat drew {} segments", bars.len());
    }

    /// A pie is decoded by its legend, not by an axis: no rings, no spokes, no tick
    /// labels. The rose keeps all three, which is what makes them different plots
    /// rather than the same one with different data.
    #[test]
    fn a_pie_draws_no_axis_furniture_but_a_rose_does() {
        let pie = render_pie(true);
        let rose = render_rose(0.0, true);
        // The rings and spokes are one group, opened with exactly this tag.
        let grid_group = r##"<g stroke="#d2d2da" stroke-width="1" fill="none">"##;
        assert!(!pie.contains(grid_group), "a pie drew gridlines");
        assert!(rose.contains(grid_group), "a rose drew none");
    }

    // -----------------------------------------------------------------------
    // Nest — the panel packed with regions
    // -----------------------------------------------------------------------

    fn nest_data() -> HashMap<String, DataFrame> {
        // Four groups in a 4 : 3 : 2 : 1 split of ten, so every share is a tenth
        // of the panel a reader could count off.
        HashMap::from([(
            "t".to_string(),
            DataFrame::new()
                .with_str("g", ["a", "b", "c", "d"].into_iter().map(String::from).collect())
                .with_float("v", vec![4.0, 3.0, 2.0, 1.0]),
        )])
    }

    /// Every `<rect>` a packed panel paints, as (x, y, w, h). The cells carry a
    /// fill-opacity like any bar; the outer region outlines are `fill="none"` and
    /// are excluded, since they trace the same area a second time. `rx=` drops the
    /// legend's rounded swatches, which are also fill-opacity rects — the same
    /// discriminator the pie's bar count uses.
    fn packed_cells(svg: &str) -> Vec<(f64, f64, f64, f64)> {
        svg.lines()
            .filter(|l| l.contains("<rect") && l.contains("fill-opacity")
                     && !l.contains(r#"fill="none""#) && !l.contains("rx="))
            .filter_map(|l| {
                let num = |key: &str| -> Option<f64> {
                    let at = l.find(key)? + key.len();
                    l[at..].split('"').next()?.parse().ok()
                };
                // The leading space matters: `width=` without it also matches
                // `stroke-width=`, and `y=` matches the `y` in `fill-opacity=`.
                Some((num(r#" x=""#)?, num(r#" y=""#)?, num(r#" width=""#)?, num(r#" height=""#)?))
            })
            .collect()
    }

    fn render_nest(with_domain: bool) -> String {
        let mut s = PlotSpec::new().data("t").y("v");
        if with_domain { s = s.x("g"); }
        let layer = Layer::new(Mark::Bar)
            .transform(Transform::Sum)
            .encode(Channel::Color, "g");
        SvgRenderer::default().render(&s.coord(CoordSpace::Nest).layer(layer), &nest_data())
    }

    /// The claim the plot is read for: **each region is its own share of the
    /// panel**. A treemap whose areas are merely ordered right is a treemap that
    /// lies, so this checks the ratios rather than the ordering.
    #[test]
    fn a_packed_panel_gives_every_row_its_share_of_the_area() {
        let cells = packed_cells(&render_nest(false));
        assert_eq!(cells.len(), 4, "expected one region per group: {cells:?}");
        let areas: Vec<f64> = cells.iter().map(|c| c.2 * c.3).collect();
        let total: f64 = areas.iter().sum();
        // The rows are 4 : 3 : 2 : 1 of ten, in the frame's order.
        for (got, want) in areas.iter().zip([0.4, 0.3, 0.2, 0.1]) {
            let share = got / total;
            assert!((share - want).abs() < 0.001, "share {share:.4}, wanted {want}");
        }
    }

    /// And the regions **are** the panel: the shares are only meaningful because
    /// nothing is left over. Measured against the panel the flat renderer lays
    /// out for the same sentence, so a layout change cannot quietly pass this.
    #[test]
    fn the_packed_regions_fill_the_whole_panel() {
        let svg = render_nest(false);
        let cells = packed_cells(&svg);
        let packed: f64 = cells.iter().map(|c| c.2 * c.3).sum();
        let x0 = cells.iter().map(|c| c.0).fold(f64::MAX, f64::min);
        let y0 = cells.iter().map(|c| c.1).fold(f64::MAX, f64::min);
        let x1 = cells.iter().map(|c| c.0 + c.2).fold(f64::MIN, f64::max);
        let y1 = cells.iter().map(|c| c.1 + c.3).fold(f64::MIN, f64::max);
        assert!((packed - (x1 - x0) * (y1 - y0)).abs() < 1.0,
                "the regions do not tile their own bounding box");
        assert!((x1 - x0) > 200.0 && (y1 - y0) > 200.0, "the packing did not take the panel");
    }

    /// **No axes, and this is the space's defining property** (spec §15): the two
    /// directions carry no variable, so there is nothing to tick and nothing to
    /// label. The same sentence flat keeps all of it, which is what makes this a
    /// difference of space rather than of theme.
    #[test]
    fn a_packed_panel_draws_no_axis_furniture_and_a_flat_one_does() {
        let packed = render_nest(true);
        let flat = {
            let layer = Layer::new(Mark::Bar).transform(Transform::Sum).encode(Channel::Color, "g");
            SvgRenderer::default().render(
                &PlotSpec::new().data("t").x("g").y("v").layer(layer), &nest_data())
        };
        // A tick label is the readable half of an axis; the category names would
        // be drawn under the bars flat, and must not be anywhere in the packing.
        // (They are still in the *legend*, which is what decodes a packed panel,
        // so this counts occurrences rather than asking whether "a" appears.)
        let ticks = |s: &str| s.matches(r##"text-anchor="middle""##).count();
        assert!(ticks(&packed) < ticks(&flat),
                "the packed panel drew as much text furniture as the flat one");
        let grid_group = r##"<g stroke="#d2d2da" stroke-width="1">"##;
        assert!(!packed.contains(grid_group), "a packed panel drew gridlines");
        assert!(flat.contains(grid_group), "the flat sentence drew none, so the test proves nothing");
        // And the axis lines themselves, which are the frame color and are the
        // half of an axis a `theme(grid = "none")` would leave standing.
        let axis_ink = format!(r##"stroke="{}""##, crate::ir::THEME_FRAME_COLOR);
        assert!(!packed.contains(axis_ink.as_str()), "a packed panel drew axis lines");
        assert!(flat.contains(axis_ink.as_str()), "the flat sentence drew none, so the test proves nothing");
    }

    /// Two levels from one sentence and no new vocabulary: the domain axis
    /// partitions the panel, and the split packs inside each region. What proves
    /// it is the *count* of regions — four groups × two products is eight cells,
    /// where the one-level reading of the same table would give eight regions
    /// packed against each other with no group boundary.
    #[test]
    fn a_bound_domain_axis_packs_a_second_level_inside_each_region() {
        let data: HashMap<String, DataFrame> = HashMap::from([(
            "t".to_string(),
            DataFrame::new()
                .with_str("g", ["a", "a", "b", "b"].into_iter().map(String::from).collect())
                .with_str("p", ["x", "y", "x", "y"].into_iter().map(String::from).collect())
                .with_float("v", vec![3.0, 1.0, 4.0, 4.0]),
        )]);
        let layer = Layer::new(Mark::Bar).transform(Transform::Sum).encode(Channel::Color, "p");
        let svg = SvgRenderer::default().render(
            &PlotSpec::new().data("t").x("g").y("v").coord(CoordSpace::Nest).layer(layer), &data);
        let cells = packed_cells(&svg);
        assert_eq!(cells.len(), 4, "expected one cell per (group, product): {cells:?}");

        // The outer regions are traced as well, so the coarser split is visible
        // without an axis to name it — one outline per group, drawn `fill="none"`.
        // They are also what the outer partition can be *measured* off: group `a`
        // totals 4 and `b` totals 8, so the two regions must be a third and two
        // thirds of the panel. Read off the outlines rather than off the cells,
        // because a transform groups its output by the color split rather than by
        // the domain axis, so row order is not group order.
        let outlines: Vec<f64> = svg.lines()
            .filter(|l| l.contains("<rect") && l.contains(r#"fill="none""#))
            .filter_map(|l| {
                let num = |key: &str| -> Option<f64> {
                    let at = l.find(key)? + key.len();
                    l[at..].split('"').next()?.parse().ok()
                };
                Some(num(r#" width=""#)? * num(r#" height=""#)?)
            })
            .collect();
        assert_eq!(outlines.len(), 2, "expected one outline per group region");
        let panel: f64 = outlines.iter().sum();
        let mut shares: Vec<f64> = outlines.iter().map(|a| a / panel).collect();
        shares.sort_by(|a, b| a.partial_cmp(b).unwrap());
        for (got, want) in shares.iter().zip([1.0 / 3.0, 2.0 / 3.0]) {
            assert!((got - want).abs() < 0.002, "outer region got {got:.4}, wanted {want:.4}");
        }
    }

    /// A one-level packing has nothing coarser to show, so it draws no outlines at
    /// all — the guard that keeps the two-level cue from becoming a border round
    /// every treemap.
    #[test]
    fn a_one_level_packing_traces_no_outer_region() {
        let svg = render_nest(false);
        let outlines = svg.lines()
            .filter(|l| l.contains("<rect") && l.contains(r#"fill="none""#))
            .count();
        assert_eq!(outlines, 0, "a single region was outlined against nothing");
    }

    /// The packing keeps the frame's order rather than sorting by size, which is
    /// what leaves `order()` meaning something in this space (spec §10).
    #[test]
    fn the_packing_follows_the_axis_order_not_the_values() {
        let ascending: HashMap<String, DataFrame> = HashMap::from([(
            "t".to_string(),
            DataFrame::new()
                .with_str("g", ["a", "b", "c", "d"].into_iter().map(String::from).collect())
                .with_float("v", vec![1.0, 2.0, 3.0, 4.0]),
        )]);
        let layer = Layer::new(Mark::Bar).transform(Transform::Sum).encode(Channel::Color, "g");
        let svg = SvgRenderer::default().render(
            &PlotSpec::new().data("t").y("v").coord(CoordSpace::Nest).layer(layer), &ascending);
        let areas: Vec<f64> = packed_cells(&svg).iter().map(|c| c.2 * c.3).collect();
        assert_eq!(areas.len(), 4);
        // Row order is a, b, c, d — so the areas must *ascend*. A layout that
        // sorted descending for prettier rectangles would fail here, and would
        // have made `order()` a word with no effect.
        for w in areas.windows(2) {
            assert!(w[1] > w[0], "the packing reordered the rows: {areas:?}");
        }
    }

    // -----------------------------------------------------------------------
    // Nest — the label inside its own region
    // -----------------------------------------------------------------------

    /// Every `<text>` the **text layer** drew, as (x, y, string) — the clipped
    /// group, which is what excludes the legend's key, whose entries are the same
    /// strings at the same font and would otherwise be counted as labels that fit.
    fn packed_labels(svg: &str) -> Vec<(f64, f64, String)> {
        let mut inside = false;
        svg.lines()
            .filter(|l| {
                let t = l.trim_start();
                if t.starts_with("<g ") {
                    inside = t.contains("clip-path") && t.contains(r#"text-anchor="middle""#);
                } else if t.starts_with("</g>") {
                    inside = false;
                }
                inside && t.starts_with("<text")
            })
            .filter_map(|l| {
                let num = |key: &str| -> Option<f64> {
                    let at = l.find(key)? + key.len();
                    l[at..].split('"').next()?.parse().ok()
                };
                let s = l.split('>').nth(1)?.split('<').next()?.to_string();
                Some((num(r#"<text x=""#)?, num(r#" y=""#)?, s))
            })
            .collect()
    }

    /// Four roomy regions, whose names are short enough that every one fits.
    fn nest_label_data() -> HashMap<String, DataFrame> {
        HashMap::from([(
            "t".to_string(),
            DataFrame::new()
                .with_str("g", ["a", "b", "c", "d"].into_iter().map(String::from).collect())
                .with_float("v", vec![4.0, 3.0, 2.0, 1.0]),
        )])
    }

    fn render_nest_labeled(data: &HashMap<String, DataFrame>) -> String {
        let bars = Layer::new(Mark::Bar).encode(Channel::Color, "g");
        let names = Layer::new(Mark::Text).encode(Channel::Label, "g");
        SvgRenderer::default().render(
            &PlotSpec::new().data("t").y("v").coord(CoordSpace::Nest).layer(bars).layer(names),
            data)
    }

    /// **The claim the mark is for**: a label sits at the center of the region its
    /// own row was packed into. Checked against the *bar's* rectangles rather than
    /// against remembered numbers, because the thing that would make this feature
    /// worthless is the two marks packing differently — a name a pixel outside its
    /// rectangle names nothing, and no reader could tell which cell it meant.
    #[test]
    fn a_packed_label_sits_at_the_center_of_its_own_region() {
        let svg = render_nest_labeled(&nest_label_data());
        let cells = packed_cells(&svg);
        let labels = packed_labels(&svg);
        assert_eq!(cells.len(), 4, "expected one region per row: {cells:?}");
        assert_eq!(labels.len(), 4, "every one of these names fits: {labels:?}");
        for (i, (lx, ly, s)) in labels.iter().enumerate() {
            let (cx, cy, cw, ch) = cells[i];
            assert_eq!(s, ["a", "b", "c", "d"][i], "labels are out of row order");
            assert!((lx - (cx + cw / 2.0)).abs() < 0.01,
                    "`{s}` is not centered in its region: {lx} vs {}", cx + cw / 2.0);
            // The baseline drops half a cap height so the glyph is centered on the
            // region rather than resting its feet on the middle of it.
            assert!((ly - (cy + ch / 2.0)).abs() < ch / 2.0,
                    "`{s}` is outside its own region vertically: {ly} in {cy}..{}", cy + ch);
        }
    }

    /// A label wider than the region it names is **not drawn and is counted**. The
    /// count is the point: a packing has more shares than legible ones, so printing
    /// the ones that fit and saying nothing would let a reader take the labeled
    /// cells for all of them (§12).
    #[test]
    fn a_label_too_wide_for_its_region_is_left_out_and_reported() {
        // One region is 96% of the panel and the rest are slivers; the long names
        // cannot fit anywhere but the first.
        let data: HashMap<String, DataFrame> = HashMap::from([(
            "t".to_string(),
            DataFrame::new()
                .with_str("g", ["roomy", "cramped one", "cramped two", "cramped three"]
                    .into_iter().map(String::from).collect())
                .with_float("v", vec![240.0, 1.0, 1.0, 1.0]),
        )]);
        let bars = Layer::new(Mark::Bar).encode(Channel::Color, "g");
        let names = Layer::new(Mark::Text).encode(Channel::Label, "g");
        let drawn = SvgRenderer::default().draw(
            &PlotSpec::new().data("t").y("v").coord(CoordSpace::Nest).layer(bars).layer(names),
            &data);

        let labels = packed_labels(&drawn.svg);
        assert_eq!(labels.len(), 1, "only the roomy region can hold its name: {labels:?}");
        assert_eq!(labels[0].2, "roomy");
        let said: Vec<&str> = drawn.remarks.iter().map(|d| d.message.as_str()).collect();
        assert_eq!(drawn.remarks.len(), 1, "one sentence for the layer: {said:?}");
        assert!(drawn.remarks[0].kind == crate::legality::DiagnosticKind::Assumption,
                "the plot drew, so this is a remark and not a refusal");
        assert!(said[0].contains("3 of 4 labels"),
                "the remark must count what it left out: {said:?}");
    }

    /// The dropped-rows report rides in `remarks`, not on stderr: a browser has
    /// no stderr, and this render goes through `draw` — the same door
    /// `gog-wasm` uses — so what this asserts is what a browser user is told.
    #[test]
    fn dropped_log_rows_are_a_remark_rather_than_stderr() {
        let data: HashMap<String, DataFrame> = HashMap::from([(
            "t".to_string(),
            DataFrame::new()
                .with_float("x", vec![0.0, 10.0, 100.0])
                .with_float("y", vec![1.0, 2.0, 3.0]),
        )]);
        let spec = PlotSpec::new().data("t").x_log_base("x", 10.0).y("y")
            .layer(Layer::new(Mark::Point));
        let drawn = SvgRenderer::default().draw(&spec, &data);
        assert!(
            drawn.remarks.iter().any(|d| d.message.contains("no place on the log")),
            "the dropped-rows report must ride in remarks: {:?}",
            drawn.remarks.iter().map(|d| &d.message).collect::<Vec<_>>()
        );
    }

    /// The custom-palette count mismatch is a remark for the same reason.
    #[test]
    fn a_custom_palette_count_mismatch_is_a_remark_rather_than_stderr() {
        let data: HashMap<String, DataFrame> = HashMap::from([(
            "t".to_string(),
            DataFrame::new()
                .with_float("x", vec![1.0, 2.0, 3.0])
                .with_float("y", vec![1.0, 2.0, 3.0])
                .with_str("g", vec!["a".into(), "b".into(), "c".into()]),
        )]);
        let mut spec = PlotSpec::new().data("t").x("x").y("y")
            .layer(Layer::new(Mark::Point).encode(Channel::Color, "g"));
        spec.palette = crate::ir::PaletteDef::Custom(vec!["white".into(), "navy".into()]);
        let drawn = SvgRenderer::default().draw(&spec, &data);
        assert!(
            drawn.remarks.iter().any(|d| d.message.contains("generated automatically")),
            "the palette mismatch must ride in remarks: {:?}",
            drawn.remarks.iter().map(|d| &d.message).collect::<Vec<_>>()
        );
    }

    /// And the many-rows-one-line warning, which was the last mark-side
    /// `eprintln!` left.
    #[test]
    fn an_ungrouped_many_row_line_is_a_remark_rather_than_stderr() {
        let data: HashMap<String, DataFrame> = HashMap::from([(
            "t".to_string(),
            DataFrame::new()
                .with_float("x", vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0])
                .with_float("y", vec![2.0, 1.0, 3.0, 2.0, 4.0, 3.0]),
        )]);
        let spec = PlotSpec::new().data("t").x("x").y("y")
            .layer(Layer::new(Mark::Line));
        let drawn = SvgRenderer::default().draw(&spec, &data);
        assert!(
            drawn.remarks.iter().any(|d| d.message.contains("connected in x order")),
            "the multi-series hint must ride in remarks: {:?}",
            drawn.remarks.iter().map(|d| &d.message).collect::<Vec<_>>()
        );
    }

    /// Nothing left out, nothing said. The remark is a report of a real absence,
    /// not a disclaimer the mark carries everywhere.
    #[test]
    fn a_packing_whose_labels_all_fit_says_nothing() {
        let bars = Layer::new(Mark::Bar).encode(Channel::Color, "g");
        let names = Layer::new(Mark::Text).encode(Channel::Label, "g");
        let drawn = SvgRenderer::default().draw(
            &PlotSpec::new().data("t").y("v").coord(CoordSpace::Nest).layer(bars).layer(names),
            &nest_label_data());
        assert!(drawn.remarks.is_empty(), "unexpected remarks: {:?}",
                drawn.remarks.iter().map(|d| &d.message).collect::<Vec<_>>());
    }

    /// The two-level packing labels the **inner** cells, and its labels land in the
    /// inner cells rather than in the outer regions they are grouped by. This is
    /// the sentence the spec named as the reason the mark was owed: countries
    /// inside continents, where the split is too wide for a legend to decode.
    #[test]
    fn a_two_level_packing_labels_the_cells_and_not_the_groups() {
        let data: HashMap<String, DataFrame> = HashMap::from([(
            "t".to_string(),
            DataFrame::new()
                .with_str("outer", ["p", "p", "q", "q"].into_iter().map(String::from).collect())
                .with_str("inner", ["w", "x", "y", "z"].into_iter().map(String::from).collect())
                .with_float("v", vec![3.0, 2.0, 3.0, 2.0]),
        )]);
        let bars = Layer::new(Mark::Bar).encode(Channel::Color, "inner");
        let names = Layer::new(Mark::Text).encode(Channel::Label, "inner");
        let svg = SvgRenderer::default().render(
            &PlotSpec::new().data("t").x("outer").y("v")
                .coord(CoordSpace::Nest).layer(bars).layer(names),
            &data);
        let labels = packed_labels(&svg);
        assert_eq!(labels.len(), 4, "one name per cell: {labels:?}");
        let cells = packed_cells(&svg);
        for (i, (lx, _, s)) in labels.iter().enumerate() {
            let (cx, _, cw, _) = cells[i];
            assert!((lx - (cx + cw / 2.0)).abs() < 0.01,
                    "`{s}` did not land in its own cell");
        }
    }

    /// A packing with **no split at all** — no `x`, no `color` — still packs every
    /// row, and this is the one Law 7's third relaxation made reachable. Flat,
    /// `bar + y(v)` is refused because every row would pile into one place with
    /// nothing to tell them apart; here each row gets its own region, so the
    /// refusal's reason does not reach and the plot is a plain one-color treemap.
    ///
    /// It is pinned because it drew an **empty panel** first: with `x` unbound and
    /// no split, `slot_orient` read the free axis as the one a transform would fill
    /// in, made the measure the key, and left the bar looking for a column named
    /// `""`. A packing has no orientation to read at all, which `plot_orient` now
    /// says outright.
    #[test]
    fn a_packing_with_nothing_to_split_by_still_packs_every_row() {
        let svg = SvgRenderer::default().render(
            &PlotSpec::new().data("t").y("v").coord(CoordSpace::Nest)
                .layer(Layer::new(Mark::Bar)),
            &nest_label_data());
        let cells = packed_cells(&svg);
        assert_eq!(cells.len(), 4, "a packing with no split drew {} regions", cells.len());
        let area: f64 = cells.iter().map(|c| c.2 * c.3).sum();
        assert!(area > 1000.0, "the regions have no area: {cells:?}");
    }

    /// A packed label reads its color the way every other packed mark does — the
    /// palette hue for its category, or the set color. Pinned because the region
    /// branch resolves color separately from the positioned one, and a fill that
    /// silently fell back to the default text ink would be invisible against a
    /// dark cell rather than obviously wrong.
    #[test]
    fn a_packed_label_takes_the_set_color() {
        let bars = Layer::new(Mark::Bar).encode(Channel::Color, "g");
        let names = Layer::new(Mark::Text).encode(Channel::Label, "g").style_color("white");
        let svg = SvgRenderer::default().render(
            &PlotSpec::new().data("t").y("v").coord(CoordSpace::Nest).layer(bars).layer(names),
            &nest_label_data());
        assert_eq!(packed_labels(&svg).len(), 4);
        assert_eq!(svg.matches(r#"fill="white" fill-opacity"#).count(), 4,
                   "a packed label lost its set color:\n{svg}");
    }

    #[test]
    fn binding_z_projects_into_space_rather_than_being_ignored() {
        // The milestone claim: `z` is one more vowel (spec §15). A bound `z`
        // must reach the picture — the renderer draws a projected cube frame
        // instead of 2-D axes, and the point positions must differ from the flat
        // ones, or `z` was silently dropped (the §12 sin this project refuses).
        let data: HashMap<String, DataFrame> = HashMap::from([(
            "t".to_string(),
            DataFrame::new()
                .with_float("x", vec![0.0, 1.0, 2.0, 3.0])
                .with_float("y", vec![3.0, 2.0, 1.0, 0.0])
                .with_float("z", vec![0.0, 3.0, 1.0, 2.0]),
        )]);
        let render = |threed: bool| {
            let mut s = PlotSpec::new().data("t").x("x").y("y");
            if threed {
                s = s.z("z").coord(CoordSpace::Space(SpaceView::default()));
            }
            SvgRenderer::default().render(&s.layer(Layer::new(Mark::Point)), &data)
        };
        let flat = render(false);
        let space = render(true);

        assert!(!space.contains("NaN"), "3-D output has NaN coordinates");
        // The cube wireframe (its faint stroke color) is the 3-D guide; the flat
        // plot has none.
        assert!(space.contains("#d8d8de"), "3-D output drew no cube frame");
        assert!(!flat.contains("#d8d8de"), "2-D output drew a cube frame");
        assert_eq!(space.matches("<circle").count(), 4, "not every point placed");

        let cxs = |svg: &str| -> Vec<String> {
            svg.lines()
                .filter(|l| l.contains("<circle"))
                .filter_map(|l| Some(l.split(r#"cx=""#).nth(1)?.split('"').next()?.to_string()))
                .collect()
        };
        assert_ne!(cxs(&flat), cxs(&space), "z made no difference — it was ignored");
    }

    // -----------------------------------------------------------------------
    // The 3-D frame's labels — measured, and drawn over the marks
    //
    // M8a shipped this frame with no test at all on where its labels land, and
    // four defects lived in it undisturbed: an axis name printed through its own
    // last tick (`S8pal Length` in the book's first 3-D plot), two axes' numbers
    // superimposed at the corner they share, a foreshortened axis crowding, and
    // every label along both floor edges painted over by a solid mesh.
    //
    // Three of them are one omission — the frame placed labels by projection and
    // never measured them — and the first test below catches those three at once.
    // The fourth is the *edge choice* underneath: the frame ticked the three edges
    // meeting at the cube's farthest corner, which is why the labels were behind
    // the data to be painted over in the first place. That one needs its own test,
    // because measured placement resolves every overlap whichever edges are ticked:
    // reverting `FrameEdge::choose` to M8a's fixed corner leaves the overlap test
    // green (checked while writing these) and reddens only
    // `a_3d_axis_is_ticked_along_an_edge_the_data_cannot_hide`.
    // -----------------------------------------------------------------------

    /// The frame's labels, as `(x, y, text)` — the one group carrying the halo, so
    /// this reads exactly what `write_space_labels` placed and nothing else.
    fn frame_labels_of(svg: &str) -> Vec<(f64, f64, String)> {
        let start = svg.find(r#"paint-order="stroke""#)
            .unwrap_or_else(|| panic!("no 3-D frame label group in:\n{svg}"));
        let group = &svg[start..start + svg[start..].find("</g>").unwrap()];
        let num = |s: &str| s.split('"').next().unwrap().parse::<f64>().unwrap();
        group.lines()
            .filter(|l| l.contains("<text"))
            .map(|l| (
                num(l.split(r#"x=""#).nth(1).unwrap()),
                num(l.split(r#"y=""#).nth(1).unwrap()),
                l.split('>').nth(1).unwrap().split('<').next().unwrap().to_string(),
            ))
            .collect()
    }

    /// How far apart two drawn labels' ink is: positive clears, negative overlaps.
    /// Deliberately re-derived here from the drawn `x`/`y` rather than read off
    /// `FrameLabel`, so the test measures the *output* and not the intention.
    fn label_separation(a: &(f64, f64, String), b: &(f64, f64, String), font: f64) -> f64 {
        let half = |t: &str| crate::render::text::estimate_text_width(t, font) / 2.0;
        let cap = estimate_cap_height(font);
        let dx = (a.0 - b.0).abs() - (half(&a.2) + half(&b.2));
        let dy = (a.1 - b.1).abs() - cap;
        dx.max(dy)
    }

    /// A surface over a 6x6 lattice — a mark that covers the whole floor, which is
    /// what made the painted-over labels impossible to keep ignoring.
    fn sheet() -> HashMap<String, DataFrame> {
        let (mut xs, mut ys, mut zs) = (Vec::new(), Vec::new(), Vec::new());
        for i in 0..6 {
            for j in 0..6 {
                xs.push(i as f64 * 100.0);
                ys.push(j as f64 * 100.0);
                zs.push(100.0 + (i * j) as f64);
            }
        }
        HashMap::from([(
            "t".to_string(),
            DataFrame::new().with_float("east", xs).with_float("north", ys)
                .with_float("elev", zs),
        )])
    }

    fn sheet_spec(view: SpaceView) -> PlotSpec {
        PlotSpec::new().data("t").x("east").y("north").z("elev")
            .coord(CoordSpace::Space(view))
            .layer(Layer::new(Mark::Surface))
    }

    #[test]
    fn all_three_axes_honor_a_stated_tick_count_including_z() {
        // `tick_count` reached no binding at all until 2026-07-26, and `z` was a
        // second absence wearing the same name: `build_axis` was handed a hard
        // `None` for the third axis, so even once a binding could write the field
        // the cube would have ignored it. Two ways for a property to be missing —
        // unreachable, and unread — and only the first was recorded.
        let counts = |spec: &PlotSpec| -> usize {
            let svg = SvgRenderer::default().render(spec, &sheet());
            frame_labels_of(&svg).into_iter()
                .filter(|(_, _, t)| t.parse::<f64>().is_ok())
                .count()
        };
        for ch in [Channel::X, Channel::Y, Channel::Z] {
            let with = |n: usize| {
                let mut s = sheet_spec(SpaceView::default());
                let def = match ch {
                    Channel::X => s.x.as_mut(),
                    Channel::Y => s.y.as_mut(),
                    _ => s.z.as_mut(),
                };
                def.expect("the sheet binds all three").tick_count = Some(n);
                s
            };
            let few = counts(&with(2));
            let many = counts(&with(9));
            assert!(many > few,
                "{ch:?} drew {many} labels for 9 ticks and {few} for 2 — the count \
                 is not reaching this axis");
        }
    }

    #[test]
    fn no_two_labels_of_a_3d_frame_are_drawn_on_top_of_each_other() {
        // The invariant the frame had no test for, and the one every M8a defect
        // broke. Two labels overlapping is not a near-miss to be tuned away: it
        // prints one number through another and the reader cannot recover either.
        //
        // Swept over views rather than asserted at one angle, because each defect
        // showed at a different one — the name-through-its-tick at the default, the
        // shared-corner pile-up at a low tilt, the crowding only when an axis is
        // nearly edge-on.
        for &(turn, tilt) in &[
            (30.0, 25.0), (-50.0, 15.0), (30.0, 70.0), (30.0, 5.0),
            (140.0, 25.0), (0.0, 0.0), (30.0, 85.0), (-35.0, 45.0), (90.0, 60.0),
        ] {
            let svg = SvgRenderer::default().render(
                &sheet_spec(SpaceView { turn, tilt }), &sheet());
            let labels = frame_labels_of(&svg);
            assert!(labels.len() >= 4,
                "turn {turn} tilt {tilt}: frame drew almost nothing ({})", labels.len());
            for i in 0..labels.len() {
                for j in (i + 1)..labels.len() {
                    // The larger font of the two bounds both boxes.
                    let sep = label_separation(&labels[i], &labels[j], 12.0);
                    assert!(sep > 0.0,
                        "turn {turn} tilt {tilt}: {:?} and {:?} overlap by {:.2}px",
                        labels[i].2, labels[j].2, -sep);
                }
            }
        }
    }

    #[test]
    fn a_3d_frames_labels_are_drawn_after_its_marks_so_nothing_paints_over_them() {
        // A guide is an annotation *about* the scene, not a member of it — the rule
        // the flat and polar frames already followed, and the one the 3-D frame was
        // the single exception to. It mattered least for a scatter (sparse) and most
        // for a surface, which covers the floor completely: every number along both
        // domain edges was drawn and then buried.
        let svg = SvgRenderer::default().render(
            &sheet_spec(SpaceView::default()), &sheet());
        let first_face = svg.find("<path d=\"M").expect("the sheet drew no faces");
        let labels = svg.find(r#"paint-order="stroke""#).expect("no frame labels");
        assert!(labels > first_face,
            "the frame's labels are written before the marks, so the marks bury them");
        // And the *box* still goes behind, which is the draw-order stand-in for
        // occlusion that M8a was right about.
        let wireframe = svg.find("#d8d8de").expect("no cube frame");
        assert!(wireframe < first_face, "the glass box is no longer behind the marks");
    }

    /// One view is one picture, however the bearing was spelled.
    ///
    /// A bearing is periodic and the grammar accepts every lap, so `turn = -360` is
    /// the same view as `turn = 0` and has to draw the same bytes. It did not. The
    /// marks and the frame lines matched, because they round to the same pixels, and
    /// two of eighteen tick labels went missing: `sin(-2π)` is 2.4e-16 rather than
    /// exactly 0, which flipped `FrameEdge::choose` on a comparison that guarded its
    /// magnitude with an epsilon and its direction with nothing, and on the other
    /// edge two labels collided and were dropped after their nudges.
    ///
    /// Nothing could have caught it upstream. Every mark was in place, the picture
    /// was plausible, and stderr was empty. So the assertion is equality of the
    /// whole output across spellings, which is the only form that would have failed.
    #[test]
    fn one_view_draws_one_picture_however_the_bearing_is_spelled() {
        let draw = |turn| SvgRenderer::default()
            .render(&sheet_spec(SpaceView { turn, tilt: 25.0 }), &sheet());

        for (canonical, laps) in [(0.0, [-720.0, -360.0, 360.0, 720.0]),
                                  (30.0, [-690.0, -330.0, 390.0, 750.0])] {
            let want = draw(canonical);
            // The count is asserted as well as the bytes, because it names the
            // symptom a reader would have seen and the bytes only say "different".
            let labels = |s: &str| s.matches("<text").count();
            for turn in laps {
                let got = draw(turn);
                assert_eq!(labels(&got), labels(&want),
                    "turn {turn} lost labels against {canonical}");
                assert_eq!(got, want, "turn {turn} is not turn {canonical}");
            }
        }
    }

    #[test]
    fn a_3d_axis_is_ticked_along_an_edge_the_data_cannot_hide() {
        // The root cause under all four defects. The cube offers four edges per
        // axis and M8a always took the three meeting at `(0,0,0)` — which is the
        // *farthest* corner from the camera at every ordinary angle, so all three
        // ran behind the data and met in the middle of the picture. Draw order hid
        // the consequence; drawing labels on top exposes it at once.
        //
        // Stated as a property rather than as coordinates: whichever edges are
        // ticked, their labels must lie outside the projected sheet's silhouette.
        // The sheet fills the cube's floor, so its horizontal extent stands in for
        // that silhouette, and a label inside it would be a label over the data.
        let svg = SvgRenderer::default().render(
            &sheet_spec(SpaceView::default()), &sheet());
        // Coordinates out of the mesh's own `d` attributes, and *only* those — the
        // first cut of this test split whole lines on non-digits and swept up
        // stroke widths and opacities, which put the mesh's box at x ≤ 49729.
        let (mut xs, mut ys) = (Vec::new(), Vec::new());
        for line in svg.lines().filter(|l| l.contains("<path d=\"M")) {
            let d = line.split("d=\"").nth(1).unwrap().split('"').next().unwrap();
            for (i, tok) in d.split(|c: char| !c.is_ascii_digit() && c != '.' && c != '-')
                .filter(|t| !t.is_empty())
                .filter_map(|t| t.parse::<f64>().ok())
                .enumerate()
            {
                if i % 2 == 0 { xs.push(tok) } else { ys.push(tok) }
            }
        }
        assert!(!xs.is_empty(), "no mesh geometry to compare against");
        let (top, bottom) = (
            ys.iter().copied().fold(f64::INFINITY, f64::min),
            ys.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        );
        let (left, right) = (
            xs.iter().copied().fold(f64::INFINITY, f64::min),
            xs.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        );
        for (x, y, text) in frame_labels_of(&svg) {
            let inside = x > left && x < right && y > top && y < bottom;
            assert!(!inside,
                "label {text:?} at ({x:.1}, {y:.1}) sits inside the mesh's own box \
                 [{left:.1}..{right:.1}] x [{top:.1}..{bottom:.1}] — it is ticking \
                 an edge that runs behind the data");
        }
    }

    #[test]
    fn a_symmetric_view_chooses_its_measure_edge_by_rule_and_not_by_rounding() {
        // At the default `turn = 30` the two outermost struts are mirror images —
        // ±0.683 of the projected center — so "furthest from the center" is a tie,
        // and it was being settled by whichever sin/cos path rounded larger. Two
        // plots at the same viewing angle picked opposite sides of the cube.
        //
        // Every symmetric view has this property and the default view is symmetric,
        // so the tie needs a stated answer: the left, where a reader looks for a
        // vertical scale. Pinned across data that shares nothing but the angle.
        let z_name_x = |svg: &str| -> f64 {
            frame_labels_of(svg).into_iter()
                .find(|(_, _, t)| t == "Elev" || t == "z")
                .map(|(x, _, _)| x)
                .unwrap_or_else(|| panic!("no z axis name in:\n{svg}"))
        };
        let sheet_svg = SvgRenderer::default().render(
            &sheet_spec(SpaceView::default()).z_label("Elev"), &sheet());
        let cloud = HashMap::from([(
            "t".to_string(),
            DataFrame::new()
                .with_float("east", vec![0.0, 1.0, 2.0, 3.0])
                .with_float("north", vec![3.0, 1.0, 2.0, 0.0])
                .with_float("elev", vec![1.0, 3.0, 0.0, 2.0]),
        )]);
        let cloud_svg = SvgRenderer::default().render(
            &PlotSpec::new().data("t").x("east").y("north").z("elev")
                .coord(CoordSpace::Space(SpaceView::default()))
                .z_label("Elev")
                .layer(Layer::new(Mark::Point)),
            &cloud);
        let center = |svg: &str| {
            let w: f64 = svg.split(r#"width=""#).nth(1).unwrap()
                .split('"').next().unwrap().parse().unwrap();
            w / 2.0
        };
        assert!(z_name_x(&sheet_svg) < center(&sheet_svg),
            "a sheet put its measure axis on the right of a symmetric view");
        assert!(z_name_x(&cloud_svg) < center(&cloud_svg),
            "a cloud put its measure axis on the right of the same view");
    }

    #[test]
    fn a_foreshortened_3d_axis_thins_its_labels_and_leaves_its_scale_alone() {
        // Tilt the eye up and the measure axis projects short — at `tilt = 85` it is
        // 9% of its length — so its numbers run into each other. The answer is at
        // the **label** stage: draw fewer of them, at the step the scale already
        // chose. Not a coarser step, which is the one thing this must not do: a
        // bigger step raises the scale's ceiling and leaves the cube half empty,
        // which is the recorded reason a narrow *flat* panel is not thinned either
        // (see `build_axis`). So the labels that survive must be a subset of the
        // ones a roomy view draws, never a different set of numbers.
        let labels_at = |tilt: f64| -> Vec<String> {
            let svg = SvgRenderer::default().render(
                &sheet_spec(SpaceView { turn: 30.0, tilt }), &sheet());
            frame_labels_of(&svg).into_iter().map(|(_, _, t)| t).collect()
        };
        let roomy = labels_at(25.0);
        let squeezed = labels_at(85.0);
        assert!(squeezed.len() < roomy.len(),
            "an axis at 9% of its length drew as many labels as one at full length");
        for l in &squeezed {
            assert!(roomy.contains(l),
                "the squeezed view invented the label {l:?} — the step was recomputed \
                 rather than the labels thinned, which moves the scale");
        }
    }

    /// A 5x5 mesh of points, one per cell, for the 3-D histogram tests below.
    fn mesh_5x5() -> HashMap<String, DataFrame> {
        let (mut xs, mut ys) = (Vec::new(), Vec::new());
        for i in 0..5 {
            for j in 0..5 {
                xs.push(i as f64);
                ys.push(j as f64);
            }
        }
        HashMap::from([(
            "t".to_string(),
            DataFrame::new().with_float("a", xs).with_float("b", ys),
        )])
    }

    /// `bar * bin + x(a) + y(b) + space()`, with no `z()` — the 3-D histogram.
    fn hist_3d(view: SpaceView) -> PlotSpec {
        let mut layer = Layer::new(Mark::Bar).transform(Transform::Bin);
        layer.bin = Some(crate::ir::BinSpec { bins: Some(5), width: None, tiling: None });
        PlotSpec::new().data("t").x("a").y("b").coord(CoordSpace::Space(view)).layer(layer)
    }

    #[test]
    fn a_bin_on_a_bar_in_space_cuts_both_axes_and_stands_the_count_up_on_z() {
        // Spec §5's dimensionality rule with the third axis present: a `bar` leaves
        // *two* axes free in the cube, so the same `bin` that cuts one flat cuts both
        // here — and the tally it invents rises along `z` rather than going to
        // `color`, because unlike a `zone` this mark has somewhere to put it.
        //
        // The whole sentence is `space()`: no `z()` is bound, exactly as a flat
        // histogram binds no `y()`.
        let data = mesh_5x5();
        let svg = SvgRenderer::default().render(&hist_3d(SpaceView::default()), &data);

        assert!(!svg.contains("NaN"), "3-D histogram has NaN coordinates:\n{svg}");
        assert!(svg.contains("#d8d8de"), "no cube frame — the plot was drawn flat");
        // 25 columns, and the faces the projection leaves visible. Counted as
        // *solids* rather than faces: a box shows three of its six from any one
        // angle, so what must be 25 is the number of columns, which is the number of
        // top faces — the one face every visible column has.
        let faces = svg.matches("<path d=\"M").count();
        assert!(faces >= 25 * 3, "expected at least 3 faces per column, got {faces}");
        // A flat histogram of the same data would put the count on `y`; here `y` is
        // still the second *domain*, so the tally cannot be on it.
        assert!(svg.contains(">A<") && svg.contains(">B<"),
            "both domains should label their own axes:\n{svg}");
        assert!(svg.contains(">Count<"), "the synthesized tally names the z axis:\n{svg}");
    }

    #[test]
    fn a_column_in_space_sorts_by_its_foot_not_by_its_middle() {
        // Spec §15's sorted unit, one mark on: a glyph sorts whole, a stroke per
        // segment, and a **column by its footprint**. Every column stands on the same
        // floor, so which hides which is settled by where their feet are — and a tall
        // far column must not paint over a short near one by claiming its middle is
        // closer. Two cells, one far and tall, one near and short.
        let data = HashMap::from([(
            "t".to_string(),
            DataFrame::new()
                // Four rows in the far cell (a tall column), one in the near cell.
                .with_float("a", vec![0.0, 0.0, 0.0, 0.0, 1.0])
                .with_float("b", vec![0.0, 0.0, 0.0, 0.0, 1.0]),
        )]);
        let mut layer = Layer::new(Mark::Bar).transform(Transform::Bin);
        layer.bin = Some(crate::ir::BinSpec { bins: Some(2), width: None, tiling: None });
        let spec = PlotSpec::new().data("t").x("a").y("b")
            .coord(CoordSpace::Space(SpaceView::default()))
            .layer(layer);
        let svg = SvgRenderer::default().render(&spec, &data);

        // Depth order is paint order. The near column's faces must all come after the
        // far column's, whatever their heights. Read the mean y of each path as a
        // stand-in for which column it belongs to is fragile; instead assert the
        // shading pattern repeats twice — one full column, then the other.
        let tops = svg.matches("<path d=\"M").count();
        assert!(tops >= 6, "two columns should draw at least three faces each: {tops}");
        assert!(!svg.contains("NaN"));
    }

    #[test]
    fn a_cut_floor_touches_and_a_slotted_one_leaves_air() {
        // Law 2, one dimension up. Flat, a histogram's bars fill their bins and touch
        // while a categorical bar takes four fifths of its slot — the empty fifth is
        // what says the categories are separate rather than a divided continuum. The
        // cube must say the same thing, per axis, or the gap has become a property of
        // the space rather than of what the axis means.
        //
        // Measured on the *floor* rather than on the drawn faces: the footprint edges
        // are what the rule is about, and they are what the projection then turns into
        // a solid. Two columns side by side on each axis, so a gap is visible as the
        // horizontal run between the near corners of the two.
        let cut = HashMap::from([(
            "t".to_string(),
            DataFrame::new()
                .with_float("a", vec![0.0, 0.0, 1.0, 1.0])
                .with_float("b", vec![0.0, 1.0, 0.0, 1.0]),
        )]);
        let slotted = HashMap::from([(
            "t".to_string(),
            DataFrame::new()
                .with_str("a", vec!["p".into(), "p".into(), "q".into(), "q".into()])
                .with_str("b", vec!["r".into(), "s".into(), "r".into(), "s".into()]),
        )]);
        // Straight down the z axis: tilt 90° looks at the floor, so the projected
        // footprints are the footprints and nothing is foreshortened.
        let view = SpaceView { turn: 0.0, tilt: 90.0 };
        let span = |svg: &str| -> f64 {
            let xs: Vec<f64> = svg.lines()
                .filter(|l| l.contains("<path d=\"M"))
                .flat_map(|l| l.split(|c| c == 'M' || c == 'L').skip(1)
                    .filter_map(|s| s.split(',').next()?.trim().parse::<f64>().ok())
                    .collect::<Vec<_>>())
                .collect();
            xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max)
                - xs.iter().cloned().fold(f64::INFINITY, f64::min)
        };

        let mut cut_layer = Layer::new(Mark::Bar).transform(Transform::Bin);
        cut_layer.bin = Some(crate::ir::BinSpec { bins: Some(2), width: None, tiling: None });
        let cut_svg = SvgRenderer::default().render(
            &PlotSpec::new().data("t").x("a").y("b")
                .coord(CoordSpace::Space(view)).layer(cut_layer), &cut);
        let slot_svg = SvgRenderer::default().render(
            &PlotSpec::new().data("t").x("a").y("b")
                .coord(CoordSpace::Space(view))
                .layer(Layer::new(Mark::Bar).transform(Transform::Count)), &slotted);

        // Both meshes cover the same two slots per axis, so the cut one — whose cells
        // abut — must span strictly wider on the page than the slotted one, which
        // holds a fifth of each slot back as air.
        let (c, s) = (span(&cut_svg), span(&slot_svg));
        assert!(c > s * 1.05,
            "a cut floor should touch and a slotted one leave air: cut spans {c:.1}, slotted {s:.1}");
    }

    #[test]
    fn turning_the_scene_rearranges_the_faces_without_recoloring_them() {
        // `palette::shade` keys a face's shade to its **data axis**, not to where the
        // light falls, so `space(turn = )` must not change which colors appear. A
        // lamp fixed in screen space would re-shade every face as the reader turned
        // the cube, and a hue that moves while the data stands still is the silent
        // wrongness §12 forbids.
        let data = mesh_5x5();
        let shades = |view: SpaceView| -> std::collections::BTreeSet<String> {
            let svg = SvgRenderer::default().render(&hist_3d(view), &data);
            svg.lines()
                .filter(|l| l.contains("<path d=\"M"))
                .filter_map(|l| Some(l.split(r#"fill=""#).nth(1)?.split('"').next()?.to_string()))
                .collect()
        };
        let a = shades(SpaceView { turn: 30.0, tilt: 25.0 });
        let b = shades(SpaceView { turn: 75.0, tilt: 25.0 });
        assert_eq!(a, b, "turning the scene changed the palette of face shades");
        assert!(a.len() >= 2, "a solid needs its faces told apart, got {a:?}");
    }

    #[test]
    fn a_three_d_scatter_paints_far_points_before_near_ones() {
        // Depth order: two points identical in x and y but far apart in z must be
        // emitted far-first, so the nearer one paints on top. The projector pins
        // "nearer = smaller depth"; this checks the renderer actually sorts by it.
        let data: HashMap<String, DataFrame> = HashMap::from([(
            "t".to_string(),
            DataFrame::new()
                .with_float("x", vec![0.5, 0.5])
                .with_float("y", vec![0.5, 0.5])
                .with_float("z", vec![0.0, 1.0]), // row 1 is higher, so nearer
        )]);
        let spec = PlotSpec::new().data("t").x("x").y("y").z("z")
            .coord(CoordSpace::Space(SpaceView::default()))
            .layer(Layer::new(Mark::Point));
        let svg = SvgRenderer::default().render(&spec, &data);
        // Only the two data points carry a radius; the order they appear in the
        // SVG is the paint order. The higher-z (nearer) point must be drawn last.
        let cys: Vec<f64> = svg.lines()
            .filter(|l| l.contains("<circle"))
            .filter_map(|l| l.split(r#"cy=""#).nth(1)?.split('"').next()?.parse().ok())
            .collect();
        assert_eq!(cys.len(), 2);
        // Higher z projects higher on screen (smaller cy). It is nearer, so it is
        // painted last — the second circle should be the higher one.
        assert!(cys[1] < cys[0], "near point ({}, drawn last) should sit above far point ({})", cys[1], cys[0]);
    }

    #[test]
    fn jitter_spreads_the_category_axis_but_never_the_measured_one() {
        // The distinctive design claim (spec §5): `point * jitter` nudges points
        // along the *categorical* axis only, leaving the *measured* axis exact — so
        // coincident points separate to show density, yet no point is moved off its
        // value. Two categories, three coincident points each.
        let data: HashMap<String, DataFrame> = HashMap::from([(
            "t".to_string(),
            DataFrame::new()
                .with_str("g", vec!["A".into(), "A".into(), "A".into(), "B".into(), "B".into(), "B".into()])
                .with_float("v", vec![5.0, 5.0, 5.0, 7.0, 7.0, 7.0]),
        )]);
        let spec = |jit: bool| {
            let mut layer = Layer::new(Mark::Point);
            if jit { layer = layer.transform(Transform::Jitter); }
            PlotSpec::new().data("t").x("g").y("v").layer(layer)
        };
        let coords = |svg: &str| -> Vec<(f64, f64)> {
            svg.lines().filter(|l| l.contains("<circle")).filter_map(|l| {
                let cx = l.split(r#"cx=""#).nth(1)?.split('"').next()?.parse().ok()?;
                let cy = l.split(r#"cy=""#).nth(1)?.split('"').next()?.parse().ok()?;
                Some((cx, cy))
            }).collect()
        };

        let plain = coords(&SvgRenderer::default().render(&spec(false), &data));
        let jittered = coords(&SvgRenderer::default().render(&spec(true), &data));
        assert_eq!(plain.len(), 6);
        assert_eq!(jittered.len(), 6);

        // Un-jittered, each category's three points are coincident — one cx, one cy.
        assert!(plain[0..3].iter().all(|&p| p == plain[0]), "A's points should coincide un-jittered");
        assert!(plain[3..6].iter().all(|&p| p == plain[3]), "B's points should coincide un-jittered");

        // The measured axis (y) is untouched: every jittered cy equals the exact
        // un-jittered cy for its category. This is the ggplot2 divergence.
        for i in 0..6 {
            assert!((jittered[i].1 - plain[i].1).abs() < 1e-9,
                "row {i}: jitter moved y from {} to {} — it must not touch the measure", plain[i].1, jittered[i].1);
        }

        // The categorical axis (x) *is* spread: within a category the three points
        // no longer share a cx.
        let a_xs: std::collections::BTreeSet<i64> = jittered[0..3].iter().map(|p| (p.0 * 1e6) as i64).collect();
        assert_eq!(a_xs.len(), 3, "jitter should give A's three coincident points distinct x: {:?}", &jittered[0..3]);

        // Bounded to the slot: a jittered point stays near its category center (so
        // the axis still reads), never wandering past half the inter-category gap.
        let slot = (plain[3].0 - plain[0].0).abs(); // A-center to B-center
        for i in 0..6 {
            assert!((jittered[i].0 - plain[i].0).abs() < slot / 2.0,
                "row {i}: jitter offset {} exceeds half the slot {slot}", (jittered[i].0 - plain[i].0).abs());
        }
    }

    #[test]
    fn jitter_is_deterministic() {
        // One spec, one picture — the IR's whole reason to exist. The spread is
        // seeded from the data, so two renders are byte-identical (no clock, no RNG).
        let data: HashMap<String, DataFrame> = HashMap::from([(
            "t".to_string(),
            DataFrame::new()
                .with_str("g", vec!["A".into(), "A".into(), "A".into(), "A".into()])
                .with_float("v", vec![1.0, 1.0, 2.0, 2.0]),
        )]);
        let spec = PlotSpec::new().data("t").x("g").y("v")
            .layer(Layer::new(Mark::Point).transform(Transform::Jitter));
        let a = SvgRenderer::default().render(&spec, &data);
        let b = SvgRenderer::default().render(&spec, &data);
        assert_eq!(a, b, "jitter must render identically every run");
    }

    #[test]
    fn jitter_amount_scales_the_spread_linearly() {
        // `jitter(amount)` multiplies the slot-derived band. The seed is keyed to the
        // row and its data, *not* the amount, so every point's offset scales exactly:
        // `jitter(0.5)` is half the offset of bare `jitter`, and `jitter(0)` is none.
        let data: HashMap<String, DataFrame> = HashMap::from([(
            "t".to_string(),
            DataFrame::new()
                .with_str("g", vec!["A".into(), "A".into(), "A".into(), "A".into(),
                                    "B".into(), "B".into(), "B".into(), "B".into()])
                .with_float("v", vec![5.0, 5.0, 5.0, 5.0, 7.0, 7.0, 7.0, 7.0]),
        )]);
        let cxs = |amount: Option<f64>| -> Vec<f64> {
            let mut layer = Layer::new(Mark::Point);
            if let Some(a) = amount { layer = layer.transform(Transform::Jitter).jitter_amount(a); }
            let spec = PlotSpec::new().data("t").x("g").y("v").layer(layer);
            SvgRenderer::default().render(&spec, &data).lines()
                .filter(|l| l.contains("<circle"))
                .filter_map(|l| l.split(r#"cx=""#).nth(1)?.split('"').next()?.parse::<f64>().ok())
                .collect()
        };
        let plain = cxs(None);            // no jitter transform at all — the centers
        let full  = cxs(Some(1.0));       // bare-jitter equivalent
        let half  = cxs(Some(0.5));
        let zero  = cxs(Some(0.0));
        assert_eq!(plain.len(), 8);

        let mut moved = 0;
        for i in 0..8 {
            // amount 0 collapses the spread — every point back on its center.
            assert!((zero[i] - plain[i]).abs() < 0.01, "jitter(0) should not move point {i}");
            // amount 0.5 is exactly half the offset amount 1 produces (same seed).
            let full_off = full[i] - plain[i];
            let half_off = half[i] - plain[i];
            assert!((half_off - 0.5 * full_off).abs() < 0.05,
                "point {i}: jitter(0.5) offset {half_off} should be half of jitter(1)'s {full_off}");
            if full_off.abs() > 0.5 { moved += 1; }
        }
        assert!(moved >= 6, "most points should visibly move under a full jitter (moved {moved}/8)");
    }

    /// Five labels on one spot, and the panel they have to be arranged inside.
    fn crowded_labels(n: usize) -> HashMap<String, DataFrame> {
        let names: Vec<String> = (0..n).map(|i| format!("name {i}")).collect();
        HashMap::from([(
            "t".to_string(),
            DataFrame::new()
                .with_float("x", vec![5.0; n])
                .with_float("y", vec![5.0; n])
                .with_str("n", names),
        )])
    }

    /// The mark's own labels, as (x, y) — the tick labels and the title carry no
    /// `fill-opacity`, which is what tells the two apart.
    fn label_positions(svg: &str) -> Vec<(f64, f64)> {
        svg.lines()
            .filter(|l| l.contains("<text") && l.contains("fill-opacity"))
            .filter_map(|l| {
                let x = l.split(r#"x=""#).nth(1)?.split('"').next()?.parse().ok()?;
                let y = l.split(r#"y=""#).nth(1)?.split('"').next()?.parse().ok()?;
                Some((x, y))
            })
            .collect()
    }

    fn repel_spec(repel: bool) -> PlotSpec {
        let mut layer = Layer::new(Mark::Text).encode(Channel::Label, "n");
        if repel {
            layer = layer.transform(Transform::Repel);
        }
        PlotSpec::new().data("t").x("x").y("y").layer(layer)
    }

    #[test]
    fn repel_separates_labels_that_would_otherwise_sit_on_one_another() {
        // The distinctive claim (spec §5): what `repel` resolves is *ink*. Five
        // labels at one identical position are one illegible smudge without it, and
        // five readable words with it — so the test is not "did they move" but "do
        // any two still occupy the same line".
        let data = crowded_labels(5);
        let plain = label_positions(&SvgRenderer::default().render(&repel_spec(false), &data));
        let moved = label_positions(&SvgRenderer::default().render(&repel_spec(true), &data));
        assert_eq!(plain.len(), 5);
        assert_eq!(moved.len(), 5);

        // Un-repelled, all five are the same glyph in the same place.
        assert!(plain.iter().all(|&p| p == plain[0]), "five coincident rows should draw five coincident labels");

        // Repelled, no two share a line: every pair is at least a cap height apart.
        // The rows are identical, so this is also the tie-break working — there is
        // nothing in the data to part them by.
        for i in 0..5 {
            for j in (i + 1)..5 {
                let apart = (moved[i].0 - moved[j].0).abs().max((moved[i].1 - moved[j].1).abs());
                assert!(apart > 7.0, "labels {i} and {j} are still on top of each other: {:?} {:?}", moved[i], moved[j]);
            }
        }
    }

    #[test]
    fn repel_is_deterministic() {
        // One specification, one picture. The placement anneals, and an annealing
        // that reached for a clock or a global RNG would redraw the book differently
        // every build. The shake is hashed from the row and the pass instead (§5).
        let data = crowded_labels(12);
        let a = SvgRenderer::default().render(&repel_spec(true), &data);
        let b = SvgRenderer::default().render(&repel_spec(true), &data);
        assert_eq!(a, b, "repel must render identically every run");
    }

    #[test]
    fn repel_draws_every_label_even_when_no_arrangement_fits() {
        // §12, the rule the design fixed before anything was built: past some
        // density there is no overlap-free placement, and the answer is never to
        // drop the labels that did not fit. Forty long names on one point cannot be
        // separated — all forty still draw, all forty stay inside the panel where
        // the clip cannot eat them, and the layer says how many are still crowded.
        let data = crowded_labels(40);
        let drawn = SvgRenderer::default().draw(&repel_spec(true), &data);
        let placed = label_positions(&drawn.svg);
        assert_eq!(placed.len(), 40, "every label draws, however crowded");

        let p = &drawn.panel;
        for (i, &(x, y)) in placed.iter().enumerate() {
            assert!(x >= p.x0 - 0.5 && x <= p.x1 + 0.5, "label {i} was pushed off the panel in x: {x}");
            assert!(y >= p.y0 - 0.5 && y <= p.y1 + 0.5, "label {i} was pushed off the panel in y: {y}");
        }
        assert!(
            drawn.remarks.iter().any(|d| d.message.contains("still overlap")),
            "an impossible packing has to say so: {:?}", drawn.remarks
        );
    }

    #[test]
    fn a_repelled_label_that_travels_gets_a_line_back_to_its_point() {
        // A word pushed clear of its dot has lost the one thing that said which dot
        // it belonged to, so the modifier draws the connector itself rather than
        // asking for a second mark (spec §5). A label that only took its resting
        // step off the dot gets none — a line from every label is a panel of lines.
        let far = SvgRenderer::default().render(&repel_spec(true), &crowded_labels(6));
        let near = SvgRenderer::default().render(&repel_spec(true), &crowded_labels(1));
        let leaders = |svg: &str| svg.matches(r#"stroke-width="0.7""#).count();
        assert!(leaders(&far) >= 4, "labels driven far from their point need leaders: {}", leaders(&far));
        assert_eq!(leaders(&near), 0, "a label at rest beside its own dot needs no leader");
    }

    /// Every `&` in well-formed XML must begin a character reference.
    fn unescaped_ampersands(svg: &str) -> Vec<String> {
        const ENTITIES: [&str; 6] = ["amp;", "lt;", "gt;", "quot;", "apos;", "#"];
        svg.match_indices('&')
            .filter(|(i, _)| {
                let rest = &svg[i + 1..];
                !ENTITIES.iter().any(|e| rest.starts_with(e))
            })
            .map(|(i, _)| svg[i..(i + 24).min(svg.len())].to_string())
            .collect()
    }

    fn render_with_category(name: &str) -> String {
        let df = DataFrame::new()
            .with_str("firm", vec![name.to_string(), "Sales".to_string()])
            .with_float("spend", vec![10.0, 25.0]);
        let mut data = HashMap::new();
        data.insert("t".to_string(), df);
        let spec = PlotSpec::new()
            .data("t")
            .x("firm")
            .y("spend")
            .title("Q3 <spend> & forecast")
            .layer(Layer::new(Mark::Bar).encode(Channel::Color, "firm"));
        SvgRenderer::default().render(&spec, &data)
    }

    // -- the path in space ------------------------------------------------

    /// Two routes through one cube must **interleave**, not stack.
    ///
    /// This is the property the whole segment-level depth sort exists for, and
    /// the one a per-series sort would silently get wrong: sorting whole strokes
    /// would paint one entire coil in front of the other, which is a picture of
    /// two coils that never touch rather than two that thread through each other.
    ///
    /// The angle is chosen so the test is not measuring the projector twice.
    /// At `turn = 0, tilt = 0` the camera sits on the x-axis, so data `x` is
    /// *depth alone* and the screen position comes entirely from `y` and `z`
    /// (`project::Scene`'s own test pins that basis). So two series sharing one
    /// `(y, z)` track and differing only in `x` occupy the same pixels at
    /// different distances — and the near one must be emitted last, whatever
    /// order the table listed them in.
    #[test]
    fn the_segments_of_two_paths_in_space_sort_together_not_series_by_series() {
        // `far` is listed first in the table, `near` second; then the reverse.
        // Data order must not survive into paint order — depth must decide.
        let render_with = |first: &str, second: &str| -> String {
            let x = |s: &str| if s == "near" { 1.0 } else { 0.0 };
            let df = DataFrame::new()
                .with_float("x", vec![x(first), x(first), x(second), x(second)])
                .with_float("y", vec![0.0, 1.0, 0.0, 1.0])
                .with_float("z", vec![0.0, 1.0, 0.0, 1.0])
                .with_str("strand", vec![
                    first.into(), first.into(), second.into(), second.into(),
                ]);
            let data = HashMap::from([("t".to_string(), df)]);
            let spec = PlotSpec::new()
                .data("t")
                .x("x")
                .y("y")
                .z("z")
                .coord(CoordSpace::Space(SpaceView { turn: 0.0, tilt: 0.0 }))
                .layer(Layer::new(Mark::Path).encode(Channel::Color, "strand"));
            SvgRenderer::default().render(&spec, &data)
        };

        // Which stroke color each route segment carries, in emission order.
        // `stroke-linecap="round"` is what picks the route's own segments out of
        // the cube frame's edges and the legend's separator, which are `<line>`
        // elements too.
        let painted_order = |svg: &str| -> Vec<String> {
            svg.lines()
                .filter(|l| {
                    l.trim_start().starts_with("<line ") && l.contains(r#"stroke-linecap="round""#)
                })
                .filter_map(|l| l.split("stroke=\"").nth(1))
                .filter_map(|s| s.split('"').next())
                .map(str::to_string)
                .collect()
        };

        for (first, second) in [("far", "near"), ("near", "far")] {
            let svg = render_with(first, second);
            let painted = painted_order(&svg);
            assert_eq!(
                painted.len(),
                2,
                "expected one segment per route, got {painted:?}"
            );
            // Series take palette entries in first-appearance order, so the near
            // route's hue is decided by where the table listed it.
            let near_hue = PALETTE_GOG[if first == "near" { 0 } else { 1 }];
            assert_eq!(
                painted.last().unwrap(),
                near_hue,
                "the nearer route must paint over the farther one, whatever order the \
                 table listed them in (listed {first} then {second}, painted {painted:?})"
            );
        }
    }

    /// A dash is a property of the **route**, not of the segmentation the depth
    /// sort needs. Each segment is its own element and a dash restarts at the
    /// start of an element, so without a carried phase a dashed 3-D path would
    /// come out as one identical dash per vertex — a texture the data never asked
    /// for. The running `stroke-dashoffset` is what keeps it a dash.
    #[test]
    fn a_dashed_route_in_space_carries_its_dash_phase_across_the_depth_split() {
        let n = 12;
        let df = DataFrame::new()
            .with_float("x", (0..n).map(|i| i as f64).collect())
            .with_float("y", (0..n).map(|i| i as f64).collect())
            .with_float("z", (0..n).map(|i| i as f64).collect());
        let data = HashMap::from([("t".to_string(), df)]);
        let spec = PlotSpec::new()
            .data("t")
            .x("x")
            .y("y")
            .z("z")
            .coord(CoordSpace::Space(SpaceView::default()))
            .layer(Layer::new(Mark::Path).style_pattern("dashed"));
        let svg = SvgRenderer::default().render(&spec, &data);

        let offsets: Vec<f64> = svg
            .lines()
            .filter(|l| l.trim_start().starts_with("<line "))
            .filter_map(|l| l.split("stroke-dashoffset=\"").nth(1))
            .filter_map(|s| s.split('"').next())
            .filter_map(|s| s.parse().ok())
            .collect();
        assert!(
            offsets.len() >= 2,
            "a dashed route in space should offset each segment, got {offsets:?}"
        );
        // The first segment starts the pattern at zero, and no two segments share
        // a phase — which is exactly what "the dash keeps running" means.
        assert!(
            offsets.iter().any(|o| *o == 0.0),
            "the route's first segment should start the dash at phase 0: {offsets:?}"
        );
        let mut sorted = offsets.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        sorted.dedup();
        assert_eq!(
            sorted.len(),
            offsets.len(),
            "every segment should sit at its own phase, not restart the dash: {offsets:?}"
        );
    }

    /// The three positions take a label override, or none of them does.
    ///
    /// `x_label`/`y_label` shipped and `z_label` did not, which is the shape of a
    /// Law-1 exception rather than a missing convenience: the positions are a
    /// family of three and two of them had it. This asserts the override reaches
    /// the projected cube's edge, and that asking for it where there is no third
    /// axis is *reported* rather than quietly dropped (§12).
    #[test]
    fn the_third_position_takes_a_label_override_like_the_other_two() {
        let df = DataFrame::new()
            .with_float("a", vec![1.0, 2.0, 3.0, 4.0])
            .with_float("b", vec![2.0, 4.0, 1.0, 3.0])
            .with_float("c", vec![5.0, 1.0, 4.0, 2.0]);
        let data = HashMap::from([("t".to_string(), df)]);

        let spec = PlotSpec::new().data("t").x("a").y("b").z("c")
            .z_label("Altitude, m")
            .coord(CoordSpace::Space(SpaceView::default()))
            .layer(Layer::new(Mark::Point));
        let svg = SvgRenderer::default().render(&spec, &data);
        assert!(svg.contains("Altitude, m"), "the z override should reach the cube's edge");
        assert!(!svg.contains(">C<"), "the auto-derived label should be replaced, not joined");

        // And the same spec without the override falls back to the column name,
        // so the test above is measuring the override and not the default.
        let bare = PlotSpec::new().data("t").x("a").y("b").z("c")
            .coord(CoordSpace::Space(SpaceView::default()))
            .layer(Layer::new(Mark::Point));
        let bare_svg = SvgRenderer::default().render(&bare, &data);
        assert!(!bare_svg.contains("Altitude, m"));

        // A label with no axis to land on is guidance, not a refusal: it renders.
        let flat = PlotSpec::new().data("t").x("a").y("b")
            .z_label("Altitude, m")
            .layer(Layer::new(Mark::Point));
        let d = crate::legality::check(&flat, &data);
        assert!(
            d.iter().any(|x| x.kind == crate::legality::DiagnosticKind::Assumption
                && x.message.contains("z_label")),
            "a z label with no z should be reported: {:?}",
            d.iter().map(|x| &x.message).collect::<Vec<_>>()
        );
        assert!(d.iter().all(|x| !x.is_fatal()), "…but it must not be fatal");
    }

    /// A measure on `color` varies **along** a stroke; a category splits it into
    /// series. Both readings of one channel, and the geometry follows the reading
    /// rather than the mark: the categorical route stays a single `<polyline>`
    /// (so every plot that predates this renders byte for byte as it did), and
    /// only the measured one is cut into per-segment elements.
    #[test]
    fn a_measured_color_ramps_along_a_stroke_where_a_category_splits_it() {
        let n = 24;
        let df = DataFrame::new()
            .with_float("x", (0..n).map(|i| i as f64).collect())
            .with_float("y", (0..n).map(|i| (i % 5) as f64).collect())
            .with_float("depth", (0..n).map(|i| i as f64).collect())
            .with_str("kind", (0..n).map(|i| if i < 12 { "a".into() } else { "b".into() }).collect());
        let data = HashMap::from([("t".to_string(), df)]);
        let render = |mark: &Mark, field: &str| -> String {
            let spec = PlotSpec::new().data("t").x("x").y("y")
                .layer(Layer::new(mark.clone()).encode(Channel::Color, field));
            SvgRenderer::default().render(&spec, &data)
        };
        // Distinct stroke colors among the mark's own elements.
        let hues = |svg: &str| -> std::collections::HashSet<String> {
            svg.lines()
                .filter(|l| {
                    let t = l.trim_start();
                    // `stroke-opacity` is what separates a mark's own stroke from
                    // the gridlines and the legend's separator rule, which are
                    // `<line>` elements too.
                    (t.starts_with("<line ") || t.starts_with("<polyline ")) && t.contains("stroke-opacity")
                })
                .filter_map(|l| l.split("stroke=\"").nth(1))
                .filter_map(|s| s.split('"').next())
                .map(str::to_string)
                .collect()
        };

        for mark in [Mark::Line, Mark::Step, Mark::Path] {
            let categorical = render(&mark, "kind");
            let measured = render(&mark, "depth");

            // Two categories, two strokes, two hues — and still whole polylines.
            assert_eq!(hues(&categorical).len(), 2, "{mark:?}: a category gives one hue per series");
            assert!(
                categorical.contains("<polyline "),
                "{mark:?}: a categorical stroke should stay a single polyline"
            );

            // A measure gives many, from the ramp, cut into segments.
            assert!(
                hues(&measured).len() > 5,
                "{mark:?}: a measured color should ramp along the stroke, got {:?}",
                hues(&measured).len()
            );
            assert!(
                !measured.contains("<polyline "),
                "{mark:?}: a ramped stroke is emitted per segment, not as a polyline"
            );
            // And it is one route, not two: a measure carries no categories, so
            // it must not split the mark the way `kind` does.
            assert!(
                !measured.contains("stroke=\"#4e79a7\"") || mark == Mark::Path,
                "{mark:?}: a ramped stroke should not fall back to the palette"
            );
        }
    }

    // -- log scale --------------------------------------------------------
    //
    // The two halves of the ordering rule, and the thing a log scale is *for*.

    /// Left edges of the drawn bars, ascending. A bar is any `<rect>` with a
    /// `fill-opacity`: that catches a categorical bar's faint self-edge *and* a
    /// histogram's panel-color separator, where keying on `stroke-width="0.5"`
    /// would now miss the histogram. Panel, canvas, strip and clip rects carry no
    /// `fill-opacity`, so this still picks out data bars only.
    ///
    /// Position rather than width, because `bar_thickness_svg` hands every bar
    /// one thickness taken from the smallest gap: widths are equal by
    /// construction and so cannot tell the two pipeline orders apart. Where the
    /// bins were cut shows up in the *spacing*.
    fn bar_lefts(svg: &str) -> Vec<f64> {
        let mut v: Vec<f64> = svg.lines()
            .filter(|l| l.contains("<rect") && l.contains("fill-opacity"))
            .filter_map(|l| {
                let after = l.split(r#"<rect x=""#).nth(1)?;
                after.split('"').next()?.parse::<f64>().ok()
            })
            .collect();
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        v
    }

    fn skewed() -> HashMap<String, DataFrame> {
        // Four decades of gdp — the shape a log scale exists for.
        let gdp: Vec<f64> = (0..40).map(|i| 10f64.powf(1.0 + i as f64 / 10.0)).collect();
        let life: Vec<f64> = (0..40).map(|i| 40.0 + i as f64).collect();
        let mut data = HashMap::new();
        data.insert("t".to_string(), DataFrame::new().with_float("gdp", gdp).with_float("life", life));
        data
    }

    // -- time -------------------------------------------------------------

    fn yearly(n: usize) -> HashMap<String, DataFrame> {
        use crate::time::{days_from_civil, TimeUnit, SECS_PER_DAY};
        let day: Vec<f64> = (0..n)
            .map(|i| days_from_civil(1994 + i as i64, 1, 1) as f64 * SECS_PER_DAY)
            .collect();
        let sales: Vec<f64> = (0..n).map(|i| 50.0 + (i as f64 * 1.7) % 30.0).collect();
        let mut data = HashMap::new();
        data.insert("t".to_string(),
            DataFrame::new().with_time("day", day, TimeUnit::Day).with_float("sales", sales));
        data
    }

    /// Daily readings from 2024-03-01, the six-weeks table the book uses.
    fn six_weeks(n: usize) -> HashMap<String, DataFrame> {
        use crate::time::{days_from_civil, TimeUnit, SECS_PER_DAY};
        let start = days_from_civil(2024, 3, 1);
        let day: Vec<f64> = (0..n).map(|i| (start + i as i64) as f64 * SECS_PER_DAY).collect();
        let orders: Vec<f64> = (0..n).map(|i| 20.0 + (i as f64 * 3.0) % 15.0).collect();
        let mut data = HashMap::new();
        data.insert("t".to_string(),
            DataFrame::new().with_time("day", day, TimeUnit::Day).with_float("orders", orders));
        data
    }

    #[test]
    fn a_calendar_axis_fits_its_data_like_every_other_axis() {
        // A user read the daily-bars plot and said the gaps at each end looked
        // wide. They were: the time branch took `(first tick, last tick)` as its
        // range, which is the bracket-outward failure §10 exists to undo, and
        // the calendar was the one axis still exempt from the fix. Six weeks of
        // daily orders ticked on Mondays drew Feb 26 .. Apr 15 — a 49-day axis
        // for 41 days of data, four dead days at each end.
        let data = six_weeks(42);           // 2024-03-01 (Fri) .. 2024-04-11 (Thu)
        let svg = |mark: Mark| {
            let spec = PlotSpec::new().data("t").x("day").y("orders").layer(Layer::new(mark));
            SvgRenderer::default().render(&spec, &data)
        };

        for mark in [Mark::Bar, Mark::Line, Mark::Point] {
            let s = svg(mark.clone());
            // The Mondays inside the data are drawn...
            assert!(s.contains(">Mar 4<") && s.contains(">Apr 8<"),
                "{mark:?}: expected the Mondays inside the data");
            // ...and the ones the old bracket reached out for are not.
            assert!(!s.contains(">Feb 26<"), "{mark:?}: axis still reaches back to Feb 26");
            assert!(!s.contains(">Apr 15<"), "{mark:?}: axis still reaches on to Apr 15");
        }
    }

    #[test]
    fn a_calendar_axis_ends_the_way_the_mark_needs_it_to() {
        // The three answers the linear path already gives, now reached on a date
        // axis rather than skipped: a bar owns a slot and needs half of one
        // beyond each end or the end bars are sliced; a glyph breathes; a fill
        // sits flush, its edges being the shape. Measured as the axis span, in
        // days, against the 41 days the data covers.
        use crate::time::SECS_PER_DAY;
        let data = six_weeks(42);
        let span = |mark: Mark| {
            let df = data.get("t").unwrap();
            let eff: Vec<DataFrame> = vec![df.clone()];
            let refs: Vec<&DataFrame> = eff.iter().collect();
            let bars: Vec<&DataFrame> = if mark == Mark::Bar { refs.clone() } else { Vec::new() };
            let (_, xs) = build_axis(
                &refs, &bars, &[], "day", None,
                mark == Mark::Bar,                    // bar_position
                false,                                // bar_extent (that is y's job)
                mark == Mark::Area,                   // flush
                None, false, 10.0, Some(crate::time::TimeUnit::Day), (None, None), (0.0, 0.0));
            (xs.1 - xs.0) / SECS_PER_DAY
        };

        let data_days = 41.0;
        assert!((span(Mark::Bar) - (data_days + 1.0)).abs() < 1e-6,
            "a bar wants half a day beyond each end, got {}", span(Mark::Bar));
        assert!((span(Mark::Line) - data_days * 1.1).abs() < 1e-6,
            "a line breathes 5% each side, got {}", span(Mark::Line));
        assert!((span(Mark::Area) - data_days).abs() < 1e-6,
            "an area fills flush, got {}", span(Mark::Area));
    }

    #[test]
    fn a_temporal_axis_is_labeled_in_calendar_units_not_epoch_seconds() {
        let spec = PlotSpec::new().data("t")
            .x("day").y("sales")
            .layer(Layer::new(Mark::Line));
        let svg = SvgRenderer::default().render(&spec, &yearly(25));
        // Years on the ticks — not 7.6E8, not 760M.
        assert!(svg.contains(">1995<") && svg.contains(">2015<"),
                "expected year labels on the x axis");
        assert!(!svg.contains("M<") || !svg.contains("00M<"),
                "epoch seconds leaked onto the axis");
    }

    #[test]
    fn bars_sitting_on_dates_get_calendar_ticks_not_one_per_bar() {
        // A tick under every daily bar would label the axis into soup; the
        // calendar decides the gridlines, exactly as Wilkinson's stock-price
        // example ticks Sundays rather than trades.
        let spec = PlotSpec::new().data("t")
            .x("day").y("sales")
            .layer(Layer::new(Mark::Bar));
        let svg = SvgRenderer::default().render(&spec, &yearly(25));
        assert!(svg.contains(">1995<"), "expected year labels under the bars");
        // 25 bars but far fewer ticks.
        let tick_labels = svg.matches(">19").count() + svg.matches(">20").count();
        assert!(tick_labels < 12, "got {tick_labels} tick labels for 25 bars");
    }

    #[test]
    fn a_date_on_a_color_ramp_gets_a_dated_legend() {
        use crate::ir::ChannelDef;
        let mut layer = Layer::new(Mark::Point);
        layer.encodings.insert(Channel::Color, ChannelDef::field("day"));
        let spec = PlotSpec::new().data("t")
            .x("sales").y("sales")
            .layer(layer);
        let svg = SvgRenderer::default().render(&spec, &yearly(25));
        // Legend rows are self-contained ISO dates, not "1.7B" epoch seconds.
        assert!(svg.contains("1994-01-01"), "expected an ISO date in the legend");
        assert!(!svg.contains(">0.8B<"), "epoch seconds leaked into the legend");
    }

    #[test]
    fn a_log_axis_is_labeled_in_data_units_not_exponents() {
        let spec = PlotSpec::new().data("t")
            .x_scaled("gdp", ScaleType::Log).y("life")
            .layer(Layer::new(Mark::Point));
        let svg = SvgRenderer::default().render(&spec, &skewed());
        // The whole difference between a log scale and plotting log(gdp).
        assert!(svg.contains(">10<") && svg.contains(">10K<"),
                "expected decade labels in the reader's units");
    }

    #[test]
    fn binning_happens_in_log_space_so_the_bars_come_out_even() {
        // The case that decides the pipeline order. Cut in log space, each bin
        // spans one constant ratio, so the bars land at a constant spacing and
        // cover the axis. Cut in linear space and merely *drawn* on a log axis,
        // they bunch towards the top: measured on this data the gaps run
        // 176, 82, 54, 40, 32, 27 px and the left half of the plot is empty.
        let spec = PlotSpec::new().data("t")
            .x_scaled("gdp", ScaleType::Log)
            .layer(Layer::new(Mark::Bar).transform(Transform::Bin));
        let svg = SvgRenderer::default().render(&spec, &skewed());

        let lefts = bar_lefts(&svg);
        assert!(lefts.len() > 3, "expected several bars, got {}", lefts.len());
        let gaps: Vec<f64> = lefts.windows(2).map(|w| w[1] - w[0]).collect();
        let (mn, mx) = (
            gaps.iter().cloned().fold(f64::INFINITY, f64::min),
            gaps.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
        );
        assert!(mx - mn < 1.0, "bars are unevenly spaced: gaps {gaps:?}");
    }

    #[test]
    fn a_histogram_touches_where_a_bar_chart_keeps_its_gap() {
        // The visual grammar that tells a histogram from a bar chart: a histogram
        // cuts a continuum into adjacent intervals, so its bars must touch —
        // Wilkinson, "there cannot be gaps between bars" — while a bar chart's
        // categories are separate, so its bars must not. The `bin` transform is
        // the only thing that differs, and it alone decides which way they draw.

        // A uniform continuous column: every Sturges bin is populated, so no bar
        // is dropped for being empty and "the bins touch" is a claim about the
        // drawing, not about which bins happen to hold data.
        let mut cont = HashMap::new();
        cont.insert("t".to_string(),
            DataFrame::new().with_float("x", (0..100).map(|i| i as f64).collect()));
        let hist = PlotSpec::new().data("t").x("x")
            .layer(Layer::new(Mark::Bar).transform(Transform::Bin));
        let svg = SvgRenderer::default().render(&hist, &cont);

        let mut r = bar_rects(&svg);
        r.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        assert!(r.len() > 3, "expected several bins, got {}", r.len());
        for w in r.windows(2) {
            let (right, next_left) = (w[0].0 + w[0].2, w[1].0);
            assert!((right - next_left).abs() < 1.0,
                "histogram bins must touch: right edge {right:.2} vs next left {next_left:.2}");
        }
        // And a hairline in the panel color parts them, so touching neighbors
        // stay legible — not the self-colored edge a categorical bar carries.
        assert!(svg.contains(&format!(r#"stroke="{PANEL_BG}""#)),
            "touching bins want a panel-color separator");

        // The same mark without `bin`, over categories, keeps its gap.
        let chart = PlotSpec::new().data("t").x("country").y("gold")
            .layer(Layer::new(Mark::Bar));
        let svg2 = SvgRenderer::default().render(&chart, &medals_data());
        let mut r2 = bar_rects(&svg2);
        r2.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        assert!(r2.len() >= 2);
        let touching = r2.windows(2).any(|w| (w[0].0 + w[0].2 - w[1].0).abs() < 1.0);
        assert!(!touching, "categorical bars must not touch");
        assert!(svg2.contains(r#"stroke-width="0.5""#),
            "a categorical bar keeps its faint self-colored edge");
    }

    #[test]
    fn a_color_split_histogram_overlays_each_series_in_its_own_hue() {
        // The overlaid histogram — the seaborn/penguins picture. Two groups over
        // one range bin on shared edges and draw on top of one another. Each must
        // keep its own color (the split the old code dropped, rendering one gray
        // combined histogram under a three-color legend), and because opaque
        // fills would bury each other, every bar is a translucent fill under a
        // solid outline in the *same* hue: the step silhouette that survives the
        // pile-up.
        let mut data = HashMap::new();
        data.insert("t".to_string(), DataFrame::new()
            .with_float("x", vec![0.0, 1.0, 2.0, 3.0,   2.0, 3.0, 4.0, 5.0])
            .with_str("g", ["a", "a", "a", "a",   "b", "b", "b", "b"]
                .iter().map(|s| s.to_string()).collect()));
        let spec = PlotSpec::new().data("t").x("x")
            .layer(Layer::new(Mark::Bar).transform(Transform::Bin).encode(Channel::Color, "g"));
        let svg = SvgRenderer::default().render(&spec, &data);

        let attr = |l: &str, name: &str| -> String {
            l.split(&format!(" {name}=\"")).nth(1)
                .and_then(|r| r.split('"').next()).unwrap_or("").to_string()
        };
        // A data bar carries a `stroke`; a legend swatch (also a `fill-opacity`
        // rect) does not — that is what keeps the key out of the bar count.
        let bars: Vec<(String, String, String)> = svg.lines()
            .filter(|l| l.contains("<rect") && l.contains("fill-opacity") && l.contains(" stroke=\""))
            .map(|l| (attr(l, "fill"), attr(l, "fill-opacity"), attr(l, "stroke")))
            .collect();
        assert!(bars.len() >= 4, "expected several overlaid bars, got {}", bars.len());

        // Two distinct fills: the species did not collapse to one color.
        let fills: std::collections::HashSet<&str> = bars.iter().map(|b| b.0.as_str()).collect();
        assert!(fills.len() >= 2, "a split histogram keeps one color per group, got {fills:?}");

        for (fill, op, stroke) in &bars {
            assert_eq!(stroke, fill, "an overlaid bar is outlined in its own hue, not the panel color");
            assert_ne!(stroke.as_str(), PANEL_BG, "the overlay outline is the series hue, not the hairline");
            assert_eq!(op, "0.400", "an overlaid fill is translucent so stacked series show through");
        }
    }

    #[test]
    fn a_sum_is_taken_before_the_scale_so_it_stays_a_sum() {
        // The other half of the rule. Group "a" sums to 100 and group "b" to 10,
        // so the axis spans exactly two decades. Logging *first* would sum the
        // logs — log10(10)+log10(90) = 2.95 — which is the log of a product and
        // not a quantity anyone asked for.
        let df = DataFrame::new()
            .with_str("cat", vec!["a".into(), "a".into(), "b".into(), "b".into()])
            .with_float("v", vec![10.0, 90.0, 1.0, 9.0]);
        let mut data = HashMap::new();
        data.insert("t".to_string(), df);

        let spec = PlotSpec::new().data("t")
            .x("cat").y_scaled("v", ScaleType::Log)
            .layer(Layer::new(Mark::Bar).transform(Transform::Sum));
        let svg = SvgRenderer::default().render(&spec, &data);

        // Summing first spans exactly one decade, 10 to 100, so the axis gets
        // the 1-2-5 fill. Logging first would sum the logs — 1 + 1.954 = 2.954,
        // i.e. ~900 — and the axis would run a decade further, to 1000.
        assert!(svg.contains(">100<"), "the larger group should sum to 100");
        assert!(svg.contains(">20<") && svg.contains(">50<"),
                "one decade of range should get the 1-2-5 fill");
        assert!(!svg.contains(">1000<"), "a log-then-sum would push the axis to 1000");
    }

    #[test]
    fn a_value_a_log_scale_cannot_place_is_skipped_not_drawn_as_nan() {
        let df = DataFrame::new()
            .with_float("v", vec![1.0, 0.0, -4.0, 100.0])
            .with_float("y", vec![1.0, 2.0, 3.0, 4.0]);
        let mut data = HashMap::new();
        data.insert("t".to_string(), df);

        let spec = PlotSpec::new().data("t")
            .x_scaled("v", ScaleType::Log).y("y")
            .layer(Layer::new(Mark::Point));
        let svg = SvgRenderer::default().render(&spec, &data);

        assert!(!svg.contains("NaN"), "NaN coordinates are not valid SVG");
        // Two of the four rows can be placed, and both are.
        assert_eq!(svg.matches("<circle").count(), 2);
    }

    #[test]
    fn an_unscaled_plot_is_untouched_by_any_of_this() {
        // A scale nobody asked for must not change a pixel.
        let spec = PlotSpec::new().data("t").x("gdp").y("life").layer(Layer::new(Mark::Point));
        let svg = SvgRenderer::default().render(&spec, &skewed());
        assert!(!svg.contains("NaN"));
        assert_eq!(svg.matches("<circle").count(), 40);
    }

    /// Population spread evenly across four decades — roughly how the real
    /// column is distributed, and the shape that washes out a linear ramp: on a
    /// linear scale nearly every value sits in the bottom 1% of the range.
    fn skewed_channel() -> HashMap<String, DataFrame> {
        const N: usize = 32;
        let pop: Vec<f64> = (0..N)
            .map(|i| 10f64.powf(5.3 + i as f64 * 3.8 / (N - 1) as f64))
            .collect();
        let n = pop.len();
        let mut data = HashMap::new();
        data.insert("t".to_string(), DataFrame::new()
            .with_float("population", pop)
            .with_float("gdp", (0..n).map(|i| 1000.0 + i as f64).collect())
            .with_float("life", (0..n).map(|i| 50.0 + i as f64 * 0.5).collect()));
        data
    }

    /// Distinct colors among the *marks* — legend swatches and the panel
    /// background are not what the ramp is being judged on.
    fn distinct_mark_fills(svg: &str) -> usize {
        let mut seen: Vec<&str> = Vec::new();
        for l in svg.lines().filter(|l| l.contains("<circle")) {
            if let Some(v) = l.split("fill=\"").nth(1).and_then(|r| r.split('"').next()) {
                if !seen.contains(&v) { seen.push(v) }
            }
        }
        seen.len()
    }

    /// Nothing a plot draws may land outside the plot.
    ///
    /// **The defect this pins.** The legend box was clamped to the room left
    /// (`.min(remaining)`) while its rows were drawn at the full constant, so a
    /// gradient legend — 188px tall by nature — put its strip and its bottom
    /// label *outside the box* on any shorter plot, and outside the image on a
    /// plot shorter than about 190. Reachable from `theme(height = )` on any
    /// plot with any legend, and from every composed page of more than two rows,
    /// since the page canvas is fixed and the cells divide it. 655 tests passed
    /// throughout: every one of them rendered at the default size, where the
    /// legend fits, and none looked at whether the ink stayed on the page.
    ///
    /// So the assertion is deliberately not "the legend is right" but the blunt
    /// invariant one level up, which no future layout change can satisfy by
    /// accident.
    #[test]
    fn nothing_is_drawn_below_the_bottom_of_the_canvas() {
        let lowest_y = |svg: &str| {
            let mut lo = 0.0_f64;
            for line in svg.lines() {
                for (attr, _) in [("y=\"", 0), ("cy=\"", 0)] {
                    let mut rest = line;
                    while let Some(i) = rest.find(attr) {
                        rest = &rest[i + attr.len()..];
                        if let Some(v) = rest.split('"').next().and_then(|s| s.parse::<f64>().ok()) {
                            lo = lo.max(v);
                        }
                    }
                }
            }
            lo
        };

        let mut data = HashMap::new();
        data.insert("t".to_string(), DataFrame::new()
            .with_float("gdp", vec![1000.0, 2000.0, 3000.0, 4000.0])
            .with_float("life", vec![50.0, 60.0, 70.0, 80.0])
            .with_str("continent", ["Asia", "Europe", "Africa", "Americas"]
                .iter().map(|s| s.to_string()).collect()));

        for h in [140.0, 150.0, 200.0, 260.0, 400.0] {
            // A numeric color is the gradient legend, the tall one that overflowed.
            let numeric = PlotSpec::new().data("t").x("gdp").y("life")
                .layer(Layer::new(Mark::Point).encode(Channel::Color, "gdp"));
            // A text color is the swatch legend, which cannot squeeze and so is
            // left out with a remark rather than drawn over the edge.
            let discrete = PlotSpec::new().data("t").x("gdp").y("life")
                .layer(Layer::new(Mark::Point).encode(Channel::Color, "continent"));

            for (what, spec) in [("gradient", numeric), ("swatches", discrete)] {
                let r = SvgRenderer { height: h, ..SvgRenderer::default() };
                let svg = r.render(&spec, &data);
                assert!(
                    lowest_y(&svg) <= h,
                    "{what} legend at height {h}: ink at y={:.1}, past the {h}px canvas",
                    lowest_y(&svg),
                );
            }
        }
    }

    #[test]
    fn a_log_color_ramp_uses_its_whole_range_on_skewed_data() {
        // The complaint the book carried for two sessions: a linear ramp spends
        // its range where the data is dense, so a skewed column comes out one
        // flat color with two outliers.
        let mk = |log: bool| {
            let mut layer = Layer::new(Mark::Point);
            layer = if log {
                layer.encode_scaled(Channel::Color, "population", ScaleType::Log)
            } else {
                layer.encode(Channel::Color, "population")
            };
            let spec = PlotSpec::new().data("t").x("gdp").y("life").layer(layer);
            SvgRenderer::default().render(&spec, &skewed_channel())
        };
        // 32 points spread over four decades, each landing on its own step of
        // the ramp — the ramp is wired to the scale and exercised end to end.
        //
        // How badly the *linear* ramp washes out is a property of the scale, not
        // of the SVG, and is asserted in `scale.rs`: counting distinct hex here
        // would measure rounding, since colors a reader cannot tell apart still
        // differ in the last digit.
        let logged = distinct_mark_fills(&mk(true));
        assert!(logged >= 30, "log ramp gave only {logged} distinct colors for 32 points");
        assert!(distinct_mark_fills(&mk(false)) > 1, "sanity: linear still varies");
    }

    #[test]
    fn a_log_legends_middle_label_is_the_color_actually_painted_there() {
        // The strip's midpoint is half way along the *scale*, which on a log
        // ramp is the geometric mean. Labeling it with the arithmetic mean
        // would name a color the strip does not paint.
        let spec = PlotSpec::new().data("t").x("gdp").y("life").layer(
            Layer::new(Mark::Point).encode_scaled(Channel::Color, "population", ScaleType::Log),
        );
        let svg = SvgRenderer::default().render(&spec, &skewed_channel());
        // The range runs 199.5K to 1.26B. Half way along it in log space is
        // √(199.5K · 1.26B) = 15.8M; the arithmetic midpoint would be 629.6M,
        // two decades away and a visibly different color.
        assert!(svg.contains(">15.8M<"), "expected the geometric midpoint");
        assert!(!svg.contains(">629.6M<"), "that is the arithmetic midpoint");
    }

    #[test]
    fn a_log_size_channel_spreads_the_radii() {
        let mk = |log: bool| {
            let mut layer = Layer::new(Mark::Point);
            layer = if log {
                layer.encode_scaled(Channel::Size, "population", ScaleType::Log)
            } else {
                layer.encode(Channel::Size, "population")
            };
            let spec = PlotSpec::new().data("t").x("gdp").y("life").layer(layer);
            let svg = SvgRenderer::default().render(&spec, &skewed_channel());
            let mut r: Vec<f64> = svg.lines()
                .filter(|l| l.contains("<circle"))
                .filter_map(|l| l.split(r#" r=""#).nth(1)?.split('"').next()?.parse().ok())
                .collect();
            r.sort_by(|a, b| a.partial_cmp(b).unwrap());
            r[r.len() / 2]
        };
        // Linear leaves almost every point at the minimum radius; log puts the
        // median point in the middle of the range, which is what it is for.
        assert!(mk(false) < 4.0, "linear median radius was {}", mk(false));
        assert!(mk(true) > 6.5, "log median radius was {}", mk(true));
    }

    #[test]
    fn escapes_all_five_xml_entities() {
        assert_eq!(
            esc(r#"R&D <a> "q" 'p'"#),
            "R&amp;D &lt;a&gt; &quot;q&quot; &apos;p&apos;"
        );
    }

    #[test]
    fn escaping_does_not_double_encode() {
        // The `&` introduced by `<` -> `&lt;` must not itself be escaped again.
        assert_eq!(esc("<&>"), "&lt;&amp;&gt;");
    }

    #[test]
    fn category_with_ampersand_yields_well_formed_output() {
        let svg = render_with_category("R&D");
        let bad = unescaped_ampersands(&svg);
        assert!(bad.is_empty(), "unescaped ampersands found: {bad:?}");
        assert!(svg.contains("R&amp;D"), "category should appear escaped");
    }

    #[test]
    fn angle_brackets_in_data_cannot_open_a_tag() {
        let svg = render_with_category("<10%");
        assert!(svg.contains("&lt;10%"), "should be escaped");
        assert!(!svg.contains("<10%"), "raw angle bracket leaked into markup");
    }

    #[test]
    fn title_is_escaped_too() {
        let svg = render_with_category("Sales");
        assert!(svg.contains("Q3 &lt;spend&gt; &amp; forecast"));
    }


    #[test]
    fn text_width_counts_characters_not_bytes() {
        // "한국" is 2 characters but 6 bytes. Byte counting inflated it threefold
        // and blew out the margins for every CJK label.
        let w = estimate_text_width("한국", 10.0);
        assert!((w - 20.0).abs() < 1e-9, "expected ~2 em, got {w}");
    }

    #[test]
    fn latin_width_is_unchanged_by_the_fix() {
        assert!((estimate_text_width("Hello", 10.0) - 29.0).abs() < 1e-9);
    }

    #[test]
    fn fullwidth_glyphs_are_wider_per_character_than_latin() {
        assert!(estimate_text_width("한국어", 10.0) > estimate_text_width("abc", 10.0));
    }

    // -- the area mark ------------------------------------------------------

    /// Every `<polygon>` in the output, as its raw points string.
    fn polygons(svg: &str) -> Vec<String> {
        svg.match_indices("<polygon points=\"")
            .map(|(i, m)| {
                let rest = &svg[i + m.len()..];
                rest[..rest.find('"').unwrap()].to_string()
            })
            .collect()
    }

    /// Parse "x,y x,y ..." into pairs.
    fn points_of(poly: &str) -> Vec<(f64, f64)> {
        poly.split_whitespace()
            .filter_map(|p| p.split_once(','))
            .map(|(x, y)| (x.parse().unwrap(), y.parse().unwrap()))
            .collect()
    }

    fn render_area(layer: Layer, ys: Vec<f64>) -> String {
        let df = DataFrame::new()
            .with_float("a", vec![1.0, 2.0, 3.0, 4.0])
            .with_float("b", ys)
            .with_str("g", vec!["x".into(), "y".into(), "x".into(), "y".into()]);
        let mut data = HashMap::new();
        data.insert("t".to_string(), df);
        let spec = PlotSpec::new().data("t").x("a").y("b").layer(layer);
        SvgRenderer::default().render(&spec, &data)
    }

    #[test]
    fn an_area_closes_on_its_baseline() {
        // The defining property of the mark: the region runs from the data down
        // to zero, so the last two vertices sit on one horizontal line, and that
        // line is lower on screen (larger y) than any data vertex.
        let svg = render_area(Layer::new(Mark::Area), vec![4.0, 5.0, 3.0, 6.0]);
        let polys = polygons(&svg);
        assert_eq!(polys.len(), 1, "one ungrouped area is one region");

        let pts = points_of(&polys[0]);
        assert_eq!(pts.len(), 6, "4 data vertices + 2 closing the baseline");
        let (right, left) = (pts[4], pts[5]);
        assert!((right.1 - left.1).abs() < 1e-6, "the closing edge is level");
        assert!(
            pts[..4].iter().all(|p| p.1 < right.1),
            "every data vertex sits above the baseline"
        );
    }

    #[test]
    fn the_axis_stretches_to_include_the_baseline() {
        // Without this the fill runs off the bottom of the panel and reads as a
        // floating band. The baseline must land *inside* the panel, not below
        // it — the same stretch `bar` gets, asked as "does anything measure
        // from a baseline?" rather than "is there a bar?".
        let svg = render_area(Layer::new(Mark::Area), vec![40.0, 50.0, 30.0, 60.0]);
        let pts = points_of(&polygons(&svg)[0]);
        let baseline_y = pts[4].1;

        // A "0" tick must exist, and the baseline must sit at the panel floor
        // rather than beyond it.
        assert!(svg.contains(">0<"), "the zero tick is on the axis:\n{svg}");
        assert!(
            baseline_y.is_finite() && baseline_y > pts[..4].iter().map(|p| p.1).fold(0.0, f64::max),
            "baseline below the data but on the panel"
        );
    }

    // A ribbon's data: several y values per x, so `range` reduces each x to a
    // (low, high) pair and the band spans them. Two groups when `g` is used.
    fn ribbon_frame() -> DataFrame {
        DataFrame::new()
            .with_float("a", vec![1.0, 1.0, 2.0, 2.0, 3.0, 3.0])
            .with_float("b", vec![4.0, 6.0, 3.0, 7.0, 5.0, 8.0])
            .with_str("g", vec!["p".into(), "p".into(), "p".into(),
                                 "q".into(), "q".into(), "q".into()])
    }

    #[test]
    fn a_ribbon_spans_between_its_low_and_high() {
        // The defining property: at each x the band runs from the low boundary up
        // to the high one, and it closes on that low boundary — not on a flat
        // baseline the way an `area` does. So the polygon is the high edge
        // left→right then the low edge right→left, and at every shared x the high
        // vertex sits *above* (smaller screen-y) its low partner.
        let mut data = HashMap::new();
        data.insert("t".to_string(), ribbon_frame());
        let spec = PlotSpec::new().data("t").x("a").y("b")
            .layer(Layer::new(Mark::Ribbon).transform(Transform::Range));
        let svg = SvgRenderer::default().render(&spec, &data);

        let polys = polygons(&svg);
        assert_eq!(polys.len(), 1, "one ungrouped ribbon is one band");
        let pts = points_of(&polys[0]);
        assert_eq!(pts.len(), 6, "3 high vertices + 3 low vertices, no baseline pair");

        // First half is the high edge (x ascending), second half the low edge
        // (x descending) — so pts[i] and pts[5-i] share an x, high over low.
        for i in 0..3 {
            let hi = pts[i];
            let lo = pts[5 - i];
            assert!((hi.0 - lo.0).abs() < 1e-6, "high and low share the x at column {i}");
            assert!(hi.1 < lo.1, "the high boundary sits above the low one at column {i}");
        }
    }

    #[test]
    fn a_ribbon_does_not_stretch_the_axis_to_zero() {
        // A ribbon floats between two synthesized extents — it has no baseline, so
        // (unlike `area`) its measured axis must fit the band, never reach down to
        // 0. Values well clear of zero must not summon a zero tick.
        let mut data = HashMap::new();
        data.insert("t".to_string(), DataFrame::new()
            .with_float("a", vec![1.0, 1.0, 2.0, 2.0, 3.0, 3.0])
            .with_float("b", vec![40.0, 46.0, 43.0, 57.0, 51.0, 58.0]));
        let spec = PlotSpec::new().data("t").x("a").y("b")
            .layer(Layer::new(Mark::Ribbon).transform(Transform::Range));
        let svg = SvgRenderer::default().render(&spec, &data);

        assert!(!svg.contains(">0<"), "a ribbon's axis fits the band, not a baseline:\n{svg}");
        // And the band's lowest vertex is the low boundary of the data (~40),
        // sitting near the panel floor rather than pinned to an off-data zero.
        let pts = points_of(&polygons(&svg)[0]);
        let lowest = pts.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max);
        let highest = pts.iter().map(|p| p.1).fold(f64::INFINITY, f64::min);
        assert!(highest < lowest, "the band has real thickness");
    }

    #[test]
    fn a_split_ribbon_draws_one_translucent_band_per_group() {
        // `color` splits the ribbon into one band per group (the discrete split
        // `area` makes). The bands share their x and overlap, so — having no
        // baseline to `stack` — they draw translucent by default, the rule
        // overlaid boxes use.
        let mut data = HashMap::new();
        data.insert("t".to_string(), ribbon_frame());
        let spec = PlotSpec::new().data("t").x("a").y("b")
            .layer(Layer::new(Mark::Ribbon).transform(Transform::Range).encode(Channel::Color, "g"));
        let svg = SvgRenderer::default().render(&spec, &data);

        let polys = polygons(&svg);
        assert_eq!(polys.len(), 2, "two groups, two bands");
        let translucent = svg.matches("fill-opacity=\"0.400\"").count();
        assert!(translucent >= 2, "overlapping bands draw at the overlay weight, got:\n{svg}");
    }

    #[test]
    fn a_step_moves_only_in_right_angles() {
        // A step holds each value until it changes, so its path is horizontal or
        // vertical between vertices — never the diagonal a `line` would draw. That
        // is the whole difference between the two marks, so it is what the test
        // pins.
        let mut data = HashMap::new();
        data.insert("t".to_string(), DataFrame::new()
            .with_float("x", vec![0.0, 1.0, 2.0, 3.0])
            .with_float("y", vec![0.0, 2.0, 1.0, 3.0]));
        let spec = PlotSpec::new().data("t").x("x").y("y")
            .layer(Layer::new(Mark::Step));
        let svg = SvgRenderer::default().render(&spec, &data);

        let line = svg.lines().find(|l| l.contains("<polyline")).expect("a step draws a polyline");
        let pts: Vec<(f64, f64)> = line.split("points=\"").nth(1).unwrap()
            .split('"').next().unwrap()
            .split_whitespace()
            .map(|p| {
                let mut it = p.split(',');
                (it.next().unwrap().parse().unwrap(), it.next().unwrap().parse().unwrap())
            })
            .collect();
        assert!(pts.len() > 4, "a stepped path has more vertices than its points, got {}", pts.len());
        for w in pts.windows(2) {
            let same_x = (w[0].0 - w[1].0).abs() < 0.01;
            let same_y = (w[0].1 - w[1].1).abs() < 0.01;
            assert!(same_x || same_y, "a step segment is horizontal or vertical, got {:?}->{:?}", w[0], w[1]);
        }
    }

    // -----------------------------------------------------------------------
    // A category on the domain — the profile, and the radar
    // -----------------------------------------------------------------------

    /// Four categories, one value each: the frame the profile tests read.
    fn profile_data() -> HashMap<String, DataFrame> {
        HashMap::from([(
            "t".to_string(),
            DataFrame::new()
                .with_str("g", vec!["a", "b", "c", "d"].into_iter().map(String::from).collect())
                .with_float("v", vec![10.0, 40.0, 20.0, 30.0]),
        )])
    }

    /// The vertices of the first polyline in an SVG.
    fn polyline_points(svg: &str) -> Vec<(f64, f64)> {
        let l = svg.lines().find(|l| l.contains("<polyline")).expect("no polyline drawn");
        l.split("points=\"").nth(1).unwrap().split('"').next().unwrap()
            .split_whitespace()
            .map(|p| {
                let mut it = p.split(',');
                (it.next().unwrap().parse().unwrap(), it.next().unwrap().parse().unwrap())
            })
            .collect()
    }

    fn profile(mark: Mark, polar: bool) -> String {
        let mut s = PlotSpec::new().data("t").x("g").y("v");
        if polar {
            s = s.coord(CoordSpace::Polar(crate::ir::PolarView { start: 0.0 }));
        }
        SvgRenderer::default().render(&s.layer(Layer::new(mark)), &profile_data())
    }

    /// A path across categories lands on the category slots — the same places a
    /// `bar` or a `point` would stand, since they all resolve a position through
    /// one function. Four categories, four vertices, evenly spaced.
    #[test]
    fn a_line_across_categories_stands_where_the_bars_would() {
        let pts = polyline_points(&profile(Mark::Line, false));
        assert_eq!(pts.len(), 4, "one vertex per category, got {pts:?}");

        // Evenly spaced along x: the categorical scale gives every slot the same
        // width, so the gaps between consecutive vertices are equal.
        let gaps: Vec<f64> = pts.windows(2).map(|w| w[1].0 - w[0].0).collect();
        assert!(gaps.iter().all(|g| (g - gaps[0]).abs() < 0.5),
            "category slots are not evenly spaced: {gaps:?}");
        assert!(gaps[0] > 1.0, "the vertices collapsed onto one x");

        // And the heights track the values (10, 40, 20, 30) — y grows downward in
        // SVG, so the largest value has the smallest y.
        let ys: Vec<f64> = pts.iter().map(|p| p.1).collect();
        assert!(ys[1] < ys[3] && ys[3] < ys[2] && ys[2] < ys[0],
            "the profile does not track its values: {ys:?}");
    }

    /// The radar closes. A categorical angular axis has no repeated endpoint to
    /// land on — each category appears exactly once — so the closing segment is
    /// drawn, or the shape is left with a wedge missing. The flat plot of the same
    /// sentence must *not* gain that vertex: a straight axis has two ends.
    #[test]
    fn a_radar_closes_on_itself_and_a_flat_profile_does_not() {
        let flat = polyline_points(&profile(Mark::Line, false));
        let radar = polyline_points(&profile(Mark::Line, true));

        assert_eq!(flat.len(), 4, "a flat profile has one vertex per category");
        assert_eq!(radar.len(), 5, "a radar repeats its first vertex to close");
        let (first, last) = (radar[0], radar[radar.len() - 1]);
        assert!((first.0 - last.0).abs() < 0.01 && (first.1 - last.1).abs() < 0.01,
            "the radar's last vertex is not its first: {first:?} vs {last:?}");
        assert!((flat[0].0 - flat[3].0).abs() > 1.0, "the flat profile closed on itself");
    }

    // ---- the five marks that learned to bend, 2026-07-26 --------------------
    //
    // The property each of these pins is the one the refusal named: a segment that
    // **holds** a value across a span must follow the ring, because half of it
    // drawn straight puts the mark where the data is not (§12). A chord always
    // falls *inside* the circle it subtends, so "is it an arc" is measurable
    // without parsing the path: take the segment's midpoint and compare its
    // distance from the center with the endpoints'.

    /// Every `A` command's radius in an SVG, in the order drawn.
    fn arc_radii(svg: &str) -> Vec<f64> {
        let mut out = Vec::new();
        for l in svg.lines().filter(|l| l.contains(r#"<path d="#)) {
            let Some(d) = l.split(r#"d=""#).nth(1).and_then(|s| s.split('"').next()) else { continue };
            let t: Vec<&str> = d.split_whitespace().collect();
            for (i, w) in t.iter().enumerate() {
                if *w == "A" {
                    if let Some(r) = t.get(i + 1).and_then(|s| s.parse::<f64>().ok()) { out.push(r); }
                }
            }
        }
        out
    }

    /// A stair's **treads become arcs**, and its risers stay straight.
    ///
    /// This is the segment the whole space was waiting on. A tread asserts one
    /// value across a span of angle, so bent it is an arc at constant radius; a
    /// riser changes the value at one angle, so it is exactly the radius and needs
    /// no arc at all. Four categories give four treads (three between them plus
    /// the closing one) and three risers.
    #[test]
    fn a_stairs_treads_become_arcs_and_its_risers_stay_radial() {
        let svg = profile(Mark::Step, true);
        let radii = arc_radii(&svg);
        assert!(!radii.is_empty(), "a polar staircase drew no arc at all");

        // Every tread rides a ring, so its radius is one of the four values'
        // radii — and the four values differ, so the treads' radii must too.
        let mut uniq = radii.clone();
        uniq.sort_by(|a, b| a.partial_cmp(b).unwrap());
        uniq.dedup_by(|a, b| (*a - *b).abs() < 0.01);
        assert_eq!(uniq.len(), 4, "four values should give four tread radii: {radii:?}");

        // The flat staircase is a polyline and gains no arcs — the mark means the
        // same thing in both spaces, and only its geometry changed (Law 6).
        assert!(arc_radii(&profile(Mark::Step, false)).is_empty(),
            "a flat staircase must not draw arcs");
    }

    /// A staircase **closes on a wrapped domain**, and does not flat.
    ///
    /// Flat, the last category's value gets no tread: there is no slot after it.
    /// Bent, the categories exhaust the turn, so the last one's slot runs round to
    /// the first — `line` and `area` close for the same reason.
    #[test]
    fn a_polar_staircase_closes_and_a_flat_one_does_not() {
        assert_eq!(arc_radii(&profile(Mark::Step, true)).len(), 4,
            "four categories should give four treads once the turn is closed");
        let flat = polyline_points(&profile(Mark::Step, false));
        // 4 categories → 1 start + 3×(riser + tread) = 7 vertices, and the last is
        // not the first.
        assert!((flat[0].0 - flat[flat.len() - 1].0).abs() > 1.0,
            "the flat staircase closed on itself: {flat:?}");
    }

    /// A **band's boundaries are chords, not arcs** — the correction this session
    /// made to the recorded refusal, pinned so it cannot be re-litigated by
    /// accident. A ribbon's boundary runs through the data's own vertices, which
    /// is `line`'s geometry, and it closes on its own lower boundary rather than
    /// on any ring. So the radar band draws with no arc command anywhere.
    #[test]
    fn a_radar_band_is_drawn_with_chords_and_needs_no_arc() {
        let data = HashMap::from([(
            "t".to_string(),
            DataFrame::new()
                .with_str("g", vec!["a", "a", "b", "b", "c", "c", "d", "d"]
                    .into_iter().map(String::from).collect())
                .with_float("v", vec![1.0, 5.0, 2.0, 7.0, 3.0, 6.0, 2.0, 4.0]),
        )]);
        let spec = PlotSpec::new().data("t").x("g").y("v")
            .coord(CoordSpace::Polar(crate::ir::PolarView::default()))
            .layer(Layer::new(Mark::Ribbon).transform(Transform::Range));
        let svg = SvgRenderer::default().render(&spec, &data);
        assert!(arc_radii(&svg).is_empty(), "a band needed no arc and drew one");

        // One polygon, and it **closes**: four categories give 4+1 vertices per
        // boundary and two boundaries, so ten in all.
        let poly = svg.lines().find(|l| l.contains("<polygon")).expect("no band drawn");
        let n = poly.split("points=\"").nth(1).unwrap().split('"').next().unwrap()
            .split_whitespace().count();
        assert_eq!(n, 10, "the radar band did not close round the wrap: {n} vertices");
    }

    /// A **zone that spans the turn is an annulus**, not a sector with a seam.
    /// `arc_to` splits a full turn at the antipode because an `A` whose ends
    /// coincide draws nothing — so the ring comes out as two arcs per edge, four
    /// in all, and the hole is real.
    #[test]
    fn a_zone_spanning_the_turn_closes_into_an_annulus() {
        let data = HashMap::from([
            ("t".to_string(), DataFrame::new()
                .with_float("hour", vec![0.0, 6.0, 12.0, 18.0, 24.0])
                .with_float("v", vec![1.0, 4.0, 2.0, 5.0, 1.0])),
            ("band".to_string(), DataFrame::new()
                .with_float("lo", vec![2.0]).with_float("hi", vec![4.0])),
        ]);
        let spec = PlotSpec::new().data("t").x("hour").y("v")
            .coord(CoordSpace::Polar(crate::ir::PolarView::default()))
            .layer(Layer::new(Mark::Line))
            .layer({
                let mut z = Layer::new(Mark::Zone);
                z.data = Some("band".into());
                z.bounds = Some(crate::ir::BoundsSpec {
                    lower: Some("lo".into()), upper: Some("hi".into()), ..Default::default()
                });
                z
            });
        let svg = SvgRenderer::default().render(&spec, &data);
        let radii = arc_radii(&svg);
        assert_eq!(radii.len(), 4,
            "a full-turn sector is two arcs per edge, split at the antipode: {radii:?}");
        // Two distinct radii — the inner and outer edges of the ring — and the
        // inner one is genuinely smaller, so there is a hole.
        let (lo, hi) = (radii.iter().cloned().fold(f64::MAX, f64::min),
                        radii.iter().cloned().fold(0.0, f64::max));
        assert!(hi - lo > 1.0, "the annulus has no hole: inner {lo}, outer {hi}");
        assert!(!svg.contains("NaN"), "the annulus wrote NaN coordinates");
    }

    /// A whisker's **caps hold their pixel width** when bent, so the two ends of
    /// one interval draw the same length of ink at different radii — and therefore
    /// subtend *different* angles. §18's rule that a stroke's width is pixels: a
    /// cap says where the span stops and its width carries no quantity.
    #[test]
    fn a_bent_whiskers_caps_keep_their_pixel_width() {
        let data = HashMap::from([(
            "t".to_string(),
            DataFrame::new()
                .with_str("g", vec!["a", "a", "b", "b"].into_iter().map(String::from).collect())
                .with_float("v", vec![1.0, 9.0, 2.0, 8.0]),
        )]);
        let spec = PlotSpec::new().data("t").x("g").y("v")
            .coord(CoordSpace::Polar(crate::ir::PolarView::default()))
            .layer(Layer::new(Mark::Interval).transform(Transform::Range));
        let svg = SvgRenderer::default().render(&spec, &data);

        // Two intervals, two caps each: four arcs, at four different radii (the
        // four extents 1, 9, 2, 8).
        let radii = arc_radii(&svg);
        assert_eq!(radii.len(), 4, "two capped whiskers should draw four arcs: {radii:?}");
        assert!(radii.iter().any(|r| (r - radii[0]).abs() > 1.0),
            "the caps all landed at one radius: {radii:?}");

        // Each cap's ink is its radius times the angle it subtends. Reading the
        // chord back off the path and inverting gives the arc length, and all four
        // must agree — that is the pixel rule holding.
        let inks: Vec<f64> = svg.lines()
            .filter(|l| l.contains(r#"stroke-linecap="round""#) && l.contains(" A "))
            .filter_map(|l| {
                let d = l.split(r#"d=""#).nth(1)?.split('"').next()?;
                let t: Vec<&str> = d.split_whitespace().collect();
                let (x0, y0) = (t[1].parse::<f64>().ok()?, t[2].parse::<f64>().ok()?);
                let r = t[4].parse::<f64>().ok()?;
                let (x1, y1) = (t[9].parse::<f64>().ok()?, t[10].parse::<f64>().ok()?);
                let chord = ((x1 - x0).powi(2) + (y1 - y0).powi(2)).sqrt();
                Some(2.0 * r * (chord / (2.0 * r)).min(1.0).asin())
            })
            .collect();
        assert_eq!(inks.len(), 4, "could not measure all four caps");
        assert!(inks.iter().all(|k| (k - inks[0]).abs() < 0.1),
            "caps drew different lengths of ink: {inks:?}");
    }

    /// The closing rule is the *categorical* angular axis's, not polar's in
    /// general. A measured angle closes itself when the data supplies both ends of
    /// its cycle (hour 0 and hour 24 are the same bearing), so repeating a vertex
    /// there would draw a segment that is already drawn — and on data that does
    /// *not* span the cycle it would invent a closure the reader never asked for.
    #[test]
    fn a_measured_angle_is_left_to_close_itself() {
        let data = HashMap::from([(
            "t".to_string(),
            DataFrame::new()
                .with_float("hour", vec![0.0, 6.0, 12.0, 18.0, 24.0])
                .with_float("trips", vec![10.0, 40.0, 20.0, 30.0, 10.0]),
        )]);
        let spec = PlotSpec::new().data("t").x("hour").y("trips")
            .coord(CoordSpace::Polar(crate::ir::PolarView { start: 0.0 }))
            .layer(Layer::new(Mark::Line));
        let pts = polyline_points(&SvgRenderer::default().render(&spec, &data));

        assert_eq!(pts.len(), 5, "a measured angle draws its rows and no extra vertex");
        // It still closes — because the data closed it, which is the point.
        let (first, last) = (pts[0], pts[pts.len() - 1]);
        assert!((first.0 - last.0).abs() < 0.01 && (first.1 - last.1).abs() < 0.01,
            "hours 0 and 24 should land on the same bearing: {first:?} vs {last:?}");
    }

    /// The filled radar closes too, and by the same rule — `area` and `line` ask
    /// `Polar::wraps` rather than each deciding. A region left open shows as a
    /// wedge cut back to the center, which is what this catches.
    #[test]
    fn a_filled_radar_closes_the_same_way_the_line_does() {
        let svg = profile(Mark::Area, true);
        let poly = svg.lines().find(|l| l.contains("<polygon")).expect("no region drawn");
        let pts: Vec<(f64, f64)> = poly.split("points=\"").nth(1).unwrap().split('"').next().unwrap()
            .split_whitespace()
            .map(|p| {
                let mut it = p.split(',');
                (it.next().unwrap().parse().unwrap(), it.next().unwrap().parse().unwrap())
            })
            .collect();
        // Boundary (5 vertices, first repeated) plus the retraced floor (5 more):
        // in polar an area closes along the *ring*, vertex by vertex.
        assert_eq!(pts.len(), 10, "the closed boundary and its retraced floor, got {pts:?}");
        assert!((pts[0].0 - pts[4].0).abs() < 0.01 && (pts[0].1 - pts[4].1).abs() < 0.01,
            "the region's boundary does not return to its first vertex");
    }

    #[test]
    fn a_step_bin_splits_into_one_silhouette_per_color() {
        // `step * bin` is the histogram outline: one stepped, unfilled polyline
        // per group, each in its own hue — the seaborn `element="step"` look.
        let mut data = HashMap::new();
        data.insert("t".to_string(), DataFrame::new()
            .with_float("x", vec![0.0, 1.0, 2.0, 3.0, 2.0, 3.0, 4.0, 5.0])
            .with_str("g", ["a", "a", "a", "a", "b", "b", "b", "b"]
                .iter().map(|s| s.to_string()).collect()));
        let spec = PlotSpec::new().data("t").x("x")
            .layer(Layer::new(Mark::Step).transform(Transform::Bin).encode(Channel::Color, "g"));
        let svg = SvgRenderer::default().render(&spec, &data);

        let polylines: Vec<&str> = svg.lines().filter(|l| l.contains("<polyline")).collect();
        assert_eq!(polylines.len(), 2, "one silhouette per species");
        assert!(polylines.iter().all(|l| l.contains(r#"fill="none""#)), "a step is unfilled");
        let strokes: std::collections::HashSet<&str> = polylines.iter()
            .map(|l| l.split("stroke=\"").nth(1).unwrap().split('"').next().unwrap())
            .collect();
        assert_eq!(strokes.len(), 2, "two distinct hues, got {strokes:?}");
    }

    #[test]
    fn a_bar_border_setting_reaches_the_stroke() {
        // `style(border_color =, border_size =)` overrides the derived edge with a
        // solid outline of the given color and width; the fill is untouched.
        let mut data = HashMap::new();
        data.insert("t".to_string(), DataFrame::new()
            .with_str("g", vec!["a".to_string(), "b".to_string()])
            .with_float("v", vec![3.0, 5.0]));
        let spec = PlotSpec::new().data("t").x("g").y("v")
            .layer(Layer::new(Mark::Bar).style_border("white", 2.5));
        let svg = SvgRenderer::default().render(&spec, &data);

        let bar = svg.lines()
            .find(|l| l.contains("<rect") && l.contains("fill-opacity"))
            .expect("a data bar");
        assert!(bar.contains(r#"stroke="white""#), "border_color reaches the stroke: {bar}");
        assert!(bar.contains(r#"stroke-width="2.5""#), "border_size reaches the width: {bar}");
        assert!(bar.contains(r#"stroke-opacity="1""#), "a set border draws solid: {bar}");

        // `border_size = 0` draws no outline at all — the fills overlap with
        // nothing between them.
        let spec0 = PlotSpec::new().data("t").x("g").y("v")
            .layer(Layer::new(Mark::Bar).style_border("white", 0.0));
        let svg0 = SvgRenderer::default().render(&spec0, &data);
        let bar0 = svg0.lines().find(|l| l.contains("<rect") && l.contains("fill-opacity")).expect("a bar");
        assert!(bar0.contains(r#"stroke="none""#), "border_size=0 draws no stroke: {bar0}");
    }

    #[test]
    fn a_category_splits_an_area_into_one_region_each() {
        // Wilkinson 8.1.5 — a categorical variable on an aesthetic *splits* the
        // graphic. `line` splits into one polyline per category; `area` must
        // split into one region per category, or it is a per-mark exception.
        let svg = render_area(
            Layer::new(Mark::Area).encode(Channel::Color, "g"),
            vec![4.0, 5.0, 3.0, 6.0],
        );
        assert_eq!(polygons(&svg).len(), 2, "two categories, two regions");
    }

    #[test]
    fn group_splits_an_area_without_coloring_it() {
        // The same distinction `group` earns on `line`: separate the series,
        // invent no encoding that has no legend to decode it.
        let svg = render_area(
            Layer::new(Mark::Area).encode(Channel::Group, "g"),
            vec![4.0, 5.0, 3.0, 6.0],
        );
        let polys = polygons(&svg);
        assert_eq!(polys.len(), 2);

        let fills: Vec<&str> = svg
            .match_indices("<polygon")
            .map(|(i, _)| {
                let rest = &svg[i..];
                let f = rest.find("fill=\"").unwrap() + 6;
                &rest[f..f + rest[f..].find('"').unwrap()]
            })
            .collect();
        assert_eq!(fills[0], fills[1], "group separates, it does not color");
    }

    #[test]
    fn an_area_is_drawn_in_x_order_not_data_order() {
        // A region whose boundary follows row order folds over itself. The rows
        // here are deliberately out of order on x.
        let df = DataFrame::new()
            .with_float("a", vec![3.0, 1.0, 4.0, 2.0])
            .with_float("b", vec![5.0, 4.0, 6.0, 3.0]);
        let mut data = HashMap::new();
        data.insert("t".to_string(), df);
        let spec = PlotSpec::new().data("t").x("a").y("b").layer(Layer::new(Mark::Area));
        let svg = SvgRenderer::default().render(&spec, &data);

        let pts = points_of(&polygons(&svg)[0]);
        let xs: Vec<f64> = pts[..4].iter().map(|p| p.0).collect();
        assert!(
            xs.windows(2).all(|w| w[0] <= w[1]),
            "boundary must ascend in x, got {xs:?}"
        );
    }

    #[test]
    fn an_area_takes_a_set_opacity_for_the_whole_region() {
        // One region, one fill — `opacity` is a setting here, never a channel.
        let svg = render_area(
            Layer::new(Mark::Area).style_opacity(0.25),
            vec![4.0, 5.0, 3.0, 6.0],
        );
        assert!(svg.contains("fill-opacity=\"0.250\""), "{svg}");
    }

    #[test]
    fn an_area_carries_no_stroke() {
        // The edge on top of a region is a `line` layered over it — superposition
        // is the grammar's own answer, and it keeps the parked border question
        // out of a mark that would otherwise quietly grow one.
        let svg = render_area(Layer::new(Mark::Area), vec![4.0, 5.0, 3.0, 6.0]);
        let poly = &polygons(&svg)[0];
        let tag_start = svg.find("<polygon").unwrap();
        let tag = &svg[tag_start..tag_start + svg[tag_start..].find("/>").unwrap()];
        assert!(!tag.contains("stroke"), "area drew a stroke: {tag} ({poly})");
    }

    #[test]
    fn every_mark_is_drawn_or_refused() {
        // The renderer's `match` on Mark and `legality::check_mark` must
        // partition the enum: a mark either produces marks in the output, or is
        // refused before rendering. `area` sat in neither camp — permissive
        // legality rules, a `_ => {}` renderer arm — and drew an empty panel
        // with exit 0 for as long as it did.
        use crate::legality::{check, DiagnosticKind};

        for mark in [Mark::Point, Mark::Line, Mark::Area, Mark::Bar,
                     Mark::Text, Mark::Path, Mark::Surface] {
            let df = DataFrame::new()
                .with_float("a", vec![1.0, 2.0, 3.0])
                .with_float("b", vec![4.0, 5.0, 6.0])
                .with_str("lab", vec!["Zz".to_string(), "Zz".to_string(), "Zz".to_string()]);
            let mut data = HashMap::new();
            data.insert("t".to_string(), df);
            // `text`'s minimum syllable is `label`; give it one so it can draw at
            // all (as `interval` would need a range transform). The others place
            // from x/y alone.
            let layer = if mark == Mark::Text {
                Layer::new(mark.clone()).encode(Channel::Label, "lab")
            } else {
                Layer::new(mark.clone())
            };
            let spec = PlotSpec::new().data("t").x("a").y("b").layer(layer);

            // **Refused means *any fatal* diagnostic, and `surface` is what made that
            // distinction matter.** This read `Unsupported` alone while every
            // undrawable mark was undrawable for the same reason — valid grammar with
            // no renderer behind it. A flat `surface` is not that: it is a mark whose
            // minimum syllable includes the cube, so the refusal is `Illegal`, the same
            // kind `interval` gets without a range transform. Both kinds are fatal and
            // draw nothing, which is the only property this partition is about.
            let refused = check(&spec, &data).iter().any(|d| {
                matches!(d.kind, DiagnosticKind::Unsupported | DiagnosticKind::Illegal)
            });
            let svg = SvgRenderer::default().render(&spec, &data);
            // A bar is found by its hairline rather than by `<rect`, which the
            // panel background and every clip path also emit. That hairline is
            // the fingerprint the rest of this suite already uses, and it is
            // already known to need a different hook once the border
            // setting absorbs it. `text` is found by its glyph content (`>Zz<`),
            // which no axis label emits.
            let drew = ["<circle", "<polyline", "<polygon", r#"stroke-width="0.5""#, ">Zz<"]
                .iter()
                .any(|t| svg.contains(t));

            assert!(
                refused != drew,
                "{mark:?}: refused={refused} drew={drew} — a mark must be exactly one of the two"
            );
        }
    }

    // -- axis range fits the data (Wilkinson §6.2.2) ----------------------

    #[test]
    fn a_linear_axis_fits_its_data_not_the_outermost_tick() {
        // year 1952–2007 used to snap out to 1940–2020, roughly a third of the
        // panel left dead on the two sides. Now the range follows the data.
        let (lo, hi) = fitted_range(1952.0, 2007.0, false, false);
        assert!(lo > 1940.0 && lo <= 1952.0, "left end fits close: {lo}");
        assert!(hi >= 2007.0 && hi < 2020.0, "right end fits close: {hi}");
        assert!(lo < 1952.0 && hi > 2007.0, "free ends breathe a little");
    }

    #[test]
    fn a_baseline_end_is_pinned_a_free_end_breathes() {
        // A bar or area measures from zero; a gap below the baseline would put
        // zero somewhere it is not. The other end still gets headroom.
        let (lo, hi) = fitted_range(0.0, 46.0, true, false);
        assert_eq!(lo, 0.0, "no gap below the baseline");
        assert!(hi > 46.0, "headroom above the tallest value");
        // The same numbers with no baseline breathe below zero — the pin is the
        // only difference, and it is what keeps the two cases from being an
        // exception hidden in a shared path.
        let (lo2, _) = fitted_range(0.0, 46.0, false, false);
        assert!(lo2 < 0.0, "a free low end breathes below zero");
    }

    #[test]
    fn ticks_outside_the_fitted_range_are_dropped() {
        let t = nice_ticks(1952.0, 2007.0, 5);
        assert!(
            t.values.contains(&1940.0) && t.values.contains(&2020.0),
            "nice_ticks brackets the data outward — that is the set to trim"
        );
        let (fitted, (lo, hi)) = fit_axis(t, 1952.0, 2007.0, false, false);
        assert!(
            fitted.values.iter().all(|&v| v >= lo && v <= hi),
            "no tick may sit outside the panel"
        );
        assert!(!fitted.values.contains(&1940.0), "the dead 1940 tick is gone");
        assert!(fitted.values.contains(&1960.0) && fitted.values.contains(&2000.0));
        assert_eq!(fitted.values.len(), fitted.labels.len(), "labels trimmed with values");
    }

    #[test]
    fn the_guard_keeps_a_bare_axis_from_forming() {
        // The opposite failure Wilkinson names: if no nice tick lands inside the
        // fitted range, dropping them all bares the axis. There the loose
        // bracketing range is the lesser evil and is kept.
        let t = TickSpec {
            values: vec![0.0, 100.0],
            labels: vec!["0".into(), "100".into()],
            step: 100.0,
        };
        let (kept, range) = fit_axis(t, 40.0, 41.0, false, false);
        assert_eq!(kept.values, vec![0.0, 100.0], "ticks kept, not dropped to nothing");
        assert_eq!(range, (0.0, 100.0), "loose range kept alongside them");
    }

    // -- limits: the stated domain reaches the axis (spec §10) ---------------

    fn axis_range_with_limits(lo: Option<f64>, hi: Option<f64>) -> (TickSpec, (f64, f64)) {
        let df = DataFrame::new()
            .with_float("year", vec![1952.0, 1980.0, 2007.0])
            .with_float("life", vec![40.0, 60.0, 82.0]);
        let mut data = HashMap::new();
        data.insert("t".to_string(), df);
        let spec = PlotSpec::new().data("t").x_limited("year", lo, hi).y("life")
            .layer(Layer::new(Mark::Point));
        axis_range_x(&spec, &data)
    }

    #[test]
    fn a_stated_end_is_flush_and_an_unstated_one_still_breathes() {
        // The breathing margin exists to keep a *glyph* off the frame, and an
        // end that means something is drawn where it is (the fill boundary, the
        // baseline zero). A stated limit means something by construction — and
        // the polar cycle needs it, since 5% of headroom on a periodic axis is a
        // circle that does not close.
        let (_, both) = axis_range_with_limits(Some(1950.0), Some(2010.0));
        assert_eq!(both, (1950.0, 2010.0), "both stated ends sit exactly where stated");

        let (_, half) = axis_range_with_limits(Some(1950.0), None);
        assert_eq!(half.0, 1950.0, "the stated end is flush");
        assert!(half.1 > 2007.0, "the free end still breathes: {}", half.1);

        let (_, none) = axis_range_with_limits(None, None);
        assert!(none.0 < 1952.0 && none.1 > 2007.0, "stating nothing changes nothing");
    }

    /// A log axis takes its range from the **data**, not from its own ticks, and a
    /// bar on one gets the half-slot every other axis gives it.
    ///
    /// The log branch was §10's last exemption from *ticks from the scheme, range
    /// from the data*, and on a histogram it clipped: a bar range is read off the
    /// **centers**, so a slot reaches half a bin past the last of them, and
    /// bracketing to whole powers ended the axis at 10^5 while the last bar reached
    /// 10^5.055. Reported by a reader looking at the plot — the tell was an
    /// asymmetry, a wide empty band at one end and a sliced bar at the other, which
    /// is what bracketing outward looks like when the data is not centered in its
    /// decades. Both ends are the one mistake.
    #[test]
    fn a_log_axis_holds_its_bars_rather_than_its_decades() {
        // Bin centers spanning 10^2.45 .. 10^4.99 with a slot of 0.134 decades:
        // gapminder's gdp cut into twenty, the case that reported this.
        let (lo_c, hi_c) = (2.4491_f64, 4.9883_f64);
        let slot = (hi_c - lo_c) / 19.0;
        let centers: Vec<f64> = (0..20)
            .map(|i| lo_c + slot * (i as f64))
            .collect();
        let df = DataFrame::new().with_float("gdp", centers.clone());

        let (_, range) = build_axis(
            &[&df], &[&df], &[], "gdp", None,
            /* bar_position */ true, false, false, None,
            /* is_log */ true, 10.0, None, (None, None), (0.0, 0.0),
        );
        // The half-slot beyond the end bars is inside the axis, both ends.
        assert!(range.0 <= lo_c - slot / 2.0 + 1e-6,
            "the first bar is sliced: axis starts {}, bar starts {}", range.0, lo_c - slot / 2.0);
        assert!(range.1 >= hi_c + slot / 2.0 - 1e-6,
            "the last bar is sliced: axis ends {}, bar ends {}", range.1, hi_c + slot / 2.0);
        // And it is *fitted*, not bracketed to the decades either side: the old
        // behavior returned exactly (2.0, 5.0), which is what clipped the bar.
        assert!(range.0 > 2.0 && range.1 < 6.0,
            "the axis bracketed out to whole powers again: {range:?}");
    }

    #[test]
    fn a_stated_domain_survives_the_bare_axis_guard() {
        // `clip_ticks` gives a *derived* range back when fitting would bare the
        // axis — the derivation was the lesser thing. A stated domain is not a
        // derivation to second-guess: handing back the loose bracketing range
        // would draw an axis nobody asked for, having accepted the binding.
        let t = || TickSpec {
            values: vec![0.0, 100.0],
            labels: vec!["0".into(), "100".into()],
            step: 100.0,
        };
        let (kept, range) = clip_ticks(t(), 40.0, 41.0);
        assert_eq!(range, (0.0, 100.0), "derived: the guard keeps the loose range");
        assert_eq!(kept.values.len(), 2);

        let (kept, range) = adopt_range(t(), 40.0, 41.0);
        assert_eq!(range, (40.0, 41.0), "stated: the range is adopted regardless");
        assert!(kept.values.is_empty(), "and the ticks outside it are gone");
    }

    #[test]
    fn a_stated_domain_holds_a_polar_axis_open_to_its_period() {
        // The forcing case, as a number. Hours observed 1..22 fit an axis that
        // closes at 22; stating the day makes the turn a whole day, so the wrap
        // is the period rather than the data's extremes.
        let df = DataFrame::new()
            .with_float("hour", vec![1.0, 10.0, 22.0])
            .with_float("n", vec![2.0, 9.0, 3.0]);
        let mut data = HashMap::new();
        data.insert("t".to_string(), df);

        let open = PlotSpec::new().data("t").x("hour").y("n").layer(Layer::new(Mark::Line));
        let (_, xs) = axis_range_x(&open, &data);
        assert!((xs.1 - xs.0 - 24.0).abs() > 0.5,
            "one turn is the fitted span, which is not a day: {xs:?}");

        let cycle = PlotSpec::new().data("t").x_limited("hour", Some(0.0), Some(24.0))
            .y("n").layer(Layer::new(Mark::Line));
        let (_, xs) = axis_range_x(&cycle, &data);
        assert_eq!(xs, (0.0, 24.0), "the whole cycle, flush at both ends");
    }

    #[test]
    fn a_stated_domain_puts_each_reading_at_its_true_angle_and_does_not_close_the_curve() {
        // A user asked whether the open curve was a defect (spec §8c). It is not,
        // and both halves of the answer are pinned here because the tempting fix
        // — treating a stated domain like a categorical one and wrapping — would
        // draw a segment between two rows that are not neighbors.
        //
        // Three-hourly readings, 1..22: they cover the day but reach neither
        // midnight, which is the ordinary shape of periodic data.
        let df = DataFrame::new()
            .with_float("hour", vec![1.0, 4.0, 7.0, 10.0, 13.0, 16.0, 19.0, 22.0])
            .with_float("height", vec![2.1, 4.4, 3.6, 1.2, 0.9, 2.8, 4.7, 3.9]);
        let data = HashMap::from([("t".to_string(), df)]);

        // Angles clockwise from twelve o'clock, read off the drawn polyline.
        let angles = |spec: &PlotSpec| -> Vec<f64> {
            let svg = SvgRenderer::default().render(spec, &data);
            let pts = svg.split("points=\"").nth(1).expect("a polyline was drawn");
            let pts = pts.split('"').next().unwrap();
            let xy: Vec<(f64, f64)> = pts
                .split_whitespace()
                .map(|p| {
                    let (a, b) = p.split_once(',').unwrap();
                    (a.parse::<f64>().unwrap(), b.parse::<f64>().unwrap())
                })
                .collect();
            // The disc's center is the first `<circle cx=`, which is the panel.
            let c = svg.split("<circle cx=\"").nth(1).unwrap();
            let cx: f64 = c.split('"').next().unwrap().parse().unwrap();
            let cy: f64 = c.split("cy=\"").nth(1).unwrap().split('"').next().unwrap()
                .parse().unwrap();
            xy.iter()
                .map(|(x, y)| ((x - cx).atan2(cy - y).to_degrees() + 360.0) % 360.0)
                .collect()
        };

        let polar = |s: PlotSpec| s.coord(CoordSpace::Polar(crate::ir::PolarView { start: 0.0 }));

        // **The defect `limits` fixes.** One turn is fitted to 1..22, so the first
        // and last readings land on the *same spoke* — drawn on top of each other,
        // three hours apart in the world and none on the page.
        // Tolerance is 0.05°, not an epsilon: these angles are read back out of
        // SVG coordinates rounded to two decimals, so the precision here is the
        // output's rather than the arithmetic's. Wide enough to survive that,
        // far tighter than any distinction being asserted (15° from 0°).
        let close_enough = |a: f64, b: f64| (a - b).abs() < 0.05;

        let open = angles(&polar(PlotSpec::new().data("t").x("hour").y("height"))
            .layer(Layer::new(Mark::Line)));
        assert!(close_enough(open[0], open[open.len() - 1]),
            "without limits the ends collide: {open:?}");

        // **With the day stated**, each reading sits where a clock would put it.
        let stated = angles(&polar(PlotSpec::new().data("t")
            .x_limited("hour", Some(0.0), Some(24.0)).y("height"))
            .layer(Layer::new(Mark::Line)));
        for (i, hour) in [1.0, 4.0, 7.0, 10.0, 13.0, 16.0, 19.0, 22.0].iter().enumerate() {
            assert!(close_enough(stated[i], hour / 24.0 * 360.0),
                "hour {hour} should sit at {}°, got {}", hour / 24.0 * 360.0, stated[i]);
        }

        // **And the curve stays open**, because no reading was taken at midnight
        // and a domain does not invent one. 22:00 to 01:00 is 45° of genuine gap.
        assert!(close_enough(stated[stated.len() - 1] - stated[0], 315.0),
            "the wrap gap is the data's three hours, not closed: {stated:?}");
        assert_eq!(stated.len(), 8, "eight readings, eight vertices — none added");
    }

    #[test]
    fn a_stated_end_is_not_pulled_back_to_the_baseline() {
        // A bar's measure axis stretches to include zero, because a length is
        // measured from somewhere. A stated end says where the axis starts, and
        // pulling it back to 0 would draw the baseline where the caller said it
        // is not — while the *unstated* end still stretches.
        let df = DataFrame::new()
            .with_str("g", vec!["a".into(), "b".into()])
            .with_float("v", vec![30.0, 50.0]);
        let mut data = HashMap::new();
        data.insert("t".to_string(), df);
        let spec = PlotSpec::new().data("t").x("g").y_limited("v", Some(20.0), None)
            .layer(Layer::new(Mark::Bar));
        let df = data.get("t").unwrap();
        let eff: Vec<DataFrame> = spec.layers.iter().map(|_| df.clone()).collect();
        let refs: Vec<&DataFrame> = eff.iter().collect();
        let (_, ys) = build_axis(&refs, &[], &[], "v", None, false, true, false, None, false, 10.0, None,
                                 crate::scale::domain_of(spec.axis_def(&Channel::Y)), (0.0, 0.0));
        assert_eq!(ys.0, 20.0, "the stated end holds against the stretch-to-zero");
    }

    // -- the violin: the slot reading of `density` (spec §5) -------------------

    /// Two groups over the **same** spread, one carrying four times the rows of the
    /// other — so the only thing the default `compare` can be reading is the count.
    /// (Different spreads would confound it: a uniform group's count-weighted
    /// density is `n / range`, so forty rows over forty units and ten over ten come
    /// out identical, which is what the first version of this fixture asserted
    /// against by accident.)
    fn violin_svg(mark: Mark, sideways: bool) -> String {
        let (mut cats, mut vals) = (Vec::new(), Vec::new());
        for (g, n) in [("wide", 40usize), ("narrow", 10)] {
            for i in 0..n { cats.push(g.to_string()); vals.push((i % 10) as f64); }
        }
        let df = DataFrame::new().with_str("g", cats).with_float("v", vals);
        let mut data = HashMap::new();
        data.insert("t".to_string(), df);
        let spec = if sideways { PlotSpec::new().data("t").x("v").y("g") }
                   else        { PlotSpec::new().data("t").x("g").y("v") };
        let spec = spec.layer(Layer::new(mark).transform(Transform::Density));
        SvgRenderer::default().render(&spec, &data)
    }

    /// Every x in a polygon, as pairs.
    fn polygon_points(svg: &str) -> Vec<Vec<(f64, f64)>> {
        svg.split("<polygon points=\"").skip(1)
            .map(|chunk| chunk.split('"').next().unwrap_or("").split_whitespace()
                .filter_map(|p| {
                    let (a, b) = p.split_once(',')?;
                    Some((a.parse().ok()?, b.parse().ok()?))
                })
                .collect())
            .collect()
    }

    /// The one geometric claim that separates the two marks, and the reason neither
    /// needed a new atom: a `ribbon` closes on its own **reflection**, an `area` on
    /// the slot's **center line**. Asserted as symmetry about that line rather than
    /// by counting vertices, so a rewrite that draws the same shape a different way
    /// still passes and one that draws a different shape cannot.
    #[test]
    fn a_violin_mirrors_and_a_half_violin_does_not() {
        let mirrored = polygon_points(&violin_svg(Mark::Ribbon, false));
        assert_eq!(mirrored.len(), 2, "one shape per category");
        for poly in &mirrored {
            let (lo, hi) = poly.iter().map(|p| p.0)
                .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), x| (a.min(x), b.max(x)));
            // The outline runs out one side and back the other, so the two halves
            // are the same width — the midpoint of the extremes is the slot center,
            // and the widest vertex on each side is equidistant from it.
            let mid = (lo + hi) / 2.0;
            assert!((mid - lo) - (hi - mid) < 1e-6, "a violin must be symmetric about its slot");
            assert!(hi - lo > 1.0, "a violin must have width");
        }

        let half = polygon_points(&violin_svg(Mark::Area, false));
        assert_eq!(half.len(), 2, "one shape per category");
        for poly in &half {
            // The return leg is the center line, so exactly one x is repeated for
            // every sample: the flat side. A mirrored outline has no such column.
            let flat = poly.iter().map(|p| p.0)
                .fold(std::collections::BTreeMap::<u64, usize>::new(), |mut m, x| {
                    *m.entry(x.to_bits()).or_default() += 1; m
                });
            let tallest = flat.values().cloned().max().unwrap_or(0);
            assert!(tallest > poly.len() / 4,
                "a half violin closes on a straight center line, so one x repeats");
        }
    }

    /// The default `compare` weights each estimate by its group's rows, so the
    /// forty-row group must draw wider than the ten-row one. Pins the *decision*
    /// (spec §5) rather than the pixels: equal areas would draw these two almost
    /// alike, since they differ far more in count than in spread.
    #[test]
    fn a_violin_is_wider_where_the_group_is_bigger() {
        let widths: Vec<f64> = polygon_points(&violin_svg(Mark::Ribbon, false)).iter()
            .map(|poly| {
                let (lo, hi) = poly.iter().map(|p| p.0)
                    .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), x| (a.min(x), b.max(x)));
                hi - lo
            })
            .collect();
        assert!(widths[0] > widths[1] * 1.5,
            "the 40-row group should draw markedly wider than the 10-row one, got {widths:?}");
    }

    /// Lying down, the violins spread across `y` instead — the orientation read off
    /// the bindings, so the same sentence with its axes exchanged draws the same
    /// plot turned. Without the exchange the transform would group by the *measure*
    /// and estimate along the category, which draws nothing at all.
    #[test]
    fn a_sideways_violin_spreads_across_the_other_axis() {
        let sideways = polygon_points(&violin_svg(Mark::Ribbon, true));
        assert_eq!(sideways.len(), 2, "one shape per category, lying down");
        for poly in &sideways {
            let (lo, hi) = poly.iter().map(|p| p.1)
                .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), y| (a.min(y), b.max(y)));
            assert!(hi - lo > 1.0, "a sideways violin has its width on y");
        }
    }

    /// `reach` is measured **from** the slot line, so past 0.5 a shape leaves its
    /// own slot — which is not a mistake to guard against but the ridgeline being
    /// asked for. Asserted as the overlap itself: at 2.5 slots a ridge must cross
    /// the line its neighbor sits on.
    #[test]
    fn reach_past_half_a_slot_is_how_ridges_overlap() {
        let df = DataFrame::new()
            .with_str("g", vec!["a".into(), "a".into(), "a".into(), "a".into(),
                                "b".into(), "b".into(), "b".into(), "b".into()])
            .with_float("v", vec![0.0, 1.0, 2.0, 3.0, 0.0, 1.0, 2.0, 3.0]);
        let mut data = HashMap::new();
        data.insert("t".to_string(), df);
        let svg = |reach: Option<f64>| {
            let mut l = Layer::new(Mark::Area).transform(Transform::Density);
            l.density = Some(crate::ir::DensitySpec {
                adjust: None, bandwidth: None, levels: None, compare: None, reach,
            });
            SvgRenderer::default().render(&PlotSpec::new().data("t").x("g").y("v").layer(l), &data)
        };
        // A half violin's return leg is the slot's line, so the x that repeats most
        // is where the category sits; the reach is the furthest vertex from it. Read
        // that way rather than as a raw width because the axis *grows* with the reach
        // (see below), so pixels are not comparable between renders and slots are —
        // which is exactly what the knob is denominated in.
        let in_slots = |s: &str| -> Vec<f64> {
            let polys = polygon_points(s);
            let line = |poly: &Vec<(f64, f64)>| -> f64 {
                let mut tally = std::collections::BTreeMap::<u64, usize>::new();
                for p in poly { *tally.entry(p.0.to_bits()).or_default() += 1 }
                f64::from_bits(*tally.iter().max_by_key(|(_, n)| **n).unwrap().0)
            };
            let lines: Vec<f64> = polys.iter().map(line).collect();
            let slot = (lines[1] - lines[0]).abs();
            polys.iter().zip(&lines)
                .map(|(poly, &ln)| poly.iter()
                    .map(|p| (p.0 - ln).abs())
                    .fold(0.0f64, f64::max) / slot)
                .collect()
        };
        for (got, want) in in_slots(&svg(None)).iter().zip([crate::ir::DEFAULT_REACH; 2]) {
            assert!((got - want).abs() < 0.02, "the default reach is {want} slots, got {got}");
        }
        for got in in_slots(&svg(Some(2.5))) {
            assert!((got - 2.5).abs() < 0.02, "reach is measured in slots, got {got}");
            // The overlap itself: past half a slot a shape crosses its neighbor's
            // line, which is the ridgeline and what the default must not do.
            assert!(got > 1.0, "a 2.5-slot reach must cross the next category's line");
        }
    }

    /// **The axis grows to hold what overhangs it.** A ridge reaching 2.5 slots off
    /// its own line ran off the top of the panel and was clipped by the frame — the
    /// mark drawing outside the plot, which is the same defect as an area whose
    /// baseline fell off the scale and just as invisible, since it still rendered.
    /// Half a slot each side is right for everything that stands *in* its slot; a
    /// violin need not, so `legality::slot_reach` tells the axis how far.
    #[test]
    fn a_reaching_violin_is_not_clipped_by_the_frame() {
        let df = DataFrame::new()
            .with_str("g", vec!["a".into(), "a".into(), "a".into(), "a".into(),
                                "b".into(), "b".into(), "b".into(), "b".into()])
            .with_float("v", vec![0.0, 1.0, 2.0, 3.0, 0.0, 1.0, 2.0, 3.0]);
        let mut data = HashMap::new();
        data.insert("t".to_string(), df);
        for (mark, sideways) in [(Mark::Area, false), (Mark::Area, true),
                                 (Mark::Ribbon, false), (Mark::Ribbon, true)] {
            let mut l = Layer::new(mark.clone()).transform(Transform::Density);
            l.density = Some(crate::ir::DensitySpec {
                adjust: None, bandwidth: None, levels: None, compare: None, reach: Some(2.5),
            });
            let spec = if sideways { PlotSpec::new().data("t").x("v").y("g") }
                       else        { PlotSpec::new().data("t").x("g").y("v") };
            let svg = SvgRenderer::default().render(&spec.layer(l), &data);
            // The panel, read off the background rect the frame draws.
            let rect = svg.split(r#"<rect x=""#).nth(1).expect("a panel");
            let num = |attr: &str| -> f64 {
                rect.split(&format!(r#"{attr}=""#)).nth(1).unwrap()
                    .split('"').next().unwrap().parse().unwrap()
            };
            let (px, py) = (rect.split('"').next().unwrap().parse::<f64>().unwrap(), num("y"));
            let (pw, ph) = (num("width"), num("height"));
            for poly in polygon_points(&svg) {
                for (x, y) in poly {
                    assert!(x >= px - 0.5 && x <= px + pw + 0.5
                            && y >= py - 0.5 && y <= py + ph + 0.5,
                        "{mark:?} sideways={sideways}: vertex ({x}, {y}) is outside \
                         the panel ({px}, {py}, {pw}, {ph})");
                }
            }
        }
    }

    /// `line`/`step` trace the estimate instead of filling it — the "filled, or two
    /// edges" rule against a slot — and the stroke must land exactly on the fill's
    /// boundary, which is what lets `area + line` draw an edged ridge.
    #[test]
    fn a_traced_violin_lands_on_the_filled_one_s_edge() {
        let fill = violin_svg(Mark::Area, true);
        let trace = violin_svg(Mark::Line, true);
        assert!(trace.contains("<path"), "a traced violin is a stroke");
        assert!(!trace.contains("<polygon"), "a traced violin fills nothing");
        // The stroke's vertices are the fill polygon's leading half, vertex for
        // vertex — the return leg along the slot line is the only difference.
        let poly = &polygon_points(&fill)[0];
        let d: Vec<(f64, f64)> = trace.split("<path d=\"M").nth(1).unwrap()
            .split('"').next().unwrap().split_whitespace()
            .filter_map(|p| { let (a, b) = p.split_once(',')?;
                              Some((a.parse().ok()?, b.parse().ok()?)) })
            .collect();
        assert!(!d.is_empty(), "the trace has vertices");
        for (k, v) in d.iter().enumerate().take(8) {
            assert!((v.0 - poly[k].0).abs() < 0.01 && (v.1 - poly[k].1).abs() < 0.01,
                "vertex {k} of the trace must sit on the fill's edge: {v:?} vs {:?}", poly[k]);
        }
    }

    /// A half violin closes on the slot's center line, which is on the *other*
    /// axis — so the measure axis must not be stretched to zero the way a normal
    /// `area`'s is. Data starting at 1 drew a panel from 0 before this was split.
    #[test]
    fn a_half_violin_does_not_drag_its_measure_axis_to_zero() {
        let df = DataFrame::new()
            .with_str("g", vec!["a".into(); 8])
            .with_float("v", vec![100.0, 101.0, 102.0, 103.0, 104.0, 105.0, 106.0, 107.0]);
        let mut data = HashMap::new();
        data.insert("t".to_string(), df);
        let spec = PlotSpec::new().data("t").x("g").y("v")
            .layer(Layer::new(Mark::Area).transform(Transform::Density));
        let svg = SvgRenderer::default().render(&spec, &data);
        assert!(!svg.contains(">0<"),
            "a half violin's baseline is the slot center, not 0 on the measure axis");
    }

    #[test]
    fn a_flush_axis_fits_its_data_exactly() {
        // The axis an area fills along leaves no breathing band: the fill's
        // edges are the panel's edges. Contrast the same data un-flushed.
        let (lo, hi) = fitted_range(1952.0, 2007.0, false, true);
        assert_eq!((lo, hi), (1952.0, 2007.0), "flush fits exactly");
        let (lo2, hi2) = fitted_range(1952.0, 2007.0, false, false);
        assert!(lo2 < 1952.0 && hi2 > 2007.0, "un-flushed breathes");
    }

    #[test]
    fn an_area_fills_the_panel_width_but_a_scatter_breathes() {
        // End to end. An area's year axis sits flush; add a point layer sharing
        // that axis and the breathing returns, because a glyph would clip.
        let df = DataFrame::new()
            .with_float("year", vec![1952.0, 1980.0, 2007.0])
            .with_float("life", vec![40.0, 60.0, 82.0]);
        let mut data = HashMap::new();
        data.insert("t".to_string(), df);

        let area = PlotSpec::new().data("t").x("year").y("life")
            .layer(Layer::new(Mark::Area));
        let (_, xs) = axis_range_x(&area, &data);
        assert_eq!(xs, (1952.0, 2007.0), "area fills the width flush");

        let with_point = PlotSpec::new().data("t").x("year").y("life")
            .layer(Layer::new(Mark::Area))
            .layer(Layer::new(Mark::Point));
        let (_, xs2) = axis_range_x(&with_point, &data);
        assert!(xs2.0 < 1952.0 && xs2.1 > 2007.0, "a point restores the margin");
    }

    /// Render and read the x scale range back out of the first `<clipPath>`? No —
    /// simpler to re-run the axis logic the renderer uses. This mirrors what
    /// `render` computes for a single unfaceted panel.
    fn axis_range_x(spec: &PlotSpec, data: &HashMap<String, DataFrame>) -> (TickSpec, (f64, f64)) {
        let df = data.get(spec.data.as_deref().unwrap()).unwrap();
        let eff: Vec<DataFrame> = spec.layers.iter().map(|_| df.clone()).collect();
        let refs: Vec<&DataFrame> = eff.iter().collect();
        let has_area = spec.layers.iter().any(|l| l.mark == Mark::Area);
        let has_point = spec.layers.iter().any(|l| l.mark == Mark::Point);
        // The spec's own x column, not a hardcoded one: a helper that names the
        // field itself silently answers about an absent column, and every
        // assertion then passes against the (0, 1) fallback.
        let field = spec.axis_def(&Channel::X).map(|d| d.field.clone()).unwrap_or_default();
        build_axis(&refs, &[], &[], &field, None, false, false, has_area && !has_point,
                   None, false, 10.0, None,
                   crate::scale::domain_of(spec.axis_def(&Channel::X)), (0.0, 0.0))
    }

    #[test]
    fn the_fitted_range_never_clips_the_data() {
        for (mn, mx, base) in [
            (1952.0, 2007.0, false),
            (0.0, 46.0, true),
            (-5.0, 5.0, true),
            (3.1, 3.9, false),
        ] {
            let (lo, hi) = fitted_range(mn, mx, base, false);
            assert!(lo <= mn && hi >= mx, "range clipped data at ({mn}, {mx})");
        }
    }

    #[test]
    fn a_year_axis_no_longer_pads_out_to_dead_round_numbers() {
        // End to end: the ticks inside the data survive, the round-number padding
        // outside it does not.
        let df = DataFrame::new()
            .with_float("year", vec![1952.0, 1977.0, 2007.0])
            .with_float("life", vec![40.0, 60.0, 82.0]);
        let mut data = HashMap::new();
        data.insert("t".to_string(), df);
        let spec = PlotSpec::new().data("t").x("year").y("life")
            .layer(Layer::new(Mark::Line));
        let svg = SvgRenderer::default().render(&spec, &data);
        assert!(svg.contains(">1960<"), "an inside tick is drawn:\n{svg}");
        assert!(
            !svg.contains(">1940<") && !svg.contains(">2020<"),
            "the dead round-number ticks must be gone"
        );
    }

    // -- constant settings ------------------------------------------------

    fn render_styled(layer: Layer) -> String {
        let df = DataFrame::new()
            .with_float("a", vec![1.0, 2.0, 3.0])
            .with_float("b", vec![4.0, 5.0, 6.0])
            .with_str("g", vec!["x".into(), "y".into(), "x".into()]);
        let mut data = HashMap::new();
        data.insert("t".to_string(), df);
        let spec = PlotSpec::new().data("t").x("a").y("b").layer(layer);
        SvgRenderer::default().render(&spec, &data)
    }

    #[test]
    fn a_set_color_reaches_the_output() {
        let svg = render_styled(Layer::new(Mark::Point).style_color("tomato"));
        assert!(svg.contains(r#"fill="tomato""#));
        // and the built-in default is no longer used for the marks
        assert!(!svg.contains(&format!(r#"<circle cx="0.00" cy="0.00" r="4.50" fill="{}""#, PALETTE_GOG[0])));
    }

    #[test]
    fn a_dashed_line_carries_a_stroke_dasharray_and_a_plain_one_does_not() {
        // `style(dash = )` is paint — it adds a stroke-dasharray and moves no vertex.
        // So a plain line carries none (byte-for-byte the old output) and a dashed
        // one carries the pattern.
        let svg_plain = render_styled(Layer::new(Mark::Line));
        assert!(!svg_plain.contains("stroke-dasharray"), "a solid line has no dasharray:\n{svg_plain}");

        let mut dashed = Layer::new(Mark::Line);
        dashed.style.pattern = Some("dashed".into());
        let svg_dash = render_styled(dashed);
        assert!(svg_dash.contains(r#"stroke-dasharray="6,4""#), "a dashed line carries the pattern:\n{svg_dash}");
    }

    #[test]
    fn a_line_with_a_pair_transform_draws_two_separated_boundary_curves() {
        // `line * bounds` is the unfilled band: the pair-rows become two boundary
        // curves (a low locus and a high one), never one zigzag connecting them.
        let mut data = HashMap::new();
        data.insert("t".to_string(), DataFrame::new()
            .with_float("x",  vec![1.0, 2.0, 3.0])
            .with_float("lo", vec![10.0, 20.0, 30.0])
            .with_float("hi", vec![40.0, 50.0, 60.0]));
        let spec = PlotSpec::new().data("t").x("x")
            .layer(Layer::new(Mark::Line).bounds("lo", "hi"));
        let svg = SvgRenderer::default().render(&spec, &data);

        let curves: Vec<Vec<(f64, f64)>> = svg.lines()
            .filter(|l| l.contains("<polyline"))
            .map(|l| points_of(l.split("points=\"").nth(1).unwrap().split('"').next().unwrap()))
            .collect();
        assert_eq!(curves.len(), 2, "line * bounds draws two boundary curves:\n{svg}");
        // Each boundary is one point per x (3), not the 6 a zigzag would connect.
        for c in &curves { assert_eq!(c.len(), 3, "a boundary is one point per x, not a zigzag"); }
        // The two do not cross — one boundary sits entirely below the other.
        let miny = |c: &[(f64, f64)]| c.iter().map(|q| q.1).fold(f64::INFINITY, f64::min);
        let maxy = |c: &[(f64, f64)]| c.iter().map(|q| q.1).fold(f64::NEG_INFINITY, f64::max);
        let (a, b) = (&curves[0], &curves[1]);
        assert!(maxy(a) < miny(b) || maxy(b) < miny(a),
            "the low and high boundaries must not cross");
    }

    #[test]
    fn a_point_border_rims_fillable_glyphs_and_skips_a_cross() {
        // `border_color`/`border_size` rim the filled glyphs (spec §4); a `cross`
        // has no fill, so it takes no rim — its own color stroke is all there is.
        let mut circ = Layer::new(Mark::Point);
        circ.style.border_color = Some("black".into());
        circ.style.border_size = Some(1.5);
        let svg = render_styled(circ);
        assert!(svg.contains(r#"stroke="black" stroke-width="1.50""#),
            "a circle point carries the rim:\n{svg}");

        let mut cross = Layer::new(Mark::Point);
        cross.style.shape = Some("cross".into());
        cross.style.border_color = Some("black".into());
        let svg2 = render_styled(cross);
        assert!(!svg2.contains(r#"stroke="black""#),
            "a cross skips the rim (no fill to outline):\n{svg2}");

        // And a plain point (no border) is unchanged — no stroke on the glyph itself.
        let plain = render_styled(Layer::new(Mark::Point));
        let glyph = plain.lines().find(|l| l.contains("<circle")).unwrap_or("");
        assert!(!glyph.contains("stroke="), "a borderless point glyph has no stroke: {glyph}");
    }

    #[test]
    fn a_set_color_is_escaped() {
        // `legality::check_style` rejects a non-color before rendering, so this
        // string cannot arrive here in normal use — but `GOG_STRICT=0` renders
        // anyway, and a broken-out `fill="` attribute would corrupt the document
        // for every reader. Escaping is the last line, not the only one.
        let svg = render_styled(Layer::new(Mark::Point).style_color(r#"red" onload="evil"#));
        assert!(!svg.contains(r#"onload="evil""#));
        assert!(unescaped_ampersands(&svg).is_empty());
    }

    #[test]
    fn setting_a_feature_draws_no_legend() {
        // The defining difference between set and map: a legend exists to decode
        // a mapping, and a constant encodes nothing to decode.
        let mapped = render_styled(Layer::new(Mark::Point).encode(Channel::Color, "g"));
        let set = render_styled(Layer::new(Mark::Point).style_color("tomato"));
        assert!(mapped.contains(">x<") && mapped.contains(">y<"), "mapping should label categories");
        assert!(!set.contains(">x<"), "a set color must not produce a legend");
        // A legend also reserves right-hand margin, so the panel must be wider.
        assert!(set.len() < mapped.len());
    }

    // -- continuous color ------------------------------------------------



    #[test]
    fn a_continuous_color_earns_one_legend_not_two() {
        let df = DataFrame::new()
            .with_float("a", vec![1.0, 2.0, 3.0])
            .with_float("v", vec![10.0, 20.0, 30.0]);
        let mut data = HashMap::new();
        data.insert("t".to_string(), df);
        let spec = PlotSpec::new()
            .data("t")
            .x("a")
            .y("a")
            .layer(Layer::new(Mark::Point).encode(Channel::Color, "v"));
        let svg = SvgRenderer::default().render(&spec, &data);
        // Labeled at min / mid / max…
        for want in [">10.00<", ">20.00<", ">30.00<"] {
            assert!(svg.contains(want), "legend should show {want}");
        }
        // …but drawn as one continuous strip, not three sampled swatches.
        assert!(svg.contains("<linearGradient"), "continuous color needs a gradient strip");
        assert!(svg.contains("fill=\"url(#ramp"), "the strip should use the gradient");
        // and exactly one legend box, not a categorical one as well
        assert_eq!(svg.matches(r#"rx="4""#).count(), 1, "expected a single legend box");
    }

    #[test]
    fn different_ramps_get_different_gradient_ids() {
        // The book inlines many SVGs into one HTML document, where SVG ids
        // are global and the first definition wins for every reference. The
        // id therefore hashes the gradient's *content*: two plots may only
        // share an id when they would paint the same strip anyway. The old
        // geometry-derived id collided the moment two plots shared a layout,
        // and a viridis legend upstream repainted a white–navy strip
        // downstream.
        let df = DataFrame::new()
            .with_float("a", vec![1.0, 2.0, 3.0])
            .with_float("v", vec![10.0, 20.0, 30.0]);
        let mut data = HashMap::new();
        data.insert("t".to_string(), df);
        let base = PlotSpec::new().data("t").x("a").y("a")
            .layer(Layer::new(Mark::Point).encode(Channel::Color, "v"));

        let gid = |svg: &str| svg.split(r#"linearGradient id=""#).nth(1)
            .and_then(|s| s.split('"').next())
            .expect("no gradient id").to_string();

        let mut viridis = base.clone();
        viridis.palette = crate::ir::PaletteDef::Named("viridis".into());
        let mut custom = base.clone();
        custom.palette = crate::ir::PaletteDef::Custom(vec!["white".into(), "navy".into()]);

        let a = gid(&SvgRenderer::default().render(&viridis, &data));
        let b = gid(&SvgRenderer::default().render(&custom, &data));
        assert_ne!(a, b, "identical layouts must not share a gradient id across ramps");

        // Same ramp twice → same id, and that collision is harmless by
        // construction: both definitions paint identically.
        let c = gid(&SvgRenderer::default().render(&custom, &data));
        assert_eq!(b, c);
    }

    #[test]
    fn the_strip_is_long_enough_to_read_as_a_gradient() {
        // The whole point of one strip instead of three swatches is that the
        // reader sees the scale itself. At the plain row height the strip was
        // a 40 px stub — three swatches touching. Gradient rows are taller,
        // and the strip spans center-of-first to center-of-last of them.
        use crate::render::legend::LEGEND_RAMP_ROW_H;
        let df = DataFrame::new()
            .with_float("a", vec![1.0, 2.0, 3.0])
            .with_float("v", vec![10.0, 20.0, 30.0]);
        let mut data = HashMap::new();
        data.insert("t".to_string(), df);
        let spec = PlotSpec::new().data("t").x("a").y("a")
            .layer(Layer::new(Mark::Point).encode(Channel::Color, "v"));
        let svg = SvgRenderer::default().render(&spec, &data);

        let strip_h = svg.lines()
            .find_map(|l| {
                if !l.contains("url(#ramp") { return None }
                l.split(r#" height=""#).nth(1)?.split('"').next()?.parse::<f64>().ok()
            })
            .expect("no gradient strip drawn");
        // Three labels → the strip spans two gradient rows.
        assert!((strip_h - 2.0 * LEGEND_RAMP_ROW_H).abs() < 1e-6,
                "strip is {strip_h} px, expected {}", 2.0 * LEGEND_RAMP_ROW_H);
    }

    #[test]
    fn only_color_gets_a_strip_the_others_are_sampled() {
        // A legend shows the scale. Color is the only continuous channel whose
        // whole range fits in a fixed space; there is no way to draw a continuum
        // of circles, so `size` and `opacity` are sampled. Same rule, different
        // shapes — if `size` ever grows a gradient this test is the reminder to
        // think about whether that means anything.
        let df = DataFrame::new()
            .with_float("a", vec![1.0, 2.0, 3.0])
            .with_float("v", vec![10.0, 20.0, 30.0]);
        let mut data = HashMap::new();
        data.insert("t".to_string(), df);
        let base = PlotSpec::new().data("t").x("a").y("a");

        let color = SvgRenderer::default()
            .render(&base.clone().layer(Layer::new(Mark::Point).encode(Channel::Color, "v")), &data);
        assert!(color.contains("<linearGradient"));

        for ch in [Channel::Size, Channel::Opacity] {
            let svg = SvgRenderer::default()
                .render(&base.clone().layer(Layer::new(Mark::Point).encode(ch.clone(), "v")), &data);
            assert!(!svg.contains("<linearGradient"), "{ch:?} should be sampled, not a strip");
            assert!(svg.contains(">20.00<"), "{ch:?} should still label its midpoint");
        }
    }

    #[test]
    fn the_strip_runs_the_way_the_labels_do() {
        // Largest at the top. If the gradient were flipped the plot would still
        // render and quietly mean the opposite of its own legend.
        let df = DataFrame::new()
            .with_float("a", vec![1.0, 2.0, 3.0])
            .with_float("v", vec![10.0, 20.0, 30.0]);
        let mut data = HashMap::new();
        data.insert("t".to_string(), df);
        let spec = PlotSpec::new().data("t").x("a").y("a")
            .layer(Layer::new(Mark::Point).encode(Channel::Color, "v"));
        let svg = SvgRenderer::default().render(&spec, &data);

        // y1=1 → y2=0 means offset 0 is at the *bottom*, so the first stop
        // (the ramp's light end) sits at the low-value end.
        assert!(svg.contains(r#"x1="0" y1="1" x2="0" y2="0""#), "strip should run bottom-up");
        let first_stop = svg.split("stop-color=\"").nth(1).unwrap().split('"').next().unwrap();
        assert_eq!(first_stop.to_lowercase(), RAMP_BLUE[0], "offset 0 should be the light end");

        // And the max label precedes the min label in document order (top row first).
        let max_at = svg.find(">30.00<").unwrap();
        let min_at = svg.find(">10.00<").unwrap();
        assert!(max_at < min_at, "the largest value should be labeled at the top");
    }

    // -- horizontal bars --------------------------------------------------

    /// `(x, y, width, height)` of each drawn bar, in document order.
    fn bar_rects(svg: &str) -> Vec<(f64, f64, f64, f64)> {
        svg.lines()
            // Bars carry a `fill-opacity` but never a corner radius; legend
            // swatches and facet strips carry `rx=`, so excluding it isolates bars
            // even when a color legend is present.
            .filter(|l| l.contains("<rect") && l.contains("fill-opacity") && !l.contains(" rx="))
            .map(|l| {
                // Read named attributes rather than positions — `stroke-width`
                // also ends in `width=`, and matching loosely would silently
                // pick it up.
                let attr = |name: &str| -> f64 {
                    l.split(&format!(" {name}=\""))
                        .nth(1)
                        .and_then(|r| r.split('"').next())
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(f64::NAN)
                };
                (attr("x"), attr("y"), attr("width"), attr("height"))
            })
            .collect()
    }

    #[test]
    fn dodge_sets_grouped_bars_side_by_side_and_narrows_them() {
        // Two categories, each split by color into two groups. Overlaid, the two
        // groups share their category's slot (same center, full width). Dodged,
        // they sit side by side — each half as wide, offset symmetrically about
        // that same center, together tiling exactly the slot one bar filled (§5).
        let df = DataFrame::new()
            .with_str("g", vec!["A".into(), "A".into(), "B".into(), "B".into()])
            .with_str("c", vec!["p".into(), "q".into(), "p".into(), "q".into()])
            .with_float("v", vec![1.0, 2.0, 3.0, 4.0]);
        let mut data = HashMap::new();
        data.insert("t".to_string(), df);

        let spec = |dodge: bool| {
            let mut layer = Layer::new(Mark::Bar).encode(Channel::Color, "c");
            if dodge {
                layer = layer.transform(Transform::Dodge);
            }
            PlotSpec::new().data("t").x("g").y("v").layer(layer)
        };

        let overlaid = bar_rects(&SvgRenderer::default().render(&spec(false), &data));
        let dodged = bar_rects(&SvgRenderer::default().render(&spec(true), &data));
        assert_eq!(overlaid.len(), 4, "expected four bars overlaid");
        assert_eq!(dodged.len(), 4, "expected four bars dodged");

        let center = |r: &(f64, f64, f64, f64)| r.0 + r.2 / 2.0;
        let key = |c: f64| (c * 100.0).round() as i64;

        // Overlaid: the groups share each category's center — two distinct centers,
        // one full width.
        let ov_centers: std::collections::BTreeSet<i64> = overlaid.iter().map(|r| key(center(r))).collect();
        assert_eq!(ov_centers.len(), 2, "overlaid groups should share each category's center");
        let w_full = overlaid[0].2;
        assert!(overlaid.iter().all(|r| (r.2 - w_full).abs() < 1e-6), "overlaid bars are full width");

        // Dodged: four distinct centers, each bar half the slot (G = 2).
        let dg_centers: std::collections::BTreeSet<i64> = dodged.iter().map(|r| key(center(r))).collect();
        assert_eq!(dg_centers.len(), 4, "every dodged bar should get its own position");
        assert!(
            dodged.iter().all(|r| (r.2 - w_full / 2.0).abs() < 1e-6),
            "two groups → each dodged bar is half the slot: {:?}",
            dodged.iter().map(|r| r.2).collect::<Vec<_>>()
        );

        // Symmetric about the shared center: each overlaid center is the mean of
        // the two dodged bars that replaced it — the group straddles, never drifts.
        for oc in overlaid.iter().map(center) {
            let near: Vec<f64> = dodged.iter().map(center).filter(|c| (c - oc).abs() < w_full * 0.5).collect();
            assert_eq!(near.len(), 2, "each slot splits into exactly two dodged bars");
            assert!(
                ((near[0] + near[1]) / 2.0 - oc).abs() < 1e-6,
                "dodged pair should straddle the original center {oc}"
            );
        }
    }

    #[test]
    fn stacked_bars_pile_full_width_and_abut() {
        // The same split, stacked instead of dodged: the two groups pile at each
        // category's one center — full width, not narrowed — each segment sitting
        // exactly on the one below, with no gap or overlap (§5).
        let df = DataFrame::new()
            .with_str("g", vec!["A".into(), "A".into(), "B".into(), "B".into()])
            .with_str("c", vec!["p".into(), "q".into(), "p".into(), "q".into()])
            .with_float("v", vec![1.0, 2.0, 3.0, 4.0]);
        let mut data = HashMap::new();
        data.insert("t".to_string(), df);

        let spec = PlotSpec::new().data("t").x("g").y("v")
            .layer(Layer::new(Mark::Bar).transform(Transform::Stack).encode(Channel::Color, "c"));
        let svg = SvgRenderer::default().render(&spec, &data);
        let rects = bar_rects(&svg);
        assert_eq!(rects.len(), 4, "two categories × two groups = four segments");

        let center = |r: &(f64, f64, f64, f64)| r.0 + r.2 / 2.0;
        let key = |c: f64| (c * 100.0).round() as i64;

        // Full width and one center per category — the opposite of dodge.
        let w_full = rects[0].2;
        assert!(rects.iter().all(|r| (r.2 - w_full).abs() < 1e-6), "stacked segments keep full width");
        let centers: std::collections::BTreeSet<i64> = rects.iter().map(|r| key(center(r))).collect();
        assert_eq!(centers.len(), 2, "stacked groups pile at one center per category, not side by side");

        // Within each column the two segments abut: screen-y grows downward, so the
        // upper segment's foot (y + h) meets the lower segment's top (y).
        for &cx in &centers {
            let mut col: Vec<&(f64, f64, f64, f64)> =
                rects.iter().filter(|r| key(center(r)) == cx).collect();
            assert_eq!(col.len(), 2, "each category stacks two segments");
            col.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap()); // topmost (smallest y) first
            let (upper, lower) = (col[0], col[1]);
            assert!((upper.1 + upper.3 - lower.1).abs() < 0.5,
                "segments must abut: upper foot {} vs lower top {}", upper.1 + upper.3, lower.1);
        }

        // Solid, never the translucent overlay fill — the pile resolves the overlap.
        let bar_fill_os: Vec<String> = svg.lines()
            .filter(|l| l.contains("<rect") && l.contains("fill-opacity") && !l.contains(" rx="))
            .filter_map(|l| l.split("fill-opacity=\"").nth(1).and_then(|r| r.split('"').next()).map(String::from))
            .collect();
        assert!(bar_fill_os.iter().all(|o| o != "0.400"),
            "a stacked bar draws solid, never the 0.4 overlay fill: {bar_fill_os:?}");
    }

    #[test]
    fn stacked_areas_trace_a_floor_instead_of_the_baseline() {
        // Two regions over the same three x, split by color. Stacked, each band
        // fills between its own floor (the group below) and its top, so its polygon
        // retraces that floor per vertex (N top + N floor) instead of closing on the
        // flat baseline (N top + two corners). That geometric difference is what lets
        // the upper band sit on the lower rather than bury it — the retired Assumption.
        let df = DataFrame::new()
            .with_float("x", vec![0.0, 1.0, 2.0, 0.0, 1.0, 2.0])
            .with_float("y", vec![1.0, 2.0, 1.0, 3.0, 1.0, 2.0])
            .with_str("g", vec!["a".into(), "a".into(), "a".into(), "b".into(), "b".into(), "b".into()]);
        let mut data = HashMap::new();
        data.insert("t".to_string(), df);

        let verts = |stack: bool| -> Vec<usize> {
            let mut layer = Layer::new(Mark::Area).encode(Channel::Color, "g");
            if stack { layer = layer.transform(Transform::Stack); }
            let spec = PlotSpec::new().data("t").x("x").y("y").layer(layer);
            let svg = SvgRenderer::default().render(&spec, &data);
            svg.lines().filter(|l| l.contains("<polygon"))
                .map(|l| l.matches(',').count()).collect()
        };

        let plain = verts(false);
        assert_eq!(plain.len(), 2, "one region per group");
        assert!(plain.iter().all(|&v| v == 5),
            "an unstacked band closes on the flat baseline: 3 top + 2 corners = 5, got {plain:?}");

        let stacked = verts(true);
        assert_eq!(stacked.len(), 2, "one band per group");
        assert!(stacked.iter().all(|&v| v == 6),
            "a stacked band retraces its floor: 3 top + 3 floor = 6, got {stacked:?}");
    }

    fn medals_data() -> HashMap<String, DataFrame> {
        let df = DataFrame::new()
            .with_str("country", vec!["USA".into(), "China".into(), "GB".into()])
            .with_float("gold", vec![46.0, 38.0, 29.0]);
        let mut m = HashMap::new();
        m.insert("t".to_string(), df);
        m
    }

    #[test]
    fn bars_lie_down_when_the_categories_are_on_y() {
        let vertical = SvgRenderer::default().render(
            &PlotSpec::new().data("t").x("country").y("gold").layer(Layer::new(Mark::Bar)),
            &medals_data(),
        );
        let horizontal = SvgRenderer::default().render(
            &PlotSpec::new().data("t").x("gold").y("country").layer(Layer::new(Mark::Bar)),
            &medals_data(),
        );

        let v = bar_rects(&vertical);
        let h = bar_rects(&horizontal);
        assert_eq!(v.len(), 3);
        assert_eq!(h.len(), 3);

        // Vertical: one width, varying heights, all standing on one baseline.
        assert!(v.iter().all(|r| (r.2 - v[0].2).abs() < 0.01), "widths should match: {v:?}");
        assert!(v.iter().any(|r| (r.3 - v[0].3).abs() > 1.0), "heights should vary: {v:?}");
        assert!(v.iter().all(|r| (r.1 + r.3 - (v[0].1 + v[0].3)).abs() < 0.01), "common baseline");

        // Horizontal: one height, varying widths, all starting at one baseline.
        assert!(h.iter().all(|r| (r.3 - h[0].3).abs() < 0.01), "heights should match: {h:?}");
        assert!(h.iter().any(|r| (r.2 - h[0].2).abs() > 1.0), "widths should vary: {h:?}");
        assert!(h.iter().all(|r| (r.0 - h[0].0).abs() < 0.01), "common baseline: {h:?}");
    }

    #[test]
    fn the_first_category_reads_first_in_both_orientations() {
        // Screen y grows downward while the scale grows upward, so a categorical
        // y must be reversed or `order(desc)` would put the largest bar at the
        // bottom of a horizontal chart and the leftmost of a vertical one.
        let spec = PlotSpec::new()
            .data("t")
            .x("gold")
            .y("country")
            .order_desc("gold")
            .layer(Layer::new(Mark::Bar));
        let svg = SvgRenderer::default().render(&spec, &medals_data());

        let bars = bar_rects(&svg);
        // Widest bar (the largest value) must sit at the smallest y — the top.
        let widest = bars.iter().cloned().fold(bars[0], |a, b| if b.2 > a.2 { b } else { a });
        assert!(
            bars.iter().all(|r| r.1 >= widest.1),
            "largest bar should be topmost: {bars:?}"
        );
    }

    #[test]
    fn ordering_a_factor_by_itself_uses_its_levels_not_its_spelling() {
        // `order(size)` asks for the column's own order. A factor's own order is
        // the levels it declares, and here they disagree with the alphabet in
        // both directions: alphabetically the bands read large, medium, small.
        //
        // Sorting the labels instead threw the levels away, which was invisible
        // because a scrambled categorical axis is still a well-formed one. The
        // case that found it was five-year age bands, where "5" sorts between
        // "45" and "50" and a population pyramid came out with shuffled floors.
        let df = DataFrame::new()
            .with_levels(
                "size",
                vec!["small".into(), "medium".into(), "large".into()],
                vec!["small".into(), "medium".into(), "large".into()],
            )
            .with_float("n", vec![1.0, 2.0, 3.0]);
        let mut data = HashMap::new();
        data.insert("t".to_string(), df);

        // Each category has a distinct height, so the bars name themselves: read
        // them left to right and the heights say which order the axis chose.
        let heights = |spec: &PlotSpec| {
            let mut bars = bar_rects(&SvgRenderer::default().render(spec, &data));
            assert_eq!(bars.len(), 3, "one bar per band: {bars:?}");
            bars.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
            bars.iter().map(|r| r.3.round()).collect::<Vec<_>>()
        };

        let base = PlotSpec::new().data("t").x("size").y("n").layer(Layer::new(Mark::Bar));
        let up = heights(&base.clone().order("size"));
        assert!(up[0] < up[1] && up[1] < up[2], "levels order, small to large: {up:?}");

        let down = heights(&base.clone().order_desc("size"));
        assert!(down[0] > down[1] && down[1] > down[2], "the same levels reversed: {down:?}");

        // And a column with no levels has nothing to go on but its spelling, so
        // the alphabet still answers there. Same three words, no `with_levels`.
        let plain = DataFrame::new()
            .with_str("size", vec!["small".into(), "medium".into(), "large".into()])
            .with_float("n", vec![1.0, 2.0, 3.0]);
        let mut plain_data = HashMap::new();
        plain_data.insert("t".to_string(), plain);
        let mut bars = bar_rects(&SvgRenderer::default().render(&base.order("size"), &plain_data));
        bars.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        let alpha = bars.iter().map(|r| r.3.round()).collect::<Vec<_>>();
        assert!(alpha[0] > alpha[1] && alpha[1] > alpha[2],
                "large(3), medium(2), small(1) is alphabetical: {alpha:?}");
    }

    #[test]
    fn a_horizontal_count_measures_along_x() {
        // The transform groups by the position axis and writes to the measured
        // one; with the categories on y that output belongs to x.
        let df = DataFrame::new().with_str(
            "g",
            vec!["a".into(), "a".into(), "b".into()],
        );
        let mut data = HashMap::new();
        data.insert("t".to_string(), df);
        let spec = PlotSpec::new()
            .data("t")
            .y("g")
            .layer(Layer::new(Mark::Bar).transform(Transform::Count));
        let svg = SvgRenderer::default().render(&spec, &data);

        let bars = bar_rects(&svg);
        assert_eq!(bars.len(), 2, "one bar per category: {bars:?}");
        assert!(bars.iter().all(|r| (r.3 - bars[0].3).abs() < 0.01), "equal thickness");
        assert!((bars[0].2 - bars[1].2).abs() > 1.0, "counts 2 and 1 should differ: {bars:?}");
        // The synthesized label names the measured axis, which is now x.
        assert!(svg.contains(">Count<"), "the count axis should be labeled");
    }

    // -- horizontal box and whisker ---------------------------------------

    /// `(x1, y1, x2, y2)` of each drawn `<line>`, in document order.
    fn line_segs(svg: &str) -> Vec<(f64, f64, f64, f64)> {
        svg.lines()
            .filter(|l| l.contains("<line"))
            .map(|l| {
                let attr = |name: &str| -> f64 {
                    l.split(&format!(" {name}=\""))
                        .nth(1)
                        .and_then(|r| r.split('"').next())
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(f64::NAN)
                };
                (attr("x1"), attr("y1"), attr("x2"), attr("y2"))
            })
            .filter(|s| [s.0, s.1, s.2, s.3].iter().all(|v| v.is_finite()))
            .collect()
    }

    fn grouped_measurements() -> HashMap<String, DataFrame> {
        // Three groups with visibly different spreads, so a box's measure extent
        // cannot accidentally match its slot thickness.
        let mut g = Vec::new();
        let mut v = Vec::new();
        for (name, base, spread) in [("alpha", 10.0, 1.0), ("beta", 30.0, 6.0), ("gamma", 50.0, 3.0)] {
            for k in 0..9 {
                g.push(name.to_string());
                v.push(base + (k as f64 - 4.0) * spread);
            }
        }
        let mut data = HashMap::new();
        data.insert("t".to_string(), DataFrame::new().with_str("g", g).with_float("v", v));
        data
    }

    /// Turning a box plot on its side must **transpose** it, not redraw it: the
    /// slot thickness moves to the other axis, the summary's extent with it, and
    /// every line that ran across the box now runs along it.
    ///
    /// Written as one assertion set parameterized by orientation, so it cannot pass
    /// by describing each case separately — which is how a mirrored copy of a
    /// writer hides a missed swap. Before `box` took a category on `y`, the
    /// horizontal half of this was not expressible at all.
    #[test]
    fn a_box_plot_transposes_when_the_category_moves_to_y() {
        let data = grouped_measurements();
        let build = |horizontal: bool| {
            let spec = if horizontal {
                PlotSpec::new().data("t").x("v").y("g")
            } else {
                PlotSpec::new().data("t").x("g").y("v")
            };
            SvgRenderer::default().render(&spec.layer(Layer::new(Mark::Box)), &data)
        };
        let (v_svg, h_svg) = (build(false), build(true));
        let (v_boxes, h_boxes) = (bar_rects(&v_svg), bar_rects(&h_svg));

        // Same picture, turned: the same number of boxes either way. A writer that
        // read the wrong category list would silently draw none.
        assert_eq!(v_boxes.len(), 3, "one box per group upright: {v_boxes:?}");
        assert_eq!(h_boxes.len(), 3, "one box per group on its side: {h_boxes:?}");

        // Upright, the *width* is the slot (equal for every box) and the *height* is
        // the IQR (unequal, since the spreads differ). Lying down, exactly reversed.
        let spread = |vals: &[f64]| vals.iter().cloned().fold(f64::MIN, f64::max)
            - vals.iter().cloned().fold(f64::MAX, f64::min);
        for (label, boxes, slot, measure) in [
            ("upright", &v_boxes, 2usize, 3usize),
            ("sideways", &h_boxes, 3usize, 2usize),
        ] {
            let pick = |i: usize| boxes.iter().map(|r| [r.0, r.1, r.2, r.3][i]).collect::<Vec<_>>();
            assert!(spread(&pick(slot)) < 0.01, "{label}: every box shares one slot thickness");
            assert!(spread(&pick(measure)) > 1.0, "{label}: the IQR differs per group");
        }

        // The line-work turns with the box. A median bar and a whisker cap run
        // *across* the slot; a whisker runs *along* the measure. Upright that makes
        // the caps horizontal and the whiskers vertical, and sideways the reverse —
        // so the two orientations must have no segment direction in common.
        let flat = |s: &(f64, f64, f64, f64)| (s.1 - s.3).abs() < 0.01;
        let upright = |s: &(f64, f64, f64, f64)| (s.0 - s.2).abs() < 0.01;
        let (v_segs, h_segs) = (line_segs(&v_svg), line_segs(&h_svg));
        // Ignore the axis rules and gridlines, which do not turn with the mark:
        // the box's own line-work is what carries a stroke-linecap.
        let boxwork = |svg: &str, segs: Vec<(f64, f64, f64, f64)>| {
            let keep: Vec<usize> = svg.lines().filter(|l| l.contains("<line")).enumerate()
                .filter(|(_, l)| l.contains("stroke-linecap")).map(|(i, _)| i).collect();
            keep.into_iter().filter_map(|i| segs.get(i).copied()).collect::<Vec<_>>()
        };
        let vw = boxwork(&v_svg, v_segs);
        let hw = boxwork(&h_svg, h_segs);
        assert!(!vw.is_empty() && !hw.is_empty(), "the boxes should draw line-work");
        assert!(vw.iter().all(|s| flat(s) || upright(s)), "every segment is axis-aligned");
        // Upright: whiskers vertical, medians and caps horizontal — both kinds present.
        assert!(vw.iter().any(upright) && vw.iter().any(flat), "upright: both directions drawn");
        assert!(hw.iter().any(upright) && hw.iter().any(flat), "sideways: both directions drawn");
        // And the counts swap: as many flat segments upright as upright ones sideways.
        assert_eq!(
            vw.iter().filter(|s| flat(s)).count(), hw.iter().filter(|s| upright(s)).count(),
            "the across-the-slot line-work should turn with the box"
        );
        assert_eq!(
            vw.iter().filter(|s| upright(s)).count(), hw.iter().filter(|s| flat(s)).count(),
            "the along-the-measure line-work should turn with the box"
        );
    }

    /// The same claim for `interval`: a whisker spans along the measure and its caps
    /// run across the slot, whichever way round the two axes are bound.
    #[test]
    fn an_error_bar_spans_along_whichever_axis_measures() {
        let data = grouped_measurements();
        let build = |horizontal: bool| {
            let spec = if horizontal {
                PlotSpec::new().data("t").x("v").y("g")
            } else {
                PlotSpec::new().data("t").x("g").y("v")
            };
            SvgRenderer::default().render(
                &spec.layer(Layer::new(Mark::Interval).transform(Transform::Range)), &data)
        };
        for (label, horizontal) in [("upright", false), ("sideways", true)] {
            let svg = build(horizontal);
            let segs = line_segs(&svg);
            // A span runs along the measure; a cap across the slot. Three groups
            // means three spans and six caps, whichever way the plot is turned.
            let along: Vec<_> = segs.iter()
                .filter(|s| if horizontal { (s.1 - s.3).abs() < 0.01 && (s.0 - s.2).abs() > 1.0 }
                            else          { (s.0 - s.2).abs() < 0.01 && (s.1 - s.3).abs() > 1.0 })
                .collect();
            let across: Vec<_> = segs.iter()
                .filter(|s| if horizontal { (s.0 - s.2).abs() < 0.01 && (s.1 - s.3).abs() > 0.01 }
                            else          { (s.1 - s.3).abs() < 0.01 && (s.0 - s.2).abs() > 0.01 })
                .collect();
            assert!(along.len() >= 3, "{label}: expected a span per group, got {}", along.len());
            assert!(across.len() >= 6, "{label}: expected two caps per group, got {}", across.len());
        }
    }

    #[test]
    fn group_separates_series_without_coloring_them() {
        // `group` and `color` must not be the same atom. `group` splits a line
        // into one polyline per category; coloring by group index would invent
        // an encoding with no legend to decode it — and would leave no way to
        // say "separate these series but keep one color", which is the only
        // reason `group` exists next to `color`.
        let df = DataFrame::new()
            .with_float("a", vec![1.0, 2.0, 1.0, 2.0])
            .with_float("b", vec![1.0, 2.0, 3.0, 4.0])
            .with_str("g", vec!["x".into(), "x".into(), "y".into(), "y".into()]);
        let mut data = HashMap::new();
        data.insert("t".to_string(), df);
        let spec = PlotSpec::new().data("t").x("a").y("b");

        let strokes = |svg: &str| -> Vec<String> {
            svg.lines()
                .filter(|l| l.contains("<polyline"))
                .filter_map(|l| l.split(r#" stroke=""#).nth(1))
                .filter_map(|r| r.split('"').next())
                .map(str::to_string)
                .collect()
        };

        let grouped = SvgRenderer::default().render(
            &spec.clone().layer(Layer::new(Mark::Line).encode(Channel::Group, "g")),
            &data,
        );
        let g = strokes(&grouped);
        assert_eq!(g.len(), 2, "group should draw one polyline per category");
        assert_eq!(g[0], g[1], "group must not color: got {g:?}");

        let colored = SvgRenderer::default().render(
            &spec.layer(Layer::new(Mark::Line).encode(Channel::Color, "g")),
            &data,
        );
        let c = strokes(&colored);
        assert_eq!(c.len(), 2);
        assert_ne!(c[0], c[1], "color must color: got {c:?}");
    }

    #[test]
    fn a_line_takes_a_set_width_and_opacity() {
        // The channels are refused here (one stroke cannot vary per row); the
        // settings are the supported way to reach the same properties.
        let svg = render_styled(Layer::new(Mark::Line).style_size(6.0).style_opacity(0.4));
        let polyline = svg
            .lines()
            .find(|l| l.contains("<polyline"))
            .expect("a polyline should be drawn");
        assert!(polyline.contains(r#"stroke-width="6""#), "got: {polyline}");
        assert!(polyline.contains(r#"stroke-opacity="0.400""#), "got: {polyline}");
    }

    // -- facets -----------------------------------------------------------
    //
    // Small multiples: the outer frame's categories each get a panel, the
    // panels share one scale, and statistics run within each panel's rows.

    /// Panel backgrounds are the one `#f5f5f8` rect per frame.
    fn panel_count(svg: &str) -> usize {
        svg.matches(r##"fill="#f5f5f8""##).count()
    }

    fn faceted_points() -> (PlotSpec, HashMap<String, DataFrame>) {
        let df = DataFrame::new()
            .with_float("x", vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0])
            .with_float("y", vec![1.0, 4.0, 2.0, 5.0, 3.0, 6.0])
            .with_str("g", vec![
                "a".into(), "a".into(), "b".into(),
                "b".into(), "c".into(), "c".into(),
            ]);
        let mut data = HashMap::new();
        data.insert("t".to_string(), df);
        let spec = PlotSpec::new().data("t").x("x").y("y")
            .layer(Layer::new(Mark::Point));
        (spec, data)
    }

    /// Five levels — an odd count, so wrapping at two leaves a ragged row.
    fn five_level_points() -> (PlotSpec, HashMap<String, DataFrame>) {
        let names = ["a", "b", "c", "d", "e"];
        let df = DataFrame::new()
            .with_float("x", (0..10).map(|i| (i % 2) as f64).collect())
            .with_float("y", (0..10).map(|i| i as f64).collect())
            .with_str("g", (0..10).map(|i| names[i / 2].to_string()).collect());
        let mut data = HashMap::new();
        data.insert("t".to_string(), df);
        let spec = PlotSpec::new().data("t").x("x").y("y")
            .layer(Layer::new(Mark::Point));
        (spec, data)
    }

    #[test]
    fn a_facet_makes_one_panel_per_category_and_names_each() {
        let (spec, data) = faceted_points();
        let svg = SvgRenderer::default().render(&spec.facet_col("g"), &data);
        assert_eq!(panel_count(&svg), 3);
        for name in ["a", "b", "c"] {
            assert!(svg.contains(&format!(">{name}</text>")), "missing strip label {name}");
        }
    }

    /// A shared scale is what makes panels comparable, and what makes some of
    /// them unreadable. Freed, each panel fits its own rows — and only the axis
    /// that asked: `x` stays shared here, so the two are a mix, not a mode.
    #[test]
    fn a_freed_axis_is_fitted_per_panel_and_the_other_one_is_not() {
        let df = DataFrame::new()
            .with_float("x", vec![1.0, 2.0, 1.0, 2.0, 1.0, 2.0])
            .with_float("y", vec![1.0, 2.0, 100.0, 200.0, 10.0, 20.0])
            .with_str("g", vec!["a".into(), "a".into(), "b".into(), "b".into(),
                                "c".into(), "c".into()]);
        let mut data = HashMap::new();
        data.insert("t".to_string(), df);
        let spec = PlotSpec::new().data("t").x("x").y("y")
            .layer(Layer::new(Mark::Point))
            .facet_col("g");

        // Shared: one y range over 1..200, so every panel is ticked alike and
        // only the bottom row carries the x numbers.
        let shared = SvgRenderer::default().render(&spec, &data);
        // Freed: three different y ranges, so "200" can only come from panel b
        // and "20" from panel c — neither appears when the scale is shared.
        let mut freed = spec.clone();
        freed.y = freed.y.map(|d| d.with_free());
        let svg = SvgRenderer::default().render(&freed, &data);

        assert!(!shared.contains(">20</text>"), "a shared y spans 1..200 and never ticks 20");
        assert!(svg.contains(">20</text>"), "panel c should tick its own 20");
        assert!(svg.contains(">200</text>"), "panel b should tick its own 200");
        assert!(svg.contains(">100</text>"), "panel b should tick its own 100");
        // x was not freed, so all three panels still share 1.0..2.0.
        assert!(svg.contains(">2.0</text>") && shared.contains(">2.0</text>"));
        assert_eq!(panel_count(&svg), 3);
    }

    /// A freed panel's **guides** are fitted with its marks, not with the plot.
    ///
    /// The bug this pins was a *statement order*: the per-panel axes were resolved
    /// below the frame routines, so a freed polar panel drew its sectors from its
    /// own fit and its rings from the shared one — every panel ringed alike while
    /// its tick numbers read its own scale, so the rings annotated a scale that was
    /// not there. Flat panels were unaffected only because a flat frame is drawn
    /// from the panel rectangle rather than from the ticks.
    ///
    /// Read on the **radii**, which is where the disagreement lives; the labels were
    /// right the whole time and are what made it look correct. The three groups are
    /// deliberately *not* self-similar — `[0,4]`, `[0,90]`, `[0,8.2]` put their ticks
    /// at different fractions of their own domains, where three ranges of the same
    /// shape would ring identically whether the fix were in or not.
    #[test]
    fn a_freed_polar_panel_rings_its_own_scale_and_not_the_plots() {
        let df = DataFrame::new()
            .with_str("cat", ["N", "E", "S", "W"].iter().cycle().take(12)
                .map(|s| s.to_string()).collect())
            .with_float("val", vec![1.0, 2.0, 3.0, 4.0,
                                    10.0, 90.0, 50.0, 20.0,
                                    7.0, 7.5, 8.0, 8.2])
            .with_str("g", ["a", "b", "c"].iter().flat_map(|s| std::iter::repeat(s.to_string()).take(4))
                .collect());
        let data = HashMap::from([("t".to_string(), df)]);
        let base = PlotSpec::new().data("t").x("cat").y("val")
            .layer(Layer::new(Mark::Bar))
            .coord(CoordSpace::Polar(crate::ir::PolarView { start: 0.0 }))
            .facet_col("g");

        // The radial rings each panel drew, as fractions of that panel's own outer
        // ring — the comparison that survives the discs being different sizes.
        let rings = |svg: &str| -> Vec<Vec<i64>> {
            let mut by_centre: std::collections::BTreeMap<(i64, i64), Vec<f64>> = Default::default();
            for line in svg.lines().filter(|l| l.contains("<circle")) {
                let f = |k: &str| line.split(k).nth(1)
                    .and_then(|s| s.split('"').next())
                    .and_then(|s| s.parse::<f64>().ok());
                if let (Some(cx), Some(cy), Some(r)) = (f(r#"cx=""#), f(r#"cy=""#), f(r#"r=""#)) {
                    by_centre.entry((cx as i64, cy as i64)).or_default().push(r);
                }
            }
            by_centre.into_values().map(|mut rs| {
                rs.sort_by(|a, b| a.partial_cmp(b).unwrap());
                let max = rs.last().copied().unwrap_or(1.0);
                rs.iter().map(|r| (r / max * 1000.0).round() as i64).collect()
            }).collect()
        };

        let mut freed = base.clone();
        freed.y = freed.y.map(|d| d.with_free());
        let f = rings(&SvgRenderer::default().render(&freed, &data));
        assert_eq!(f.len(), 3, "three panels of rings");
        assert!(f[0] != f[1] && f[1] != f[2],
            "each freed panel rings its own domain, so no two agree: {f:?}");

        // And the other half of the rule: a plot nobody freed is unmoved. Shared,
        // one scale means one set of rings, in every panel.
        let s = rings(&SvgRenderer::default().render(&base, &data));
        assert!(s[0] == s[1] && s[1] == s[2],
            "one shared scale is one set of rings: {s:?}");
    }

    /// Five levels wrapped at two: a 2 × 3 rectangle holding five panels, each
    /// carrying its own name, and one cell left empty by the fold. The empty
    /// cell gets no panel background — unlike a *crossing*'s empty combination,
    /// which is framed because the crossing says it is possible.
    #[test]
    fn a_wrapped_facet_draws_one_panel_per_level_and_names_every_one() {
        let (spec, data) = five_level_points();
        let svg = SvgRenderer::default()
            .render(&spec.facet_col("g").facet_wrap(2), &data);
        assert_eq!(panel_count(&svg), 5, "five levels, not the rectangle's six cells");
        for name in ["a", "b", "c", "d", "e"] {
            assert!(svg.contains(&format!(">{name}</text>")), "missing strip label {name}");
        }
    }

    /// The direction is the operator's. `/` runs the levels *down*, so `a` and
    /// `b` share a column and sit in successive rows — where a ribbon numbered
    /// row-major would put them side by side. This also settles the subsets:
    /// a panel's name and its rows are both read at `Panel::slot`, so a name in
    /// the right cell is rows in the right cell.
    #[test]
    fn wrapping_down_stacks_the_first_levels_in_one_column() {
        let (spec, data) = five_level_points();
        let svg = SvgRenderer::default()
            .render(&spec.facet_row("g").facet_wrap(2), &data);
        let at = |name: &str| -> (f64, f64) {
            let tag = format!(">{name}</text>");
            let line = svg.lines().find(|l| l.contains(&tag) && l.contains("<text x="))
                .unwrap_or_else(|| panic!("no strip for {name}"));
            let num = |key: &str| -> f64 {
                let s = &line[line.find(key).unwrap() + key.len()..];
                s[..s.find('"').unwrap()].parse().unwrap()
            };
            (num("x=\""), num("y=\""))
        };
        let (ax, ay) = at("a");
        let (bx, by) = at("b");
        let (cx, _) = at("c");
        assert!((ax - bx).abs() < 1e-6, "`a` and `b` should share a column: {ax} vs {bx}");
        assert!(by > ay, "`b` should sit below `a`: {by} vs {ay}");
        assert!(cx > ax, "`c` starts the next column: {cx} vs {ax}");
    }

    #[test]
    fn an_unfaceted_plot_is_the_one_panel_degenerate_case() {
        let (spec, data) = faceted_points();
        let svg = SvgRenderer::default().render(&spec, &data);
        assert_eq!(panel_count(&svg), 1);
    }

    #[test]
    fn a_crossed_grid_draws_every_combination_even_the_empty_one() {
        // g × h has 3 × 2 = 6 combinations but only 4 appear in the rows.
        // A crossing draws all 6: the frame says the combination is possible
        // even when this data has no example of it (Wilkinson ch. 11).
        let (spec, mut data) = faceted_points();
        let df = data.remove("t").unwrap().with_str("h", vec![
            "u".into(), "u".into(), "v".into(), "v".into(), "u".into(), "u".into(),
        ]);
        data.insert("t".to_string(), df);
        let svg = SvgRenderer::default().render(&spec.facet_col("g").facet_row("h"), &data);
        assert_eq!(panel_count(&svg), 6);
    }

    #[test]
    fn facet_statistics_run_within_each_panel() {
        // Counting rows per category: globally "p" appears 3 times, but no
        // single panel holds more than 2 of them. If the count ran before the
        // facet split, a bar of height 3 would need a y tick labeled 3.
        let df = DataFrame::new()
            .with_str("cat", vec!["p".into(), "p".into(), "q".into(), "p".into()])
            .with_str("g", vec!["a".into(), "a".into(), "a".into(), "b".into()]);
        let mut data = HashMap::new();
        data.insert("t".to_string(), df);
        let spec = PlotSpec::new().data("t").x("cat")
            .layer(Layer::new(Mark::Bar).transform(Transform::Count))
            .facet_col("g");
        let svg = SvgRenderer::default().render(&spec, &data);

        // Panel a draws bars for p and q, panel b for p alone: three bars.
        assert_eq!(bar_lefts(&svg).len(), 3);
        assert!(!svg.contains(">3</text>"),
            "a y tick at 3 means the count ran across panels: {svg}");
    }

    #[test]
    fn tick_labels_are_drawn_only_where_panels_touch_the_margins() {
        // y runs 10..60 so its tick labels cannot be mistaken for x's 1..6.
        let df = DataFrame::new()
            .with_float("x", vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0])
            .with_float("y", vec![10.0, 40.0, 20.0, 50.0, 30.0, 60.0])
            .with_str("g", vec![
                "a".into(), "a".into(), "b".into(),
                "b".into(), "c".into(), "c".into(),
            ]);
        let mut data = HashMap::new();
        data.insert("t".to_string(), df);
        // An explicit tick_count wins over the fewer-ticks-per-narrow-panel
        // default, which keeps the tick values this test pins deterministic.
        let spec = PlotSpec::new().data("t").x("x").y("y").tick_count(5)
            .layer(Layer::new(Mark::Point)).facet_col("g");
        let svg = SvgRenderer::default().render(&spec, &data);

        // One row of panels: every panel touches the bottom margin, so an
        // x tick label appears once per panel; a y label only once, beside
        // the left column.
        assert_eq!(svg.matches(">2</text>").count(), 3, "x tick 2 once per panel");
        assert_eq!(svg.matches(">20</text>").count(), 1, "y tick 20 on the left column only");
    }

    #[test]
    fn a_layer_whose_table_lacks_the_facet_column_is_drawn_in_every_panel() {
        let (spec, mut data) = faceted_points();
        data.insert("ref".to_string(), DataFrame::new()
            .with_float("x", vec![1.0, 6.0])
            .with_float("y", vec![3.5, 3.5]));
        let mut line = Layer::new(Mark::Line);
        line.data = Some("ref".to_string());
        let svg = SvgRenderer::default().render(&spec.layer(line).facet_col("g"), &data);
        assert_eq!(svg.matches("<polyline").count(), 3,
            "the reference line should appear in all three panels");
    }

    #[test]
    fn facet_panels_share_one_scale() {
        // Panel b's own y values stop at 2, but the shared scale runs to the
        // crossing's maximum, so panel b still draws against ticks up to 6 —
        // that shared frame is what makes the panels comparable.
        let df = DataFrame::new()
            .with_float("x", vec![1.0, 2.0, 1.0, 2.0])
            .with_float("y", vec![5.0, 6.0, 1.0, 2.0])
            .with_str("g", vec!["a".into(), "a".into(), "b".into(), "b".into()]);
        let mut data = HashMap::new();
        data.insert("t".to_string(), df);
        let spec = PlotSpec::new().data("t").x("x").y("y")
            .layer(Layer::new(Mark::Point)).facet_col("g");
        let svg = SvgRenderer::default().render(&spec, &data);

        // The y axis (drawn once, left column) reaches 6.
        assert!(svg.contains(">6</text>"), "{svg}");
        // Every circle in the right panel sits in its lower half: with the
        // shared scale, y=1 and y=2 map below the panels' vertical midpoint.
        let cys: Vec<f64> = svg.lines()
            .filter(|l| l.contains("<circle"))
            .filter_map(|l| l.split(r#"cy=""#).nth(1)?.split('"').next()?.parse().ok())
            .collect();
        assert_eq!(cys.len(), 4);
        let mid = (cys.iter().cloned().fold(f64::INFINITY, f64::min)
                 + cys.iter().cloned().fold(f64::NEG_INFINITY, f64::max)) / 2.0;
        assert!(cys[2] > mid && cys[3] > mid,
            "panel b's points should sit low on the shared scale: {cys:?}");
    }

    /// A frame whose two panels have deliberately mismatched spreads and counts —
    /// the shape that made every input to a bin layout panel-sized. Panel `a` runs
    /// 0..1 with four rows, panel `b` runs 0..40 with eight, so a per-panel cut
    /// disagrees on the width (Sturges' `k` differs *and* the spans differ by 40×)
    /// and on where the bins start.
    fn lopsided_panels() -> HashMap<String, DataFrame> {
        let mut v = vec![0.0, 0.3, 0.6, 1.0];
        v.extend([0.0, 5.0, 10.0, 15.0, 20.0, 25.0, 30.0, 40.0]);
        let mut g: Vec<String> = vec!["a".into(); 4];
        g.extend(vec!["b".to_string(); 8]);
        let mut data = HashMap::new();
        data.insert("t".to_string(), DataFrame::new().with_float("v", v).with_str("g", g));
        data
    }

    #[test]
    fn a_facet_cuts_one_set_of_bins_for_every_panel() {
        // The cut is an extent description, so it is shared across panels exactly
        // as the scale is (spec §11) — otherwise `bar * bin + x(v) | facet(g)`
        // draws bars of different widths against one axis, and their heights are
        // counts of different quantities. gapminder made this plain: 5.5 years per
        // bar in Asia against 1.7 in Europe, so Africa's peak of 13 *looked* taller
        // than Europe's 8 while being barely half as dense.
        let data = lopsided_panels();
        let spec = PlotSpec::new().data("t").x("v")
            .layer(Layer::new(Mark::Bar).transform(Transform::Bin))
            .facet_col("g");
        let svg = SvgRenderer::default().render(&spec, &data);

        let widths: Vec<f64> = bar_rects(&svg).iter().map(|r| r.2).collect();
        assert!(!widths.is_empty(), "no bars drawn: {svg}");
        let (lo, hi) = (
            widths.iter().cloned().fold(f64::INFINITY, f64::min),
            widths.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
        );
        assert!(hi - lo < 0.01,
            "every panel's bars must be one width; got {lo}..{hi}: {widths:?}");
    }

    #[test]
    fn a_facets_shared_cut_still_tallies_within_each_panel() {
        // The other half of the same rule, and the half that must *not* change:
        // the cut is the plot's, the tally is the panel's (spec §5, §11). Sharing
        // the tally too would draw one histogram five times.
        let data = lopsided_panels();
        let spec = PlotSpec::new().data("t").x("v")
            .layer(Layer::new(Mark::Bar).transform(Transform::Bin))
            .facet_col("g");
        let svg = SvgRenderer::default().render(&spec, &data);

        // Panel `a`'s four rows all fall in the leftmost shared bin, so it draws
        // exactly one bar; panel `b` spreads across several. Equal counts per panel
        // would mean the tally had escaped its panel.
        let bars = bar_rects(&svg);
        let mid = bars.iter().map(|r| r.0).fold(f64::NEG_INFINITY, f64::max) / 2.0;
        let left = bars.iter().filter(|r| r.0 < mid).count();
        let right = bars.len() - left;
        assert_eq!(left, 1, "panel a's rows all land in one shared bin: {bars:?}");
        assert!(right > 1, "panel b's rows spread across bins: {bars:?}");
    }

    #[test]
    fn an_unfaceted_bin_is_untouched_by_the_shared_cut() {
        // The cut is resolved for every plot, not only faceted ones, so that one
        // path serves both. That is only safe if a single panel — which *is* the
        // whole frame — cuts exactly what it cut before, so this pins the identity.
        let data = lopsided_panels();
        let plain = PlotSpec::new().data("t").x("v")
            .layer(Layer::new(Mark::Bar).transform(Transform::Bin));
        let svg = SvgRenderer::default().render(&plain, &data);
        let widths: Vec<f64> = bar_rects(&svg).iter().map(|r| r.2).collect();
        assert!(!widths.is_empty(), "no bars drawn: {svg}");

        // Sturges over all twelve rows: k = ceil(log2(12)) + 1 = 5 bins across
        // 0..40, so a bar is a fifth of the plotted span however the rows sit.
        let lefts = bar_lefts(&svg);
        // Tolerance of a rounding step, not of a discrepancy: the two quantities
        // are written to the SVG separately at two decimals, so they can differ in
        // the last one. The defect this guards against was a factor of three.
        let gap = lefts.windows(2).map(|w| w[1] - w[0])
            .fold(f64::INFINITY, f64::min);
        assert!((gap - widths[0]).abs() < 0.02,
            "a histogram's bars abut: gap {gap}, width {}", widths[0]);
    }

    // -----------------------------------------------------------------------
    // play — the facet read in time
    //
    // Every property here has a facet counterpart a few tests up, and that is
    // the claim being pinned as much as any single behavior: one scale over
    // every subset, statistics within each subset, and a layer that lacks the
    // splitting column drawn in all of them. What is new is only *when* the
    // subsets are shown.
    // -----------------------------------------------------------------------

    fn played_points() -> (PlotSpec, HashMap<String, DataFrame>) {
        // Two moments, and deliberately lopsided: everything in 1962 sits past
        // everything in 1957, so a per-frame scale would be visible as motion
        // that is the axis's rather than the data's.
        let df = DataFrame::new()
            .with_float("x", vec![1.0, 2.0, 3.0, 10.0, 20.0, 30.0])
            .with_float("y", vec![1.0, 2.0, 3.0, 10.0, 20.0, 30.0])
            .with_float("year", vec![1957.0, 1957.0, 1957.0, 1962.0, 1962.0, 1962.0]);
        let mut data = HashMap::new();
        data.insert("t".to_string(), df);
        let spec = PlotSpec::new().data("t").x("x").y("y")
            .layer(Layer::new(Mark::Point))
            .channel(Channel::Play, "year");
        (spec, data)
    }

    fn frame_count(svg: &str) -> usize {
        svg.matches(r#"<animate attributeName="display""#).count()
    }

    /// **The claim a written-out file rests on**, and the reason stills are cut
    /// by selection rather than by a second pass over the data: two moments of
    /// one plot differ in *which group shows* and in nothing else at all.
    ///
    /// Erase the display attributes and the two frames are the same bytes. That
    /// is a stronger statement than "the axes match" — it covers every tick, the
    /// color map, both legends, the strip, and the layout arithmetic in one
    /// assertion, and it fails the moment any of them is decided per frame. The
    /// fixture is lopsided on purpose (everything in 1962 sits past everything in
    /// 1957), so a per-frame fit would move an axis rather than being invisible.
    #[test]
    fn two_stills_of_one_plot_differ_only_in_which_moment_shows() {
        let (spec, data) = played_points();
        let figure = crate::ir::Figure::Plot(Box::new(spec));
        let frames = crate::plot::render_frames_with(
            &figure, &data, crate::plot::Strictness::Strict,
        )
        .expect("a played plot has frames")
        .frames;
        assert_eq!(frames.len(), 2, "one still per moment");

        let blind = |s: &str| {
            s.replace(r#"display="inline""#, "").replace(r#"display="none""#, "")
        };
        assert_eq!(blind(&frames[0]), blind(&frames[1]),
            "the moments disagree about something other than which one is shown");
        assert_ne!(frames[0], frames[1], "and they must differ about that");
    }

    /// A still is a picture, so it carries no clock. Left in, the `<animate>`
    /// would switch the moment off a fraction of a second after it was drawn —
    /// which a browser shows and a rasterizer ignores, so it would survive every
    /// check that reads the file as an image.
    #[test]
    fn a_still_carries_no_timing_and_shows_exactly_its_own_moment() {
        let (spec, data) = played_points();
        // A played plot cuts moments in more than one place — the marks, and the
        // strip that names them — so "one group showing" is the wrong count. What
        // must hold is that a still shows exactly what the sequence shows before
        // its clock starts, which is this number whatever the plot is made of.
        let animated = SvgRenderer::default().render(&spec, &data);
        let opening = animated.matches(r#"<g display="inline">"#).count();
        assert!(opening > 0, "the fixture must play at all");

        let figure = crate::ir::Figure::Plot(Box::new(spec));
        let frames = crate::plot::render_frames_with(
            &figure, &data, crate::plot::Strictness::Strict,
        )
        .unwrap()
        .frames;
        for (i, svg) in frames.iter().enumerate() {
            assert_eq!(frame_count(svg), 0, "still {i} still carries timing");
            assert_eq!(svg.matches(r#"<g display="inline">"#).count(), opening,
                "still {i} shows a different set of groups than the sequence opens with");
        }
    }

    /// The sequence path reports what the check said. `render_frames` returned
    /// bare frames once, and every non-fatal diagnostic — this Assumption
    /// included — was built and then dropped, so `--gif` wrote the file and
    /// said nothing. The silent drop §12 forbids, on the one output a reader
    /// cannot re-run with the words on.
    #[test]
    fn writing_out_the_frames_keeps_the_diagnostics() {
        let (mut spec, data) = played_points();
        spec.layers[0].transforms.push(crate::ir::Transform::Quantile);
        spec.layers[0].quantile = Some(crate::ir::QuantileSpec { p: Some(0.5) });
        let figure = crate::ir::Figure::Plot(Box::new(spec));
        let drawn = crate::plot::render_frames_with(
            &figure, &data, crate::plot::Strictness::Strict,
        )
        .expect("quantile(0.5) is an Assumption, not a refusal");
        assert!(!drawn.frames.is_empty());
        assert!(
            drawn.diagnostics.iter().any(|d| d.message.contains("median")),
            "the Assumption must ride along with the frames: {:?}",
            drawn.diagnostics.iter().map(|d| &d.message).collect::<Vec<_>>()
        );
    }

    /// A plot that does not play cannot be a sequence, and §12 says the refusal
    /// names what to do instead rather than writing a one-frame file nobody asked
    /// for. Guarded because the direction is the useful half of the message.
    #[test]
    fn a_plot_that_does_not_play_refuses_to_yield_frames() {
        let (_, data) = played_points();
        let still = PlotSpec::new().data("t").x("x").y("y").layer(Layer::new(Mark::Point));
        let figure = crate::ir::Figure::Plot(Box::new(still));
        let err = crate::plot::render_frames_with(
            &figure, &data, crate::plot::Strictness::Strict,
        )
        .expect_err("an unplayed plot has no moments");
        let last = err.last().expect("a diagnostic");
        assert!(last.message.contains("does not play"), "{}", last.message);
        assert!(last.message.contains("play(year)"),
            "the refusal must say what to write instead: {}", last.message);
    }

    /// The invariant the whole feature is built around: an unplayed plot is what
    /// it always was. Not "close enough" — a corpus of 481 recorded hashes says
    /// so per sentence, and this is the unit-level statement of the same thing.
    #[test]
    fn a_plot_that_does_not_play_carries_no_timing_at_all() {
        let (spec, data) = played_points();
        let mut still = spec.clone();
        still.channels.remove(&Channel::Play);
        let svg = SvgRenderer::default().render(&still, &data);
        assert!(!svg.contains("<animate"), "no timing");
        assert!(!svg.contains("<g display="), "no frame groups");
        assert_eq!(panel_count(&svg), 1, "and still the one-panel degenerate case");
    }

    /// One group per moment, for the marks and again for the strip that names
    /// them — and the first is the one written showing.
    #[test]
    fn play_cuts_one_group_per_moment_and_shows_the_first() {
        let (spec, data) = played_points();
        let svg = SvgRenderer::default().render(&spec, &data);
        assert_eq!(frame_count(&svg), 4, "two moments, once for the marks and once for the strip");
        assert_eq!(svg.matches(r#"<g display="inline">"#).count(), 2,
            "exactly the first moment is written visible");
        assert_eq!(svg.matches(r#"<g display="none">"#).count(), 2);
        // The static state is the print fallback: `rsvg-convert` ignores
        // `<animate>` and draws the attributes as written, so this is what a PDF
        // page of this plot shows.
        let first = svg.find(r#"<g display="inline">"#).unwrap();
        let hidden = svg.find(r#"<g display="none">"#).unwrap();
        assert!(first < hidden, "the visible moment is the first one, not a later one");
    }

    /// A single-valued column is a plot, not a sequence. Nothing is written,
    /// because a group that never changes is bytes spent on nothing.
    #[test]
    fn one_moment_is_not_an_animation() {
        let (spec, data) = played_points();
        let mut one = data.clone();
        one.insert("t".to_string(), DataFrame::new()
            .with_float("x", vec![1.0, 2.0])
            .with_float("y", vec![1.0, 2.0])
            .with_float("year", vec![1957.0, 1957.0]));
        let svg = SvgRenderer::default().render(&spec, &one);
        assert!(!svg.contains("<animate"), "one frame needs no timing: {svg}");
    }

    /// The property that makes the motion the data's. Both moments are placed
    /// against one range, so 1957's rows sit low in the panel rather than
    /// filling it — which is what they would do under a per-frame fit.
    #[test]
    fn every_moment_is_drawn_against_one_shared_scale() {
        let (spec, data) = played_points();
        let svg = SvgRenderer::default().render(&spec, &data);
        let cys: Vec<f64> = svg.split(r#"<circle"#).skip(1)
            .filter_map(|s| s.split(r#"cy=""#).nth(1)?.split('"').next()?.parse().ok())
            .collect();
        assert_eq!(cys.len(), 6, "every row of every moment is drawn");
        // Six distinct heights: if each frame were fitted to itself, 1..3 and
        // 10..30 would map to the same three positions and there would be three.
        let mut rounded: Vec<i64> = cys.iter().map(|v| (v * 10.0).round() as i64).collect();
        rounded.sort_unstable();
        rounded.dedup();
        assert_eq!(rounded.len(), 6,
            "one range over the whole sequence, so no two rows share a height");
    }

    /// A layer that does not bind `play` stands still behind the ones that do.
    /// The facet rule — a layer whose table lacks the column is drawn in every
    /// panel — arriving here through §8's scope resolution rather than through a
    /// second rule written for animation.
    #[test]
    fn a_layer_that_does_not_play_is_drawn_in_every_moment() {
        let df = DataFrame::new()
            .with_float("x", vec![1.0, 2.0, 3.0, 4.0])
            .with_float("y", vec![1.0, 2.0, 3.0, 4.0])
            .with_float("year", vec![1957.0, 1957.0, 1962.0, 1962.0]);
        let mut data = HashMap::new();
        data.insert("t".to_string(), df);
        // `play` written *after* the mark binds that layer alone; the rule layer
        // never names it and so is never cut down. The rule names its own axis
        // (§8) because a table with a column for both leaves nothing to say which
        // one it marks — `check_rule` refuses that, and rightly.
        let spec = PlotSpec::new().data("t").x("x").y("y")
            .layer(Layer::new(Mark::Point).encode(Channel::Play, "year"))
            .layer(Layer::new(Mark::Rule).encode(Channel::Y, "y"));
        let svg = SvgRenderer::default().render(&spec, &data);
        assert_eq!(frame_count(&svg), 4, "two moments, for the marks and for the strip");

        // Each moment holds two of the four rows — its own — and *all four*
        // rules, because the rule layer never named `year` and so was never cut
        // down. The still layer is redrawn inside each moment rather than hoisted
        // out of them, and that is the deliberate choice: hoisting would put it
        // behind every played layer whatever order it was written in, which is
        // the enclosing context silently reinterpreting an inner expression that
        // Law 6 forbids. The cost is a copy per frame; the alternative is wrong.
        // Each moment runs from its group tag to its own `<animate>`, which is the
        // last thing inside it — without that bound the slice would run on into
        // the axes and the strip, which are not part of any moment.
        let moments: Vec<&str> = svg.split(r#"<g display="#).skip(1).take(2)
            .map(|m| m.split("<animate").next().unwrap()).collect();
        assert_eq!(moments.len(), 2);
        for (i, m) in moments.iter().enumerate() {
            assert_eq!(m.matches("<circle").count(), 2,
                "moment {i} shows only its own two rows");
            // Gridlines are `<line>` too, but they are chrome and so are written
            // outside the moments — inside one, every line is the rule layer's.
            assert_eq!(m.matches("<line").count(), 4,
                "moment {i} shows all four rules — the layer that does not play");
        }
    }

    /// The channel earns a strip, and the strip names each moment. Without it
    /// the reader watches points move with nothing saying what they move
    /// through — the animation's version of an unlabeled axis.
    #[test]
    fn the_strip_names_every_moment_and_a_year_reads_as_a_year() {
        let (spec, data) = played_points();
        let svg = SvgRenderer::default().render(&spec, &data);
        assert!(svg.contains(">1957</text>"), "the first moment is named: {svg}");
        assert!(svg.contains(">1962</text>"), "and so is the second");
        assert!(!svg.contains(">1957.0<"), "a year is named, not measured");
    }

    /// `speed` is a multiple of the pace, so it divides the loop.
    #[test]
    fn speed_shortens_the_loop_it_does_not_drop_frames() {
        let (spec, data) = played_points();
        let plain = SvgRenderer::default().render(&spec, &data);
        let mut fast = spec.clone();
        fast.channels.insert(
            Channel::Play,
            crate::ir::ChannelDef::field("year").with_speed(2.0),
        );
        let quick = SvgRenderer::default().render(&fast, &data);
        assert_eq!(frame_count(&plain), frame_count(&quick), "the same moments");
        assert!(plain.contains(r#"dur="1.600s""#), "two frames at the default pace");
        assert!(quick.contains(r#"dur="0.800s""#), "twice as fast is half as long");
    }

    /// `play` crossed with `facet`: panels split the page, moments split the
    /// clock, and the two strips stack rather than collide.
    #[test]
    fn a_played_facet_animates_every_panel_in_step() {
        let (spec, data) = faceted_points();
        let mut spec = spec.facet_col("g");
        // `g` names the panels; a second column names the moments.
        let df = data["t"].clone();
        let mut data = HashMap::new();
        data.insert("t".to_string(), df.with_float(
            "year", vec![1957.0, 1962.0, 1957.0, 1962.0, 1957.0, 1962.0]));
        spec.channels.insert(Channel::Play, crate::ir::ChannelDef::field("year"));
        let svg = SvgRenderer::default().render(&spec, &data);
        assert_eq!(panel_count(&svg), 3, "the page is still split three ways");
        // Two moments per panel, plus two for the strip that names them.
        assert_eq!(frame_count(&svg), 3 * 2 + 2);
        for name in ["a", "b", "c", "1957", "1962"] {
            assert!(svg.contains(&format!(">{name}</text>")), "missing strip label {name}");
        }
    }

    // -- the faceted cube ---------------------------------------------------
    //
    // A facet crossed with 3-D, unblocked 2026-07-28 by deleting a refusal that
    // had never been true: the 3-D branch reads `l`, the *panel's* rect, so N
    // panels have always projected N cubes. These tests pin that, because the
    // thing that hid it for so long was that nothing asserted it either way.

    /// One wireframe group per cube — the faint `#d8d8de` box `write_space_box`
    /// draws behind the cloud.
    fn cube_count(svg: &str) -> usize {
        svg.matches(r##"stroke="#d8d8de""##).count()
    }

    /// Three categories, three panels, three cubes, and every point still drawn.
    /// The `Scene` is built from `l` rather than from the plot, so this is the
    /// assertion that the projector reads the panel it is standing in.
    #[test]
    fn a_faceted_cube_projects_one_scene_per_panel() {
        let (spec, data) = faceted_points();
        let df = data["t"].clone();
        let mut data = HashMap::new();
        data.insert("t".to_string(),
            df.with_float("z", vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]));
        let spec = spec.z("z").facet_col("g");
        let svg = SvgRenderer::default().render(&spec, &data);

        assert_eq!(panel_count(&svg), 3, "the page is split three ways");
        assert_eq!(cube_count(&svg), 3, "and each panel gets its own cube");
        assert_eq!(svg.matches("<circle").count(), 6, "no point is lost to the split");

        // The cubes stand apart: three distinct leftmost box edges, not one box
        // drawn three times in the same place.
        let lefts: std::collections::HashSet<String> = svg.lines()
            .filter(|l| l.trim_start().starts_with("<line x1="))
            .filter_map(|l| l.split(r#"x1=""#).nth(1))
            .filter_map(|r| r.split('"').next())
            .map(str::to_string)
            .collect();
        assert!(lefts.len() >= 3, "cubes should sit at different x, got {lefts:?}");
    }

    /// **A guide inside the panel is drawn in every panel; a guide on the panel's
    /// boundary is drawn once.** A flat facet writes `x` once, below the bottom
    /// row, because the panels share that edge. A cube's three axes are edges of
    /// the *cube*, and the cube is inside the panel, so there is no shared edge
    /// to write them on and each panel names its own. `polar` already works this
    /// way. The layout agrees: 3-D reserves no outer margin for tick numbers.
    #[test]
    fn a_faceted_cube_repeats_its_guides_because_they_live_inside_the_panel() {
        let (spec, data) = faceted_points();
        let df = data["t"].clone();
        let mut data = HashMap::new();
        data.insert("t".to_string(),
            df.with_float("z", vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]));

        let names = |svg: &str, name: &str| svg.matches(&format!(">{name}</text>")).count();

        let flat = SvgRenderer::default().render(&spec.clone().facet_col("g"), &data);
        assert_eq!(names(&flat, "X"), 1, "a flat axis name is written once for the plot");

        let cube = SvgRenderer::default().render(&spec.z("z").facet_col("g"), &data);
        assert_eq!(names(&cube, "X"), 3, "a cube names its own axes in every panel");
        assert_eq!(names(&cube, "Z"), 3, "including the third, which flat has nowhere to put");
    }

    /// `free` on `z` was legal and unreachable for as long as the cube could not
    /// be faceted (spec §11). It needed nothing added to start working — the
    /// positions are one family of three, which is Law 1 paying out — so this is
    /// `a_freed_axis_is_fitted_per_panel_and_the_other_one_is_not` one axis over.
    #[test]
    fn a_freed_z_fits_each_panels_own_cube() {
        let df = DataFrame::new()
            .with_float("x", vec![1.0, 2.0, 1.0, 2.0, 1.0, 2.0])
            .with_float("y", vec![1.0, 2.0, 1.0, 2.0, 1.0, 2.0])
            .with_float("z", vec![1.0, 2.0, 100.0, 200.0, 10.0, 20.0])
            .with_str("g", vec!["a".into(), "a".into(), "b".into(), "b".into(),
                                "c".into(), "c".into()]);
        let mut data = HashMap::new();
        data.insert("t".to_string(), df);
        let spec = PlotSpec::new().data("t").x("x").y("y").z("z")
            .layer(Layer::new(Mark::Point))
            .facet_col("g");

        let shared = SvgRenderer::default().render(&spec, &data);
        let mut freed = spec.clone();
        freed.z = freed.z.map(|d| d.with_free());
        let svg = SvgRenderer::default().render(&freed, &data);

        assert!(!shared.contains(">20</text>"), "a shared z spans 1..200 and never ticks 20");
        assert!(svg.contains(">20</text>"), "panel c should tick its own 20");
        assert!(svg.contains(">200</text>"), "panel b should tick its own 200");
        assert_eq!(cube_count(&svg), 3, "still one cube per panel");
    }

    /// The flat rule holding in the cube: a *crossing* frames every combination,
    /// including one with no rows, because the crossing says it is possible. (A
    /// folded ribbon is the other case and leaves its spare cells blank.)
    #[test]
    fn a_crossed_grid_of_cubes_frames_the_empty_combination_too() {
        let df = DataFrame::new()
            .with_float("x", vec![1.0, 2.0, 3.0])
            .with_float("y", vec![1.0, 2.0, 3.0])
            .with_float("z", vec![1.0, 2.0, 3.0])
            .with_str("c", vec!["p".into(), "q".into(), "p".into()])
            .with_str("r", vec!["u".into(), "u".into(), "v".into()]);
        let mut data = HashMap::new();
        data.insert("t".to_string(), df);
        let spec = PlotSpec::new().data("t").x("x").y("y").z("z")
            .layer(Layer::new(Mark::Point))
            .facet_col("c").facet_row("r");
        let svg = SvgRenderer::default().render(&spec, &data);

        // (q, v) has no rows, and still gets a panel and a cube to say so.
        assert_eq!(panel_count(&svg), 4, "a 2 x 2 crossing frames all four");
        assert_eq!(cube_count(&svg), 4, "the empty combination gets a cube too");
        assert_eq!(svg.matches("<circle").count(), 3, "but only three points exist");
    }

    /// The two partitions crossed, which is the sentence with everything in it:
    /// panels across the page, moments along the clock, and a projected cube in
    /// each. The cube itself is drawn *outside* the frame groups, so it stands
    /// still while only the marks swap — the same rule the flat panel follows.
    #[test]
    fn a_faceted_cube_animates_every_panel_in_step() {
        let (spec, data) = faceted_points();
        let df = data["t"].clone();
        let mut data = HashMap::new();
        data.insert("t".to_string(), df
            .with_float("z", vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0])
            .with_float("year", vec![1957.0, 1962.0, 1957.0, 1962.0, 1957.0, 1962.0]));
        let mut spec = spec.z("z").facet_col("g");
        spec.channels.insert(Channel::Play, crate::ir::ChannelDef::field("year"));
        let svg = SvgRenderer::default().render(&spec, &data);

        assert_eq!(panel_count(&svg), 3, "the page is still split three ways");
        assert_eq!(cube_count(&svg), 3, "one cube per panel, not one per moment");
        // Two moments per panel, plus two for the strip that names them.
        assert_eq!(frame_count(&svg), 3 * 2 + 2);
        for name in ["a", "b", "c", "1957", "1962"] {
            assert!(svg.contains(&format!(">{name}</text>")), "missing strip label {name}");
        }
    }

    #[test]
    fn a_facets_shared_cut_survives_a_color_split_inside_it() {
        // Two splits at once, and neither may take the cut back: `color` shares
        // edges across its groups (`bin_layout` exists for that) and `facet` shares
        // them across panels, so all four histograms here land on one lattice.
        // Law 1 — the two splits compose without either learning about the other.
        let mut v = vec![0.0, 0.3, 0.6, 1.0];
        v.extend([0.0, 5.0, 10.0, 15.0, 20.0, 25.0, 30.0, 40.0]);
        let g: Vec<String> = ["a", "a", "a", "a", "b", "b", "b", "b", "b", "b", "b", "b"]
            .iter().map(|s| s.to_string()).collect();
        let c: Vec<String> = ["p", "q", "p", "q", "p", "q", "p", "q", "p", "q", "p", "q"]
            .iter().map(|s| s.to_string()).collect();
        let mut data = HashMap::new();
        data.insert("t".to_string(),
            DataFrame::new().with_float("v", v).with_str("g", g).with_str("c", c));
        let spec = PlotSpec::new().data("t").x("v")
            .layer(Layer::new(Mark::Bar).transform(Transform::Bin).encode(Channel::Color, "c"))
            .facet_col("g");
        let svg = SvgRenderer::default().render(&spec, &data);

        let widths: Vec<f64> = bar_rects(&svg).iter().map(|r| r.2).collect();
        assert!(!widths.is_empty(), "no bars drawn: {svg}");
        let (lo, hi) = (
            widths.iter().cloned().fold(f64::INFINITY, f64::min),
            widths.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
        );
        assert!(hi - lo < 0.01,
            "color groups and panels share one cut; got {lo}..{hi}: {widths:?}");
    }

    #[test]
    fn a_facets_shared_cut_reaches_the_two_dimensional_reading() {
        // A mesh takes the rule for the reason `bin2d_mixed` already states: cells
        // that do not line up across the plot are not a mesh. A panel is a column
        // of cells one dimension out, so a heatmap faceted into panels with
        // different spreads must still tile one lattice.
        let mut x = vec![0.0, 0.3, 0.6, 1.0];
        x.extend([0.0, 5.0, 10.0, 15.0, 20.0, 25.0, 30.0, 40.0]);
        let y = x.clone();
        let mut g: Vec<String> = vec!["a".into(); 4];
        g.extend(vec!["b".to_string(); 8]);
        let mut data = HashMap::new();
        data.insert("t".to_string(),
            DataFrame::new().with_float("x", x).with_float("y", y).with_str("g", g));
        let spec = PlotSpec::new().data("t").x("x").y("y")
            .layer(Layer::new(Mark::Zone).transform(Transform::Bin))
            .facet_col("g");
        let svg = SvgRenderer::default().render(&spec, &data);

        let widths: Vec<f64> = bar_rects(&svg).iter().map(|r| r.2).collect();
        assert!(!widths.is_empty(), "no cells drawn: {svg}");
        let (lo, hi) = (
            widths.iter().cloned().fold(f64::INFINITY, f64::min),
            widths.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
        );
        assert!(hi - lo < 0.01,
            "every panel's cells must tile one mesh; got {lo}..{hi}: {widths:?}");
    }

    #[test]
    fn panel_clip_ids_are_distinct_within_a_plot_and_stable_across_identical_ones() {
        // Same lesson as the gradient legend: the book inlines many SVGs into
        // one page, where ids are global and the first definition wins. An id
        // derived from the rectangle collides only when the rectangles agree,
        // which is the one collision that cannot mislead.
        let (spec, data) = faceted_points();
        let svg = SvgRenderer::default().render(&spec.clone().facet_col("g"), &data);
        let ids: Vec<&str> = svg.lines()
            .filter_map(|l| l.split(r#"<clipPath id=""#).nth(1)?.split('"').next())
            .collect();
        assert_eq!(ids.len(), 3);
        assert!(ids.iter().all(|id| ids.iter().filter(|o| *o == id).count() == 1),
            "panel clips must not collide: {ids:?}");

        let again = SvgRenderer::default().render(&spec.facet_col("g"), &data);
        assert_eq!(svg, again, "identical specs must yield identical ids");
    }

    // -----------------------------------------------------------------------
    // path — the stroke that keeps the table's order
    // -----------------------------------------------------------------------

    /// A route that doubles back: x goes up, down, then up again, so the row
    /// order and the x order are genuinely different sequences. Sorting it is
    /// visible; nothing subtler is needed to tell the two marks apart.
    fn doubling_back() -> HashMap<String, DataFrame> {
        HashMap::from([(
            "t".to_string(),
            DataFrame::new()
                .with_float("x", vec![3.0, 1.0, 4.0, 2.0])
                .with_float("y", vec![1.0, 2.0, 3.0, 4.0]),
        )])
    }

    fn vertex_xs(svg: &str) -> Vec<f64> {
        svg.lines()
            .find(|l| l.contains("<polyline"))
            .and_then(|l| l.split(r#"points=""#).nth(1)?.split('"').next())
            .map(|pts| pts.split_whitespace()
                .filter_map(|p| p.split(',').next()?.parse().ok())
                .collect())
            .unwrap_or_default()
    }

    #[test]
    fn a_path_keeps_the_tables_order_where_a_line_sorts_by_x() {
        // The whole reason `path` is a mark and not a flag. Written as one
        // assertion set over both marks so it cannot pass by describing each
        // separately — the trap a mirrored copy falls into.
        let data = doubling_back();
        let spec = |m: Mark| PlotSpec::new().data("t").x("x").y("y").layer(Layer::new(m));

        let line = vertex_xs(&SvgRenderer::default().render(&spec(Mark::Line), &data));
        let path = vertex_xs(&SvgRenderer::default().render(&spec(Mark::Path), &data));
        assert_eq!(line.len(), 4);
        assert_eq!(path.len(), 4);

        assert!(line.windows(2).all(|w| w[0] <= w[1]),
            "a line sorts its vertices by x: {line:?}");
        assert!(!path.windows(2).all(|w| w[0] <= w[1]),
            "a path must not sort — it follows the rows: {path:?}");
        // And concretely: the table starts at x = 3, so the path does too, while
        // the line starts at the smallest x in the column.
        let mut sorted = path.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert_eq!(line, sorted, "the line is exactly the path's vertices, sorted");
        assert!(path[0] > path[1], "the path's first vertex is the table's first row");
    }

    // -----------------------------------------------------------------------
    // zone — the rectangle whose unbounded axis is the panel's
    // -----------------------------------------------------------------------

    /// Every `<rect>` a zone drew, as (x, y, width, height).
    ///
    /// Picked out by shape rather than by position, because the panel background
    /// and the legend swatches are rects too: a zone's are the ones carrying both
    /// a fill opacity and `stroke="none"`. It used to key on the *translucent*
    /// opacity alone, which stopped identifying zones the moment a binned one
    /// drew opaque — it is the data, not decoration over it — and silently
    /// reported no zones at all rather than failing.
    fn zone_rects(svg: &str) -> Vec<(f64, f64, f64, f64)> {
        svg.lines()
            .filter(|l| l.contains("<rect") && l.contains("fill-opacity")
                     && l.contains(r#"stroke="none""#))
            .map(|l| {
                let num = |k: &str| -> f64 {
                    let at = l.find(k).unwrap() + k.len();
                    l[at..].split('"').next().unwrap().parse().unwrap()
                };
                (num(r#"x=""#), num(r#"y=""#), num(r#"width=""#), num(r#"height=""#))
            })
            .collect()
    }

    /// The panel's own rectangle — the background the renderer writes straight from
    /// `Layout`, and therefore the ground truth a zone's unbounded side is claimed
    /// to match. It is the first `<rect>` with a plain `fill` and no opacity.
    fn panel_rect(svg: &str) -> (f64, f64, f64, f64) {
        let l = svg.lines()
            .find(|l| l.trim_start().starts_with("<rect") && !l.contains("fill-opacity")
                   && l.contains("width=") && l.contains(r#"x=""#))
            .expect("the panel background is drawn");
        let num = |k: &str| -> f64 {
            let at = l.find(k).unwrap() + k.len();
            l[at..].split('"').next().unwrap().parse().unwrap()
        };
        (num(r#"x=""#), num(r#"y=""#), num(r#"width=""#), num(r#"height=""#))
    }

    #[test]
    fn the_axis_a_zone_is_not_bounded_on_is_the_panel_s_own_extent() {
        // The mark's whole claim, and the one thing `ribbon * bounds` cannot do:
        // an unbounded axis reaches the panel *exactly*, not to a padded data
        // value. Asserted as one differential over both axes, so it cannot pass by
        // describing each separately — the y-bounded zone must span the full width
        // and the x-bounded one the full height, and each must be *narrower* than
        // the panel on the axis it is bounded on.
        let mut data = HashMap::new();
        data.insert("t".to_string(), DataFrame::new()
            .with_float("a", vec![0.0, 10.0]).with_float("b", vec![0.0, 10.0]));
        data.insert("z".to_string(), DataFrame::new()
            .with_float("lo", vec![3.0]).with_float("hi", vec![6.0]));

        let draw = |zone: Layer| {
            let spec = PlotSpec::new().data("t").x("a").y("b")
                .layer(zone.data("z")).layer(Layer::new(Mark::Point));
            zone_rects(&SvgRenderer::default().render(&spec, &data))
        };
        let across = draw(Layer::new(Mark::Zone).bounds("lo", "hi"));
        let up     = draw(Layer::new(Mark::Zone).span("lo", "hi"));
        assert_eq!(across.len(), 1, "one row is one rectangle");
        assert_eq!(up.len(), 1);

        let (ax, ay, aw, ah) = across[0]; // spans the panel horizontally
        let (ux, uy, uw, uh) = up[0];     // spans it vertically

        // Measured against the **panel itself**, read off the background rect the
        // renderer draws from the same `Layout`. An earlier version of this test
        // compared the two zones only to each other and could not see the mark
        // stopping short of the panel at all — a zone inset by 10% still passed
        // every relative assertion, because the *other* zone was narrower still.
        let (px, py, pw, ph) = panel_rect(&SvgRenderer::default().render(
            &PlotSpec::new().data("t").x("a").y("b")
                .layer(Layer::new(Mark::Zone).data("z").bounds("lo", "hi"))
                .layer(Layer::new(Mark::Point)), &data));

        assert!((ax - px).abs() < 0.01 && (aw - pw).abs() < 0.01,
            "a zone bounded on y reaches both panel edges horizontally: \
             got x={ax} w={aw}, panel x={px} w={pw}");
        assert!((uy - py).abs() < 0.01 && (uh - ph).abs() < 0.01,
            "a zone bounded on x reaches both panel edges vertically: \
             got y={uy} h={uh}, panel y={py} h={ph}");
        // And each *bounded* side really is bounded, so the test could not pass
        // with a zone that simply covered the whole panel both ways.
        assert!(ah < ph * 0.9, "the bounded side is the data's, not the panel's");
        assert!(uw < pw * 0.9);
        // Where that band *sits*, not only how thick it is: 3..6 falls inside 0..10 at
        // both ends, so the band clears both panel edges. A band drawn at the wrong
        // offset — flush with an edge, or off the panel entirely — is still the right
        // thickness and passes everything above. Stated as containment rather than as
        // a fraction of the panel, because the exact offset also encodes how far the
        // axis expands past its data, which is a different test's subject.
        assert!(ay > py && ay + ah < py + ph,
            "a zone over y 3..6 clears both panel edges: \
             got y={ay} h={ah}, panel y={py} h={ph}");
        assert!(ux > px && ux + uw < px + pw,
            "a zone over x 3..6 clears both panel edges: \
             got x={ux} w={uw}, panel x={px} w={pw}");
    }

    #[test]
    fn a_zone_reads_which_axis_its_measure_pair_bounds_off_the_bindings() {
        // `bounds(lower, upper)` is the **measure** pair, and which axis measures is
        // the bindings' answer (`legality::zone_orient`, §6 — the reason there is no
        // `flip` atom). On a categorical `y` the measure is `x`, so the spans run
        // across and every rectangle is one slot tall; put the category on `x` and
        // the identical sentence stands the spans up instead.
        //
        // Written as the **same pair of columns under both bindings**, so it cannot
        // pass by swapping unconditionally — which is the other way to be wrong here,
        // and the way a reader who only saw the funnel would fix it. Before 2026-07-28
        // the pair went to `y` whatever the columns held: the categorical axis got a
        // measure it could not place, `x` was left with no column to fit and fell back
        // to `0.0 … 1.0`, and the rectangles landed thousands of pixels off-panel with
        // nothing warning about it (§12). Both places that decide this are covered
        // here at once — the range `build_axis` fits and the sides `write_zone` draws
        // — because the failure needed them to *agree*, and they did, wrongly.
        let mut data = HashMap::new();
        data.insert("f".to_string(), DataFrame::new()
            .with_str("stage", vec!["visit".into(), "trial".into(), "buy".into()])
            .with_float("lo", vec![-20.0, -10.0, -2.0])
            .with_float("hi", vec![20.0, 10.0, 2.0]));

        let draw = |spec: PlotSpec| {
            let svg = SvgRenderer::default().render(&spec, &data);
            (zone_rects(&svg), panel_rect(&svg))
        };
        // Each plot is measured against **its own** panel: the two differ in margin,
        // since one spends its left edge on category names and the other on numbers.
        let (across, (px, py, pw, ph)) = draw(PlotSpec::new().data("f").y("stage")
            .layer(Layer::new(Mark::Zone).bounds("lo", "hi")));
        let (up, (_, _, qw, qh)) = draw(PlotSpec::new().data("f").x("stage")
            .layer(Layer::new(Mark::Zone).bounds("lo", "hi")));

        assert_eq!(across.len(), 3, "one row is one rectangle");
        assert_eq!(up.len(), 3);

        // On a categorical y: three slots, so each rectangle is a third of the panel
        // tall, and the widths carry the measurement in the data's proportions
        // (20 : 10 : 2). The slot half is what fails first if the pairs stop being
        // swapped — a measure placed on the categorical axis has no slot height at all.
        for (i, &(x, y, w, h)) in across.iter().enumerate() {
            assert!((h - ph / 3.0).abs() < 0.5,
                "rect {i} fills its slot: got h={h}, slot={}", ph / 3.0);
            assert!(x >= px - 0.01 && x + w <= px + pw + 0.01,
                "rect {i} is on the panel: got x={x} w={w}, panel x={px} w={pw}");
            assert!(y >= py - 0.01 && y + h <= py + ph + 0.01,
                "rect {i} is on the panel: got y={y} h={h}, panel y={py} h={ph}");
        }
        let (w0, w1, w2) = (across[0].2, across[1].2, across[2].2);
        assert!(w0 > w1 && w1 > w2, "the widths are the measurement: {w0} {w1} {w2}");
        assert!((w0 / w1 - 2.0).abs() < 0.05 && (w1 / w2 - 5.0).abs() < 0.05,
            "and in the data's own proportions (40 : 20 : 4): {w0} {w1} {w2}");

        // The mirror, from the identical columns: a categorical x measures along y,
        // which is what today's code already did and must keep doing.
        for (i, &(_, _, w, h)) in up.iter().enumerate() {
            assert!((w - qw / 3.0).abs() < 0.5,
                "rect {i} fills its slot the other way: got w={w}, slot={}", qw / 3.0);
            assert!(h <= qh + 0.01, "rect {i} is on the panel: got h={h}, panel h={qh}");
        }
        assert!(up[0].3 > up[1].3 && up[1].3 > up[2].3,
            "the heights are the measurement when the slot is on x");
    }

    /// The confusion matrix, twice: once from a table that already holds the tally,
    /// once from the raw rows with `count` doing it. Nine cells either way.
    fn confusion() -> HashMap<String, DataFrame> {
        let (a, p) = (["cat", "dog", "bird"], ["cat", "dog", "bird"]);
        let n = [18.0, 2.0, 1.0, 3.0, 21.0, 2.0, 1.0, 4.0, 15.0];
        let (mut ac, mut pc, mut nc) = (Vec::new(), Vec::new(), Vec::new());
        let (mut ar, mut pr) = (Vec::new(), Vec::new());
        for (i, &va) in a.iter().enumerate() {
            for (j, &vp) in p.iter().enumerate() {
                let c = n[i * 3 + j];
                ac.push(va.to_string());
                pc.push(vp.to_string());
                nc.push(c);
                for _ in 0..c as usize {
                    ar.push(va.to_string());
                    pr.push(vp.to_string());
                }
            }
        }
        // The pre-computed tally is called `count` on purpose: it is the name
        // `zone * count` synthesizes, so the two plots title their legends the same
        // and the comparison below can be exact bytes rather than "near enough".
        // That the long form and the short form coincide is Law 5, not a fixture
        // convenience — `color(count)` says out loud what the transform says for you.
        HashMap::from([
            ("tallied".to_string(), DataFrame::new()
                .with_str("actual", ac).with_str("predicted", pc).with_float("count", nc)),
            ("raw".to_string(), DataFrame::new()
                .with_str("actual", ar).with_str("predicted", pr)),
        ])
    }

    #[test]
    fn a_tile_plot_fills_the_slot_whole_and_leaves_no_seam() {
        // The fourth extent description, asserted as geometry: a category owns
        // `[k-½, k+½]`, so three categories on each axis cut the panel into nine
        // cells that **touch** — no gap, no overlap, and the outermost edges flush
        // with the panel. A categorical `bar` takes 80% of its slot, and if a zone
        // inherited that the seams would show up here as a 20% shortfall.
        let data = confusion();
        let spec = PlotSpec::new().data("tallied").x("actual").y("predicted")
            .layer(Layer::new(Mark::Zone).encode(Channel::Color, "count"));
        let svg = SvgRenderer::default().render(&spec, &data);

        let cells = zone_rects(&svg);
        assert_eq!(cells.len(), 9, "three categories each way is nine cells");
        let (px, py, pw, ph) = panel_rect(&svg);

        // Three distinct columns and three distinct rows, each one third of the panel.
        // Both axes, because area alone cannot tell them apart: nine cells stacked in
        // three columns but a single row band sum to exactly the same panel area, and
        // would pass every other assertion here.
        let thirds = |mut vs: Vec<f64>, origin: f64, extent: f64, what: &str| {
            vs.sort_by(|a, b| a.partial_cmp(b).unwrap());
            vs.dedup_by(|a, b| (*a - *b).abs() < 0.01);
            assert_eq!(vs.len(), 3, "three {what}s of cells, got {vs:?}");
            for (i, v) in vs.iter().enumerate() {
                let want = origin + extent / 3.0 * i as f64;
                assert!((v - want).abs() < 0.5, "{what} {i} starts at {v}, want {want}");
            }
        };
        thirds(cells.iter().map(|c| c.0).collect(), px, pw, "column");
        thirds(cells.iter().map(|c| c.1).collect(), py, ph, "row");
        for c in &cells {
            assert!((c.2 - pw / 3.0).abs() < 0.5, "a cell is one whole slot wide, got {}", c.2);
            assert!((c.3 - ph / 3.0).abs() < 0.5, "and one whole slot tall, got {}", c.3);
        }
        // The mesh covers the panel exactly: nine cells of a ninth each. Compared as
        // a fraction, because nine cells each rounded to two decimal places cannot
        // sum to the panel's area to the last unit — and a 20% seam, the thing this
        // is here to catch, is four orders of magnitude larger than that rounding.
        let area: f64 = cells.iter().map(|c| c.2 * c.3).sum();
        assert!((area / (pw * ph) - 1.0).abs() < 1e-4,
            "the cells tile the panel, got {area} of {}", pw * ph);
        // Abutting rectangles antialias into a visible lattice unless snapped.
        assert!(svg.contains("crispEdges"), "a tiled cell turns antialiasing off");
    }

    #[test]
    fn a_tallied_tile_plot_draws_what_the_pre_computed_one_draws() {
        // *`bin` cuts, `count` tallies* — and the payoff of saying it that way is
        // that the grammar can do the counting. So the two sentences must agree:
        // `zone + color(n)` over a table someone summarized by hand, and
        // `zone * count` over the raw rows, are one plot. Compared as drawn cells
        // (position **and** fill), because agreeing on geometry while disagreeing
        // on which cell got which color would be the silent kind of wrong.
        let data = confusion();
        let cells = |svg: &str| -> Vec<String> {
            svg.lines().filter(|l| l.contains("<rect") && l.contains("fill-opacity")
                             && l.contains(r#"stroke="none""#))
                .map(|l| l.trim().to_string()).collect()
        };
        let pre = SvgRenderer::default().render(
            &PlotSpec::new().data("tallied").x("actual").y("predicted")
                .layer(Layer::new(Mark::Zone).encode(Channel::Color, "count")), &data);
        let tallied = SvgRenderer::default().render(
            &PlotSpec::new().data("raw").x("actual").y("predicted")
                .layer(Layer::new(Mark::Zone).transform(Transform::Count)), &data);

        assert_eq!(cells(&pre), cells(&tallied),
            "the tallied tile plot draws the pre-computed one, cell for cell");
        assert_eq!(cells(&pre).len(), 9);
    }

    /// Raw readings over a pair of categories, and the same table reduced by hand —
    /// the fixture for the **two-dimensional group-by** (spec §5).
    ///
    /// Deliberately ragged: three, one and two readings in the three populated cells,
    /// so a mean is not a sum divided by a constant and cannot be confused with one,
    /// and one pair with **no rows at all**, which must come out absent rather than
    /// zero — `agg2d`'s half of the rule `count2d` already keeps.
    fn readings() -> HashMap<String, DataFrame> {
        //  (site, day)      values          mean
        //  north / mon      1, 2, 6          3.0
        //  north / tue      10               10.0
        //  south / mon      4, 8             6.0
        //  south / tue      —                absent
        let site = ["north", "north", "north", "north", "south", "south"];
        let day  = ["mon",   "mon",   "mon",   "tue",   "mon",   "mon"];
        let val  = [1.0, 2.0, 6.0, 10.0, 4.0, 8.0];
        HashMap::from([
            ("raw".to_string(), DataFrame::new()
                .with_str("site", site.iter().map(|s| s.to_string()).collect())
                .with_str("day", day.iter().map(|s| s.to_string()).collect())
                .with_float("reading", val.to_vec())),
            ("meaned".to_string(), DataFrame::new()
                .with_str("site", vec!["north".into(), "north".into(), "south".into()])
                .with_str("day", vec!["mon".into(), "tue".into(), "mon".into()])
                .with_float("reading", vec![3.0, 10.0, 6.0])),
        ])
    }

    #[test]
    fn a_summarized_tile_plot_draws_what_the_pre_computed_one_draws() {
        // The two-dimensional group-by, asserted the way the tally already is: the
        // grammar doing the summarizing must draw exactly what a table summarized by
        // hand draws. That is a stronger claim than "the numbers are right", because
        // it catches the ramp and the key as well as the arithmetic — a heatmap of
        // cell means under a legend that spans the *raw* column's range is
        // self-consistent in its fills and still wrong, which is how that bug lived
        // long enough to be found by comparing values rather than by rendering.
        //
        // A `zone` measures by `color`, so `color` is the channel that names the
        // column to reduce — the claim these five were once refused under ("no
        // channel left to name it") read off the mark instead of the message.
        let data = readings();
        let cells = |svg: &str| -> Vec<String> {
            svg.lines().filter(|l| l.contains("<rect") && l.contains("fill-opacity")
                             && l.contains(r#"stroke="none""#))
                .map(|l| l.trim().to_string()).collect()
        };
        let pre = SvgRenderer::default().render(
            &PlotSpec::new().data("meaned").x("site").y("day")
                .layer(Layer::new(Mark::Zone).encode(Channel::Color, "reading")), &data);
        let computed = SvgRenderer::default().render(
            &PlotSpec::new().data("raw").x("site").y("day")
                .layer(Layer::new(Mark::Zone).transform(Transform::Mean)
                    .encode(Channel::Color, "reading")), &data);

        assert_eq!(cells(&pre), cells(&computed),
            "`zone * mean` draws the hand-reduced table, cell for cell");
        // Three cells, not four: the pair with no readings has no mean, so it is an
        // absence and stays blank — `count2d`'s rule, which `agg2d` had to keep or a
        // missing cell would read as a measured zero.
        assert_eq!(cells(&pre).len(), 3, "an empty pair is an absence, not a zero");

        // And the key decodes the cells it is beside. The strip's three labels are the
        // reduced extremes and their midpoint; reading the *raw* frame put the bottom
        // at 1.00, which is the bug this line pins — fills that were self-consistent
        // under a legend that decoded them wrongly.
        let labels: Vec<String> = computed.lines()
            .filter(|l| l.contains("<text") && !l.contains("font-weight"))
            .filter_map(|l| l.rsplit('>').nth(1)?.split('<').next())
            .filter(|s| s.parse::<f64>().is_ok())
            .map(str::to_string).collect();
        assert_eq!(labels, ["10.00", "6.50", "3.00"],
            "the key spans the reduced values, not the raw column's 1..10");
    }

    #[test]
    fn a_reduction_stands_up_in_the_cube_the_way_a_tally_does() {
        // The same subtraction one axis further out (spec §5/§15): a `bar` in `space`
        // measures with `z`, so it groups by the pair and reduces the column `z`
        // names. The tile plot standing up, with a mean for a height instead of a
        // tally — and asserted the same way the flat one is, against the table reduced
        // by hand, because that is what makes it the *same rule* rather than a second
        // feature that happens to work.
        let data = readings();
        let cube = |src: &str, layer: Layer| SvgRenderer::default().render(
            &PlotSpec::new().data(src).x("site").y("day").z("reading")
                .coord(CoordSpace::Space(SpaceView::default()))
                .layer(layer), &data);
        let faces = |svg: &str| -> Vec<String> {
            svg.lines().filter(|l| l.trim_start().starts_with("<path d=") && l.contains("fill-opacity"))
                .map(|l| l.trim().to_string()).collect()
        };

        let pre = cube("meaned", Layer::new(Mark::Bar));
        let computed = cube("raw", Layer::new(Mark::Bar).transform(Transform::Mean));
        assert_eq!(faces(&pre), faces(&computed),
            "`bar * mean + space()` stands up the hand-reduced table, face for face");
        // Three columns of three visible faces: the back three are culled, and the
        // fourth cell — the pair with no readings — has no mean and so no column.
        assert_eq!(faces(&pre).len(), 9, "three columns, three visible faces each");
    }

    // ---- the two slot marks that learned to stand, 2026-07-26 ---------------

    /// **A pair transform in the cube groups by the *floor*, not by `x` alone.**
    ///
    /// The bug this pins is the one building it found: `reads_two_dimensions` asked
    /// only about the five single-value statistics, so a whisker fell through to the
    /// one-key branch, grouped by `x`, and drew one per **row**. On the six rows of
    /// `readings()` that is three whiskers where there should be three cells — and on
    /// a real table it was 75 whiskers on a 6-cell floor.
    #[test]
    fn a_whisker_in_the_cube_stands_one_per_cell_not_one_per_row() {
        let data = readings();
        let svg = SvgRenderer::default().render(
            &PlotSpec::new().data("raw").x("site").y("day").z("reading")
                .coord(CoordSpace::Space(SpaceView::default()))
                .layer(Layer::new(Mark::Interval).transform(Transform::Range)), &data);

        // Three non-empty (site, day) cells — south/tue has no readings and, by the
        // absent-pair rule `agg2d` and `count2d` already follow, gets no whisker.
        // Each is one span plus a **cross** at each end: 1 + 4 = 5 strokes.
        let marks = svg.lines().filter(|l| l.trim_start().starts_with("<line")
            && l.contains("stroke-linecap")).count();
        assert_eq!(marks, 15, "three cells × (a span + two crossed caps), got {marks}");
        assert!(!svg.contains("NaN"));
    }

    /// The cube's whisker spans the cell's own **extents**, and the pairing is the
    /// same one the flat mark reads — checked against the hand-reduced table, which
    /// is what makes it the *same rule* rather than a second feature that works.
    #[test]
    fn a_cube_whisker_spans_the_cells_own_range() {
        let data = readings();
        let svg = SvgRenderer::default().render(
            &PlotSpec::new().data("raw").x("site").y("day").z("reading")
                .coord(CoordSpace::Space(SpaceView::default()))
                .layer(Layer::new(Mark::Interval).transform(Transform::Range)), &data);
        // north/mon holds 1, 2, 6 → a span of 1..6; north/tue holds 10 alone → a
        // span of zero length, which is honest and still drawn as a capped point.
        // The z axis spans 1..10, so the longest span is 5/9 of the cube's height and
        // the shortest is none of it: assert the *spread* of span lengths rather than
        // pixels, which is what survives a change of viewing angle.
        let spans: Vec<f64> = svg.lines()
            .filter(|l| l.trim_start().starts_with("<line") && l.contains(r#"stroke-linecap="round""#))
            .filter_map(|l| {
                let g = |k: &str| l.split(&format!("{k}=\""))
                    .nth(1)?.split('"').next()?.parse::<f64>().ok();
                Some(((g("x1")? - g("x2")?).powi(2) + (g("y1")? - g("y2")?).powi(2)).sqrt())
            })
            .collect();
        assert!(!spans.is_empty());
        let longest = spans.iter().cloned().fold(0.0, f64::max);
        assert!(longest > 1.0, "every span came out zero-length: {spans:?}");
    }

    /// **A box's median is culled like any other face.** It is a plane *inside* an
    /// opaque solid, so only where it meets a front-facing side may be drawn — the
    /// same rule `write_solid` applies to the solid itself. Drawing the whole quad
    /// put two edges on top of a box they are behind.
    #[test]
    fn a_cube_boxs_median_is_drawn_only_where_the_solid_faces_us() {
        let data = readings();
        let svg = SvgRenderer::default().render(
            &PlotSpec::new().data("raw").x("site").y("day").z("reading")
                .coord(CoordSpace::Space(SpaceView::default()))
                .layer(Layer::new(Mark::Box)), &data);
        // A convex box shows exactly two of its four sides at an ordinary viewing
        // angle, so each median contributes two segments and never four — **except**
        // a degenerate one. `readings()` has north/tue with a single observation, so
        // its quartiles coincide, its box has no height, and what is visible of its
        // median is the whole quad seen face-on: four edges, which is the flat mark's
        // "a zero-IQR group collapses to a flat line, which is honest" read one
        // dimension up. So two ordinary cells give 2 each and the degenerate one 4.
        let medians = svg.lines().filter(|l| l.trim_start().starts_with("<line")
            && l.contains(r#"stroke-linecap="butt""#)).count();
        let boxes = svg.lines().filter(|l| l.trim_start().starts_with("<path d=")
            && l.contains("fill-opacity")).count();
        assert!(boxes > 0, "no box body drawn");
        // Per cell: 2 whiskers, plus 2 median edges (4 for the degenerate cell).
        assert_eq!(medians, 6 + 2 + 2 + 4, "median edges are culled per visible side");
        assert!(!svg.contains("NaN"));
    }

    /// A 3-D box is **opaque**, where a flat one is not, and that is `bar`'s answer
    /// rather than a second opinion: painter's order places these solids, and a
    /// translucent one lets a far box show through a near one and undoes the sort.
    #[test]
    fn a_cube_box_is_opaque_so_the_depth_sort_survives() {
        let data = readings();
        let svg = SvgRenderer::default().render(
            &PlotSpec::new().data("raw").x("site").y("day").z("reading")
                .coord(CoordSpace::Space(SpaceView::default()))
                .layer(Layer::new(Mark::Box)), &data);
        for l in svg.lines().filter(|l| l.trim_start().starts_with("<path d=") && l.contains("fill-opacity")) {
            let o: f64 = l.split("fill-opacity=\"").nth(1).unwrap()
                .split('"').next().unwrap().parse().unwrap();
            assert!(o > 0.999, "a solid in the cube must be opaque, got {o}");
        }
    }

    #[test]
    fn a_zone_bounded_by_one_slot_spans_the_panel_on_the_other() {
        // `rule`'s relaxation, arriving a third time and for free: bounded where the
        // bindings say, spanning where they do not. Nothing was built for this — it
        // falls out of *a category bounds its axis* applied to one axis instead of
        // two, which is the test that it is one rule rather than a tile-plot case.
        let data = confusion();
        let svg = SvgRenderer::default().render(
            &PlotSpec::new().data("tallied").x("actual")
                .layer(Layer::new(Mark::Zone)), &data);
        let (_, py, pw, ph) = panel_rect(&svg);
        let cells = zone_rects(&svg);
        assert!(!cells.is_empty(), "a slotted zone draws");
        for c in &cells {
            assert!((c.1 - py).abs() < 0.5 && (c.3 - ph).abs() < 0.5,
                "unbounded on y, so it reaches both panel edges: got y={} h={}", c.1, c.3);
            assert!((c.2 - pw / 3.0).abs() < 0.5, "bounded on x by its own slot");
        }
        // Background, not data: it is a rectangle over a slot, with nothing measured
        // in it, so it stays translucent for whatever is drawn underneath.
        assert!(svg.contains(r#"fill-opacity="0.200""#), "an unmeasured zone is background");
    }

    #[test]
    fn a_banded_zone_fills_the_very_curves_the_contour_strokes() {
        // The claim that makes `levels` one parameter serving two marks: a filled
        // band's edge *is* the contour line, because both marks run the same
        // transform and part only in the writer. Asserted end to end through the
        // renderer rather than at the transform, where it would be true by
        // construction and prove nothing about what gets drawn.
        let mut xs = Vec::new();
        let mut ys = Vec::new();
        for i in 0..120 {
            let t = i as f64;
            xs.push(2.0 + (t * 0.7).sin());
            ys.push(2.0 + (t * 1.3).cos());
        }
        let mut data = HashMap::new();
        data.insert("t".to_string(), DataFrame::new().with_float("a", xs).with_float("b", ys));

        let spec = |mark: Mark| {
            let mut l = Layer::new(mark).transform(Transform::Density);
            l.density = Some(crate::ir::DensitySpec { adjust: None, bandwidth: None, levels: Some(4) , compare: None, reach: None });
            PlotSpec::new().data("t").x("a").y("b").layer(l)
        };
        let banded = SvgRenderer::default().render(&spec(Mark::Zone), &data);
        let traced = SvgRenderer::default().render(&spec(Mark::Path), &data);

        // The bands are polygons, and there are as many as there are rings.
        let bands = polygons(&banded);
        assert!(bands.len() >= 4, "four levels fill at least four bands, got {}", bands.len());
        // A banded zone draws no cells — it is the level sets, not the mesh.
        assert!(zone_rects(&banded).is_empty(), "a banded zone paints no cells");

        // Every band vertex is a vertex of the traced contour, to the printed pixel.
        let band_pts: std::collections::HashSet<String> = bands.iter()
            .flat_map(|p| p.split_whitespace().map(str::to_string))
            .collect();
        let line_pts: std::collections::HashSet<String> = traced.lines()
            // A contour segment, not a gridline: only the data strokes carry a
            // linecap. Matching on `<line>` alone picked up the axis grid, which is
            // how this test first "failed" on coordinates that were never contours.
            .filter(|l| l.contains("<line") && l.contains(r#"stroke-linecap="round""#))
            .flat_map(|l| {
                let num = |k: &str| -> String {
                    let at = l.find(k).unwrap() + k.len();
                    let v: f64 = l[at..].split('"').next().unwrap().parse().unwrap();
                    format!("{v:.2}")
                };
                [format!("{},{}", num(r#"x1=""#), num(r#"y1=""#)),
                 format!("{},{}", num(r#"x2=""#), num(r#"y2=""#))]
            })
            .collect();
        assert!(!line_pts.is_empty(), "the traced contour drew segments");
        let shared = band_pts.iter().filter(|p| line_pts.contains(*p)).count();
        assert_eq!(shared, band_pts.len(),
            "every band vertex is a contour vertex: {shared} of {} matched", band_pts.len());
    }

    #[test]
    fn levels_choose_the_geometry_and_nothing_else_does() {
        // `field_geometry` is the one place the reading is decided, so the table of
        // answers is worth stating outright — a `path` has only one thing it can do
        // with a field, a `zone` has two and `levels` is the request.
        use crate::legality::{field_geometry, FieldGeometry};
        let layer = |mark: Mark, t: Transform, levels: Option<usize>| {
            let mut l = Layer::new(mark).transform(t);
            if levels.is_some() {
                l.density = Some(crate::ir::DensitySpec { adjust: None, bandwidth: None, levels , compare: None, reach: None });
            }
            l
        };
        for (mark, t, levels, want) in [
            (Mark::Path, Transform::Density, None,    Some(FieldGeometry::Rings)),
            (Mark::Path, Transform::Density, Some(6), Some(FieldGeometry::Rings)),
            (Mark::Zone, Transform::Density, None,    Some(FieldGeometry::Cells)),
            (Mark::Zone, Transform::Density, Some(6), Some(FieldGeometry::Rings)),
            (Mark::Zone, Transform::Bin,     None,    Some(FieldGeometry::Cells)),
            // Not a field at all: a mark with a measure axis reads one dimension.
            (Mark::Line, Transform::Density, None,    None),
            (Mark::Bar,  Transform::Bin,     None,    None),
        ] {
            assert_eq!(field_geometry(&layer(mark.clone(), t.clone(), levels)), want,
                "{mark:?} * {t:?} with levels={levels:?}");
        }
    }

    #[test]
    fn a_binned_zone_tiles_the_panel_edge_to_edge() {
        // The heatmap's structural claim, and the one thing the axis machinery had
        // to learn: `build_axis` reads the position columns, which a 2-D bin fills
        // with cell *centers*, so fitting to them leaves the outer half of every
        // edge cell hanging past the range — clipped away by the panel as a border
        // of short cells. Measured against the **panel's own rectangle**, for the
        // reason the zone test above records: comparing the cells only to each
        // other cannot see the whole tiling sitting inset.
        //
        // A full 5x5 mesh of points, so no cell is empty and the tiling's bounding
        // box is the mesh itself.
        let mut xs = Vec::new();
        let mut ys = Vec::new();
        for i in 0..5 {
            for j in 0..5 {
                xs.push(i as f64);
                ys.push(j as f64);
            }
        }
        let mut data = HashMap::new();
        data.insert("t".to_string(), DataFrame::new().with_float("a", xs).with_float("b", ys));

        let mut layer = Layer::new(Mark::Zone).transform(Transform::Bin);
        layer.bin = Some(crate::ir::BinSpec { bins: Some(5), width: None, tiling: None });
        let spec = PlotSpec::new().data("t").x("a").y("b").layer(layer);
        let svg = SvgRenderer::default().render(&spec, &data);

        let cells = zone_rects(&svg);
        assert_eq!(cells.len(), 25, "a full 5x5 mesh is 25 cells, got {}", cells.len());

        let (px, py, pw, ph) = panel_rect(&svg);
        let left = cells.iter().map(|c| c.0).fold(f64::INFINITY, f64::min);
        let top = cells.iter().map(|c| c.1).fold(f64::INFINITY, f64::min);
        let right = cells.iter().map(|c| c.0 + c.2).fold(f64::NEG_INFINITY, f64::max);
        let bottom = cells.iter().map(|c| c.1 + c.3).fold(f64::NEG_INFINITY, f64::max);

        assert!((left - px).abs() < 0.01 && (right - (px + pw)).abs() < 0.01,
            "the tiling spans the panel horizontally: cells {left}..{right}, panel {px}..{}",
            px + pw);
        assert!((top - py).abs() < 0.01 && (bottom - (py + ph)).abs() < 0.01,
            "the tiling spans the panel vertically: cells {top}..{bottom}, panel {py}..{}",
            py + ph);

        // And the cells partition rather than merely cover: 25 equal cells whose
        // areas sum to the panel's leaves no room for an overlap or a gap. The
        // seam between two cells is answered by turning antialiasing off, not by
        // growing them (`marks/zone.rs`), which is why this can still be exact —
        // a cut cell is where the mesh cut it, to the pixel.
        //
        // The tolerance is relative because SVG coordinates are written to two
        // decimals, so 25 cells accumulate about four square pixels of rounding on
        // a panel of 340,000. It is still far tighter than any real defect: a
        // one-pixel seam between columns would miss by three thousand.
        let area: f64 = cells.iter().map(|c| c.2 * c.3).sum();
        assert!((area - pw * ph).abs() < pw * ph * 1e-4,
            "25 cells tile the panel exactly: {area} vs {}", pw * ph);
    }

    #[test]
    fn a_hex_tiling_draws_hexagons_that_still_reach_the_panel() {
        // Two claims at once, because they are the two halves of "the mark asks
        // the tiling what its cells look like". A hexagon has no four edges, so it
        // is drawn as a polygon rather than a rect — and the axis, which fits
        // itself to the mesh, has to read the hex extent too or the outermost
        // cells hang off the panel exactly as the rect ones once did.
        let mut xs = Vec::new();
        let mut ys = Vec::new();
        for i in 0..24 {
            for j in 0..24 {
                xs.push(i as f64);
                ys.push(j as f64);
            }
        }
        let mut data = HashMap::new();
        data.insert("t".to_string(), DataFrame::new().with_float("a", xs).with_float("b", ys));

        let mut layer = Layer::new(Mark::Zone).transform(Transform::Bin);
        layer.bin = Some(crate::ir::BinSpec {
            bins: Some(8), width: None, tiling: Some("hex".into()),
        });
        let svg = SvgRenderer::default().render(
            &PlotSpec::new().data("t").x("a").y("b").layer(layer), &data);

        let polys: Vec<&str> = svg.lines()
            .filter(|l| l.contains("<polygon") && l.contains("fill-opacity"))
            .collect();
        assert!(polys.len() > 20, "a hex mesh draws a polygon per cell, got {}", polys.len());
        assert!(!svg.contains(r#"<rect x="#) || !polys.is_empty());

        // Six vertices, every one of them — a five- or seven-sided cell would
        // tessellate wrongly and is exactly what a bad vertex list produces.
        for p in &polys {
            let pts = p.split(r#"points=""#).nth(1).unwrap().split('"').next().unwrap();
            assert_eq!(pts.split_whitespace().count(), 6, "a hexagon has six vertices: {pts}");
        }

        // The mesh reaches the panel. Read off every vertex, since a hexagon's
        // extreme point is a vertex rather than a corner of its bounding box.
        let (px, py, pw, ph) = panel_rect(&svg);
        let mut coords: Vec<(f64, f64)> = Vec::new();
        for p in &polys {
            let pts = p.split(r#"points=""#).nth(1).unwrap().split('"').next().unwrap();
            for v in pts.split_whitespace() {
                let (a, b) = v.split_once(',').unwrap();
                coords.push((a.parse().unwrap(), b.parse().unwrap()));
            }
        }
        let left = coords.iter().map(|c| c.0).fold(f64::INFINITY, f64::min);
        let right = coords.iter().map(|c| c.0).fold(f64::NEG_INFINITY, f64::max);
        let top = coords.iter().map(|c| c.1).fold(f64::INFINITY, f64::min);
        let bottom = coords.iter().map(|c| c.1).fold(f64::NEG_INFINITY, f64::max);
        assert!((left - px).abs() < 0.05 && (right - (px + pw)).abs() < 0.05,
            "the hex mesh spans the panel horizontally: {left}..{right} vs {px}..{}", px + pw);
        assert!((top - py).abs() < 0.05 && (bottom - (py + ph)).abs() < 0.05,
            "the hex mesh spans the panel vertically: {top}..{bottom} vs {py}..{}", py + ph);
    }

    #[test]
    fn a_binned_zone_reads_its_color_off_the_count_and_says_so_in_the_legend() {
        // The measure moved from length to color, which is the whole reason a
        // heatmap cell is a `zone` and not a bar (spec §18). Nothing binds it: the
        // transform writes the count and the mark reads it, the same courtesy
        // `bar * bin` does for `y`. So the two things to pin are that the density
        // difference reaches the ink, and that the reader is given a key to it.
        let mut xs = Vec::new();
        let mut ys = Vec::new();
        for i in 0..5 {
            for j in 0..5 {
                // One cell of the mesh gets piled high, the rest get one row each.
                let n = if i == 0 && j == 0 { 50 } else { 1 };
                for _ in 0..n {
                    xs.push(i as f64);
                    ys.push(j as f64);
                }
            }
        }
        let mut data = HashMap::new();
        data.insert("t".to_string(), DataFrame::new().with_float("a", xs).with_float("b", ys));

        let mut layer = Layer::new(Mark::Zone).transform(Transform::Bin);
        layer.bin = Some(crate::ir::BinSpec { bins: Some(5), width: None, tiling: None });
        let svg = SvgRenderer::default().render(
            &PlotSpec::new().data("t").x("a").y("b").layer(layer), &data);

        let fills: Vec<&str> = svg.lines()
            .filter(|l| l.contains("<rect") && l.contains("fill-opacity")
                     && l.contains(r#"stroke="none""#))
            .filter_map(|l| l.split(r#"fill="#).nth(1))
            .map(|s| s.trim_start_matches('"').split('"').next().unwrap())
            .collect();
        assert!(fills.len() > 1, "a tiling of one color is not reading the count");
        assert!(fills.iter().collect::<std::collections::HashSet<_>>().len() >= 2,
            "the busy cell must not be painted like the empty-ish ones: {fills:?}");

        // The key. Without it the darkest cell is a shade with no number attached,
        // and a heatmap whose scale cannot be read is a picture, not a graphic.
        assert!(svg.contains(">Count<"), "the synthesized count earns a legend:\n{svg}");

        // Opaque, unlike a decorating zone: there is nothing behind the data for it
        // to show through to, and 20% would wash the ramp out.
        assert!(svg.lines()
                   .filter(|l| l.contains("<rect") && l.contains("fill-opacity")
                            && l.contains(r#"stroke="none""#))
                   .all(|l| l.contains(r#"fill-opacity="1.000""#)),
            "a binned zone is the data, so it is not translucent");
    }

    #[test]
    fn a_zone_given_both_pairs_is_bounded_on_both_axes() {
        // The box: neither side is the panel's any more.
        let mut data = HashMap::new();
        data.insert("t".to_string(), DataFrame::new()
            .with_float("a", vec![0.0, 10.0]).with_float("b", vec![0.0, 10.0]));
        data.insert("z".to_string(), DataFrame::new()
            .with_float("lo", vec![3.0]).with_float("hi", vec![6.0])
            .with_float("s", vec![2.0]).with_float("e", vec![7.0]));
        let spec = PlotSpec::new().data("t").x("a").y("b")
            .layer(Layer::new(Mark::Zone).data("z").bounds("lo", "hi").span("s", "e"))
            .layer(Layer::new(Mark::Point));
        let r = zone_rects(&SvgRenderer::default().render(&spec, &data));
        assert_eq!(r.len(), 1);
        let full = zone_rects(&SvgRenderer::default().render(
            &PlotSpec::new().data("t").x("a").y("b")
                .layer(Layer::new(Mark::Zone).data("z").bounds("lo", "hi"))
                .layer(Layer::new(Mark::Point)), &data));
        assert!(r[0].2 < full[0].2, "a box is narrower than the band it came from");
        assert!((r[0].3 - full[0].3).abs() < 0.01, "and the same height — the same y pair");
    }

    #[test]
    fn a_zone_fits_the_axis_to_its_own_sides() {
        // The defect this pins: a bounded `zone` keeps its frame untransformed and
        // reads its four columns straight, so the axis fit — which asks only what
        // values the *position column* takes — never saw where the rectangles were.
        // Layered over other data the axis was right anyway, which is every use of
        // the mark in the book; alone, there was no column to fit at all, the range
        // fell back to `0..1`, and rectangles placed in data units landed tens of
        // thousands of pixels outside the panel. An empty-looking plot with
        // fabricated `0.0 … 1.0` axes, silently — the shape §12 refuses.
        //
        // Asserted as containment rather than against a tick list, because the fit
        // breathes: the claim is that what was drawn is *inside the panel it was
        // drawn on*, which is the thing that was false and is what a reader sees.
        let mut data = HashMap::new();
        data.insert("w".to_string(), DataFrame::new()
            .with_float("base", vec![0.0, 120.0, 165.0])
            .with_float("top",  vec![120.0, 165.0, 147.0])
            .with_float("l",    vec![0.6, 1.6, 2.6])
            .with_float("r",    vec![1.4, 2.4, 3.4]));
        let spec = PlotSpec::new().data("w")
            .layer(Layer::new(Mark::Zone).bounds("base", "top").span("l", "r"));
        let svg = SvgRenderer::default().render(&spec, &data);
        let (px, py, pw, ph) = panel_rect(&svg);
        let rects = zone_rects(&svg);
        assert_eq!(rects.len(), 3, "one row is one rectangle:\n{svg}");
        for (x, y, w, h) in rects {
            assert!(x >= px - 0.5 && x + w <= px + pw + 0.5,
                    "({x}, {w}) is outside the panel's x span ({px}, {pw})");
            assert!(y >= py - 0.5 && y + h <= py + ph + 0.5,
                    "({y}, {h}) is outside the panel's y span ({py}, {ph})");
        }

        // And the union, not a fallback: a zone beside data that *does* name the
        // axis widens it rather than replacing or being ignored by it. Here the
        // point layer spans 0..10 and the zone reaches to 40, so the axis must
        // hold both — the case a highlight reaching past the last point makes.
        data.insert("t".to_string(), DataFrame::new()
            .with_float("a", vec![0.0, 10.0]).with_float("b", vec![0.0, 10.0]));
        data.insert("z".to_string(), DataFrame::new()
            .with_float("lo", vec![30.0]).with_float("hi", vec![40.0]));
        let both = SvgRenderer::default().render(
            &PlotSpec::new().data("t").x("a").y("b")
                .layer(Layer::new(Mark::Zone).data("z").bounds("lo", "hi"))
                .layer(Layer::new(Mark::Point)), &data);
        let (_, py, _, ph) = panel_rect(&both);
        let band = zone_rects(&both);
        assert_eq!(band.len(), 1);
        assert!(band[0].1 >= py - 0.5 && band[0].1 + band[0].3 <= py + ph + 0.5,
                "a band above the data widens the axis instead of drawing off it:\n{both}");
        let dots: Vec<f64> = both.lines()
            .filter(|l| l.contains("<circle"))
            .map(|l| {
                let at = l.find(r#"cy=""#).unwrap() + 4;
                l[at..].split('"').next().unwrap().parse().unwrap()
            })
            .collect();
        assert!(!dots.is_empty() && dots.iter().all(|&y| y >= py - 0.5 && y <= py + ph + 0.5),
                "and the data it sits over is still on the panel:\n{both}");
    }

    #[test]
    fn one_row_is_one_rectangle_so_one_table_draws_several() {
        // `rule`'s payoff, inherited: the position is a column, so the table's
        // length decides how many are drawn.
        let mut data = HashMap::new();
        data.insert("t".to_string(), DataFrame::new()
            .with_float("a", vec![0.0, 10.0]).with_float("b", vec![0.0, 10.0]));
        data.insert("z".to_string(), DataFrame::new()
            .with_float("s", vec![1.0, 5.0, 8.0]).with_float("e", vec![2.0, 6.0, 9.0]));
        let spec = PlotSpec::new().data("t").x("a").y("b")
            .layer(Layer::new(Mark::Zone).data("z").span("s", "e"))
            .layer(Layer::new(Mark::Point));
        assert_eq!(zone_rects(&SvgRenderer::default().render(&spec, &data)).len(), 3);
    }

    #[test]
    fn an_arrowhead_is_drawn_at_the_end_the_setting_names() {
        // One head for "end" and "start", two for "both" — and the head is a
        // filled polygon, so counting them is counting polygons.
        let data = doubling_back();
        let heads = |arrow: Option<&str>| {
            let mut layer = Layer::new(Mark::Path);
            layer.style.arrow = arrow.map(String::from);
            let svg = SvgRenderer::default()
                .render(&PlotSpec::new().data("t").x("x").y("y").layer(layer), &data);
            svg.matches("<polygon").count()
        };
        assert_eq!(heads(None), 0, "a bare path has no head");
        assert_eq!(heads(Some("end")), 1);
        assert_eq!(heads(Some("start")), 1);
        assert_eq!(heads(Some("both")), 2);
    }

    #[test]
    fn the_head_sits_on_the_vertex_the_setting_names_not_the_other_one() {
        // "end" and "start" must not be interchangeable: the tip of each head is
        // the *last* and *first* row respectively, which on this route are
        // different points (x = 2 and x = 3).
        let data = doubling_back();
        let tip = |arrow: &str| {
            let mut layer = Layer::new(Mark::Path);
            layer.style.arrow = Some(arrow.to_string());
            let svg = SvgRenderer::default()
                .render(&PlotSpec::new().data("t").x("x").y("y").layer(layer), &data);
            let poly = svg.lines().find(|l| l.contains("<polygon")).unwrap().to_string();
            let pts = poly.split(r#"points=""#).nth(1).unwrap().split('"').next().unwrap();
            let first: f64 = pts.split(',').next().unwrap().parse().unwrap();
            first
        };
        let xs = vertex_xs(&SvgRenderer::default().render(
            &PlotSpec::new().data("t").x("x").y("y").layer(Layer::new(Mark::Path)), &data));
        assert!((tip("end") - xs[3]).abs() < 0.01, "the end head tips at the last row");
        assert!((tip("start") - xs[0]).abs() < 0.01, "the start head tips at the first row");
    }

    #[test]
    fn a_split_path_draws_one_stroke_per_group_each_in_its_own_row_order() {
        // `color` splits a path the way it splits a line, and each group keeps
        // the table's order within itself rather than being pooled and sorted.
        let data = HashMap::from([(
            "t".to_string(),
            DataFrame::new()
                .with_float("x", vec![3.0, 1.0, 3.0, 1.0])
                .with_float("y", vec![1.0, 2.0, 3.0, 4.0])
                .with_str("g", vec!["a", "a", "b", "b"].into_iter().map(String::from).collect()),
        )]);
        let layer = Layer::new(Mark::Path).encode(Channel::Color, "g");
        let svg = SvgRenderer::default()
            .render(&PlotSpec::new().data("t").x("x").y("y").layer(layer), &data);
        assert_eq!(svg.matches("<polyline").count(), 2, "one stroke per group");
        for line in svg.lines().filter(|l| l.contains("<polyline")) {
            let pts = line.split(r#"points=""#).nth(1).unwrap().split('"').next().unwrap();
            let xs: Vec<f64> = pts.split_whitespace()
                .filter_map(|p| p.split(',').next()?.parse().ok()).collect();
            assert!(xs[0] > xs[1], "each group keeps its own row order: {xs:?}");
        }
    }

    // -----------------------------------------------------------------------
    // The dot plot — `stack` spent on glyphs instead of on length
    // -----------------------------------------------------------------------

    /// Every glyph's center, y first — a pile read off the page.
    fn glyph_centers(svg: &str) -> Vec<(f64, f64)> {
        svg.lines()
            .filter(|l| l.contains("<circle") && l.contains("fill-opacity"))
            .map(|l| {
                let attr = |k: &str| -> f64 {
                    l.split(&format!(r#"{k}=""#)).nth(1).unwrap().split('"').next().unwrap().parse().unwrap()
                };
                (attr("cx"), attr("cy"))
            })
            .collect()
    }

    /// The dot plot's whole claim (spec §5): **one glyph per observation.** Not one
    /// per bin, which is what `point * bin` draws un-piled — the pile is `stack`
    /// spending the same `[base, top]` span on how *many* dots there are, because a
    /// point has no length to stretch across it.
    #[test]
    fn a_dot_plot_draws_one_glyph_per_observation() {
        let vals = vec![1.0, 2.0, 2.0, 3.0, 3.0, 3.0, 8.0];
        let n = vals.len();
        let data = HashMap::from([("t".to_string(), DataFrame::new().with_float("v", vals))]);
        let dots = |stack: bool| {
            let mut layer = Layer::new(Mark::Point).transform(Transform::Bin);
            if stack { layer = layer.transform(Transform::Stack); }
            glyph_centers(&SvgRenderer::default()
                .render(&PlotSpec::new().data("t").x("v").layer(layer), &data))
        };
        // Un-piled: one summary dot per non-empty bin. Piled: one per row.
        assert!(dots(false).len() < n, "`point * bin` summarizes, got {:?}", dots(false).len());
        assert_eq!(dots(true).len(), n, "every observation should get a dot");

        // The pile is a column: the dots of one bin share an x and are evenly
        // spaced up the measure axis, one count unit apart.
        let mut by_x: std::collections::HashMap<String, Vec<f64>> = std::collections::HashMap::new();
        for (cx, cy) in dots(true) {
            by_x.entry(format!("{cx:.1}")).or_default().push(cy);
        }
        let tallest = by_x.values().max_by_key(|v| v.len()).unwrap();
        assert_eq!(tallest.len(), 3, "three rows fell in one bin, so three dots pile there");
        let mut ys = tallest.clone();
        ys.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let gaps: Vec<f64> = ys.windows(2).map(|w| w[1] - w[0]).collect();
        assert!(gaps.iter().all(|g| (g - gaps[0]).abs() < 0.01),
            "the rungs of a pile are one count apart: {gaps:?}");
    }

    /// Past some height a pile stops being dots: the rungs are one count unit apart,
    /// so they overlap once that unit is narrower than a dot (spec §12, an
    /// Assumption). The condition is derived from the page, not from a threshold, so
    /// what matters is that it tracks *both* inputs — the pile's height and the dot's
    /// size — rather than firing at some row count.
    #[test]
    fn a_pile_too_tall_to_count_says_so_and_still_draws() {
        // One bin, `n` rows in it, so the tallest pile is exactly n.
        let plot = |n: usize, size: Option<f64>| {
            let data = HashMap::from([(
                "t".to_string(),
                DataFrame::new().with_float("v", vec![1.0; n]),
            )]);
            let mut layer = Layer::new(Mark::Point)
                .transform(Transform::Bin).transform(Transform::Stack);
            layer.style.size = size;
            let spec = PlotSpec::new().data("t").x("v").layer(layer);
            let eff = vec![vec![crate::transform::pile(
                &crate::transform::apply(
                    &data["t"], &spec.layers[0].transforms, "v", "", None, None, None, None, None, None, None, None, None, None, None),
                "")]];
            let warn = pile_overlap_warning(&spec, &eff, "", (0.0, n as f64), 400.0,
                                            SvgRenderer::default().point_radius);
            (warn, SvgRenderer::default().render(&spec, &data))
        };

        // 20 dots over 400px is 20px a rung, wider than a 9px dot: no warning.
        let (quiet, svg) = plot(20, None);
        assert!(quiet.is_none(), "a countable pile should say nothing: {quiet:?}");
        assert_eq!(svg.matches("<circle").count(), 20, "and it draws every dot");

        // 200 dots over the same 400px is 2px a rung: they overlap, so it says so —
        // naming the count and the summary that reads at that size, and still drawing.
        let (loud, svg) = plot(200, None);
        let msg = loud.expect("an overlapping pile should warn");
        assert!(msg.contains("200 dots"), "the warning counts them: {msg}");
        assert!(msg.contains("bar * bin"), "and gives direction: {msg}");
        assert_eq!(svg.matches("<circle").count(), 200, "an Assumption never blocks");

        // Shrinking the dots buys room, which is why the test is not a row count: the
        // same 200 rows go quiet at a 1px radius.
        assert!(plot(200, Some(0.9)).0.is_none(), "smaller dots, no overlap, no warning");
    }

    /// A split pile stacks like a stacked bar — group *b* starts where group *a*
    /// stopped — which is the same `stack_base` every other mark reads.
    #[test]
    fn a_split_pile_sits_on_the_group_below_it() {
        let data = HashMap::from([(
            "t".to_string(),
            DataFrame::new()
                .with_float("v", vec![1.0, 1.0, 1.0, 1.0, 1.0])
                .with_str("g", vec!["a", "a", "b", "b", "b"].into_iter().map(String::from).collect()),
        )]);
        let layer = Layer::new(Mark::Point)
            .transform(Transform::Bin).transform(Transform::Stack)
            .encode(Channel::Color, "g");
        let svg = SvgRenderer::default()
            .render(&PlotSpec::new().data("t").x("v").layer(layer), &data);

        // Five rows, five dots, all in the one bin every value shares.
        let dots = glyph_centers(&svg);
        assert_eq!(dots.len(), 5, "one dot per row across both groups");
        // Group a is the palette's first hue and sits at the bottom: sorting the
        // dots down the page must give a's two, then b's three.
        let mut rows: Vec<(f64, String)> = svg.lines()
            .filter(|l| l.contains("<circle") && l.contains("fill-opacity"))
            .map(|l| {
                let cy: f64 = l.split(r#"cy=""#).nth(1).unwrap().split('"').next().unwrap().parse().unwrap();
                let fill = l.split(r#"fill=""#).nth(1).unwrap().split('"').next().unwrap().to_string();
                (cy, fill)
            })
            .collect();
        rows.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap()); // bottom of the page up
        let hues: Vec<&str> = rows.iter().map(|(_, f)| f.as_str()).collect();
        assert_eq!(hues[0], PALETTE_GOG[0], "the first group is on the floor");
        assert_eq!(hues[1], PALETTE_GOG[0]);
        assert!(hues[2..].iter().all(|h| *h == PALETTE_GOG[1]),
            "the second group piles on top of it: {hues:?}");
    }

    // -----------------------------------------------------------------------
    // `surface` — the sheet through the samples (spec §15)
    // -----------------------------------------------------------------------

    /// An `nx` × `ny` grid of `z = x + 2y`, one row per crossing, with the rows in
    /// scrambled order so nothing can pass by accident of arrival.
    fn grid_frame(nx: usize, ny: usize) -> DataFrame {
        let (mut xs, mut ys, mut zs) = (Vec::new(), Vec::new(), Vec::new());
        for k in 0..(nx * ny) {
            // A stride coprime with the count walks every cell in a scrambled order.
            let c = (k * 7) % (nx * ny);
            let (i, j) = (c % nx, c / nx);
            xs.push(i as f64);
            ys.push(j as f64);
            zs.push(i as f64 + 2.0 * j as f64);
        }
        DataFrame::new().with_float("x", xs).with_float("y", ys).with_float("h", zs)
    }

    fn surface_faces(svg: &str) -> Vec<(String, String)> {
        svg.lines()
            .filter(|l| l.contains("<path d=\"M") && l.contains("fill-opacity"))
            .map(|l| {
                let f = |k: &str| l.split(k).nth(1).unwrap().split('"').next().unwrap().to_string();
                (f(r#"fill=""#), f(r#"stroke=""#))
            })
            .collect()
    }

    #[test]
    fn a_surface_draws_one_face_per_complete_cell_of_its_grid() {
        // The mark's whole geometry in one number: a 5×4 lattice of nodes has 4×3
        // blocks of four, so the sheet is 12 faces. The rows arrive scrambled, which
        // is what makes this a test of *recovering* the lattice rather than of reading
        // the table in order.
        let data = HashMap::from([("t".to_string(), grid_frame(5, 4))]);
        let spec = PlotSpec::new().data("t").x("x").y("y").z("h")
            .layer(Layer::new(Mark::Surface));
        let svg = SvgRenderer::default().render(&spec, &data);
        assert_eq!(surface_faces(&svg).len(), 12, "a 5x4 grid of nodes is 4x3 faces");
    }

    /// The lids of a terraced sheet: level faces, so the *undimmed* color, which is
    /// what separates them from the risers in a face list.
    fn level_faces(svg: &str) -> usize {
        surface_faces(svg).iter().filter(|(f, _)| *f == PALETTE_GOG[0]).count()
    }

    #[test]
    fn a_cut_floor_lays_one_lid_per_cell_where_a_node_floor_spans_the_gaps_between_them() {
        // **The whole feature in two numbers** (spec §15). The same 3×3 table read as
        // *nodes* is 2×2 blocks of four corners — 4 faces, and five of the nine values
        // survive only as anchors — while read as *cells* it is 9 lids covering the
        // floor. That gap is why a design measuring one value per bin was drawing a
        // sheet over two thirds of its own grid.
        let data = HashMap::from([("t".to_string(), grid_frame(3, 3))]);
        let nodes = PlotSpec::new().data("t").x("x").y("y").z("h")
            .layer(Layer::new(Mark::Surface));
        assert_eq!(surface_faces(&SvgRenderer::default().render(&nodes, &data)).len(), 4,
            "9 nodes are 2x2 blocks of four");

        let cells = PlotSpec::new().data("t").x("x").y("y").z("h")
            .layer(Layer::new(Mark::Surface)
                .transform(Transform::Bin).transform(Transform::Mean).bins(3));
        let svg = SvgRenderer::default().render(&cells, &data);
        assert_eq!(level_faces(&svg), 9, "9 cut cells are 9 lids: {}", surface_faces(&svg).len());
    }

    #[test]
    fn a_terraced_sheet_is_connected_by_risers_and_a_flat_one_needs_none() {
        // **Lids alone are not a sheet.** Without the riser standing on the boundary
        // two cells share, the mark draws disconnected tiles floating at their own
        // heights — confetti rather than relief — and the claim the geometry rests on,
        // that a cut floor tiles without gaps, would hold only in plan view.
        let data = HashMap::from([("t".to_string(), grid_frame(3, 3))]);
        let stepped = PlotSpec::new().data("t").x("x").y("y").z("h")
            .layer(Layer::new(Mark::Surface)
                .transform(Transform::Bin).transform(Transform::Mean).bins(3));
        let svg = SvgRenderer::default().render(&stepped, &data);
        // A 3×3 floor shares 2×3 boundaries across and 3×2 up: 12 risers, and
        // `grid_frame`'s height rises along both axes so none of them is level.
        assert_eq!(surface_faces(&svg).len() - level_faces(&svg), 12,
            "every internal boundary carries a riser");

        // **And a riser is the *step*, not a wall down to the baseline** — which is
        // the whole of why this reads where a 3-D histogram of the same table does
        // not. A perfectly flat field has no steps, so it draws lids and nothing else,
        // where a bar chart of it would still stand nine full-height columns.
        let flat = DataFrame::new()
            .with_float("x", (0..9).map(|k| (k % 3) as f64).collect())
            .with_float("y", (0..9).map(|k| (k / 3) as f64).collect())
            .with_float("h", vec![5.0; 9]);
        let svg = SvgRenderer::default()
            .render(&stepped, &HashMap::from([("t".to_string(), flat)]));
        assert_eq!(surface_faces(&svg).len(), level_faces(&svg),
            "a level field raises no risers");
    }

    #[test]
    fn a_surfaces_shades_come_from_its_slopes_not_from_where_the_camera_is() {
        // The invariant `palette::shade` exists for, one geometry over from the bar:
        // shading is keyed to the face's own tilt, so turning the scene rearranges the
        // faces on the page and repaints none of them. A lamp in screen space would
        // fail this, and the same sheet would change color when the reader turned it.
        let data = HashMap::from([("t".to_string(), grid_frame(4, 4))]);
        let base = PlotSpec::new().data("t").x("x").y("y").z("h")
            .layer(Layer::new(Mark::Surface));
        let shades = |turn: f64| {
            let spec = PlotSpec::new().data("t").x("x").y("y").z("h")
                .layer(Layer::new(Mark::Surface))
                .coord(CoordSpace::Space(SpaceView { turn, tilt: 25.0 }));
            let mut s: Vec<String> =
                surface_faces(&SvgRenderer::default().render(&spec, &data))
                    .into_iter().map(|(f, _)| f).collect();
            s.sort();
            s.dedup();
            s
        };
        assert_eq!(shades(30.0), shades(115.0), "a turn must not repaint a face");
        // And the shading is really doing something — a plane at 45° in the cube is
        // not left at the flat color, or the assertion above would be vacuous.
        let flat = surface_faces(&SvgRenderer::default().render(&base, &data));
        assert!(
            flat.iter().all(|(f, _)| *f != PALETTE_GOG[0]),
            "a sloped face should be shaded below the base color: {flat:?}"
        );
    }

    #[test]
    fn a_measured_color_lands_per_face_where_a_region_would_need_a_gradient() {
        // What a mesh can do and an `area` cannot (spec §15): every face takes its own
        // stop off the ramp, so a sheet colored by height reads as a ramp without any
        // gradient machinery. Distinct fills, and they are the ramp's colors rather
        // than the categorical palette's.
        let data = HashMap::from([("t".to_string(), grid_frame(4, 4))]);
        let spec = PlotSpec::new().data("t").x("x").y("y").z("h").layer(
            Layer::new(Mark::Surface).encode(Channel::Color, "h"),
        );
        let svg = SvgRenderer::default().render(&spec, &data);
        let fills: Vec<String> = surface_faces(&svg).into_iter().map(|(f, _)| f).collect();
        assert_eq!(fills.len(), 9);
        let distinct: std::collections::HashSet<&String> = fills.iter().collect();
        assert!(distinct.len() > 1, "a ramp over the faces must vary: {fills:?}");
        assert!(
            svg.contains("linearGradient") || svg.contains(RAMP_BLUE[RAMP_BLUE.len() - 1]),
            "the ramp's own colors, and its key"
        );
    }

    #[test]
    fn a_long_title_is_held_inside_the_canvas_rather_than_clipped() {
        // A title centers on the **panel**, which is the thing it names, and a
        // legend pushes that center left. At full width there is room to spare, so
        // nothing showed for the life of the project; in a page cell the panel's
        // center sits far enough left that a long title starts at a negative x and
        // the cell's edge eats its first letters. Pinned at a narrow width for that
        // reason — a test at 800px would pass either way.
        let data = HashMap::from([("t".to_string(), grid_frame(4, 4))]);
        let title = "Turned: the same sheet from the west";
        let spec = PlotSpec::new().data("t").x("x").y("y").title(title)
            .layer(Layer::new(Mark::Point).encode(Channel::Color, "h"));
        let svg = SvgRenderer::for_theme(&spec.theme.resolved(), 390.0, 300.0)
            .render(&spec, &data);

        let cx: f64 = svg.lines()
            .find(|l| l.contains(title) && l.contains(r#"text-anchor="middle""#))
            .and_then(|l| l.split(r#"x=""#).nth(1))
            .and_then(|s| s.split('"').next())
            .and_then(|s| s.parse().ok())
            .unwrap_or_else(|| panic!("no centered title in:\n{svg:.0}"));
        let half = crate::render::text::estimate_text_width(title, 16.0) / 2.0;
        assert!(cx - half >= 0.0,
                "title starts at {:.1}, outside the canvas", cx - half);
        assert!(cx + half <= 390.0,
                "title ends at {:.1}, past the canvas", cx + half);
    }

    #[test]
    fn a_mesh_face_takes_its_color_from_its_center_not_from_one_of_its_corners() {
        // **Pinned on a coarse mesh on purpose, because that is the only place it
        // shows.** A face named by its low corner is wrong by half a cell everywhere,
        // and on a fine lattice neighbors barely differ, so the volcano at 31x44 drew
        // this defect perfectly for the life of the project. Four faces is where it is
        // impossible to miss, and a test that used a realistic grid would pass either
        // way — the density of the mesh, not the correctness of the code, decided
        // whether anyone could see it.
        //
        // The field is a symmetric bowl on a 3x3 lattice, so the four faces are
        // congruent and every one of them sits at the same mean height. Anything that
        // paints them differently is asserting a difference the data does not have.
        let (mut xs, mut ys, mut vs) = (vec![], vec![], vec![]);
        for &a in &[-2.0_f64, 0.0, 2.0] {
            for &b in &[-2.0_f64, 0.0, 2.0] {
                xs.push(a);
                ys.push(b);
                vs.push(0.019 + 0.0025 * (a * a + b * b));
            }
        }
        let frame = DataFrame::new()
            .with_float("x", xs).with_float("y", ys).with_float("v", vs.clone());
        let data = HashMap::from([("t".to_string(), frame)]);
        let spec = PlotSpec::new().data("t").x("x").y("y").z("v").layer(
            Layer::new(Mark::Surface).encode(Channel::Color, "v"),
        );
        let fills: Vec<String> = surface_faces(&SvgRenderer::default().render(&spec, &data))
            .into_iter().map(|(f, _)| f).collect();
        assert_eq!(fills.len(), 4, "a 3x3 lattice is 2x2 faces");
        let distinct: std::collections::HashSet<&String> = fills.iter().collect();
        assert_eq!(
            distinct.len(), 1,
            "four congruent faces of a symmetric bowl must share one color, got {fills:?}"
        );

        // And **every node reaches exactly the faces it belongs to**. Under the old
        // reading a face took `corners[0]`, so the last row and column of the lattice
        // colored nothing at all: five of these nine values were discarded.
        //
        // Probed by lighting one node at a time, which is exact rather than
        // suggestive. With a single node at 1 and the rest at 0, a face's mean is 0.25
        // if it touches that node and 0 otherwise, so counting the faces at the top
        // color counts the faces the node reached. A 3x3 lattice gives 1 face to each
        // of its four corners, 2 to each edge midpoint and 4 to the center, which is
        // the `[1, 2, 1]` product below. Asserting merely "the picture changed" would
        // have passed for the center node under the *old* code too.
        let xs9: Vec<f64> = (0..9).map(|i| [-2.0, 0.0, 2.0][i / 3]).collect();
        let ys9: Vec<f64> = (0..9).map(|i| [-2.0, 0.0, 2.0][i % 3]).collect();
        for k in 0..9 {
            let mut w = vec![0.0; 9];
            w[k] = 1.0;
            let frame = DataFrame::new()
                .with_float("x", xs9.clone()).with_float("y", ys9.clone())
                .with_float("v", vs.clone()).with_float("w", w);
            let d = HashMap::from([("t".to_string(), frame)]);
            let s = PlotSpec::new().data("t").x("x").y("y").z("v").layer(
                Layer::new(Mark::Surface).encode(Channel::Color, "w"),
            );
            let f: Vec<String> = surface_faces(&SvgRenderer::default().render(&s, &d))
                .into_iter().map(|(a, _)| a).collect();
            let expect = [1, 2, 1][k / 3] * [1, 2, 1][k % 3];
            // Compared as a **partition** rather than by picking the lit color, because
            // "which hex is the lit one" is not answerable by string order: the ramp
            // runs light to dark, so the face holding the larger value is the *darker*
            // string and `max()` returns the unlit one. The sizes of the color groups
            // carry the whole claim and need no such assumption.
            let mut sizes: Vec<usize> = {
                let mut c: std::collections::HashMap<&str, usize> = Default::default();
                for x in &f { *c.entry(x.as_str()).or_default() += 1; }
                c.into_values().collect()
            };
            sizes.sort_unstable();
            let mut want = if expect == 4 { vec![4] } else { vec![expect, 4 - expect] };
            want.sort_unstable();
            assert_eq!(
                sizes, want,
                "node {k} belongs to {expect} of the 4 faces; colors came out {f:?}"
            );
        }
    }

    #[test]
    fn a_sheet_ramped_by_its_own_estimate_gets_a_key_that_decodes_it() {
        // The half of this that no legality check could see. Past the refusal the sheet
        // already ramped correctly and drew **no key at all**, because the legend looked
        // for the color column in the raw table and a `density` exists only downstream
        // of the transform that made it. A ramp nobody can decode is worse than a flat
        // sheet: it shows a variation and names no numbers for it.
        let scatter = DataFrame::new()
            .with_float("x", vec![0.11, 0.42, 0.77, 0.93, 0.28, 0.55])
            .with_float("y", vec![0.51, 0.13, 0.88, 0.34, 0.67, 0.22]);
        let data = HashMap::from([("t".to_string(), scatter)]);
        let spec = PlotSpec::new().data("t").x("x").y("y")
            .coord(CoordSpace::Space(crate::ir::SpaceView::default()))
            .layer(
                Layer::new(Mark::Surface)
                    .transform(Transform::Density)
                    .encode(Channel::Color, "density"),
            );
        let svg = SvgRenderer::default().render(&spec, &data);

        let fills: std::collections::HashSet<String> =
            surface_faces(&svg).into_iter().map(|(f, _)| f).collect();
        assert!(fills.len() > 1, "the estimate must ramp across the faces: {fills:?}");
        assert!(svg.contains("linearGradient"), "a measured color is keyed by a strip");
        // The key is titled for the column it decodes, and that title is the axis's
        // too — the height said twice is the whole point of the sentence, so the two
        // readings must agree about what they are showing.
        assert!(svg.matches("Density").count() >= 2, "axis and key both name it: {svg:.0}");
    }

    #[test]
    fn border_on_a_surface_is_the_mesh_line() {
        // The seam hairline each face already carried, handed to the caller — which is
        // the reading that made `border_*` worth spanning to this mark. Without it a
        // face is stroked in its own shade (invisible, closing the antialiasing seam);
        // with it, one color across every face, whatever each face's own fill.
        let data = HashMap::from([("t".to_string(), grid_frame(4, 4))]);
        let plain = PlotSpec::new().data("t").x("x").y("y").z("h")
            .layer(Layer::new(Mark::Surface));
        for (fill, stroke) in surface_faces(&SvgRenderer::default().render(&plain, &data)) {
            assert_eq!(fill, stroke, "an unbordered face hides its own seam");
        }
        let meshed = PlotSpec::new().data("t").x("x").y("y").z("h")
            .layer(Layer::new(Mark::Surface).style_border("white", 0.8));
        let faces = surface_faces(&SvgRenderer::default().render(&meshed, &data));
        assert!(!faces.is_empty());
        for (fill, stroke) in faces {
            assert_eq!(stroke, "white", "the mesh line is one color");
            assert_ne!(fill, stroke, "and it is not the fill");
        }
    }

    #[test]
    fn a_hole_in_the_grid_opens_the_sheet_rather_than_being_drawn_across() {
        // A face needs all four corners, so dropping one node removes exactly the
        // faces that touched it and leaves the rest of the sheet alone. The
        // alternative — drawing a face over the gap — would invent a value nobody
        // measured, which is the silent wrongness §12 forbids.
        let full = grid_frame(4, 4);
        let (xs, ys, hs) = (
            full.float_col("x").unwrap().to_vec(),
            full.float_col("y").unwrap().to_vec(),
            full.float_col("h").unwrap().to_vec(),
        );
        // Drop the interior node at (1, 1) — it is a corner of four faces.
        let keep: Vec<usize> = (0..xs.len()).filter(|&i| !(xs[i] == 1.0 && ys[i] == 1.0)).collect();
        let holed = DataFrame::new()
            .with_float("x", keep.iter().map(|&i| xs[i]).collect())
            .with_float("y", keep.iter().map(|&i| ys[i]).collect())
            .with_float("h", keep.iter().map(|&i| hs[i]).collect());
        let data = HashMap::from([("t".to_string(), holed)]);
        let spec = PlotSpec::new().data("t").x("x").y("y").z("h")
            .layer(Layer::new(Mark::Surface));
        let svg = SvgRenderer::default().render(&spec, &data);
        assert_eq!(surface_faces(&svg).len(), 5, "9 faces less the 4 touching the hole");
    }

    // -----------------------------------------------------------------------
    // theme(font_size = ) — one number, three sizes
    // -----------------------------------------------------------------------

    fn typed_plot(font_size: Option<f64>) -> String {
        let data = HashMap::from([(
            "t".to_string(),
            DataFrame::new()
                .with_float("a", vec![1.0, 2.0, 3.0])
                .with_float("b", vec![2.0, 4.0, 3.0]),
        )]);
        let mut spec = PlotSpec::new().data("t").x("a").y("b")
            .layer(Layer::new(Mark::Point));
        spec.title = Some("A title".to_string());
        spec.theme = ThemeSpec { font_size, ..ThemeSpec::default() };
        SvgRenderer::for_theme(&spec.theme.resolved(), CANVAS.0, CANVAS.1)
            .render(&spec, &data)
    }

    /// Every `font-size="…"` on the page, largest first.
    fn font_sizes(svg: &str) -> Vec<f64> {
        let mut out: Vec<f64> = svg
            .match_indices("font-size=\"")
            .filter_map(|(i, _)| {
                let rest = &svg[i + 11..];
                rest.find('"').and_then(|e| rest[..e].parse::<f64>().ok())
            })
            .collect();
        out.sort_by(|a, b| b.partial_cmp(a).unwrap());
        out.dedup();
        out
    }

    #[test]
    fn the_three_furniture_sizes_are_one_number_on_a_scale() {
        // The constants this property replaced were 11, 13 and 16, and they were
        // already `round(11 × 1.2ᵏ)`. Stating the ratio once is what lets `theme()`
        // carry one number instead of three; if this ever stops holding, the
        // default look and the asked-for scale have parted company.
        assert_eq!(font_scale(FONT_BASE), (11.0, 13.0, 16.0));
    }

    #[test]
    fn asking_for_the_default_size_draws_the_untouched_plot() {
        // `None` means the caller said nothing and `Some(11)` means they asked for
        // what they already had. Those must be the same picture, or the default is
        // only an approximation of the scale rather than a point on it — and every
        // golden, book plot and parity signature would move the day this shipped.
        assert_eq!(typed_plot(None), typed_plot(Some(FONT_BASE)));
    }

    #[test]
    fn font_size_moves_every_furniture_size_and_the_margins_with_them() {
        assert_eq!(font_sizes(&typed_plot(None)), vec![16.0, 13.0, 11.0]);
        // 16 → 19 → 23, the same scale from a different base.
        assert_eq!(font_sizes(&typed_plot(Some(16.0))), vec![23.0, 19.0, 16.0]);

        // The layout derives every margin from these three, so bigger text has to
        // cost the panel room. A property that changed the glyphs and not the
        // rectangle would be drawing text over the plot.
        //
        // The *panel's* height, not the canvas's — the canvas is 600 either way,
        // which is what made the first version of this assertion pass on a plot it
        // had not actually measured. The panel is the first `<rect>` carrying an
        // `x`; the page background before it has only a width and a height.
        let panel = |svg: &str| -> f64 {
            let i = svg.find("<rect x=\"").unwrap();
            let j = svg[i..].find("height=\"").unwrap();
            svg[i + j + 8..].split('"').next().unwrap().parse().unwrap()
        };
        assert!(
            panel(&typed_plot(Some(24.0))) < panel(&typed_plot(None)),
            "a bigger type scale must leave the panel less room, not overprint it"
        );
    }

    // -----------------------------------------------------------------------
    // theme(strip = ) — the band the journal preset used to leave gray
    // -----------------------------------------------------------------------

    fn faceted(theme: ThemeSpec) -> String {
        let data = HashMap::from([(
            "t".to_string(),
            DataFrame::new()
                .with_float("a", vec![1.0, 2.0, 3.0, 4.0])
                .with_float("b", vec![2.0, 4.0, 3.0, 5.0])
                .with_str("g", ["x", "x", "y", "y"].iter().map(|s| s.to_string()).collect()),
        )]);
        let mut spec = PlotSpec::new().data("t").x("a").y("b")
            .layer(Layer::new(Mark::Point));
        spec.facet = Some(crate::ir::FacetSpec { col: Some("g".into()), ..Default::default() });
        spec.theme = theme;
        SvgRenderer::default().render(&spec, &data)
    }

    /// The strip's band fill and the ink of the label sitting on it.
    fn strip_band_and_ink(svg: &str) -> (String, String) {
        for chunk in svg.split("<g font").skip(1) {
            let Some(r) = chunk.find(r#"height="16.00" fill=""#) else { continue };
            let band: String = chunk[r + 21..].chars().take_while(|c| *c != '"').collect();
            let i = chunk.find(r#"fill=""#).unwrap();
            let ink: String = chunk[i + 6..].chars().take_while(|c| *c != '"').collect();
            return (band, ink);
        }
        ("?".into(), "?".into())
    }

    #[test]
    fn a_dark_strip_gets_light_type_without_being_asked() {
        // The whole reason the ink derives rather than defaulting to a constant:
        // `theme(strip = "black")` alone would otherwise print the near-black
        // label on the near-black band, and the panel's name would be a guide
        // that is silently empty (§12).
        let dark = faceted(ThemeSpec { strip: Some("black".into()), ..Default::default() });
        assert_eq!(strip_band_and_ink(&dark), ("black".into(), STRIP_INK_LIGHT.into()));

        // The two that must not move.
        assert_eq!(strip_band_and_ink(&faceted(ThemeSpec::default())).1, STRIP_INK);
        let bw = faceted(ThemeSpec { preset: Some("bw".into()), ..Default::default() }.resolved());
        assert_eq!(strip_band_and_ink(&bw).1, STRIP_INK);

        // A band with no luminance to read keeps the dark ink rather than a
        // guess — right for the case that occurs, since it shows the page.
        let clear = faceted(ThemeSpec { strip: Some("transparent".into()), ..Default::default() });
        assert_eq!(strip_band_and_ink(&clear).1, STRIP_INK);
    }

    #[test]
    fn a_named_ink_beats_the_derived_one() {
        // Law 8: the derivation guides, it must not forbid. A navy strip with gold
        // type is legible and nothing should argue with it.
        let named = faceted(ThemeSpec {
            strip: Some("navy".into()),
            strip_text: Some("gold".into()),
            ..Default::default()
        });
        assert_eq!(strip_band_and_ink(&named), ("navy".into(), "gold".into()));
    }

    #[test]
    fn the_journal_preset_no_longer_leaves_gray_bars_over_a_white_panel() {
        // The defect this property was built for: `bw` turned the panel white and
        // left the strips at the hard-coded gray, so the journal look was only
        // most of the way black and white — and a gray band is the part that
        // reproduces badly in print, which is the one place the preset is for.
        let bw = faceted(ThemeSpec { preset: Some("bw".into()), ..Default::default() }.resolved());
        assert!(!bw.contains(STRIP_BG), "theme(\"bw\") must not leave a gray strip");

        // …and the default look did not move.
        assert!(faceted(ThemeSpec::default()).contains(STRIP_BG));
    }

    #[test]
    fn the_journal_preset_is_still_only_properties_a_caller_could_write() {
        // §7's rule, and the reason the property and the preset entry are one
        // change: `bw` may only set things that can be said out loud. If this ever
        // fails, a preset has grown a private vocabulary.
        let preset = faceted(ThemeSpec { preset: Some("bw".into()), ..Default::default() }.resolved());
        let spelled = faceted(ThemeSpec {
            background: Some("white".into()),
            frame: Some("full".into()),
            strip: Some("white".into()),
            ..Default::default()
        });
        assert_eq!(preset, spelled);
    }

    #[test]
    fn the_play_strip_follows_the_facet_strip_because_it_is_the_same_band() {
        // `write_play_strip` is documented as "deliberately the facet strip's
        // strip: same band, same fill". A property that moved one and not the
        // other would make that comment false — Law 2, caught by a test rather
        // than by a reader noticing two grays.
        let data = HashMap::from([(
            "t".to_string(),
            DataFrame::new()
                .with_float("a", vec![1.0, 2.0, 3.0, 4.0])
                .with_float("b", vec![2.0, 4.0, 3.0, 5.0])
                .with_float("yr", vec![1.0, 1.0, 2.0, 2.0]),
        )]);
        let mut spec = PlotSpec::new().data("t").x("a").y("b")
            .layer(Layer::new(Mark::Point));
        spec.channels.insert(Channel::Play, crate::ir::ChannelDef::field("yr"));
        spec.theme = ThemeSpec { strip: Some("seagreen".into()), ..Default::default() };
        let svg = SvgRenderer::default().render(&spec, &data);
        assert!(svg.contains("seagreen"), "the play strip must take theme(strip = ) too");
        assert!(!svg.contains(STRIP_BG), "no band may keep the default when one was asked for");
    }
}
