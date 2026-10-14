//! SVG renderer.
//!
//! Produces a self-contained SVG string from a `PlotSpec`. Text is real vector
//! text (crisp at any zoom), and the output opens directly in any browser or
//! vector editor. Raster output is obtained by converting the SVG.
//!
//! This file is the orchestrator: [`SvgRenderer::draw`] runs the stages in order
//! and hands each layer to its mark in `render::marks`. What it calls is one
//! concern to a file beside it: the axes, the chrome around the marks, the frames
//! of the spaces that are not flat, crowding, the brush, play, and the drawing's
//! names.

use std::collections::HashMap;
use std::fmt::Write;

use crate::data::DataFrame;
use crate::ir::{Channel, CoordSpace, Layer, Mark, PlotSpec, SpaceView, ThemeSpec, Transform};
use crate::legality::Diagnostic;
use crate::render::ticks::{auto_label, log_ticks, log_ticks_counted, nice_ticks_within, ticks_at, ticks_with_labels, time_ticks, time_ticks_counted, TickSpec};
use crate::scale;
use crate::render::layout::{Fit, LegendRoom, PanelGrid};
use crate::render::nest::Nest;
use crate::render::polar::Polar;
use crate::render::project::{self, Screen};
use crate::render::{AxisFacts, Drawn, Layout, RenderContext, Whole};
use crate::render::legend::{
    collect_legends, plan_across, write_legends, write_legends_across, LegendSide,
    LEGEND_ACROSS_GAP, LEGEND_EDGE, LEGEND_PADDING, LEGEND_PLOT_GAP,
};
use crate::render::palette::{build_color_map, resolve_ramp};
use crate::render::text::{esc, estimate_cap_height, estimate_text_width};

mod axes;
mod chrome;
mod frames;
mod crowding;
mod selection;
mod play;
mod ids;
mod series;

use self::axes::*;
use self::chrome::*;
use self::frames::*;
use self::crowding::*;
use self::series::*;
pub(crate) use self::play::*;
pub(crate) use self::ids::*;

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

/// The order one panel's layers are drawn in, as `(moment, in that moment's group,
/// the layers drawn)`: every layer in every moment, except that a played plot
/// draws a layer that stands still **once** where that leaves the picture as it
/// was. Those are the still layers written before every played one, drawn once
/// under the moments, and those written after every played one, drawn once over
/// them. A still layer between two played ones stays in every moment, since its
/// place in the order is between theirs (Law 6: hoisting every still layer would
/// put it behind the played ones whatever order it was written in).
///
/// Only where each moment would draw it the same: one selection pass (a
/// selection draws each layer's unselected rows first and pushes them back, so a
/// still layer's two passes sit between the played layers'), not a repelled
/// `text` (it steps around the moment's dots) or an `edge` (its heads stop at the
/// moment's nodes), and only where the caller's space draws the layers in their
/// written order. A basemap under played points was copied into every moment:
/// 28 KB of borders per frame on a globe, and its hidden rows counted once for
/// each moment in the note that reports them.
fn draw_steps(
    spec: &PlotSpec, nframes: usize, passes: usize, in_order: bool,
) -> Vec<(usize, bool, std::ops::Range<usize>)> {
    let n = spec.layers.len();
    let still = |l: &&Layer| !l.encodings.contains_key(&Channel::Play)
        && l.mark != Mark::Edge
        && !(l.mark == Mark::Text && l.transforms.contains(&Transform::Repel));
    let (lead, trail) = if in_order && nframes >= 2 && passes == 1 {
        let lead = spec.layers.iter().take_while(still).count();
        if lead == n { (0, 0) } else { (lead, spec.layers.iter().rev().take_while(still).count()) }
    } else {
        (0, 0)
    };
    let mut steps = Vec::new();
    if lead > 0 {
        steps.push((0, false, 0..lead));
    }
    for fi in 0..nframes {
        steps.push((fi, true, lead..n - trail));
    }
    if trail > 0 {
        steps.push((0, false, n - trail..n));
    }
    steps
}

/// The columns that split a **two-dimensional reading** into one field per group.
///
/// A split runs the whole reading once per group, the way every statistic in
/// `transform::apply` already does: a contour per species, on shared axes. Every
/// channel that splits takes part (`legality::split_fields_of`) except the one that
/// carries the measurement: on a mark with no measure axis that is `color`, and
/// `check_field` refuses any other field there. In the cube a `bar` measures along
/// `z`, so its categorical `color` is a split like any other.
fn field_split(layer: &Layer) -> Vec<&str> {
    let measured_by_color = crate::legality::has_no_measure_axis(&layer.mark);
    let color_field = layer.encodings.get(&Channel::Color).map(|e| e.field.as_str());
    crate::legality::split_fields_of(layer).into_iter()
        .filter(|f| !(measured_by_color && color_field == Some(*f)))
        .collect()
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
    /// The played column, its moments in order, and when each one shows.
    play: Option<(&'a str, &'a [crate::data::FrameLevel], Clock<'a>)>,
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
    /// Where the free ends of `x` and of `y` have to reach for every label to fit
    /// the panel, in the scales' units: what the first drawing measured
    /// ([`room_for_labels`]), handed to the second. `None` on a first drawing,
    /// and only a first drawing measures, so a plot is drawn at most twice.
    pub(crate) label_room: Option<[(Option<f64>, Option<f64>); 2]>,
}

/// The canvas a plot gets when it asks for nothing: `theme(width =, height =)`
/// is what asks. Named because two callers need to agree on it — the renderer's
/// own default and the page that divides a canvas into cells.
pub const CANVAS: (f64, f64) = (800.0, 600.0);

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
            label_room: None,
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
        // The sentence as given, for a second drawing that gives labels room.
        let given = spec;
        let mut resolved = crate::legality::resolve_scopes(spec);
        crate::render::legend::neutral_beside_a_color_guide(&mut resolved);
        crate::render::marks::link_flow_ground(&mut resolved);
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
        // The sphere itself, viewed (spec §15): `x` is longitude, `y` latitude,
        // and a mark stands at its place on the facing hemisphere. Asked for
        // outright like `polar` — it re-reads the two positions the plot already
        // has — and carrying a view like the cube. `check_globe` refuses
        // `globe()` with a `z`, so it cannot collide with the 3-D gate.
        let globe_view = match &spec.coord {
            CoordSpace::Globe(v) => Some(*v),
            _ => None,
        };
        let is_globe = globe_view.is_some();
        // The third space that changes where a value lands, and the one that does
        // not land it anywhere: `nest()` turns each row's measure into an area and
        // partitions the panel (spec §15). Everything the other spaces reserve room
        // for — ticks, tick labels, axis names, gridlines — is absent here because
        // the two directions carry no variable, so this flag reads as "no guides"
        // through the layout below, the way `is_3d` reads as "the guides are on the
        // cube's edges". `check_nest` refuses `nest()` with a `z`, so it cannot
        // collide with the 3-D gate.
        let is_nest = matches!(spec.coord, CoordSpace::Nest);
        // The graph-theoretic space, `nest`'s twin: no axes, no ticks, no
        // grid, because a layout's positions mean nothing as quantities. Which
        // *form* it takes rides on whether a view was stated — bare
        // `network()` draws flat, a stated angle draws the cube (the ruling
        // `CoordSpace::Network` records).
        let is_network = matches!(spec.coord, CoordSpace::Network(_));
        let network_cube: Option<crate::ir::SpaceView> = match &spec.coord {
            CoordSpace::Network(v) if v.cube() => Some(v.view()),
            _ => None,
        };
        // Asked of `space_of`, the one source the legality checks read, rather than
        // re-derived here. It agrees with the sentence above and adds the case that
        // sentence predates: a **synthesized** `z` is a third dimension too, so
        // `bar * bin + x(a) + y(b) + space()` projects with no `z()` binding, the
        // way a flat histogram raises a count axis with no `y()` (spec §5/§15).
        let is_3d = crate::legality::space_of(spec) == crate::legality::SpaceKind::Space;

        // **An axis the sentence leaves out is drawn by nobody** (`x(lon, axis =
        // FALSE)`, spec §10). A page already keeps that rule for an axis a
        // page-mate draws (`Fit::draw_x_axis`); here the plot asks for it itself,
        // so it joins the page's flag and reaches everything that flag reaches:
        // the tick marks, the numbers, the name, and the margin they would have
        // taken. The ticks are still chosen, because the gridlines stand on them.
        let draw_x_axis = self.fit.draw_x_axis && !spec.hides_axis(&Channel::X);
        let draw_y_axis = self.fit.draw_y_axis && !spec.hides_axis(&Channel::Y);
        // **A shared flow draws neither axis** (2026-10-03, at the author's word).
        // The axis across the count measures nothing: the bands reach into the
        // panel by a convention. The count axis is a running total in which every
        // amount stands twice, once at each end, so its numbers are no reading of
        // the data, and a ring shows none either. Its places are what a reader
        // needs along it, and the `text` layer writes their names just beyond the
        // slots, where the numbers would have stood (`shared_names` below sizes
        // that room). So neither axis has ticks, numbers, a name or gridlines.
        let shared_flow = spec.layers.iter().any(|l| l.flow_is_shared());
        let shared_down = shared_flow && spec.layers.iter().any(|l| crate::legality::flow_runs_down(spec, l));
        // A layered flow's axes are blank for the same reasons: its layers are
        // worked out from the links and have no names, and its places stand a gap
        // apart, so a count along the axis runs past every layer's total.
        let link_flow = spec.layers.iter().any(|l| l.flow_reads_links());
        let (blank_x, blank_y) = (link_flow, link_flow);
        let draw_x_axis = draw_x_axis && !blank_x;
        let draw_y_axis = draw_y_axis && !blank_y;
        let draw_z_axis = !spec.hides_axis(&Channel::Z);

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
        } else if is_globe {
            Some("globe")
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
        // A shared flow supplies its count interval and nothing on the other axis,
        // so like the pie it has one position, and the angle takes it.
        let supplies_positions = spec.layers.iter()
            .any(|l| l.transforms.contains(&Transform::Partition)
                || (l.transforms.contains(&Transform::Flow) && !l.flow_is_shared()));
        // Nested only, for the radial fallback below: crossed, the second axis is
        // apportioning the measure one level down rather than stepping a ring, so
        // its synthesized column is a share and not a depth.
        let rings_the_depth = spec.layers.iter().any(|l| {
            l.transforms.contains(&Transform::Partition)
                && !l.partition.as_ref().map(|p| p.cross).unwrap_or(false)
        });
        // A shared flow's count goes on the angle whichever axis it was bound to:
        // its radius measures nothing, so a ring or a spoke would decode nothing,
        // and its running count round the rim would sit among the place names.
        let measure_on_angle = is_polar
            && ((x_field.is_empty() && !supplies_positions) || shared_flow);
        let view = match &spec.coord {
            CoordSpace::Space(v) => *v,
            // A network with a stated angle is a cube seen from it, each
            // unstated angle taking `space()`'s own default.
            CoordSpace::Network(v) if v.cube() => v.view(),
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
            ("", false) if matches!(&spec.coord, CoordSpace::Network(v) if v.cube()) =>
                crate::transform::LAYOUT_Z,
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
        // The **measure** axis of a partition falls back too, to the tally's name,
        // since an unbound `x` means every leaf weighs 1. Nested, the ring above
        // already parted the two names; crossed, there is no ring, and the second
        // axis holds each cell's height under the name `y` reads. With neither
        // position bound that name was the measure's as well, so the heights were
        // overwritten by the widths: the cells were drawn off the panel and every
        // label ran along a diagonal.
        let partitions = spec.layers.iter()
            .any(|l| l.transforms.contains(&Transform::Partition));
        let x_field = match (x_field, partitions) {
            ("", true) => crate::transform::CELL_COUNT,
            _ => x_field,
        };

        // A **flow**'s two axes fall back the same way, and for the same reason:
        // the transform places its own marks, so a sentence with nothing bound is
        // well-formed and the axes have to find the synthesized columns. The
        // domain axis reads the stage column the layout published — categorical,
        // its levels the atom's argument order — and an unbound measure axis
        // reads the tally's name, exactly as a 3-D reading's synthesized `z` does.
        // A shared flow has no stage axis at all: its places lie on the count axis,
        // so the other axis is left empty, as a stacked bar's is, and in `polar()`
        // the count goes on the angle by the pie's rule. Its count still falls
        // back to the tally's name when nothing is bound.
        let any_flow = spec.layers.iter()
            .any(|l| l.transforms.contains(&Transform::Flow));
        let stages_the_domain = spec.layers.iter()
            .any(|l| l.transforms.contains(&Transform::Flow) && !l.flow_is_shared());
        // **Top to bottom when the count is on `x`** (`legality::flow_runs_down`):
        // orientation read off the bindings, as a bar's is, so the stages take the
        // axis the count left unbound. A categorical `y` axis lists its first
        // category at the top, so the first stage is the top one.
        let flow_down = spec.layers.iter().any(|l| crate::legality::flow_runs_down(spec, l));
        let x_field = match (x_field, stages_the_domain && !flow_down) {
            ("", true) => crate::transform::FLOW_STAGE,
            _ => x_field,
        };
        let y_field = match (y_field, stages_the_domain, flow_down) {
            ("", true, true) => crate::transform::FLOW_STAGE,
            ("", true, false) => crate::transform::CELL_COUNT,
            _ => y_field,
        };
        let y_field = match (y_field, any_flow && !stages_the_domain && !flow_down) {
            ("", true) => crate::transform::CELL_COUNT,
            _ => y_field,
        };

        // A **layout** places its own nodes, so nothing is bound and all three
        // axes fall back to the columns it published — the same rule a fourth
        // and fifth time. `z` only in the cube form, where the layout filled
        // the third coordinate.
        let layout_the_positions = spec.layers.iter()
            .any(|l| l.transforms.contains(&Transform::Layout));
        let x_field = match (x_field, layout_the_positions) {
            ("", true) => crate::transform::LAYOUT_X,
            _ => x_field,
        };
        let y_field = match (y_field, layout_the_positions) {
            ("", true) => crate::transform::LAYOUT_Y,
            _ => y_field,
        };

        // A **cluster** tree's distance axis falls back the same way once
        // more: the leaf axis is the bound position, the other carries the
        // merge distance the transform synthesized, and the axis, the scale
        // and the mark all read one column. Whichever position is unbound is
        // the tree's — up over `x(leaves)`, sideways over `y(leaves)`.
        let clusters_a_tree = spec.layers.iter()
            .any(|l| l.mark == Mark::Path
                && l.transforms.contains(&Transform::Cluster));
        let x_field = match (x_field, clusters_a_tree) {
            ("", true) => crate::transform::CLUSTER_DISTANCE,
            _ => x_field,
        };
        let y_field = match (y_field, clusters_a_tree) {
            ("", true) => crate::transform::CLUSTER_DISTANCE,
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
        // Each moment's hold in proportion to the gap it stands for (§15), and the
        // pace those holds are counted in, so `speed` divides every hold alike.
        let play_holds = crate::data::frame_holds(&play_levels);
        let clock = Clock { holds: &play_holds, pace: frame_seconds };

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
            // A joined mark breaks where a row was dropped for a missing value of
            // its own position, so it reads the table with those rows back in.
            let mut base = with_breaks(df, spec, layer).unwrap_or_else(|| df.clone());
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
            !((horizontal && crate::legality::reads_orientation(layer))
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

        // The domain the author stated for an axis, in the axis's own units. An end
        // the page wrote to line this plot up with its neighbors is not one
        // (`Fit::shared_domain_x`): it holds their margins, and nothing is cut on it.
        // The end the author stated beside it still is.
        let authored_domain = |ch: Channel, is_log: bool, base_of: f64| -> (Option<f64>, Option<f64>) {
            let page_ends = match ch {
                Channel::X => self.fit.shared_domain_x,
                Channel::Y => self.fit.shared_domain_y,
                _ => [false; 2],
            };
            let (lo, hi) = stated_domain(scale::domain_of(spec.axis_def(&ch)), is_log, base_of);
            (lo.filter(|_| !page_ends[0]), hi.filter(|_| !page_ends[1]))
        };

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
                //
                // A stated domain (spec §10) is where the bins start and stop: the
                // rows outside it are gone already, and cutting over the survivors'
                // own range left the stated ends out of every bin.
                let logged = |ch: Channel, field: &str, is_log: bool, base_of: f64| -> Option<crate::transform::BinLayout> {
                    let df = if is_log { scale::log_column(&base, field, base_of) } else { base.clone() };
                    let stated = authored_domain(ch, is_log, base_of);
                    df.float_col(field)
                        .and_then(|xs| crate::transform::bin_layout_within(xs, layer.bin.as_ref(), stated))
                };
                // A two-dimensional reading cuts both axes; a one-dimensional one
                // cuts whichever it groups by, and leaves the other alone.
                if crate::legality::reads_two_dimensions(
                    &layer.mark, &layer.transforms, crate::legality::space_of(spec)) {
                    crate::transform::BinCut {
                        x: logged(Channel::X, x_field, x_log, x_base),
                        y: logged(Channel::Y, y_field, y_log, y_base),
                    }
                } else if key_is_x(layer, &base) {
                    crate::transform::BinCut { x: logged(Channel::X, x_field, x_log, x_base), y: None }
                } else {
                    crate::transform::BinCut { x: None, y: logged(Channel::Y, y_field, y_log, y_base) }
                }
        };
        let layer_cuts: Vec<crate::transform::BinCut> =
            spec.layers.iter().map(|layer| cut_of(layer, &[])).collect();

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
        // Each panel's rows, as the facet filters that cut them out.
        let panel_sets: Vec<Vec<Slice<'_>>> = panel_levels.iter()
            .map(|(cv, rv)| {
                let mut filters: Vec<Slice<'_>> = Vec::new();
                if let (Some(f), Some(v)) = (col_field.as_deref(), *cv) {
                    filters.push(Slice::Str(f, v));
                }
                if let (Some(f), Some(v)) = (row_field.as_deref(), *rv) {
                    filters.push(Slice::Str(f, v));
                }
                filters
            })
            .collect();

        // **One smoothing length per `density` layer** (spec §5; the rule and its
        // reasons are `transform::Smoothing`'s): the mean of the bandwidths the
        // layer's groups would each choose alone, where a group is every estimate the
        // layer draws, in every panel and every moment. Resolved here, beside the cut,
        // for the cut's reason: the statistic is the panel's, and how smooth it is
        // drawn is the plot's. `eff_for` then hands the answer down as a stated
        // bandwidth, so every group reads it through the one path a stated bandwidth
        // already takes.
        //
        // The rows each group is asked about are the rows it will be estimated from:
        // `prepared`, the moment's filter, and the log on the axis the estimate reads,
        // exactly as `eff_for` builds them below.
        let own_bandwidths = |layer: &Layer, filters: &[Slice<'_>]| -> (Vec<f64>, Vec<f64>) {
            let Some(base) = prepared(layer, filters) else { return Default::default() };
            if crate::legality::reads_two_dimensions(
                &layer.mark, &layer.transforms, crate::legality::space_of(spec)) {
                let mut input = base;
                if x_log { input = scale::log_column(&input, x_field, x_base); }
                if y_log { input = scale::log_column(&input, y_field, y_base); }
                crate::transform::density2d_own_bandwidths(&input, x_field, y_field, &field_split(layer))
            } else if layer.mark == Mark::Zone {
                Default::default()
            } else {
                let on_x = key_is_x(layer, &base);
                let (key, key_log, key_base, out) =
                    if on_x { (x_field, x_log, x_base, y_field) } else { (y_field, y_log, y_base, x_field) };
                let input = if key_log { scale::log_column(&base, key, key_base) } else { base };
                let own = crate::transform::density_own_bandwidths(
                    &input, key, out, &crate::legality::split_fields_of(layer));
                // A curve spreads along its key, a violin along its measure.
                if on_x != input.str_col(key).is_some() { (own, Vec::new()) } else { (Vec::new(), own) }
            }
        };
        let smoothing_over = |layer: &Layer, panels: &[Vec<Slice<'_>>]| -> crate::transform::Smoothing {
            let d = layer.density.as_ref();
            // A stated bandwidth is already one length for every group.
            if !layer.transforms.contains(&Transform::Density) || d.is_some_and(|s| s.bandwidth.is_some()) {
                return crate::transform::Smoothing::default();
            }
            let moments: Vec<Option<&crate::data::FrameLevel>> =
                if layer.encodings.contains_key(&Channel::Play) && !play_levels.is_empty() {
                    play_levels.iter().map(Some).collect()
                } else {
                    vec![None]
                };
            let (mut along_x, mut along_y) = (Vec::new(), Vec::new());
            for panel in panels {
                for &frame in &moments {
                    let mut filters = panel.clone();
                    filters.extend(moment_filter(layer, frame));
                    let (x, y) = own_bandwidths(layer, &filters);
                    along_x.extend(x);
                    along_y.extend(y);
                }
            }
            let adjust = d.and_then(|s| s.adjust);
            crate::transform::Smoothing {
                x: crate::transform::shared_bandwidth(&along_x, adjust),
                y: crate::transform::shared_bandwidth(&along_y, adjust),
            }
        };
        let layer_smoothing: Vec<crate::transform::Smoothing> =
            spec.layers.iter().map(|layer| smoothing_over(layer, &panel_sets)).collect();

        // One placement per layer, resolved from every panel's and every
        // moment's rows at once — the cut's rule applied to a graph (spec §15,
        // the network entry). A node's position is the plot's, as a bin's edges
        // are; the edges a panel draws, and a node's degree there, stay the
        // panel's, as a bin's tally does. So a node stands in one place in every
        // panel, and a panel draws only the relations its own rows state.
        let layout_dims = match &spec.coord {
            crate::ir::CoordSpace::Network(v) if v.cube() => 3,
            _ => 2,
        };
        let layer_places: Vec<Option<crate::transform::NodePlaces>> = spec.layers.iter()
            .map(|layer| {
                if !layer.transforms.contains(&Transform::Layout) {
                    return None;
                }
                let lay = layer.layout.as_ref()?;
                let base = prepared(layer, &[])?;
                crate::transform::layout_places(&base, &lay.from, &lay.to, layout_dims)
            })
            .collect();

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
                    filters.extend(moment_filter(layer, frame));
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
                    // The smoothing follows the cut: a freed axis takes the mean of
                    // this panel's groups, across its moments, and a shared one keeps
                    // the plot's.
                    let smoothing = if any_free && layer.transforms.contains(&Transform::Density) {
                        let own = smoothing_over(layer, &[panel_rows.to_vec()]);
                        crate::transform::Smoothing {
                            x: if free_x { own.x } else { layer_smoothing[li].x },
                            y: if free_y { own.y } else { layer_smoothing[li].y },
                        }
                    } else {
                        layer_smoothing[li]
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
                        // The scope rule, as `check_partition` reads it: a
                        // layer naming no measure reads the first one written.
                        let measure = spec.position_for(layer, &Channel::X)
                            .map(|e| e.field.as_str());
                        let nodes = crate::transform::partition(
                            &base, &levels, measure, x_field, y_field, cross);
                        // **A zone colored by its ring reads the ring's number**
                        // (spec §8, a written column is a column). `depth` holds
                        // each ring's midpoint, `d + 0.5`, because the same column
                        // places the ring on the radius, and a legend of 1.5, 2.5,
                        // 3.5 would contradict "counting the innermost as 1". A
                        // zone draws from its four edge columns and fits its axis
                        // to them, so its frame can carry the whole number; a
                        // `text` cannot, since its place may be `y(depth)`, and
                        // prints the number instead (`marks::text`). Re-centering
                        // the rings on whole numbers was drawn and declined: it
                        // emptied the rim of every donut that sets `limits`.
                        let nodes = match layer.mark == Mark::Zone
                            && layer.encodings.get(&Channel::Color)
                                .is_some_and(|c| c.field == crate::transform::NODE_DEPTH)
                        {
                            true => match nodes.float_col(crate::transform::NODE_DEPTH).cloned() {
                                Some(mid) => nodes.with_float(crate::transform::NODE_DEPTH,
                                    mid.iter().map(|v| v - 0.5).collect()),
                                None => nodes,
                            },
                            false => nodes,
                        };
                        // `* proportion` divides the measure axis by its total,
                        // which is what it does to every measurement (spec §5).
                        return match layer.transforms.contains(&Transform::Proportion) {
                            true => crate::transform::partition_shares(&nodes, x_field),
                            false => nodes,
                        };
                    }
                    // A **flow** takes the same door, for the same reason: its
                    // stages are columns named in the atom, and the bound `y` is
                    // its weight rather than a place. One layout, projected per
                    // reading mark — the band rows for `ribbon`, the node rows
                    // for `zone` and `text` — which is how `bin`'s output feeds
                    // a heatmap's cells and their labels without either writer
                    // learning the other's shape.
                    if layer.transforms.contains(&crate::ir::Transform::Flow) {
                        let stages: Vec<String> = layer.flow.as_ref()
                            .map(|f| f.stages.clone()).unwrap_or_default();
                        // The count rides `y`, or `x` when the flow runs down,
                        // and a layer naming none reads the first one written:
                        // the scope rule `check_flow` validated the weight by.
                        let (count_on, measure_out) = match flow_down {
                            true => (Channel::X, x_field),
                            false => (Channel::Y, y_field),
                        };
                        let measure = spec.position_for(layer, &count_on)
                            .map(|e| e.field.as_str());
                        // A band's `color` or `pattern` of categories that is not
                        // a stage splits each band by its values, as it splits a
                        // line; a stage needs no split, since a path already has
                        // one value there.
                        let mut split: Vec<String> = Vec::new();
                        for ch in [Channel::Color, Channel::Pattern] {
                            if let Some(def) = layer.encodings.get(&ch) {
                                if !stages.contains(&def.field) && !split.contains(&def.field)
                                    && base.str_col(&def.field).is_some()
                                {
                                    split.push(def.field.clone());
                                }
                            }
                        }
                        // A list of links whose places stand in layers worked out
                        // from it: the Sankey diagram. Its frames have the stage
                        // flow's shape, a layer standing for a stage.
                        if layer.flow_is_layered() && stages.len() == 2 {
                            let (from, to) = (stages[0].as_str(), stages[1].as_str());
                            return match layer.mark {
                                Mark::Ribbon => crate::transform::layered_flow_bands(
                                    &base, from, to, &split, measure, measure_out, flow_down),
                                _ => crate::transform::layered_flow_nodes(
                                    &base, from, to, measure, measure_out, flow_down),
                            };
                        }
                        // Two columns that share one set of places lie on one axis:
                        // the arc diagram, or the chord diagram on a ring.
                        if layer.flow_is_shared() && stages.len() == 2 {
                            let (from, to) = (stages[0].as_str(), stages[1].as_str());
                            return match layer.mark {
                                Mark::Ribbon => crate::transform::shared_flow_bands(
                                    &base, from, to, &split, measure, measure_out, flow_down),
                                _ => crate::transform::shared_flow_nodes(
                                    &base, from, to, measure, measure_out, flow_down),
                            };
                        }
                        return match layer.mark {
                            Mark::Ribbon => crate::transform::flow_bands(
                                &base, &stages, &split, measure, measure_out, flow_down),
                            _ => crate::transform::flow_nodes(
                                &base, &stages, measure, measure_out, flow_down),
                        };
                    }
                    // A **layout** takes the same door a third time: its two
                    // endpoint columns are named in the atom, nothing is
                    // bound, and one deterministic computation is projected
                    // per reading mark — endpoint pairs for `edge`, the node
                    // list for `point` and `text`. Determinism *is* the
                    // cross-layer agreement: three layers over one table land
                    // on identical positions, the guarantee two-layer
                    // partition sentences already stand on. The placement was
                    // made once above, from all of the layer's rows; this
                    // panel reads its own rows at those positions.
                    if layer.transforms.contains(&crate::ir::Transform::Layout) {
                        let (from, to) = layer.layout.as_ref()
                            .map(|sp| (sp.from.clone(), sp.to.clone()))
                            .unwrap_or_default();
                        let Some(places) = layer_places[li].as_ref() else {
                            return DataFrame::new();
                        };
                        return match layer.mark {
                            Mark::Edge => crate::transform::layout_edges_at(
                                places, &base, &from, &to, layout_dims),
                            _ => crate::transform::layout_nodes_at(
                                places, &base, &from, &to, layout_dims),
                        };
                    }
                    // A **cluster** takes the same door a fourth time: its value
                    // and profile columns are named in the atom, the leaf axis is
                    // the bound categorical position, and one deterministic tree
                    // is projected per reading mark — the elbow vertices for
                    // `path`, the same rows in the tree's leaf order for `zone`.
                    // The order travels as the leaf column's declared levels,
                    // which is the one channel every categorical axis already
                    // reads; nothing downstream learns a cluster reading.
                    if layer.transforms.contains(&crate::ir::Transform::Cluster) {
                        let cl = layer.cluster.clone().unwrap_or_default();
                        let over = cl.over.clone();
                        // The leaf axis is read off the *bindings*, not off the
                        // working field names — by the time this runs, the
                        // unbound position already carries the synthesized
                        // distance column's name, and mistaking that for a leaf
                        // column clusters nothing. A layer naming no leaves reads
                        // the first leaf column written, as `check_cluster` does.
                        // A position naming the merge `distance` is the tree's own,
                        // so `x(distance) + y(<leaves>)` takes its leaves from `y`.
                        let x_bound = spec.position_for(layer, &Channel::X)
                            .is_some_and(|d| d.field != crate::transform::CLUSTER_DISTANCE);
                        let leaf = match layer.mark {
                            Mark::Zone => {
                                let o = over.as_deref().unwrap_or("");
                                if x_field == o { y_field } else { x_field }
                            }
                            _ => if x_bound { x_field } else { y_field },
                        };
                        let value = cl.value.clone()
                            .or_else(|| (layer.mark == Mark::Zone)
                                .then(|| layer.encodings.get(&Channel::Color)
                                    .or_else(|| spec.channels.get(&Channel::Color))
                                    .map(|d| d.field.clone()))
                                .flatten())
                            .unwrap_or_default();
                        return match layer.mark {
                            Mark::Zone => {
                                match crate::transform::cluster_order(
                                    &base, leaf, &value, over.as_deref()) {
                                    Some(order) => {
                                        let vals = base.str_col(leaf)
                                            .cloned().unwrap_or_default();
                                        base.clone().with_levels(
                                            leaf.to_string(), vals, order)
                                    }
                                    None => base.clone(),
                                }
                            }
                            _ => crate::transform::cluster_tree(
                                &base, leaf, &value, over.as_deref()),
                        };
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
                        // One field per group, every group at the layer's smoothing.
                        let split = field_split(layer);
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
                        let cells = crate::transform::by_groups(&input, &split, &|sub: &DataFrame| match which {
                            Cut::Rings => {
                                crate::transform::density2d_contour(sub, x_field, y_field, d, smoothing)
                            }
                            // The heatmap, counted: one row per non-empty cell.
                            Cut::Cells if ts.contains(&Transform::Bin) => {
                                crate::transform::bin2d(sub, x_field, y_field, layer.bin.as_ref(), cut)
                            }
                            // The heatmap, estimated: one row per cell of the field.
                            Cut::Cells => {
                                crate::transform::density2d_cells(sub, x_field, y_field, d, smoothing)
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
                                // Every pair statistic `pairs2d` computes, named here.
                                // `deviation` was left out until 2026-10-14, so a cube's
                                // `interval * deviation` drew each cell's range in silence.
                                let kind = ts.iter().find(|t| matches!(
                                    t, Transform::Range | Transform::Confidence
                                        | Transform::Deviation | Transform::Box))
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
                        // dimension down (spec §5). A reduction's answer sits in the
                        // user's own column, so that column is named here: the cell
                        // frame alone cannot say which of its numbers was measured.
                        let reduced = match which {
                            Cut::Reduce(field, _) | Cut::CutReduce(field, _) => Some(field),
                            _ => None,
                        };
                        crate::transform::share_cells(&cells, ts, reduced)
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
                        // Every channel that splits splits the statistic: the
                        // transform runs within each combination and tags every
                        // output row with each column, so a histogram split by
                        // species is three histograms and the renderer can color,
                        // dash or shape them. Which channels split is `legality`'s
                        // to state, because a check that counts rows per group has
                        // to count the groups this draw actually makes.
                        let group_fields = crate::legality::split_fields_of(layer);
                        // The layer's one smoothing length, along the axis the
                        // estimate spreads over: the key's for a curve, the measure's
                        // for a violin, told apart by the key column's type as
                        // `density` tells them apart. Handed down as a stated
                        // bandwidth, so `adjust` is already inside it.
                        let spreads_x = on_x != input.str_col(key).is_some();
                        // **A stated end is the estimate's floor or ceiling**
                        // (`DensitySpec::support`): `x(gdp, limits = c(0, NA))` says
                        // no value lies below 0, and the estimate is drawn from 0 and
                        // corrected there. Only an end the author wrote: an end the
                        // page wrote for a shared axis is a layout, not a claim about
                        // the data. In the units the estimate is made in: a curve's
                        // key is logged above, a violin's measure is not.
                        let support = {
                            let (ch, page_ends) = if spreads_x {
                                (Channel::X, self.fit.shared_domain_x)
                            } else {
                                (Channel::Y, self.fit.shared_domain_y)
                            };
                            let logged = (spreads_x == on_x) && key_log;
                            let units = |v: Option<f64>| v.and_then(|v| match logged {
                                true if v > 0.0 => Some(scale::to_log(v, key_base)),
                                true => None,
                                false => Some(v),
                            });
                            match spec.axis_def(&ch).and_then(|d| d.limits) {
                                Some([lo, hi]) => (
                                    units(lo.filter(|_| !page_ends[0])),
                                    units(hi.filter(|_| !page_ends[1])),
                                ),
                                None => (None, None),
                            }
                        };
                        let stated = layer.density.clone().unwrap_or_default();
                        let smoothed = smoothing.along(spreads_x)
                            .map(|h| crate::ir::DensitySpec {
                                bandwidth: Some(h),
                                adjust: None,
                                support,
                                ..stated.clone()
                            })
                            .or_else(|| (support != (None, None))
                                .then(|| crate::ir::DensitySpec { support, ..stated.clone() }));
                        let density = smoothed.as_ref().or(layer.density.as_ref());
                        let done = crate::transform::apply(&input, &layer.transforms, key, out, layer.bin.as_ref(), cut.axis(on_x), density, layer.range.as_ref(), layer.level_spec(), layer.deviation.as_ref(), layer.quantile.as_ref(), layer.r#box.as_ref(), layer.bounds.as_ref(), layer.stack.as_ref(), layer.cumulative.as_ref(), &group_fields);
                        // **The measure under its own name as well** (spec §8, a
                        // written column is a column). It arrives under whatever the
                        // measure axis was called, the empty string when `y()` is left
                        // out, so `size(count)` found no `count` on the rows it draws
                        // and was dropped. A copy, before the log scaling, which reads
                        // only the axis's column: a size decodes the tally, not its
                        // place. A violin's estimate is a width, not this column.
                        let done = match crate::transform::cell_measure(&layer.transforms) {
                            Some(name) if name != out && done.float_col(name).is_none()
                                && crate::legality::slot_density(spec, layer, Some(&input)).is_none() =>
                            {
                                match done.float_col(out).cloned() {
                                    Some(col) => done.with_float(name, col),
                                    None => done,
                                }
                            }
                            _ => done,
                        };
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
                        // A running total is drawn on the measure's scale too.
                        let done = match out_log && crate::transform::accumulates(&layer.transforms) {
                            true => scale::log_column(&done, crate::transform::CELL_CUMULATIVE, out_base),
                            false => done,
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
        // it was before there were moments at all. Which levels each panel shows is
        // `panel_levels`, above, where the smoothing needed it first.
        let npanels = panel_levels.len();
        let mut panel_eff: Vec<Vec<DataFrame>> = Vec::with_capacity(nframes * npanels);
        for f in 0..nframes {
            let frame = play_levels.get(f);
            for filters in &panel_sets {
                panel_eff.push(eff_for(filters, frame));
            }
        }

        // **A share of a statistic is drawn from `proportion`** (F2, and the feature
        // that replaced the `label(population)` refusal): the transform keeps the
        // statistic's values under the column it read and writes the shares to
        // `proportion`, so the measure field follows here, after every transform has
        // run and before anything is fitted. Every other frame copies its own measure
        // under that name, so a layer beside the shares draws as it did.
        let shares_measure = spec.layers.iter().any(|l| crate::transform::shares_a_statistic(&l.transforms)
            && !crate::legality::has_no_measure_axis(&l.mark));
        if shares_measure {
            let measure = if horizontal { x_field } else { y_field };
            for frames in panel_eff.iter_mut() {
                for df in frames.iter_mut() {
                    if df.float_col(crate::transform::CELL_SHARE).is_none() {
                        if let Some(col) = df.float_col(measure).cloned() {
                            *df = df.clone().with_float(crate::transform::CELL_SHARE, col);
                        }
                    }
                }
            }
        }
        let (x_field, y_field) = match (shares_measure, horizontal) {
            (true, true) => (crate::transform::CELL_SHARE, y_field),
            (true, false) => (x_field, crate::transform::CELL_SHARE),
            _ => (x_field, y_field),
        };

        // **A running total is drawn from `cumulative`** (spec §5, 2026-10-11), by
        // the share's rule just above: the transform writes the totals beside the
        // measure, so the measure field follows here, and every other frame copies
        // its own measure under that name, so a layer beside the totals draws as it
        // did.
        let totals_measure = spec.layers.iter().any(|l| crate::transform::accumulates(&l.transforms)
            && !crate::legality::has_no_measure_axis(&l.mark));
        if totals_measure {
            let measure = if horizontal { x_field } else { y_field };
            for frames in panel_eff.iter_mut() {
                for df in frames.iter_mut() {
                    if df.float_col(crate::transform::CELL_CUMULATIVE).is_none() {
                        if let Some(col) = df.float_col(measure).cloned() {
                            *df = df.clone().with_float(crate::transform::CELL_CUMULATIVE, col);
                        }
                    }
                }
            }
        }
        let (x_field, y_field) = match (totals_measure, horizontal) {
            (true, true) => (crate::transform::CELL_CUMULATIVE, y_field),
            (true, false) => (x_field, crate::transform::CELL_CUMULATIVE),
            _ => (x_field, y_field),
        };

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
            // **A map with no longitude spans the world** (and with no latitude, the
            // world's height): a lone `rule + y(lat)` read nothing on `x`, so the
            // panel had no width and drew as a sliver (2026-10-13, the mark × space
            // grid; the author: "yes do"). A position the data fills keeps its own
            // extent, and a stated `limits` still decides (`map_domain`).
            let (lon_span, lat_span) = (span(x_field), span(y_field));
            let degrees = match (lon_span, lat_span) {
                (None, None) => None,
                (lo, la) => Some((lo.unwrap_or((-180.0, 180.0)), la.unwrap_or((-90.0, 90.0)))),
            };
            let geo = crate::render::geo::Geo::new(view);
            for frames in panel_eff.iter_mut() {
                for (li, df) in frames.iter_mut().enumerate() {
                    // A path is cut where it crosses the seam, in degrees, before
                    // anything is projected (`geo::cut_at_seam`); its strokes split
                    // by a categorical color, a group or a pattern, as it draws them.
                    if let Some(layer) = spec.layers.get(li).filter(|l| l.mark == Mark::Path) {
                        let series: Vec<&str> = [Channel::Color, Channel::Group, Channel::Pattern].iter()
                            .filter_map(|ch| layer.encodings.get(ch).map(|c| c.field.as_str()))
                            .filter(|f| df.str_col(f).is_some())
                            .collect();
                        *df = crate::render::geo::cut_at_seam(df, x_field, y_field, &series);
                    }
                    *df = crate::render::geo::project_frame(df, &geo, x_field, y_field);
                }
            }
            degrees.map(|d| (geo, d, lon_span.is_none(), lat_span.is_none()))
        } else {
            None
        };
        // A stated domain on a map is written in degrees, and the panel is fitted in
        // projected units, so the box is projected once here for every fit below
        // (`Geo::project_domain`). Read as projected numbers, `limits = c(-12, 40)`
        // drew an empty panel with its tick labels piled on one spot. The rows
        // outside the box are already gone, cut in degrees by `limit_cut` before
        // anything was projected.
        let map_domain = map_degrees.as_ref().map(|(geo, (_, lat), no_lon, no_lat)| {
            // A position nothing fills spans the world, unless `limits` says
            // otherwise: the whole turn of longitude, the whole height of latitude.
            let world = |stated: (Option<f64>, Option<f64>), empty: bool, (lo, hi): (f64, f64)| match empty {
                true => (stated.0.or(Some(lo)), stated.1.or(Some(hi))),
                false => stated,
            };
            geo.project_domain(
                world(scale::domain_of(spec.axis_def(&Channel::X)), *no_lon, (-180.0, 180.0)),
                world(scale::domain_of(spec.axis_def(&Channel::Y)), *no_lat, (-90.0, 90.0)),
                *lat,
            )
        });
        let fit_domain = |ch: Channel| match (&map_domain, &ch) {
            (Some((x, _)), Channel::X) => *x,
            (Some((_, y)), Channel::Y) => *y,
            _ => scale::domain_of(spec.axis_def(&ch)),
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
        // **A value a statistic wrote outside a domain the author stated has no
        // position** (spec §10; `unplace_outside_domains`). Not on a map, whose
        // positions are projected by now while its domain is stated in degrees.
        let stated_axes: Vec<StatedAxis> = if map_degrees.is_some() {
            Vec::new()
        } else {
            [(Channel::X, x_field, x_log, x_base), (Channel::Y, y_field, y_log, y_base)]
                .into_iter()
                .filter(|&(_, field, _, _)| !field.is_empty())
                .map(|(channel, field, log, base)| StatedAxis {
                    judged: authored_domain(channel.clone(), log, base),
                    quoted: authored_domain(channel.clone(), false, base),
                    channel, field,
                })
                .filter(|a| a.judged.0.is_some() || a.judged.1.is_some())
                .collect()
        };
        let mut outside = vec![(0usize, 0usize); stated_axes.len()];
        if !stated_axes.is_empty() {
            for frames in panel_eff.iter_mut() {
                for (k, (gone, of)) in unplace_outside_domains(spec, frames, &stated_axes).into_iter().enumerate() {
                    outside[k].0 += gone;
                    outside[k].1 += of;
                }
            }
        }

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
        warn_outside_domains(&mut remarks, &stated_axes, &outside);

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
        //
        // Which axes `order()` sorts. A column of the other categorical axis is
        // that axis's own order (spec §9), and it holds no number to rank this
        // axis by, so this axis keeps its own order. Every categorical axis used
        // to take the plot's `order`, and one that did not own the column ranked
        // by it as by any other: under a tally that is the row count whatever
        // the column, so `zone * count + x(month) + y(category) +
        // order(category)` sorted the months by how many rows each had.
        let category_axis = |other: &str| spec.layers.iter()
            .any(|l| ctx.resolve_data(&l.data).is_some_and(|df| df.str_col(other).is_some()));
        let order_for = |field: &str, other: &str| spec.order.as_ref()
            .filter(|o| o.field == field || o.field != other || !category_axis(other));
        let (order_x, order_y) = (order_for(x_field, y_field), order_for(y_field, x_field));
        // What `order()` ranks each category by when it names a column: the first
        // layer's own statistic over all of a category's rows, read from its
        // table rather than from its drawn pieces (`transform::category_ranks`).
        // Over the whole table, so every panel and every frame agrees on one order.
        // A statistic turned into a share (`sum * proportion`) writes `proportion`,
        // which the table does not hold; each share is its total over one sum, so
        // `order(proportion)` ranks by the statistic over the column it reads, bound
        // on the position that is not this axis (the measure field above is already
        // `proportion` by now).
        let ranked = |field: &str, order: Option<&crate::ir::OrderSpec>| -> Option<Vec<(String, f64)>> {
            let o = order.filter(|o| o.field != field)?;
            spec.layers.iter().find_map(|layer| {
                let df = ctx.resolve_data(&layer.data)?;
                df.str_col(field)?;
                let measure = if field == x_field { Channel::Y } else { Channel::X };
                let by = match o.field == crate::transform::CELL_SHARE
                    && crate::transform::shares_a_statistic(&layer.transforms)
                {
                    true => spec.position_for(layer, &measure)?.field.as_str(),
                    false => o.field.as_str(),
                };
                crate::transform::category_ranks(
                    df, field, by, &layer.transforms, layer.quantile.as_ref())
            })
        };
        let (ranked_x, ranked_y) = (ranked(x_field, order_x), ranked(y_field, order_y));
        // The order each category column has in the unsplit tables, for an axis
        // fitted over several panels or frames (`detect_categories`). A clustered
        // layer derives its own order, which no table holds, so it gets none.
        let natural = |field: &str| -> Option<Vec<String>> {
            if spec.layers.iter().any(|l| l.transforms.contains(&Transform::Cluster)) {
                return None;
            }
            let tables: Vec<&DataFrame> = spec.layers.iter()
                .filter_map(|l| ctx.resolve_data(&l.data))
                .filter(|df| df.str_col(field).is_some())
                .collect();
            (!tables.is_empty()).then(|| crate::data::categories_across(&tables, field))
        };
        let (natural_x, natural_y) = (natural(x_field), natural(y_field));
        let fit_axes = |panels: &[&Vec<DataFrame>]| -> PanelAxes {
            let eff: Vec<&DataFrame> = panels.iter().flat_map(|p| p.iter()).collect();
            let split = panels.len() > 1;
            let cat_x = detect_categories(&eff, order_x, x_field, false, ranked_x.as_deref(),
                natural_x.as_deref().filter(|_| split));
            let cat_y = detect_categories(&eff, order_y, y_field, true, ranked_y.as_deref(),
                natural_y.as_deref().filter(|_| split));
            // A composed page may have decided this axis's category order — a
            // clustered panel derived one, and a shared categorical axis takes
            // it (§9). The page's list arrives in axis order, is used only to
            // *re-order* what this plot already has, and never changes
            // membership: levels decide order, never membership, here too.
            let impose = |own: Option<Vec<String>>, page: &Option<Vec<String>>| {
                match (own, page) {
                    (Some(own), Some(page)) => {
                        let mut merged: Vec<String> = page.iter()
                            .filter(|c| own.contains(c)).cloned().collect();
                        let tail: Vec<String> = own.iter()
                            .filter(|c| !merged.contains(c)).cloned().collect();
                        merged.extend(tail);
                        Some(merged)
                    }
                    (own, _) => own,
                }
            };
            let cat_x = impose(cat_x, &self.fit.cats_x);
            let cat_y = impose(cat_y, &self.fit.cats_y);

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

        // The ticks are chosen here without the panel's width, which only the
        // layout knows. Where their labels then collide, in a narrow panel or a
        // short one, the crowded-labels block after `PanelGrid::compute` draws one
        // in every `stride` (ruled 2026-09-27; the rule before it drew every tick
        // and let the labels overprint). Coarsening the step here instead was
        // tried while the range still followed the ticks, and the ceiling climbed
        // with the step; `fit_axis` pins the range to the data now, but thinning
        // after the layout needs no guess at the width. The one coarsening here is
        // `ticks::MAX_TICKS`, a ceiling on the count whatever the width, and it is
        // said out loud below.
        let (x_ticks, xs, x_extent) = build_axis(
            &eff, &bar_frames, &x_sides, x_field, cat_x.as_deref(),
            has_plain_bar && !horizontal,
            has_any_bar && horizontal,
            flush_x,
            scale::tick_count_of(spec.axis_def(&Channel::X)),
            x_log, x_base, x_time,
            fit_domain(Channel::X),
            crate::legality::slot_reach(spec, data, Channel::X),
            self.fit.ticks_x,
            self.label_room.map_or((None, None), |r| r[0]),
        );
        let (y_ticks, ys, y_extent) = build_axis(
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
            fit_domain(Channel::Y),
            crate::legality::slot_reach(spec, data, Channel::Y),
            self.fit.ticks_y,
            self.label_room.map_or((None, None), |r| r[1]),
        );
        // **An axis nobody bound carries no scale.** A lone `rule + y(life)`, or a
        // `zone` bounded on one axis, spans the other axis and reads nothing on it,
        // so `build_axis` fitted that axis to a placeholder 0 to 1 and printed ticks
        // for a quantity that is not there (found 2026-10-13 walking the mark ×
        // channel grid; the author: "yes do what you recommended"). The empty name
        // is not enough to tell: with no position bound a `count` publishes its
        // tallies under it, and that axis is real. So unbound means no field, no
        // column under the empty name, and no bounded zone's sides on the axis. The
        // panel still spans it; only the ticks, their labels and their gridlines go.
        let unbound = |field: &str, sides: &[(&DataFrame, &str)]| field.is_empty()
            && sides.is_empty()
            && eff.iter().all(|df| df.float_col("").is_none() && df.str_col("").is_none());
        let x_ticks = if unbound(x_field, &x_sides) { TickSpec::empty() } else { x_ticks };
        let y_ticks = if unbound(y_field, &y_sides) { TickSpec::empty() } else { y_ticks };

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
        // Only a plain numeric axis may choose its ticks again over the cells, with
        // the count `build_axis` chose with (5 unless stated). A partition's axes
        // always do, and every other mesh only when the center-chosen ticks miss.
        // A **flow** as well: its count axis is ticked over the slots' centers,
        // whose span depends on which stages are named, so `flow(class, survived)`
        // labeled 500 / 1000 / 1500 on the same 0 to 2201 that `flow(class, sex,
        // survived)` labeled 0K / 1K / 2K. The whole range is what it measures.
        let flows = spec.layers.iter().any(|l| l.transforms.contains(&Transform::Flow));
        let retick = |ch: Channel, cats: bool, log: bool, time: bool| {
            match (!cats && !log && !time, partitions || flows) {
                (false, _) => Retick::Never,
                (true, true) => Retick::Always(scale::tick_count_of(spec.axis_def(&ch)).unwrap_or(5)),
                (true, false) => Retick::WhenMissed(scale::tick_count_of(spec.axis_def(&ch)).unwrap_or(5)),
            }
        };
        let (x_ticks, xs) = fit_to_cells(
            &cell_frames, crate::transform::CELL_START, crate::transform::CELL_END,
            crate::transform::CELL_X, crate::transform::CELL_DX, x_ticks, xs,
            stated_domain(fit_domain(Channel::X), x_log, x_base),
            retick(Channel::X, cat_x.is_some(), x_log, x_time.is_some()));
        let (y_ticks, ys) = fit_to_cells(
            &cell_frames, crate::transform::CELL_LOWER, crate::transform::CELL_UPPER,
            crate::transform::CELL_Y, crate::transform::CELL_DY, y_ticks, ys,
            stated_domain(fit_domain(Channel::Y), y_log, y_base),
            retick(Channel::Y, cat_y.is_some(), y_log, y_time.is_some()));

        // A **network** is fitted to the unit square by statement, not from its
        // columns: the layout normalizes to [0, 1], the margin keeps a glyph at
        // the rim off the panel edge, and fitting from columns would repeat the
        // flow family's starvation defect one family later — an edge-only plot
        // carries its second endpoints in columns no fit reads.
        let (xs, ys) = if is_network { ((-0.06, 1.06), (-0.06, 1.06)) } else { (xs, ys) };

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
        //
        // A **stated** end is not widened. The stated domain is the turn (spec
        // §10), and the bins are cut on it, so their support ends there already;
        // half a slot more at each end opened a gap at the seam, a 35° one for
        // `x(bearing, limits = c(0, 360))`, and moved 0 off the top.
        let xs = if is_polar && cat_x.is_none() && !bar_frames.is_empty() {
            let wide = widen_to_slot_support(&bar_frames, x_field, xs);
            let stated = authored_domain(Channel::X, x_log, x_base);
            (if stated.0.is_some() { xs.0 } else { wide.0 }, if stated.1.is_some() { xs.1 } else { wide.1 })
        } else {
            xs
        };

        // **A flat histogram's end bins reach half a bin past their centers**, and
        // its axis was fitted to the centers with the ordinary breathing, which is
        // less than half a bin below eleven bins: the panel's clip sliced a quarter
        // off each end bar of a six-bin histogram, and 40% at three bins. The bins'
        // outer edges now join the range, the reach a plain bar's slots are given
        // already (`bar_x_ticks_eff`). From eleven bins the breathing covers them
        // and nothing moves.
        let (xs, ys) = if cell_space == crate::legality::SpaceKind::Flat {
            let binned: Vec<&DataFrame> = spec.layers.iter().enumerate()
                .filter(|(_, l)| l.mark == Mark::Bar && l.transforms.contains(&Transform::Bin))
                .flat_map(|(i, _)| panels.iter().map(move |p| &p[i]))
                .collect();
            let reach = |field: &str, range: (f64, f64)| match slot_support(&binned, field) {
                Some((lo, hi)) => (range.0.min(lo), range.1.max(hi)),
                None => range,
            };
            match (horizontal, cat_x.is_some(), cat_y.is_some()) {
                (false, false, _) => (reach(x_field, xs), ys),
                (true, _, false) => (xs, reach(y_field, ys)),
                _ => (xs, ys),
            }
        } else {
            (xs, ys)
        };

        // A bar that divides one slot has no position axis at all (spec §15), so
        // there is no range to fit and nothing to tick. Give it a unit slot centered
        // on zero, which is what `write_bars` places every element at, and an empty
        // tick list so no axis is drawn under a chart that has none.
        let one_slot = x_field.is_empty()
            && spec.layers.iter()
                .any(|l| crate::legality::bar_is_one_pile(spec, l, ctx.resolve_data(&l.data)));
        let (x_ticks, xs) = if one_slot {
            (TickSpec::empty(), (-0.5, 0.5))
        } else {
            (x_ticks, xs)
        };
        // The horizontal pile has the same one slot, on `y` (F8).
        let one_slot_y = y_field.is_empty()
            && spec.layers.iter()
                .any(|l| crate::legality::bar_is_one_pile_sideways(spec, l, ctx.resolve_data(&l.data)));
        let (y_ticks, ys) = if one_slot_y {
            (TickSpec::empty(), (-0.5, 0.5))
        } else {
            (y_ticks, ys)
        };

        // The z-axis, when there is one. Built by the same routine as x and y —
        // a plain continuous axis (no bars stand on z, none measure it, and it
        // never carries a baseline), so most of `build_axis`'s cases fall away.
        // `zs` normalizes the third coordinate into the unit cube; `z_ticks`
        // labels the frame. A log `z` is refused with direction in `legality`
        // rather than half-drawn here, so this stays linear; a date on `z` is
        // read off its column, as it is for `x` and `y`.
        //
        // The tick count is read here too, and it was not before: this call passed
        // a hard `None` while `z_axis.tick_count` existed in the IR, so the third
        // axis would have ignored the field even once a binding could write it.
        // Two absences wearing one name, and only the first was recorded.
        // The globe fits `z` too: a spike measures along the radius, and its
        // scale is fitted across panels like every other so two faceted globes
        // stay comparable. The spike's own baseline is the surface — the writer
        // reads the fitted top and measures from zero.
        let (z_ticks, zs) = if network_cube.is_some() {
            // The cube form: the layout filled [0, 1], stated for `x`/`y`'s
            // reason, with no ticks to select.
            (TickSpec::empty(), (-0.06, 1.06))
        } else if is_3d || (is_globe && !z_field.is_empty()) {
            // No `sides`: a `zone` is refused in the cube (`mark_draws_in_space`),
            // so nothing places itself on `z` other than through the column.
            // A date on `z` is ticked on the calendar, as it is on `x` and `y`: it
            // passed no unit here and drew its moments as epoch seconds, `1710M`
            // where the same column on `x` reads `Mar 4`.
            let (t, range, _) = build_axis(&eff, &[], &[], z_field, None, false, false, false,
                       scale::tick_count_of(spec.axis_def(&Channel::Z)),
                       false, 10.0, detect_time(&ctx, z_field),
                       scale::domain_of(spec.axis_def(&Channel::Z)),
                       // A violin stands on a *flat* slot axis; the cube has none.
                       (0.0, 0.0),
                       // A page shares `x` and `y`; the cube's third axis is its own.
                       None,
                       // A label in the cube is not given room on an axis.
                       (None, None));
            (t, range)
        } else {
            (TickSpec::empty(), (0.0, 1.0))
        };

            PanelAxes {
                x_ticks, xs, y_ticks, ys, z_ticks, zs, cat_x, cat_y, inner_edge,
                x_extent, y_extent,
            }
        };

        // The shared fit: every panel at once, which is what makes the panels
        // comparable and is the default (spec §11).
        let every_panel: Vec<&Vec<DataFrame>> = panel_eff.iter().collect();
        let shared = fit_axes(&every_panel);
        let PanelAxes {
            x_ticks, xs, y_ticks, ys, z_ticks, zs, cat_x, cat_y, inner_edge, ..
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
        //
        // **The count is the caller's here too.** These are the degree ticks, chosen
        // after the fit, so a `tick_count` on `x` or `y` has to reach them or it
        // reaches nothing: the map drew 7 and 5 whatever was written, and
        // `x(east, tick_count = 40)` rendered byte for byte as the default did, with
        // no message. It is counted over the degrees the map shows, and a count past
        // the ceiling widens as every other axis's does, and says so.
        //
        // **The edge is the panel's, and so is the test for a tick.** The meridian
        // was met at the data's southernmost latitude and every gridline drawn
        // straight up from there, so a gridline was right along that one parallel
        // and off by up to 20 degrees elsewhere: on the Fiji plot the 180° line lay
        // 192 px left of a point at 180° and -20°. A tick was also kept on its
        // degrees alone, so 170° was labeled outside the panel. So the latitudes at
        // the panel's own bottom and top are found (`Geo::lat_at_y`), each tick is
        // placed where its line meets the panel's edge and kept only if that is
        // inside the panel, and the meridians are drawn as the curves they are
        // (`write_grid`), which under Mercator are the straight lines they were.
        let (x_ticks, y_ticks, meridians) = match &map_degrees {
            Some((geo, (lon, lat), _, _)) => {
                let edge = (geo.lat_at_y(ys.0.min(ys.1)), geo.lat_at_y(ys.0.max(ys.1)));
                let degrees = |(lo, hi): (f64, f64), n: usize, at: &dyn Fn(f64) -> f64, panel: (f64, f64)| {
                    let picked = crate::render::ticks::nice_ticks_within(lo, hi, n, Some((lo, hi)));
                    let (pmin, pmax) = (panel.0.min(panel.1), panel.0.max(panel.1));
                    let slack = 1e-9 * (pmax - pmin).abs().max(1e-12);
                    let kept: Vec<(f64, f64, String)> = picked
                        .values
                        .iter()
                        .zip(picked.labels.iter())
                        .filter(|(v, _)| **v >= lo && **v <= hi)
                        .map(|(v, l)| (*v, at(*v), format!("{l}°")))
                        .filter(|(_, p, _)| *p >= pmin - slack && *p <= pmax + slack)
                        .collect();
                    let spec = TickSpec {
                        widened: picked.widened.clone().map(|w| crate::render::ticks::Widening {
                            asked: format!("{}°", w.asked),
                            drawn: format!("{}°", w.drawn),
                        }),
                        ..crate::render::ticks::ticks_with_labels(
                            kept.iter().map(|k| k.1).collect(),
                            kept.iter().map(|k| k.2.clone()).collect(),
                        )
                    };
                    (spec, kept.iter().map(|k| k.0).collect::<Vec<f64>>())
                };
                let count = |c: Channel, default: usize| {
                    scale::tick_count_of(spec.axis_def(&c)).unwrap_or(default)
                };
                // The ticks span what the panel spans: a stated end in degrees,
                // else the data's, so a map cut to a box is ticked across the box.
                let span = |ch: Channel, data: (f64, f64)| {
                    let (lo, hi) = scale::domain_of(spec.axis_def(&ch));
                    (lo.unwrap_or(data.0), hi.unwrap_or(data.1))
                };
                let (lon, lat) = (span(Channel::X, *lon), span(Channel::Y, *lat));
                let (xt, lons) = degrees(lon, count(Channel::X, 7), &|v| geo.project(v, edge.0).0, xs);
                let (yt, _) = degrees(lat, count(Channel::Y, 5), &|v| geo.project(lon.0, v).1, ys);
                (xt, yt, Some((*geo, lons, edge)))
            }
            None => (x_ticks, y_ticks, None),
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
                        x_extent: if free_x { own.x_extent } else { shared.x_extent },
                        y_extent: if free_y { own.y_extent } else { shared.y_extent },
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
            // A statistic's result divided into shares is read as shares, whatever
            // column the statistic read (F2): `bar * sum * proportion +
            // y(population)` titled its axis Population over 0 to 0.6.
            // A heatmap measures by color, so its share is titled on the key instead.
            if measured && spec.layers.iter().any(|l| crate::transform::shares_a_statistic(&l.transforms)
                && !crate::legality::has_no_measure_axis(&l.mark))
            {
                return "Proportion".to_string();
            }
            // A running total is read as a total, whatever column it adds up.
            if measured && spec.layers.iter().any(|l| crate::transform::accumulates(&l.transforms)
                && !crate::legality::has_no_measure_axis(&l.mark))
            {
                return "Cumulative".to_string();
            }
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
        //
        // **A name the reader wrote is kept**, on this axis and on a displaced pile's
        // below. What earns silence is what gog would *derive*: numbers nobody looks
        // up, and the column's own name under them. A written name is the reader's
        // word for the quantity, chosen knowing all this, the rule `axis_label`
        // follows for a filled pile's `Share`; dropping it was accepted and silent,
        // the one thing §12 forbids. An axis that does not exist at all (`nest`, a
        // network, a globe) is different, and refuses the name instead.
        let (y_ticks, y_label) = match depth_is_the_radius {
            true => (TickSpec::empty(),
                     spec.y_axis.label.clone().unwrap_or_default()),
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
        // of what a streamgraph asks a reader to do. And a name the reader wrote stays,
        // as it does on the ring: it names the quantity a thickness is read in, which
        // is still true of every band.
        let displaced = spec.layers.iter().any(|l| {
            l.transforms.contains(&Transform::Stack)
                && l.stack.as_ref().and_then(|s| s.baseline.as_deref())
                    .is_some_and(|b| b != "zero")
        });
        let written = |label: &Option<String>| label.clone().unwrap_or_default();
        let (x_ticks, x_label, y_ticks, y_label) = match (displaced, horizontal) {
            (true, true) => (TickSpec::empty(),
                             written(&spec.x_axis.label), y_ticks, y_label),
            (true, false) => (x_ticks, x_label,
                              TickSpec::empty(),
                              written(&spec.y_axis.label)),
            (false, _) => (x_ticks, x_label, y_ticks, y_label),
        };

        // **A stated count past `ticks::MAX_TICKS` is said out loud** (spec §12).
        // The step widened until the ticks fit, which keeps the axis labeled end to
        // end, but the count the caller wrote is not the count drawn. Read off the
        // ticks each axis actually draws, a freed axis's per panel, so an axis whose
        // ticks were replaced above (a map's degrees, a displaced pile) says
        // nothing. The cube's thinned labels are the other half of this rule: that
        // one is about labels that do not fit, this one is about the step.
        for (c, channel, free, shared) in [
            ("x", Channel::X, free_x, &x_ticks),
            ("y", Channel::Y, free_y, &y_ticks),
            ("z", Channel::Z, free_z, &z_ticks),
        ] {
            let def = spec.axis_def(&channel);
            let Some(asked) = scale::tick_count_of(def) else { continue };
            let drawn: Vec<&TickSpec> = match free {
                true => per_panel.iter().map(|a| a.ticks(&channel)).collect(),
                false => vec![shared],
            };
            if let Some(t) = drawn.into_iter().find(|t| t.widened.is_some()) {
                let field = def.map_or("", |d| d.field.as_str());
                remarks.extend(widened_ticks(c, field, asked, t));
            }
        }

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
            let mut whole = eff_for(&[], None);
            unplace_outside_domains(spec, &mut whole, &stated_axes);
            eff_global = whole;
            &eff_global
        };
        let color_map = build_color_map(spec, color_frames, &mut remarks);
        let ramp = resolve_ramp(&spec.palette);
        // The other channels are fitted once too, per layer: every range over
        // every panel at every moment, and every category in the unsplit frame's
        // order (`Whole`). A writer draws one share of its layer and reads it
        // against these; the legend decodes them. Fitted per share, they drew a
        // played bubble chart with each year's largest country at the largest
        // size, whatever its population.
        //
        // **A column a `shape` or `pattern` maps is one set of categories across the
        // plot**, joined from each layer's own order in layer order, which is how the
        // color map joins its categories (`palette::build_color_map`). Each layer
        // ordering its own table's categories drew a second table's first category in
        // the first table's first glyph, under a key of the first table's alone.
        let own = |li: usize| Whole {
            unsplit: &color_frames[li],
            shares: panel_eff.iter().map(|frames| &frames[li]).collect(),
            shared: None,
        };
        let mut plot_categories: HashMap<String, Vec<String>> = HashMap::new();
        for (li, layer) in spec.layers.iter().enumerate() {
            for channel in [Channel::Shape, Channel::Pattern] {
                let Some(def) = layer.encodings.get(&channel) else { continue };
                let order = plot_categories.entry(def.field.clone()).or_default();
                for c in own(li).own_categories(&def.field) {
                    if !order.contains(&c) {
                        order.push(c);
                    }
                }
            }
        }
        let wholes: Vec<Whole> = (0..spec.layers.len())
            .map(|li| Whole { shared: Some(&plot_categories), ..own(li) })
            .collect();

        let legends = collect_legends(&ctx, &color_map, color_frames, &wholes);
        // **Which side the keys sit on, and the room they take there**
        // (`theme(legend = )`). Beside the plot the cards stand in a column and
        // cost width: the widest card and the gap that parts it from the plot.
        // Above or below the plot they are laid across (`legend::Across`) and cost
        // height, wrapped to the canvas less an edge on each side. A card too wide
        // for that even with one key to a line is left out and said, the rule a
        // column too tall for its canvas already keeps (`write_legends`).
        let side = LegendSide::of(&spec.theme.resolved());
        let fonts = (self.font_sm, self.font_md);
        let across_w = (self.width - 2.0 * LEGEND_EDGE).max(0.0);
        let legends = match side.across() && !legends.is_empty() {
            false => legends,
            true => {
                let (_, left_out) = plan_across(&legends, fonts, across_w);
                if !left_out.is_empty() {
                    remarks.push(legend_too_wide_across(&legends, &left_out, fonts, across_w));
                }
                legends.into_iter().enumerate()
                    .filter(|(i, _)| !left_out.contains(i))
                    .map(|(_, b)| b)
                    .collect()
            }
        };
        let plan = side.across().then(|| plan_across(&legends, fonts, across_w).0);
        let plan_h = plan.as_ref().map_or(0.0, |p| p.height);
        let widest = legends.iter()
            .map(|b| b.width(self.font_sm, self.font_md))
            .fold(0.0_f64, f64::max);
        let room = match (side, legends.is_empty()) {
            (_, true) => LegendRoom::default(),
            (LegendSide::Right, false) => LegendRoom { right: LEGEND_PLOT_GAP + widest, ..LegendRoom::default() },
            (LegendSide::Left, false) => LegendRoom {
                left: LEGEND_EDGE + widest + LEGEND_PLOT_GAP, ..LegendRoom::default()
            },
            (LegendSide::Top, false) => LegendRoom { top: plan_h + LEGEND_ACROSS_GAP, ..LegendRoom::default() },
            (LegendSide::Bottom, false) => LegendRoom { bottom: LEGEND_ACROSS_GAP + plan_h, ..LegendRoom::default() },
        };

        // A 3-D plot reserves no margin for 2-D tick labels or axis titles —
        // its axes live on the cube's edges, inside the panel — so the layout is
        // fed empty ones and the cube takes the whole panel. The ranges above
        // are still needed to normalize coordinates; only the layout hints change.
        let no_ticks = TickSpec::empty();
        // The places a flat shared flow names beyond its slots, each at its slot's
        // middle on the count axis, read off the first panel's `text * flow`
        // frame: the tick list the layout sizes their room from, and turns.
        let shared_names: Option<TickSpec> = if shared_flow && !is_polar {
            spec.layers.iter().position(|l| l.mark == Mark::Text && l.flow_is_shared())
                .and_then(|li| panel_eff.first().and_then(|f| f.get(li)))
                .and_then(|df| {
                    let (lo_name, hi_name) = if shared_down {
                        (crate::transform::CELL_START, crate::transform::CELL_END)
                    } else {
                        (crate::transform::CELL_LOWER, crate::transform::CELL_UPPER)
                    };
                    let names = df.str_col(crate::transform::NODE_NAME)?;
                    let (lo, hi) = (df.float_col(lo_name)?, df.float_col(hi_name)?);
                    let values = lo.iter().zip(hi).map(|(a, b)| (a + b) / 2.0).collect();
                    Some(TickSpec { values, labels: names.clone(), step: 1.0, widened: None })
                })
        } else {
            None
        };
        // A polar plot reserves no tick-label margin either — its angular labels
        // ring the circle and its radial ones run up the spoke, both *inside* the
        // panel. It keeps the axis names, though, unlike 3-D: they still say what
        // the angle and the radius measure, and there is nowhere in the circle to
        // put them.
        let (grid_xt, grid_yt, grid_xl, grid_yl): (&TickSpec, &TickSpec, &str, &str) =
            if is_3d || network_cube.is_some() {
                (&no_ticks, &no_ticks, "", "")
            } else if is_nest || is_network {
                // A packing has no axes at all — not axes drawn elsewhere, as in
                // 3-D and polar, but none. So it reserves no margin for either, and
                // it keeps no axis names: `check_nest` refuses `x_label()`/
                // `y_label()` outright rather than letting one be set and dropped,
                // and the *derived* names (a column's own) would be worse still,
                // since they would promise that the direction under them measures
                // that column.
                (&no_ticks, &no_ticks, "", "")
            } else if is_globe {
                // A globe draws no axes at all — the first space with none. The
                // sphere has no straight side to write one along, so no margin is
                // reserved and no axis names are kept: `check_globe` refuses
                // `x_label()`/`y_label()` rather than letting one be set and
                // dropped (`nest`'s own rule), and the graticule is the panel
                // grid's reading here, drawn inside the disk.
                (&no_ticks, &no_ticks, "", "")
            } else if is_polar {
                (
                    &no_ticks, &no_ticks,
                    if draw_x_axis { x_label.as_str() } else { "" },
                    if draw_y_axis { y_label.as_str() } else { "" },
                )
            } else {
                // An axis another plot on the page is drawing costs *this* plot
                // no margin — which is what puts a marginal histogram flush
                // against the scatter below it rather than a tick label's worth
                // of white away from it. The ticks themselves are unchanged, so
                // the gridlines still stand where the shared axis says they do.
                //
                // A shared flow's place names stand where its count axis's numbers
                // would, so the layout sizes that margin from them, turned as
                // crowded category names turn, and the `text` layer writes them.
                //
                // **A freed axis costs the outer margin nothing either**: every cell
                // already owes its own numbers (`cell_axis`) and writes them beside
                // itself, so the shared scale's labels, never drawn there, sized a
                // band of white on the left of the first column (or under the last
                // row), as wide as the shared labels and wider than the panels' own.
                let names = shared_names.as_ref();
                (
                    if draw_x_axis && !free_x { &x_ticks } else if shared_down { names.unwrap_or(&no_ticks) } else { &no_ticks },
                    if draw_y_axis && !free_y { &y_ticks } else if shared_flow && !shared_down { names.unwrap_or(&no_ticks) } else { &no_ticks },
                    if draw_x_axis { x_label.as_str() } else { "" },
                    if draw_y_axis { y_label.as_str() } else { "" },
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

        // A closure because crowded category names can ask for the layout twice
        // more (below): once turned, once thinned. Everything but the two tick
        // lists and the angle is the same question every time. The legend's
        // width and the page's fit are open as well, for the one question asked
        // before the layout is settled: whether the legend fits at all (below).
        let grid_with = |xt: &TickSpec, yt: &TickSpec, angle: Option<f64>, legend: LegendRoom, fit: Fit|
            PanelGrid::compute(
                self.width, self.height,
                (self.font_sm, self.font_md, self.font_lg),
                xt, yt, grid_xl, grid_yl,
                spec.title.is_some(),
                legend,
                // Cloned because `panel_levels` borrows both, and each panel now
                // states its own level when it writes itself out, which is after
                // this. One short list of level names per facet, copied once.
                col_values.clone(), row_values.clone(),
                map_ratio.or(spec.theme.resolved().ratio),
                angle,
                Self::axis_label_beside_theme(&spec.theme.resolved()),
                play_def.is_some(),
                facet_wrap,
                (free_x, free_y),
                cell_axis,
                fit,
            );
        let stated_angle = spec.theme.resolved().tick_angle;

        // **A legend never takes the panels' room.** Its width is set aside
        // beside the panels before they are laid out, so a plot too narrow for
        // it drew a panel of negative width, in silence: -47px for a scatter
        // keyed by continent at 150px wide, -34px as a marginal plot's right
        // cell. A legend that would leave the panels narrower than itself is left
        // out and said so, which is the height check's rule (`write_legends`) for
        // a legend too tall to fit, read across instead of down.
        //
        // Decided on the plot laid out alone, as a page's first pass lays it out,
        // so a page's two passes reach one answer: the second may drop a shared
        // axis and gain room, and a legend kept there but not in the measuring
        // pass would be drawn beside a panel the page had already placed.
        //
        // **Laid across, the same rule reads down.** A legend above or below the
        // plot that would leave the panels shorter than itself is left out and
        // said, with `theme(height = )` as the way to give it room.
        let (legends, room) = match legends.is_empty() {
            true => (legends, room),
            false => {
                let widest = match side {
                    LegendSide::Right => room.right - LEGEND_PLOT_GAP,
                    _ => widest,
                };
                let alone = grid_with(grid_xt, grid_yt, stated_angle, room, Fit::free());
                let fits = match side.across() {
                    false => alone.outer.w() >= widest,
                    true => alone.outer.h() >= plan_h,
                };
                if fits {
                    (legends, room)
                } else {
                    remarks.push(match side.across() {
                        false => legend_too_wide(&legends, widest, self.width),
                        true => legend_too_tall_across(&legends, side, plan_h, self.height),
                    });
                    (Vec::new(), LegendRoom::default())
                }
            }
        };
        let compute_grid = |xt: &TickSpec, yt: &TickSpec, angle: Option<f64>|
            grid_with(xt, yt, angle, room, self.fit.clone());
        let grid = compute_grid(grid_xt, grid_yt, stated_angle);

        // **Crowded category names: turn them, then thin them, never print one
        // through another** (spec §10's rule for labels, which the cube already
        // keeps). Measured against the panel the layout just made, since only it
        // knows how far apart two slots are. A category axis only: numbers are
        // short and never turn, and how many a narrow panel draws is a question
        // about the step. A freed axis is left alone, since its names differ per
        // panel and every cell sized its band before any of this.
        //
        // Names that fit side by side are untouched. Names that do not turn to
        // 90 degrees, unless the caller stated an angle, which is kept. If even
        // turned they collide, every `stride`-th one is drawn, and that is said
        // out loud with the fix, because a name not printed is information the
        // binding carried (§12).
        //
        // **Crowded numbers thin by the same rule** (ruled 2026-09-27). Numbers do
        // not turn, but a panel narrow or short enough still printed them through
        // each other. They are measured as names are, side by side on `x` and one
        // above another on `y`, and every `stride`-th is kept, starting where an
        // even axis lands on the wider step. A number not printed hides no data,
        // since the axis still carries the rest, so this is said only when the
        // caller wrote the count (`tick_count`), which the drawing then departs from.
        let x_labeled = !grid_xt.labels.is_empty();
        let y_labeled = !grid_yt.labels.is_empty();
        let x_names = !is_polar && !free_x && cat_x.is_some() && x_labeled;
        let y_names = !is_polar && !free_y && cat_y.is_some() && y_labeled;
        // A shared flow's place names under its count axis turn as category names
        // do. They are never thinned: the `text` layer writes every one. And they
        // are not numbers, so the numbers' thinning below must not take them.
        let shared_x_names = shared_names.is_some() && shared_down && x_labeled;
        let shared_y_names = shared_names.is_some() && !shared_down && y_labeled;
        // A map's degrees are placed by its projection, but each label is written
        // where its line meets the panel's straight edge, so the measure above
        // holds there too. Map axes kept every tick until seven maps in a row
        // printed "-100° 0° 100°" through each other. The labels' values are
        // projected positions rather than round degrees, so a thinned map axis
        // keeps every `stride`-th label from the first, as a log axis does.
        let projected = matches!(spec.coord, CoordSpace::Map(_));
        let x_numbers = !is_polar && !free_x && cat_x.is_none() && x_labeled && !shared_x_names;
        let y_numbers = !is_polar && !free_y && cat_y.is_none() && y_labeled && !shared_y_names;
        let across = crate::render::layout::name_pitch(self.font_sm);
        let widths = |t: &TickSpec| -> Vec<f64> {
            t.labels.iter().map(|s| estimate_text_width(s, self.font_sm)).collect()
        };
        let x_at = |g: &PanelGrid, t: &TickSpec| -> Vec<f64> {
            g.panels.first().map_or_else(Vec::new, |p|
                t.values.iter().map(|&v| p.rect.map_x(v, xs.0, xs.1)).collect())
        };
        let y_at = |g: &PanelGrid, t: &TickSpec| -> Vec<f64> {
            g.panels.first().map_or_else(Vec::new, |p|
                t.values.iter().map(|&v| p.rect.map_y(v, ys.0, ys.1)).collect())
        };
        let tick_angle = match x_names || shared_x_names {
            true => crate::render::layout::names_angle(
                &x_at(&grid, grid_xt), &widths(grid_xt), stated_angle),
            false => stated_angle,
        };
        let grid = if tick_angle != stated_angle { compute_grid(grid_xt, grid_yt, tick_angle) } else { grid };
        let x_linear = !x_log && x_time.is_none() && !projected;
        let y_linear = !y_log && y_time.is_none() && !projected;
        let x_stride = match (x_names, x_numbers, tick_angle) {
            (true, _, a) | (_, true, a @ Some(_)) => crate::render::layout::names_stride(
                &x_at(&grid, grid_xt), &widths(grid_xt), across, a.unwrap_or(0.0)),
            // Upright numbers, placed as they are drawn: in a grid of several
            // columns an edge number is drawn inward from its tick.
            (_, true, None) => grid.panels.first().map_or(1, |p|
                crate::render::layout::numbers_stride(
                    &grid_xt.values, &x_at(&grid, grid_xt), &widths(grid_xt),
                    p.rect.x0, p.rect.x1, grid.ncols > 1, x_linear)),
            _ => 1,
        };
        let y_stride = match (y_names, y_numbers) {
            (true, _) => crate::render::layout::names_stride(
                &y_at(&grid, grid_yt), &widths(grid_yt), across, 90.0),
            (_, true) => crate::render::layout::numbers_stride_stacked(
                &grid_yt.values, &y_at(&grid, grid_yt), &widths(grid_yt), across, y_linear),
            _ => 1,
        };
        // The height that would name every one: the frame around the panels as an
        // upright axis leaves it, plus a pitch per slot in every row.
        let taller = |span: f64| -> f64 {
            let rows = grid.nrows as f64;
            let frame = compute_grid(grid_xt, grid_yt, None).panels.first()
                .map_or(0.0, |p| self.height - rows * p.rect.h());
            ((frame + rows * span * across) / 10.0).ceil() * 10.0
        };
        let nearest = |at: Vec<f64>| -> f64 {
            at.windows(2).map(|w| (w[1] - w[0]).abs()).fold(f64::INFINITY, f64::min)
        };
        if x_stride > 1 && x_names {
            remarks.push(thinned_names('x', x_field, x_ticks.labels.len(), x_stride,
                nearest(x_at(&grid, grid_xt)), across, tick_angle, taller(xs.1 - xs.0)));
        }
        if y_stride > 1 && y_names {
            remarks.push(thinned_names('y', y_field, y_ticks.labels.len(), y_stride,
                nearest(y_at(&grid, grid_yt)), across, None, taller(ys.1 - ys.0)));
        }
        let x_asked = scale::tick_count_of(spec.axis_def(&Channel::X));
        let y_asked = scale::tick_count_of(spec.axis_def(&Channel::Y));
        let (x_chosen, y_chosen) = (x_ticks.labels.len(), y_ticks.labels.len());
        let x_ticks = match (x_stride > 1, x_names) {
            (true, true) => thin_names(&x_ticks, x_stride),
            (true, false) => thin_numbers(&x_ticks, x_stride, x_linear),
            _ => x_ticks,
        };
        let y_ticks = match (y_stride > 1, y_names) {
            (true, true) => thin_names(&y_ticks, y_stride),
            (true, false) => thin_numbers(&y_ticks, y_stride, y_linear),
            _ => y_ticks,
        };
        if let (true, true, Some(n)) = (x_stride > 1, x_numbers, x_asked) {
            remarks.push(thinned_numbers('x', x_field, n, x_chosen, x_ticks.labels.len()));
        }
        if let (true, true, Some(n)) = (y_stride > 1, y_numbers, y_asked) {
            remarks.push(thinned_numbers('y', y_field, n, y_chosen, y_ticks.labels.len()));
        }
        // The names keep sizing their room through every layout asked for below.
        let names = shared_names.as_ref().unwrap_or(&no_ticks);
        let grid_xt: &TickSpec = if shared_x_names { names } else if x_labeled { &x_ticks } else { &no_ticks };
        let grid_yt: &TickSpec = if shared_y_names { names } else if y_labeled { &y_ticks } else { &no_ticks };
        let grid = if x_stride > 1 || y_stride > 1 { compute_grid(grid_xt, grid_yt, tick_angle) } else { grid };

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
        self.write_header(&mut svg, &accessible_name(spec));
        // **A background that paints nothing paints nothing anywhere.** The theme's
        // `background` is the panel's fill, and the canvas behind the whole figure
        // was white whatever it said, so `transparent` made the panel show the
        // canvas, which is white: the figure stayed white everywhere and the page it
        // was placed on never showed through. The one reason to ask for it is that
        // page, so the canvas goes with the panel, and the legend's card with them
        // (`write_legends`). Any other color keeps the white canvas around the
        // panel it paints, as it always has.
        let see_through = spec.theme.resolved().background.as_deref()
            .is_some_and(crate::color::paints_nothing);
        // The ground a `text` halo takes when its color is unset: the panel's background.
        let ground = spec.theme.resolved().background_or(PANEL_BG);
        if !see_through {
            self.write_canvas(&mut svg);
        }

        // What every `point * dodge` swarm could not clear, over all panels and
        // moments, so a crowded plot says it once rather than once per panel.
        let mut swarm = crate::render::marks::SwarmTally::default();
        // What the globe's far side hides and what sinks below its surface, per
        // layer: (hidden, total, sunk), counted in every panel and said once after
        // the loop, so a faceted globe gives one line per table rather than one per
        // panel that no reader can tell apart.
        let mut globe_tally = vec![(0usize, 0usize, 0usize); spec.layers.len()];
        // What each path layer drew and left out, over all panels and moments: a
        // series of one position draws nothing, and is said once after the loop.
        let mut series_tally: Vec<Option<SeriesTally>> = spec.layers.iter()
            .map(|layer| SeriesTally::for_layer(spec, layer))
            .collect();
        // What `repel` could not separate, by layer, panel and moment, with a
        // selection's two passes added together: said once per layer after the
        // loop, from each panel's most crowded moment, naming the panels.
        let mut repel_crowding: HashMap<(usize, usize, usize), crate::render::marks::Crowding> =
            HashMap::new();
        // How far every flat label's letters reach past its point, over all panels
        // and moments, for the room the axes give them below.
        let mut label_ink: Vec<crate::render::marks::LabelInk> = Vec::new();

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
            // A chord diagram's place names stand outside its ring, turned outward,
            // so the ring leaves them the room the widest one takes. A pie keeps
            // that room, having no names round its rim.
            let ring_names = spec.layers.iter()
                .filter(|ly| ly.mark == Mark::Text && ly.flow_is_shared())
                .filter_map(|ly| {
                    let df = data.get(ly.data.as_ref().or(spec.data.as_ref())?)?;
                    let fs = ly.style.size.unwrap_or(self.font_md);
                    Some(ly.flow.as_ref()?.stages.iter().filter_map(|c| df.str_col(c))
                        .flat_map(|col| col.iter())
                        .map(|v| estimate_text_width(v, fs))
                        .fold(0.0, f64::max))
                })
                .fold(0.0, f64::max);
            let pol = pol.map(|p| if ring_names > 0.0 {
                p.leave_room(l, ring_names + crate::render::polar::RING_NAME_GAP + 4.0)
            } else {
                p
            });
            // The globe's panel is the disk, built once per panel like `Polar`
            // and `Scene` so the frame, the graticule and every mark read one
            // fit. Its frame routine owns the background, the clip and the
            // gridlines, the way polar's does.
            // Spikes need headroom: a `bar` measures out along the radius, so
            // the sphere shrinks by the spike ceiling and every tip stays on
            // the panel instead of being shaved by the clip.
            let spiked = !z_field.is_empty()
                && spec.layers.iter().any(|ly| ly.mark == Mark::Bar);
            let glb = globe_view.map(|v| crate::render::globe::Globe::new(
                l, v, GLOBE_INSET,
                if spiked { crate::render::globe::SPIKE_MAX } else { 0.0 }));
            match (&glb, &pol) {
                (Some(g), _) => self.write_globe_frame(&mut svg, g, &clip,
                                                       &spec.theme.resolved()),
                (None, Some(p)) => self.write_polar_frame(&mut svg, p, x_ticks, xs, y_ticks, ys,
                                                          &clip, &spec.theme.resolved(), inner_edge),
                (None, None) => self.write_panel_background(&mut svg, l, &clip,
                                                            &spec.theme.resolved()),
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
                    .map(|d| (d.field.as_str(), play_levels.as_slice(), clock)))
                    .flatten(),
                place,
            };
            self.write_brush_frame(&mut svg, spec, l, xs, ys, cat_x.as_ref(), cat_y.as_ref(),
                                   x_field, y_field, !is_nest && !is_3d && !is_globe && !is_network,
                                   x_log.then_some(x_base), y_log.then_some(y_base), &facts);
            // Which subset this panel shows at moment `fi`. Moments are the outer
            // stride, so at one frame this is exactly the index it always was.
            let eff_at = |fi: usize| &panel_eff[fi * npanels + panel.slot];

            // The series each path layer draws here, at every moment it plays. A
            // layer that does not play holds the same rows at every moment, so it is
            // counted once, as the globe's tally counts it.
            let panel_name = (grid.panels.len() > 1).then(|| {
                match panel_levels.get(panel.slot).copied().unwrap_or((None, None)) {
                    (Some(c), Some(r)) => format!("{c} / {r}"),
                    (Some(v), None) | (None, Some(v)) => v.to_string(),
                    (None, None) => String::new(),
                }
            }).filter(|name| !name.is_empty());
            for (li, layer) in spec.layers.iter().enumerate() {
                let Some(tally) = series_tally[li].as_mut() else { continue };
                let plays = layer.encodings.contains_key(&Channel::Play) && !play_levels.is_empty();
                for fi in 0..if plays { nframes } else { 1 } {
                    let Some(df) = eff_at(fi).get(li) else { continue };
                    let moment = plays.then(|| play_levels[fi].label.as_str());
                    tally.count(spec, layer, df, x_field, y_field, panel_name.as_deref(), moment);
                }
            }

            // The globe branch: marks routed through the sphere's projection and
            // culled to the facing hemisphere, inside the disk the frame routine
            // drew above. Like the cube it sits inside the panel loop, so a
            // faceted globe is N spheres sharing one view with no layout work.
            // Which marks arrive is `mark_draws_in_space(_, Globe)` and nothing
            // else; a checked spec never brings another, and under a downgraded
            // refusal the refused layer is left out rather than half-drawn.
            if let Some(g) = &glb {
                // **What a view hides is counted and said** (§12). The far
                // hemisphere is culled by construction — that is what a globe
                // is — so the report is an Assumption naming the count, not a
                // warning per row: a reader must know whether the half they
                // face is the whole of the data.
                for (li, layer) in spec.layers.iter().enumerate() {
                    if !crate::legality::mark_draws_in_space(&layer.mark,
                        crate::legality::SpaceKind::Globe) { continue }
                    // A spike's own visibility is the sphere's clip, not the
                    // hemisphere test: one standing just behind the horizon
                    // still peeks over the limb, so only a spike the sphere
                    // hides whole counts as hidden. A value below the surface
                    // has nowhere to point and is counted separately.
                    let spike_layer = layer.mark == Mark::Bar;
                    let top = zs.1.max(zs.0);
                    let (mut hidden, mut total, mut sunk) = (0usize, 0usize, 0usize);
                    // A layer that does not play holds the same rows in every
                    // moment, so it is counted once: counted per moment, a
                    // basemap's 4150 rows were reported as 49800 over 12 months.
                    let moments = if layer.encodings.contains_key(&Channel::Play) { nframes } else { 1 };
                    for fi in 0..moments {
                        let Some(df) = eff_at(fi).get(li) else { continue };
                        let (Some(lons), Some(lats)) =
                            (df.float_col(x_field), df.float_col(y_field)) else { continue };
                        let vals = if spike_layer { df.float_col(z_field) } else { None };
                        for i in 0..lons.len().min(lats.len()) {
                            if !(lons[i].is_finite() && lats[i].is_finite()) { continue }
                            if spike_layer {
                                let v = vals.and_then(|c| c.get(i)).copied()
                                    .unwrap_or(f64::NAN);
                                if v < 0.0 { sunk += 1; continue }
                                total += 1;
                                let h = if v.is_finite() && top > 0.0 {
                                    crate::render::globe::SPIKE_MAX * (v / top).min(1.0)
                                } else { 0.0 };
                                if g.spike(lons[i], lats[i], h).is_none() { hidden += 1 }
                            } else {
                                total += 1;
                                if !g.front(lons[i], lats[i]) { hidden += 1 }
                            }
                        }
                    }
                    let tally = &mut globe_tally[li];
                    tally.0 += hidden;
                    tally.1 += total;
                    tally.2 += sunk;
                }
                // **A label is clipped by the panel, not the disk.** It names a place
                // on the facing hemisphere, and a name set beside a place near the
                // limb runs past it: clipped 8 px outside the disk, "Anchorage" lost
                // its last three letters. The panel's rectangle is its own edge.
                let label_clip = format!("{clip}-labels");
                if spec.layers.iter().any(|ly| ly.mark == Mark::Text) {
                    writeln!(svg,
                        r#"  <clipPath id="{label_clip}"><rect x="{x0:.2}" y="{y0:.2}" width="{w:.2}" height="{h:.2}"/></clipPath>"#,
                        x0 = l.x0, y0 = l.y0, w = l.w(), h = l.h()).unwrap();
                }
                for (fi, framed, run) in draw_steps(spec, nframes, self.selection_passes(spec).len(), true) {
                    let eff = eff_at(fi);
                    if framed { self.open_frame(&mut svg, fi, nframes); }
                    for &dim in self.selection_passes(spec) {
                        self.open_pass(&mut svg, dim);
                        for (li, ((layer, df), whole)) in spec.layers.iter().zip(eff.iter()).zip(&wholes).enumerate() {
                            if !run.contains(&li) { continue }
                            let Some(df) = self.pass_rows(spec, layer, df, dim) else { continue };
                            let df = &*df;
                            if df.is_empty() { continue }
                            match layer.mark {
                                Mark::Point => { self.write_points(&mut svg, layer, df, whole, l, xs, ys,
                                    x_field, y_field, cat_x.as_deref(), cat_y.as_deref(),
                                    &color_map, &ramp, &clip, zs, z_field, None, None, Some(g)); }
                                Mark::Text => {
                                    // The facing hemisphere's dots, as the flat
                                    // branch reads its panel's (below).
                                    let dots = if layer.transforms.contains(&Transform::Repel) {
                                        self.panel_dots(spec, eff, &wholes, l, xs, ys, x_field, y_field,
                                            cat_x.as_deref(), cat_y.as_deref(), None, Some(g), None)
                                    } else {
                                        Vec::new()
                                    };
                                    let crowding = self.write_text(&mut svg, layer, df, whole, l, xs, ys,
                                        x_field, y_field, cat_x.as_deref(), cat_y.as_deref(),
                                        &color_map, &label_clip, &ground, None, None, Some(g), None, &dots, &mut remarks,
                                        &mut Vec::new());
                                    repel_crowding.entry((li, panel.slot, fi)).or_default().add(crowding);
                                }
                                Mark::Path => self.write_path(&mut svg, layer, df, whole, l, xs, ys,
                                    x_field, y_field, cat_x.as_deref(), cat_y.as_deref(),
                                    &color_map, &ramp, &clip, zs, z_field, None, None, Some(g)),
                                Mark::Rule => self.write_rule(&mut svg, layer, df, whole, l, xs, ys,
                                    spec, cat_x.as_deref(), cat_y.as_deref(), &color_map,
                                    &clip, None, Some(g), None),
                                Mark::Zone => self.write_zone(&mut svg, layer, df, whole, l, xs, ys,
                                    x_field, y_field, cat_x.as_deref(), cat_y.as_deref(),
                                    &color_map, &ramp, &clip, None, Some(g)),
                                // The spike: the measure standing on the radius,
                                // the one direction the sphere has that its
                                // flattening does not.
                                Mark::Bar => self.write_bars_globe(&mut svg, layer, df, whole,
                                    x_field, y_field, z_field, zs, &color_map, &ramp,
                                    &clip, g),
                                _ => {}
                            }
                        }
                        self.close_pass(&mut svg, dim);
                    }
                    if framed { self.close_frame(&mut svg, fi, nframes, clock); }
                }
                continue;
            }

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
            if is_3d || network_cube.is_some() {
                let scene = project::Scene::new(view, l.x0, l.y0, l.x1, l.y1, FRAME_INSET);
                self.write_space_box(&mut svg, &scene);
                // One group per moment, wrapping the marks and nothing else. The
                // panel behind them, its gridlines and its axes are shared by every
                // frame — they are drawn from one scale, so redrawing them per frame
                // would be bytes spent making them flicker. At one frame this writes
                // no group and no timing, which is what leaves an unplayed plot
                // byte-for-byte what it was.
                //
                // **A still layer is drawn once here too** (`draw_steps`). The cube
                // paints its layers in the order they are written, each sorted by
                // depth within itself, so the flat rule holds unchanged: the still
                // layers before every played one go once under the moments, those
                // after every played one once over them. A surface under played
                // points was copied into every moment. A network's cube draws its
                // layers together, as its flat form does.
                let in_order = network_cube.is_none();
                for (fi, framed, run) in draw_steps(spec, nframes, self.selection_passes(spec).len(), in_order) {
                    let eff = eff_at(fi);
                    if framed { self.open_frame(&mut svg, fi, nframes); }
                    for &dim in self.selection_passes(spec) {
                      self.open_pass(&mut svg, dim);
                      for (li, ((layer, df), whole)) in spec.layers.iter().zip(eff.iter()).zip(&wholes).enumerate() {
                        if !run.contains(&li) { continue }
                        let Some(df) = self.pass_rows(spec, layer, df, dim) else { continue };
                        let df = &*df;
                        if df.is_empty() { continue }
                        match layer.mark {
                            Mark::Point => { self.write_points(&mut svg, layer, df, whole, l, xs, ys,
                                x_field, y_field, cat_x.as_deref(), cat_y.as_deref(),
                                &color_map, &ramp, &clip, zs, z_field, Some(&scene), None, None); }
                            Mark::Path => self.write_path(&mut svg, layer, df, whole, l, xs, ys,
                                x_field, y_field, cat_x.as_deref(), cat_y.as_deref(),
                                &color_map, &ramp, &clip, zs, z_field, Some(&scene), None, None),
                            // The layout's endpoint pairs in the cube — only
                            // ever reached inside a `network()` with a stated
                            // view, depth-sorted per stroke inside the writer.
                            Mark::Edge => self.write_edge_3d(&mut svg, layer, df, whole, xs, ys, zs,
                                &color_map, &clip, &scene, &self.node_radii(spec, eff, &wholes)),
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
                            Mark::Interval => self.write_interval_3d(&mut svg, layer, df, whole, xs, ys, zs,
                                x_field, y_field, z_field, cat_x.as_deref(), cat_y.as_deref(),
                                &color_map, &clip, &scene),
                            Mark::Box => self.write_box_3d(&mut svg, layer, df, xs, ys, zs,
                                x_field, y_field, z_field, cat_x.as_deref(), cat_y.as_deref(),
                                &color_map, &clip, &scene),
                            Mark::Surface => self.write_surface(&mut svg, layer, df, whole, xs, ys, zs,
                                x_field, y_field, z_field, &color_map, &ramp, &clip, &scene),
                            // A label at a place in the cube (built 2026-10-09),
                            // `point`'s third position read by the word. A repelled
                            // label steps around the cube's dots where they project.
                            Mark::Text => {
                                let dots = if layer.transforms.contains(&Transform::Repel) {
                                    self.panel_dots(spec, eff, &wholes, l, xs, ys, x_field, y_field,
                                        cat_x.as_deref(), cat_y.as_deref(), None, None,
                                        Some((&scene, zs, z_field)))
                                } else {
                                    Vec::new()
                                };
                                let crowding = self.write_text(&mut svg, layer, df, whole, l, xs, ys,
                                    x_field, y_field, cat_x.as_deref(), cat_y.as_deref(),
                                    &color_map, &clip, &ground, None, None, None,
                                    Some((&scene, zs, z_field)), &dots, &mut remarks, &mut Vec::new());
                                repel_crowding.entry((li, panel.slot, fi)).or_default().add(crowding);
                            }
                            _ => {}
                        }
                      }
                      self.close_pass(&mut svg, dim);
                    }
                    if framed { self.close_frame(&mut svg, fi, nframes, clock); }
                }
                // The frame's labels go on last, the same rule the flat and polar
                // frames follow below: a guide is an annotation *about* the scene,
                // not a member of it, so nothing the marks paint can take a
                // measurement's name away. This is what a solid mesh made
                // impossible to keep ignoring — it covers the whole floor, so
                // every number along both domain edges went under it.
                // A network's cube keeps the glass box for depth and skips the
                // labels: its coordinates are the layout's, and a number on a
                // meaningless axis is the reading the treemap entry warns
                // against.
                // An axis the sentence leaves out (`z(depth, axis = FALSE)`) is
                // handed over with no numbers and no name, which is how an axis
                // seen end-on is already left unlabeled; its edge stays in the
                // box, as the panel's lines stay on a flat plot. Asked of the
                // sentence alone: a page shares flat axes, never a cube's.
                if network_cube.is_none() {
                let shown = |c: Channel| !spec.hides_axis(&c);
                let (cube_x, cube_y) = (shown(Channel::X), shown(Channel::Y));
                self.write_space_labels(&mut svg, &scene, l, &spec.theme.resolved(), view,
                    [scale::tick_count_of(spec.axis_def(&Channel::X)),
                     scale::tick_count_of(spec.axis_def(&Channel::Y)),
                     scale::tick_count_of(spec.axis_def(&Channel::Z))],
                    if cube_x { &x_ticks } else { &no_ticks }, xs,
                    if cube_x { x_label.as_str() } else { "" },
                    if cube_y { &y_ticks } else { &no_ticks }, ys,
                    if cube_y { y_label.as_str() } else { "" },
                    if draw_z_axis { &z_ticks } else { &no_ticks }, zs,
                    if draw_z_axis { z_label.as_str() } else { "" },
                    [cat_x.is_some(), cat_y.is_some(), false],
                    &mut remarks);
                }
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
            //
            // A tree's leaf axis has none either, by the bar rule's reasoning: each
            // leaf is already a line standing at its slot, so a gridline there only
            // continues it past its join to the top of the panel, one per leaf. The
            // distance axis keeps its lines; they are how a join's height is read.
            if pol.is_none() && !is_nest && !is_network {
                let theme = spec.theme.resolved();
                self.write_grid(&mut svg, l, &x_ticks, xs, &y_ticks, ys,
                                (has_plain_bar && !horizontal) || !theme.grid_x()
                                    || (clusters_a_tree && cat_x.is_some()) || blank_x,
                                (has_plain_bar && horizontal) || !theme.grid_y()
                                    || (clusters_a_tree && cat_y.is_some()) || blank_y,
                                meridians.as_ref());
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
            // A still layer is drawn once where that keeps the picture
            // (`draw_steps`); a packing and a network draw their layers together.
            let in_order = !is_nest && !is_network;
            // **A density beside rows stops at its rows** (`Fit::rows_x`): its marks
            // are clipped to the span between its smallest and largest row on the
            // axis it shares, so nothing is drawn over the neighbor's margin. Only a
            // plot every layer of which is such a density has the range at all.
            let rows_clip = (self.fit.rows_x.is_some() || self.fit.rows_y.is_some()).then(|| {
                let (x0, x1) = self.fit.rows_x
                    .map(|(lo, hi)| (l.map_x(lo, xs.0, xs.1), l.map_x(hi, xs.0, xs.1)))
                    .map_or((l.x0, l.x1), |(a, b)| (a.min(b).max(l.x0), a.max(b).min(l.x1)));
                let (y0, y1) = self.fit.rows_y
                    .map(|(lo, hi)| (l.map_y(lo, ys.0, ys.1), l.map_y(hi, ys.0, ys.1)))
                    .map_or((l.y0, l.y1), |(a, b)| (a.min(b).max(l.y0), a.max(b).min(l.y1)));
                let id = format!("{clip}-rows");
                writeln!(svg,
                    r#"  <clipPath id="{id}"><rect x="{x0:.2}" y="{y0:.2}" width="{:.2}" height="{:.2}"/></clipPath>"#,
                    (x1 - x0).max(0.0), (y1 - y0).max(0.0)).unwrap();
                id
            });
            for (fi, framed, run) in draw_steps(spec, nframes, self.selection_passes(spec).len(), in_order) {
                let eff = eff_at(fi);
                if framed { self.open_frame(&mut svg, fi, nframes); }
                // One pass unless the reader has selected something, in which case
                // the unselected rows are drawn first and pushed back.
                for &dim in self.selection_passes(spec) {
                  self.open_pass(&mut svg, dim);
                  for (li, ((layer, df), whole)) in spec.layers.iter().zip(eff.iter()).zip(&wholes).enumerate() {
                    if !run.contains(&li) { continue }
                    let clip = rows_clip.clone().unwrap_or_else(|| clip.clone());
                    // The share the pass is cut from, and which of its rows this
                    // pass draws: a bar is measured on the first and draws the second.
                    let share = df;
                    let Some(keep) = self.pass_keep(spec, layer, share, dim) else { continue };
                    let cut;
                    let df: &DataFrame = match &keep {
                        None => share,
                        Some(side) => { cut = share.keep_rows(side); &cut }
                    };
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
                        self.write_violin(&mut svg, layer, df, whole, l, xs, ys, x_field, y_field,
                            cat_x.as_deref(), cat_y.as_deref(),
                            orient == crate::legality::Orient::Horizontal,
                            shape, &color_map, &clip, pol_ref);
                        continue;
                    }
                    match layer.mark {
                        Mark::Point => swarm.absorb(self.write_points(&mut svg, layer, df, whole, l, xs, ys, x_field, y_field, cat_x.as_deref(), cat_y.as_deref(), &color_map, &ramp, &clip, zs, z_field, None, pol_ref, None)),
                        Mark::Line  => self.write_line(&mut svg, layer, df, whole, l, xs, ys, x_field, y_field, cat_x.as_deref(), &color_map, &ramp, &clip, pol_ref),
                        Mark::Area  => self.write_area(&mut svg, layer, df, whole, l, xs, ys, x_field, y_field, cat_x.as_deref(), area_base, &color_map, &clip, pol_ref, &ground),
                        Mark::Bar   => self.write_bars(&mut svg, layer, share, keep.as_deref(), whole, l, xs, ys, x_field, y_field, cat_x.as_deref(), cat_y.as_deref(), horizontal, ext_base, &color_map, &clip, pol_ref, &ground, nst.as_ref()),
                        Mark::Step  => self.write_step(&mut svg, layer, df, whole, l, xs, ys, x_field, y_field, cat_x.as_deref(), area_base, &color_map, &ramp, &clip, pol_ref),
                        Mark::Interval => self.write_interval(&mut svg, layer, df, whole, l, xs, ys, x_field, y_field, cat_x.as_deref(), cat_y.as_deref(), horizontal, &color_map, &clip, pol_ref),
                        Mark::Box => self.write_box(&mut svg, layer, df, whole, l, xs, ys, x_field, y_field, cat_x.as_deref(), cat_y.as_deref(), horizontal, &color_map, &clip, pol_ref),
                        // The flow's band reading arrives here on the ribbon, the
                        // violin's dispatch shape one transform over: the mark is
                        // the reader's name, the writer is the reading's.
                        // A shared flow's three readers draw along one axis, or round
                        // one ring in `polar()`, which the stage flow's never do.
                        Mark::Ribbon if layer.flow_is_shared() =>
                            self.write_shared_bands(&mut svg, layer, df, whole, l, xs, ys, flow_down, &color_map, &clip, pol_ref),
                        Mark::Zone if layer.flow_is_shared() =>
                            self.write_shared_slots(&mut svg, layer, df, l, xs, ys, flow_down, &clip, !spec.brush.is_empty(), pol_ref),
                        Mark::Text if layer.flow_is_shared() =>
                            self.write_shared_names(&mut svg, layer, df, l, xs, ys, flow_down, pol_ref, tick_angle),
                        // A layered flow's names stand beside its thin slots; its
                        // slots and bands are the stage flow's writers'.
                        Mark::Text if layer.flow_is_layered() =>
                            self.write_layered_names(&mut svg, layer, df, l, xs, ys, cat_x.as_deref(), cat_y.as_deref(), flow_down),
                        Mark::Ribbon if layer.transforms.contains(&Transform::Flow) =>
                            self.write_flow_bands(&mut svg, layer, df, whole, l, xs, ys, cat_x.as_deref(), cat_y.as_deref(), flow_down, &color_map, &clip),
                        Mark::Ribbon => self.write_ribbon(&mut svg, layer, df, whole, l, xs, ys, x_field, y_field, cat_x.as_deref(), &color_map, &clip, pol_ref),
                        Mark::Text => {
                            // A repelled label steps around every dot drawn in
                            // this panel at this moment, at the size it is
                            // drawn: a network's hubs sized by `degree`, a
                            // bubble sized by `size`, and the unnamed rows of a
                            // point layer fuller than the label table. Each
                            // point layer is read from its own frame, the way
                            // `node_radii` reads the dots an arrowhead stops at.
                            let dots = if layer.transforms.contains(&Transform::Repel) && nst.is_none() {
                                self.panel_dots(spec, eff, &wholes, l, xs, ys, x_field, y_field,
                                    cat_x.as_deref(), cat_y.as_deref(), pol_ref, None, None)
                            } else {
                                Vec::new()
                            };
                            let crowding = self.write_text(&mut svg, layer, df, whole, l, xs, ys, x_field, y_field, cat_x.as_deref(), cat_y.as_deref(), &color_map, &clip, &ground, pol_ref, nst.as_ref(), None, None, &dots, &mut remarks, &mut label_ink);
                            repel_crowding.entry((li, panel.slot, fi)).or_default().add(crowding);
                        }
                        // The stroke between two layout-supplied endpoints —
                        // only ever reached inside `network()`, where the
                        // legality gate has already admitted it.
                        Mark::Edge => self.write_edge(&mut svg, layer, df, whole, l, xs, ys, &color_map, &clip, None,
                                                      &self.node_radii(spec, eff, &wholes)),
                        Mark::Path => self.write_path(&mut svg, layer, df, whole, l, xs, ys, x_field, y_field, cat_x.as_deref(), cat_y.as_deref(), &color_map, &ramp, &clip, zs, z_field, None, pol_ref, None),
                        // The one mark handed the whole spec rather than the two
                        // resolved field names: which axis places it is read off
                        // *both* positions against this layer's own table
                        // (`legality::rule_axis`), so a pair of `&str` cannot say it.
                        Mark::Rule => self.write_rule(&mut svg, layer, df, whole, l, xs, ys, spec, cat_x.as_deref(), cat_y.as_deref(), &color_map, &clip, pol_ref, None,
                            map_degrees.as_ref().map(|(geo, ..)| geo)),
                        // Drawn like any other mark, but its frame reached here
                        // untransformed — a zone's `bounds` names four columns rather
                        // than reshaping rows into pairs (see the effective-frame
                        // branch above), so it reads them straight off the table.
                        // The flow's slot reading, the band's twin above.
                        Mark::Zone if layer.transforms.contains(&Transform::Flow) =>
                            self.write_flow_nodes(&mut svg, layer, df, l, xs, ys, cat_x.as_deref(), cat_y.as_deref(), flow_down, &clip, !spec.brush.is_empty()),
                        Mark::Zone => self.write_zone(&mut svg, layer, df, whole, l, xs, ys, x_field, y_field, cat_x.as_deref(), cat_y.as_deref(), &color_map, &ramp, &clip, pol_ref, None),
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
                if framed { self.close_frame(&mut svg, fi, nframes, clock); }
            }

            // The tick labels go on last in both spaces, so they stay readable over
            // whatever the marks painted. In polar that is the ring of category
            // names outside the circle and the radial numbers up the spoke.
            match &pol {
                // An axis left out keeps its spokes or rings, which are the grid,
                // and loses only its numbers: the angle's ring of names, or the
                // radius's numbers up the spoke.
                Some(p) => self.write_polar_ticks(&mut svg, p,
                    if draw_x_axis { &x_ticks } else { &no_ticks }, xs,
                    if draw_y_axis { &y_ticks } else { &no_ticks }, ys),
                // Nothing at all in a packed panel. An axis line is the edge of a
                // measurement and a tick is a place on one; this space has neither,
                // and the cells' own edges are what the reader has instead.
                None if is_nest || is_network => {}
                None => {
                    self.write_axes(&mut svg, l, &spec.theme.resolved());
                    // A shared axis is one axis, so it is ticked once — by the
                    // plot nearest the edge it lives on. The same sentence
                    // `labels_x` says about the panels of a facet, said about
                    // the plots of a page (`render::page`).
                    self.write_ticks(&mut svg, l, &x_ticks, xs, &y_ticks, ys,
                                     grid.labels_x(panel) && draw_x_axis,
                                     grid.labels_y(panel) && draw_y_axis,
                                     grid.ncols > 1,
                                     tick_angle);
                }
            }
        }

        let shortened = self.write_strips(&mut svg, &grid, &spec.theme.resolved());
        if shortened > 0 {
            remarks.push(shortened_strip_names(shortened));
        }
        self.write_play_strip(&mut svg, &grid, &play_levels, clock,
                              &spec.theme.resolved());
        // In 3-D the axis names sit on the cube's edges, so the outer margin
        // carries only the title — the 2-D x/y labels would float against no axis.
        let (outer_xl, outer_yl) = if is_3d || is_nest || is_globe || is_network { ("", "") } else { (x_label.as_str(), y_label.as_str()) };
        // A name belongs to the axis, so it goes wherever the ticks went: on the
        // one plot of the page that draws the shared axis, and nowhere else.
        let (outer_xl, outer_yl) = (
            if draw_x_axis { outer_xl } else { "" },
            if draw_y_axis { outer_yl } else { "" },
        );
        // **Where the panel actually is, inside the rectangle the margins left.**
        // `theme(ratio = )` shrinks the panel inside its cell and centers it, and a
        // circle sits centered inside a panel wider or taller than it is; either way
        // the plot drawn is inset from `outer`, and the names and the legend were
        // placed against `outer` regardless. A square panel in a wide image kept its
        // y name at the image's edge, 135px from its tick labels, and a rose's
        // radial name sat 150px from the circle it names. So both are placed against
        // the plot as drawn: the names beside it, the legend beside it and level
        // with its top. The title moves down with it too, except where a facet's
        // strips stay at the top of the grid, which a title moved down would land on.
        let inset = {
            let (gx, gy) = grid.inset;
            let (px, py) = match (is_polar, grid.panels.first()) {
                (true, Some(p)) => crate::render::polar::inset(
                    &p.rect, POLAR_RIM_GAP + estimate_cap_height(self.font_sm) + 4.0,
                    measure_on_angle),
                _ => (0.0, 0.0),
            };
            (gx + px, gy + py)
        };
        // The band the *margin* reserved, which is `grid_xt` rather than `x_ticks`:
        // polar draws real angular labels and hands the layout an empty list,
        // because those labels go inside the circle.
        // The height of the x names' row as drawn: upright, one cap height; turned,
        // w·sin θ + h·cos θ, the band `layout` reserves for them. The axis name
        // is set below this row, so it sits under the names it titles; measured
        // as one upright row whatever the angle, it was drawn across the turned
        // names (`theme(tick_angle = 45)` on the continents put "Continent" over
        // "Africa"). The angle is the one drawn, stated or turned for crowding.
        let x_band = match tick_angle.filter(|d| *d != 0.0) {
            Some(deg) => {
                let t = deg.abs().to_radians();
                grid_xt.labels.iter().map(|l| estimate_text_width(l, self.font_sm))
                    .fold(0.0_f64, f64::max) * t.sin()
                    + estimate_cap_height(self.font_sm) * t.cos()
            }
            None => estimate_cap_height(self.font_sm),
        };
        let y_name = self.write_labels(&mut svg, &grid.outer, inset, grid.moved_x, room, outer_xl,
                          outer_yl, spec, !grid_xt.labels.is_empty(), x_band);
        if !legends.is_empty() {
            // The canvas is the floor, not the panel — a legend has always been
            // allowed to run past the panel's bottom edge into the margin beside
            // the x tick labels, and bounding it at `grid.outer.y1` would drop
            // legends that fit the image.
            let floor = self.height - LEGEND_PADDING;
            let top = grid.outer.y0 + inset.1;
            match (side, plan.as_ref()) {
                (LegendSide::Left, _) => write_legends(
                    &mut svg, LEGEND_EDGE + inset.0 + grid.moved_x, top, &legends, fonts, floor,
                    see_through, &mut remarks),
                (LegendSide::Top | LegendSide::Bottom, Some(plan)) => {
                    // Centered on the plot as drawn, held inside the canvas. Above
                    // the plot the band sits under the title, where `layout` set
                    // it aside, and moves down with a panel a `ratio` centered, as
                    // the title does. Below, it sits under the x axis's name, which
                    // is placed by `write_labels` on the same arithmetic as here.
                    let mid = (grid.outer.x0 + grid.outer.x1) / 2.0;
                    let x = (mid - plan.width / 2.0)
                        .clamp(LEGEND_EDGE, (self.width - LEGEND_EDGE - plan.width).max(LEGEND_EDGE));
                    let y = match side {
                        LegendSide::Top => grid.outer.y0 + title_drop(spec, inset)
                            - y_name_above(outer_yl, &spec.theme, self.font_md) - room.top,
                        _ => {
                            let tick_row = if grid_xt.labels.is_empty() { 0.0 } else { 5.0 + x_band };
                            let name_row = match outer_xl.is_empty() {
                                true => 0.0,
                                false => 8.0 + estimate_cap_height(self.font_md) + 4.0,
                            };
                            grid.outer.y1 - inset.1 + tick_row + name_row + LEGEND_ACROSS_GAP
                        }
                    };
                    write_legends_across(&mut svg, &legends, plan, fonts, x, y, see_through);
                }
                _ => write_legends(
                    &mut svg, grid.outer.x1 - inset.0 + LEGEND_PLOT_GAP, top, &legends, fonts, floor,
                    see_through, &mut remarks),
            }
        }
        // **A plot with its legend on the left keeps its own y name** rather than
        // joining a page's column of them (`page::align`). The column is the
        // leftmost of the names beside their panels, which on a page beside a plot
        // with wider tick labels lands inside this plot's legend. Beside its own
        // panel the name always clears the legend, which `layout` placed outside it.
        let y_name = match (side, legends.is_empty()) {
            (LegendSide::Left, false) => None,
            _ => y_name,
        };
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
        // see `AxisFacts::projected` for why neither unit works. A globe is the
        // stronger case of the same fact: its positions never touch the fitted
        // scales at all, so no stated domain could reach them.
        let projected = map_degrees.is_some() || is_globe;
        // **Only a flat plot offers its axes to a page.** A cube's, a disc's, a
        // map's and a globe's positions are placed by their space rather than along
        // a straight axis, so a page that lined one up with a flat plot on the same
        // column read it against the other's ticks: the equal-area map below a
        // Mercator one was drawn to the Mercator's longitudes, and two cubes side by
        // side labeled a floor range neither had alone. An empty column joins no
        // share group, which is the rule for a plot with nothing bound.
        let offers = crate::legality::space_of(spec) == crate::legality::SpaceKind::Flat;
        // **A share is not in its column's units, so its axis is offered to no
        // page.** `stack(share = true)` and `proportion` divide what the column
        // measured, so the measured axis reads fractions of one whatever `y(n)`
        // named, the fact `axis_label` acts on when it writes "Share" over a named
        // column. Offered under that name, a page gave it one scale with a plot of
        // the counts beside it: the shares were drawn on an axis running to 885 and
        // every pile was too short to draw. Like an unbound axis, a share is a
        // measurement the plot made for itself. A partition's `proportion` divides
        // its own measure axis rather than this one (`synth_y_label`).
        let shares = share_stacked(spec) || spec.layers.iter().any(|l|
            l.transforms.contains(&Transform::Proportion)
                && !l.transforms.contains(&Transform::Partition));
        let (x_offered, y_offered) = match (shares, horizontal) {
            (true, false) => (x_field, ""),
            (true, true) => ("", y_field),
            (false, _) => (x_field, y_field),
        };
        // The binding behind an offered column, for how the sentence reads it. An
        // axis with no binding of that name reads it the column's own way.
        let reading_of = |field: &str| -> crate::legality::AxisReading {
            [Channel::X, Channel::Y].iter()
                .filter_map(|c| spec.axis_def(c))
                .find(|d| d.field == field)
                .map(crate::legality::AxisReading::of)
                .unwrap_or_default()
        };
        // **An estimate's tails are not rows** (`AxisFacts::estimate_rows`). Asked
        // only where every layer is a one-dimensional density spreading along the
        // axis, a curve along its key and a violin along its measure, which is the
        // marginal a page lines up; a plot that also draws rows on the axis offers
        // its range as before. The rows are the ones the estimate is made from,
        // logged where the axis is, so the range is in the scale's units.
        let estimate_rows = |on_x: bool| -> Option<(f64, f64)> {
            let (field, log, base) = if on_x { (x_field, x_log, x_base) } else { (y_field, y_log, y_base) };
            let mut reach: Option<(f64, f64)> = None;
            for layer in &spec.layers {
                if !layer.transforms.contains(&Transform::Density) || layer.mark == Mark::Zone
                    || crate::legality::reads_two_dimensions(
                        &layer.mark, &layer.transforms, crate::legality::space_of(spec)) {
                    return None;
                }
                let rows = prepared(layer, &[])?;
                let key = if key_is_x(layer, &rows) { x_field } else { y_field };
                let spreads_x = (key == x_field) != rows.str_col(key).is_some();
                if spreads_x != on_x {
                    return None;
                }
                let rows = if log { scale::log_column(&rows, field, base) } else { rows };
                for &v in rows.float_col(field)?.iter().filter(|v| v.is_finite()) {
                    reach = Some(reach.map_or((v, v), |(lo, hi)| (lo.min(v), hi.max(v))));
                }
            }
            reach
        };
        let facts = |field: &str, range: (f64, f64), cats: Option<&Vec<String>>, log: bool,
                     base: f64, ticks_over: (f64, f64), estimate: Option<(f64, f64)>| AxisFacts {
            field: if offers { field.to_string() } else { String::new() },
            reading: reading_of(field),
            range,
            cats: cats.cloned(),
            log_base: log.then_some(base),
            projected,
            ticks_over,
            estimate_rows: if cats.is_some() { None } else { estimate },
        };
        remarks.extend(swarm.remark());
        // **What `repel` could not separate is said once per layer, naming the
        // panels.** Said by each drawing, a faceted plot printed one line per
        // panel, and a played one per moment, with nothing to tell them apart.
        for li in 0..spec.layers.len() {
            let crowded: Vec<(Option<String>, crate::render::marks::Crowding)> = grid.panels.iter()
                .filter_map(|panel| {
                    let worst = (0..nframes)
                        .filter_map(|fi| repel_crowding.get(&(li, panel.slot, fi)).copied())
                        .max_by_key(|c| c.stuck)
                        .filter(|c| c.stuck > 0)?;
                    let name = match panel_levels.get(panel.slot).copied().unwrap_or((None, None)) {
                        (Some(c), Some(r)) => Some(format!("{c} / {r}")),
                        (Some(v), None) | (None, Some(v)) => Some(v.to_string()),
                        (None, None) => None,
                    };
                    Some((name, worst))
                })
                .collect();
            remarks.extend(crate::render::marks::Crowding::remark(
                &crowded, grid.panels.len(), nframes));
        }
        // **Each globe note names its table.** Unnamed, two layers from two tables
        // with the same counts printed one line, and a plot with several layers gave
        // counts no reader could match to a layer. Two layers drawing one table hide
        // the same rows, so they still print one line, through the dedup below.
        if let Some(v) = globe_view {
            for (layer, &(hidden, total, sunk)) in spec.layers.iter().zip(&globe_tally) {
                let of = layer.data.as_ref().or(spec.data.as_ref())
                    .map(|t| format!(" of `{t}`")).unwrap_or_default();
                if hidden > 0 {
                    remarks.push(Diagnostic {
                        kind: crate::legality::DiagnosticKind::Assumption,
                        message: format!(
                            "gog: {hidden} of {total} row(s){of} face away from \
                             `globe(turn = {}, tilt = {})` and are hidden behind the \
                             sphere. Turn to face them, or put a second view beside \
                             this one with `|`.",
                            v.turn, v.tilt
                        ),
                    });
                }
                if sunk > 0 {
                    remarks.push(Diagnostic {
                        kind: crate::legality::DiagnosticKind::Assumption,
                        message: format!(
                            "gog: {sunk} row(s){of} measure below zero, and a spike cannot \
                             point into the earth — they are left undrawn. A bar here \
                             stands on the surface and measures outward; shift the \
                             column in the host if the sign is a convention."
                        ),
                    });
                }
            }
        }
        // **A path with one position draws nothing** (`series.rs`): refused when the
        // whole plot would be empty, and said when a series is left out.
        remarks.extend(one_position(spec, &series_tally));
        // **A panel with no room to draw in is refused** (ruled 2026-09-29). The
        // margins are measured from what sits beside the panel, its axes' numbers
        // and names, a title, a legend, so only the drawing knows when they take
        // the whole plot; `check_theme_size`'s floor of 40 px cannot. A plot 60 px
        // wide drew a panel -9 px wide, off its own edge, with no message. A
        // legend is already left out when it will not fit, so what is left here is
        // the plot's own frame.
        if let Some(r) = grid.panels.iter().map(|p| &p.rect).find(|r| r.w() < 1.0 || r.h() < 1.0) {
            let (arg, dim, size, panel) = if r.w() < 1.0 {
                ("width", "wide", self.width, r.w())
            } else {
                ("height", "tall", self.height, r.h())
            };
            let (its, which) = if grid.panels.len() > 1 {
                ("its panels", "a panel")
            } else {
                ("its panel", "the panel")
            };
            remarks.push(Diagnostic {
                kind: crate::legality::DiagnosticKind::Illegal,
                message: format!(
                    "gog: the plot is {size:.0} px {dim}, and what is drawn around {its}, \
                     the axes' numbers and names and any title, takes all of that: {which} \
                     would be {panel:.0} px {dim}, with no room to draw in. Give the plot \
                     more {arg}, `theme({arg} = )`, or take something from around {its}."
                ),
            });
        }
        // **Room for the labels' letters** (`LabelInk`, `room_for_labels`). The axes
        // were fitted in the data's units before any letter had a size, so a label
        // at the edge of the data could run past the panel and be cut there. Each
        // free end of a numeric axis on the flat plane is widened just enough to
        // hold the letters, and the plot is drawn again with those ends. A stated
        // end keeps its place (spec §10), as does a bar's baseline, and an end no
        // label passes is not moved, so a plot whose labels all fit is drawn once,
        // exactly as before. A category axis keeps its slots, a freed axis its
        // panels' own fits, and every other space has no axis end to widen.
        let flat_plane = !(is_polar || is_3d || is_globe || is_nest || is_network)
            && network_cube.is_none() && map_degrees.is_none();
        if self.label_room.is_none() && flat_plane && !label_ink.is_empty() {
            let ends = |ch: Channel, range: (f64, f64), baseline: bool, open: bool| {
                let (lo, hi) = fit_domain(ch);
                (open && lo.is_none() && !(baseline && range.0 == 0.0),
                 open && hi.is_none() && !(baseline && range.1 == 0.0))
            };
            let free_ends_x = ends(Channel::X, xs, has_any_bar && horizontal,
                !free_x && cat_x.is_none() && !x_field.is_empty());
            let free_ends_y = ends(Channel::Y, ys,
                (has_any_bar && !horizontal) || has_any_area || has_step_hist,
                !free_y && cat_y.is_none() && !y_field.is_empty());
            let along_x: Vec<(f64, f64, f64, f64)> = label_ink.iter()
                .map(|k| (k.at.0, k.left, k.right, k.panel.0)).collect();
            let along_y: Vec<(f64, f64, f64, f64)> = label_ink.iter()
                .map(|k| (k.at.1, k.down, k.up, k.panel.1)).collect();
            let room = [room_for_labels(xs, &along_x, free_ends_x),
                        room_for_labels(ys, &along_y, free_ends_y)];
            if room.iter().any(|r| r.0.is_some() || r.1.is_some()) {
                return SvgRenderer { fit: self.fit.clone(), label_room: Some(room), ..*self }
                    .draw(given, data);
            }
        }

        let mut seen = std::collections::HashSet::new();
        remarks.retain(|d| seen.insert(d.message.clone()));
        Drawn {
            svg: namespace_ids(&svg),
            panel: area,
            x: facts(x_offered, xs, cat_x.as_ref(), x_log, x_base, shared.x_extent,
                     estimate_rows(true)),
            y: facts(y_offered, ys, cat_y.as_ref(), y_log, y_base, shared.y_extent,
                     estimate_rows(false)),
            y_name,
            remarks,
        }
    }
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

/// A joined mark's table with its gaps back in, or `None` to read it as it is.
///
/// A row with a missing value in a mapped column is dropped when the table
/// arrives, for every layer drawn from it, so a `line` joined the rows on either
/// side as if nothing had been there. Most plotting tools break the line there
/// instead: a missing value says that nothing is known at that place, and a
/// straight segment across it draws a value nobody measured. So the rows dropped
/// for a missing value of **this layer's own measured position** come back, with
/// the value still missing, and the writer ends one stroke before such a row and
/// starts the next after it (`marks::runs`).
///
/// Only for the four marks that join their rows with no transform between:
/// `line`, `area` and `step`, which break at a missing `y` (a missing `x` has no
/// place to break at, and stays dropped), and `path`, which joins rows in the
/// table's order and so breaks at a missing `x`, `y` or, in the cube, `z`. A row
/// missing any other mapped column, a color or a group, has no series to break
/// and stays dropped, as the message says. A layer with a transform reads the
/// table as it is: a summary of the rows that are there is still a summary.
fn with_breaks(df: &DataFrame, spec: &PlotSpec, layer: &Layer) -> Option<DataFrame> {
    let gaps = df.gaps()?;
    if !layer.transforms.is_empty() {
        return None;
    }
    let field = |ch: Channel| spec.position_for(layer, &ch).map(|c| c.field.as_str());
    let measured: Vec<&str> = match layer.mark {
        Mark::Line | Mark::Area | Mark::Step => field(Channel::Y).into_iter().collect(),
        Mark::Path => [Channel::X, Channel::Y, Channel::Z].into_iter().filter_map(field).collect(),
        _ => return None,
    };
    let keep: Vec<bool> = gaps.missing.iter()
        .map(|m| m.iter().all(|c| measured.contains(&c.as_str())))
        .collect();
    // Nothing to restore: every dropped row was missing something else.
    if keep.iter().zip(&gaps.missing).all(|(&k, m)| !k || m.is_empty()) {
        return None;
    }
    Some(gaps.all.keep_rows(&keep))
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

#[cfg(test)]
mod tests;
