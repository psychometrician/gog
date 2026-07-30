//! The page — separate plots arranged together, and the one rule relating them.
//!
//! Faceting is one plot split by a variable; composition is several plots on one
//! page, each keeping its own coordinate space (spec §11). Everything a page
//! does beyond *arranging* comes from a single derivation:
//!
//! > **The same column on the same axis in two composed plots is one axis** —
//! > one scale, one panel extent, drawn once.
//!
//! That is the whole of the marginal plot. `top / (main | right)` puts a
//! histogram of `speed` above a scatter of `speed` against `dist`, and a
//! histogram of `dist` beside it: `top` and `main` share `speed` on x, so the
//! histogram's panel is squeezed to the scatter's panel and the `speed` axis is
//! ticked once, underneath; `main` and `right` share `dist` on y the same way.
//! The blank top-right corner is not a spacer anyone asked for — it is the room
//! the shared extent left over.
//!
//! **Why the axis is drawn once.** Two plots that share an axis would otherwise
//! draw it twice, identically, one above the other. A facet already answers this
//! for its panels (`layout::PanelGrid::labels_x`: tick labels only under the
//! bottom row) and the answer carries over word for word with "panel" read as
//! "plot" — which is also what makes the marginal histogram sit flush against the
//! scatter, since an axis it does not draw costs it no margin.
//!
//! **Why each cell is rendered twice.** A plot's panel rectangle is the *end* of
//! its layout: it depends on the tick labels, which depend on the ticks, which
//! depend on the transformed frames. So the page asks each plot where it would
//! put its panel by having it draw one (`Drawn`), intersects the answers, and
//! has it draw again knowing the page's. The alternative — predicting the
//! rectangle without rendering — is a second implementation of the layout, and
//! two implementations of one rule is how a rule stops being one.

use std::collections::HashMap;

use crate::data::DataFrame;
use crate::ir::{Arrange, Channel, Figure, PageSpec, PlotSpec};
use crate::legality::{Diagnostic, DiagnosticKind};
use crate::render::layout::Fit;
use crate::render::svg::SvgRenderer;
use crate::render::{Drawn, Layout};

/// Space between two cells of a page, in px.
///
/// Wider than the panel gap a facet uses (`layout::PANEL_GAP`), and the reason is
/// that the two are not the same measurement even though both sit between two
/// rectangles. A facet panel is a *bare* rectangle: its neighbor begins where its
/// frame ends, and 8px of air is plenty. A page cell is a whole plot, so it ends
/// in an **axis title**, and this gap has to separate that title from the next
/// plot rather than one frame from another.
///
/// The two shared one constant until 2026-07-29, on the argument that a reader
/// should not have to learn that one kind of neighbor sits closer than another.
/// That argument reads well and describes the wrong thing: the neighbors are not
/// alike, so one number for both is a coincidence rather than a consistency. What
/// it produced was a 13px axis title sitting 8px from the next plot's frame,
/// which reads as crowded at any zoom below 1:1. Nothing was ever clipped, and
/// the browser's own box for the label clears its cell by `10 * scale` px at
/// every width; it simply looked wrong, which for a manual is the same problem.
const CELL_GAP: f64 = 20.0;

/// How much of a shared extent has to survive the intersection for the panels to
/// be aligned to it, in px.
///
/// Two plots *side by side* on the same x column share the scale but not the
/// place: their panels are in different parts of the page, so intersecting them
/// leaves nothing (or a sliver where they nearly touch). The scale is still
/// shared — it is the same variable — but each keeps its own extent and draws
/// its own axis. This is the number that tells the two cases apart.
const ALIGNABLE: f64 = 24.0;

/// One plot and the rectangle of the page it was given.
struct Cell<'a> {
    spec: &'a PlotSpec,
    rect: Layout,
}

/// Draw a page. `width`/`height` are the whole canvas; the cells divide it.
pub(crate) fn render(
    page: &PageSpec,
    data: &HashMap<String, DataFrame>,
    width: f64,
    height: f64,
) -> (String, Vec<Diagnostic>) {
    let root = Figure::Page(page.clone());
    let mut cells = Vec::new();
    place(&root, Layout { x0: 0.0, y0: 0.0, x1: width, y1: height }, &mut cells);

    // Pass one: every plot draws itself alone, to say where its panel would go
    // and what its axes measure.
    let measured: Vec<Drawn> = cells
        .iter()
        .map(|c| {
            SvgRenderer::for_theme(&c.spec.theme.resolved(), c.rect.w(), c.rect.h())
                .draw(c.spec, data)
        })
        .collect();

    let mut diagnostics = Vec::new();
    let mut fits: Vec<Fit> = vec![Fit::free(); cells.len()];
    let mut specs: Vec<PlotSpec> = cells.iter().map(|c| c.spec.clone()).collect();

    for channel in [Channel::X, Channel::Y] {
        for group in groups(&measured, &channel) {
            share(&group, &cells, &measured, &channel, &mut fits, &mut specs, &mut diagnostics);
        }
    }

    // Pass two: each plot draws again, knowing what the page decided.
    let mut svg = String::with_capacity(96 * 1024);
    svg.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{height}\" \
         viewBox=\"0 0 {width} {height}\">\n"
    ));
    svg.push_str(&format!(
        "  <rect width=\"{width}\" height=\"{height}\" fill=\"white\"/>\n"
    ));
    for (i, cell) in cells.iter().enumerate() {
        let drawn = SvgRenderer {
            fit: fits[i],
            ..SvgRenderer::for_theme(
                &specs[i].theme.resolved(),
                cell.rect.w(),
                cell.rect.h(),
            )
        }
        .draw(&specs[i], data);
        // Nested rather than translated: an `<svg>` inside an `<svg>` is its own
        // viewport, so every cell keeps the coordinates it drew itself in and
        // nothing inside it has to know it was composed. Ids stay safe because
        // the engine derives them from content — a clip from its rectangle, a
        // gradient from its stops — so two cells collide only where they are
        // asking for the same thing (`svg::clip_id`, `legend`'s ramp id).
        svg.push_str(&nest(&drawn.svg, cell.rect.x0, cell.rect.y0));
        // What the *second* pass found, never the first: pass one draws every plot
        // at its own size to measure it, and a page then resizes it, so a remark
        // about what fits is only true of the drawing that reaches the reader.
        diagnostics.extend(drawn.remarks);
    }
    svg.push_str("</svg>\n");
    (svg, diagnostics)
}

/// Put `<svg …>` inside another one at (x, y).
fn nest(cell_svg: &str, x: f64, y: f64) -> String {
    match cell_svg.strip_prefix("<svg ") {
        Some(rest) => format!("<svg x=\"{x:.2}\" y=\"{y:.2}\" {rest}"),
        // Unreachable while `write_header` writes the tag: kept as a passthrough
        // rather than a panic, because a page that draws its cells in the wrong
        // place is a better failure than one that draws nothing.
        None => cell_svg.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Placement — the tree of cells becomes a set of rectangles
// ---------------------------------------------------------------------------

/// Divide `rect` between `fig`'s cells, recursively, collecting the leaves.
fn place<'a>(fig: &'a Figure, rect: Layout, out: &mut Vec<Cell<'a>>) {
    let page = match fig {
        Figure::Plot(spec) => {
            out.push(Cell { spec, rect });
            return;
        }
        Figure::Page(page) => page,
    };

    let horizontal = page.arrange == Arrange::Beside;
    let total = if horizontal { rect.w() } else { rect.h() };
    let gaps = CELL_GAP * (page.cells.len().saturating_sub(1)) as f64;
    let sizes = divide(&page.cells, horizontal, (total - gaps).max(0.0));

    let mut at = if horizontal { rect.x0 } else { rect.y0 };
    for (cell, size) in page.cells.iter().zip(sizes) {
        let sub = if horizontal {
            Layout { x0: at, y0: rect.y0, x1: at + size, y1: rect.y1 }
        } else {
            Layout { x0: rect.x0, y0: at, x1: rect.x1, y1: at + size }
        };
        place(cell, sub, out);
        at += size + CELL_GAP;
    }
}

/// How much of `total` each cell gets.
///
/// A cell that asked for a size (`theme(width =, height =)`) is given it; what is
/// left over is split evenly between the cells that asked for nothing. When every
/// cell has asked, the asks are scaled to fill the page rather than leaving a
/// band of white — a page states proportions as well as sizes, and the two
/// readings only differ by a constant.
fn divide(cells: &[Figure], horizontal: bool, total: f64) -> Vec<f64> {
    let asks: Vec<Option<f64>> = cells.iter().map(|c| c.ask(horizontal)).collect();
    let claimed: f64 = asks.iter().flatten().sum();
    let free = asks.iter().filter(|a| a.is_none()).count();

    if free == 0 {
        let scale = if claimed > 0.0 { total / claimed } else { 1.0 };
        return asks.iter().map(|a| a.unwrap_or(0.0) * scale).collect();
    }
    let each = ((total - claimed) / free as f64).max(0.0);
    asks.iter().map(|a| a.unwrap_or(each)).collect()
}

// ---------------------------------------------------------------------------
// Sharing — the one rule
// ---------------------------------------------------------------------------

/// The cells that bind the same column to `channel`, grouped by that column.
///
/// An unbound axis (a histogram's count) has no column and joins nothing: it is
/// a measurement each plot made for itself, and two of them are not one axis
/// however alike they look.
fn groups(measured: &[Drawn], channel: &Channel) -> Vec<Vec<usize>> {
    let mut out: Vec<(String, Vec<usize>)> = Vec::new();
    for (i, drawn) in measured.iter().enumerate() {
        let field = axis(drawn, channel).field.clone();
        if field.is_empty() {
            continue;
        }
        match out.iter_mut().find(|(f, _)| *f == field) {
            Some((_, members)) => members.push(i),
            None => out.push((field, vec![i])),
        }
    }
    out.into_iter().map(|(_, m)| m).filter(|m| m.len() > 1).collect()
}

fn axis<'a>(drawn: &'a Drawn, channel: &Channel) -> &'a crate::render::AxisFacts {
    match channel {
        Channel::Y => &drawn.y,
        _ => &drawn.x,
    }
}

/// Apply the rule to one group: one scale, one extent, drawn once.
fn share(
    group: &[usize],
    cells: &[Cell],
    measured: &[Drawn],
    channel: &Channel,
    fits: &mut [Fit],
    specs: &mut [PlotSpec],
    diagnostics: &mut Vec<Diagnostic>,
) {
    let horizontal = *channel == Channel::X;
    let facts: Vec<&crate::render::AxisFacts> =
        group.iter().map(|&i| axis(&measured[i], channel)).collect();
    let field = facts[0].field.clone();

    // --- one scale ---------------------------------------------------------
    // Categorical first, because its domain is a *list* and unioning two lists
    // of slots is not something the wire can say today: `limits` is a pair of
    // numbers. Where the categories agree — the ordinary case, since the plots
    // are usually reading one table — there is nothing to unify; where they do
    // not, the plots are placed against different slots and that is said out
    // loud rather than drawn as though it were not happening.
    if facts.iter().any(|f| f.cats.is_some()) {
        let first = facts[0].cats.clone().unwrap_or_default();
        if facts.iter().any(|f| f.cats.clone().unwrap_or_default() != first) {
            diagnostics.push(Diagnostic {
                kind: DiagnosticKind::Assumption,
                message: format!(
                    "gog: composed plots share `{field}` on {ax}, but their categories differ, \
                     so each is drawn against its own slots. Give both plots the same rows for \
                     `{field}` — or facet by it instead of composing — if they are meant to line up.",
                    ax = if horizontal { "x" } else { "y" },
                ),
            });
        }
    } else {
        let lo = facts.iter().map(|f| f.range.0).fold(f64::INFINITY, f64::min);
        let hi = facts.iter().map(|f| f.range.1).fold(f64::NEG_INFINITY, f64::max);
        if lo.is_finite() && hi.is_finite() && hi > lo {
            for &i in group {
                // The range is in the units the *scale* works in; a stated
                // domain arrives in the data's own (spec §10), so a log axis
                // converts back out of decades before it says so.
                let base = axis(&measured[i], channel).log_base;
                let out = |v: f64| match base {
                    Some(b) => b.powf(v),
                    None => v,
                };
                set_limits(&mut specs[i], channel, out(lo), out(hi));
            }
        }
    }

    // --- one extent, and one axis ------------------------------------------
    // In page coordinates, because the panels being compared are in different
    // cells. The intersection is what every plot in the group can reach, so
    // fitting to it only ever takes room away — no tick label is squeezed out of
    // a margin that was measured for it.
    let (mut lo, mut hi) = (f64::NEG_INFINITY, f64::INFINITY);
    for &i in group {
        let (c, p) = (&cells[i].rect, &measured[i].panel);
        let (a, b) = if horizontal {
            (c.x0 + p.x0, c.x0 + p.x1)
        } else {
            (c.y0 + p.y0, c.y0 + p.y1)
        };
        lo = lo.max(a);
        hi = hi.min(b);
    }
    if hi - lo < ALIGNABLE {
        // Side by side on the same column: one scale, two places. Each plot
        // keeps its own extent and draws its own axis.
        return;
    }

    // The axis belongs to the edge it lives on — x along the bottom, y down the
    // left — so the plot nearest that edge is the one that draws it, and the
    // rest give up the margin. Ties (two plots ending at the same edge) leave
    // both drawing, which is right: they are neighbors, not one axis split.
    let edge = group
        .iter()
        .map(|&i| if horizontal { cells[i].rect.y1 } else { cells[i].rect.x0 })
        .fold(if horizontal { f64::NEG_INFINITY } else { f64::INFINITY },
              if horizontal { f64::max } else { f64::min });

    for &i in group {
        let c = &cells[i].rect;
        let draws = if horizontal { c.y1 >= edge - 0.5 } else { c.x0 <= edge + 0.5 };
        if horizontal {
            fits[i].panel_x = Some((lo - c.x0, hi - c.x0));
            fits[i].draw_x_axis = draws;
        } else {
            fits[i].panel_y = Some((lo - c.y0, hi - c.y0));
            fits[i].draw_y_axis = draws;
        }
    }
}

/// State the domain of `channel` on the binding the axis is read from.
///
/// The same search [`PlotSpec::axis_def`] makes, because that is the definition
/// the renderer will consult: the plot's own binding when there is one, else the
/// first layer that names its own. Every layer that binds the channel gets it,
/// so a two-table plot cannot end up with one layer on the page's scale and
/// another on its own.
fn set_limits(spec: &mut PlotSpec, channel: &Channel, lo: f64, hi: f64) {
    let limits = Some([Some(lo), Some(hi)]);
    let plot_level = match channel {
        Channel::X => spec.x.as_mut(),
        Channel::Y => spec.y.as_mut(),
        _ => None,
    };
    if let Some(def) = plot_level {
        // A domain the caller stated themselves is the caller's, not the page's
        // to overrule (spec §10).
        if def.limits.is_none() {
            def.limits = limits;
        }
    }
    for layer in spec.layers.iter_mut() {
        if let Some(def) = layer.encodings.get_mut(channel) {
            if def.limits.is_none() {
                def.limits = limits;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{Layer, Mark, PageSpec, ThemeSpec, Transform};

    fn data() -> HashMap<String, DataFrame> {
        let df = DataFrame::new()
            .with_float("speed", vec![4.0, 7.0, 8.0, 12.0, 15.0, 18.0, 20.0, 24.0])
            .with_float("dist", vec![2.0, 4.0, 16.0, 24.0, 36.0, 56.0, 64.0, 120.0]);
        let mut data = HashMap::new();
        data.insert("cars".to_string(), df);
        data
    }

    fn scatter() -> PlotSpec {
        PlotSpec::new().data("cars").x("speed").y("dist").layer(Layer::new(Mark::Point))
    }

    fn top() -> PlotSpec {
        let mut spec = PlotSpec::new()
            .data("cars")
            .x("speed")
            .layer(Layer::new(Mark::Bar).transform(Transform::Bin));
        spec.theme = ThemeSpec { height: Some(120.0), ..ThemeSpec::default() };
        spec
    }

    fn right() -> PlotSpec {
        let mut spec = PlotSpec::new()
            .data("cars")
            .y("dist")
            .layer(Layer::new(Mark::Bar).transform(Transform::Bin));
        spec.theme = ThemeSpec { width: Some(120.0), ..ThemeSpec::default() };
        spec
    }

    /// The marginal plot: `top / (main | right)`.
    fn marginal() -> PageSpec {
        PageSpec {
            arrange: Arrange::Below,
            cells: vec![
                top().into(),
                Figure::Page(PageSpec {
                    arrange: Arrange::Beside,
                    cells: vec![scatter().into(), right().into()],
                }),
            ],
        }
    }

    fn placed(page: &PageSpec) -> Vec<Layout> {
        let mut cells = Vec::new();
        let root = Figure::Page(page.clone());
        place(&root, Layout { x0: 0.0, y0: 0.0, x1: 800.0, y1: 600.0 }, &mut cells);
        cells.into_iter().map(|c| c.rect).collect()
    }

    /// A cell that asked for a size gets it; the rest split what is left. This is
    /// the whole of `theme(width =, height =)` on a page.
    #[test]
    fn a_stated_size_is_the_cell_and_the_rest_share() {
        let rects = placed(&marginal());
        assert_eq!(rects.len(), 3);
        assert!((rects[0].h() - 120.0).abs() < 1e-9, "the marginal asked for 120px of height");
        assert!((rects[2].w() - 120.0).abs() < 1e-9, "and the other for 120px of width");
        assert!((rects[1].h() - (600.0 - 120.0 - CELL_GAP)).abs() < 1e-9,
                "the scatter takes what is left, less the gap between the two rows");
        assert!((rects[1].w() + rects[2].w() + CELL_GAP - 800.0).abs() < 1e-9,
                "and the width of its row is spent exactly");
    }

    /// Cells that ask for nothing divide the page evenly — the ordinary page.
    #[test]
    fn cells_that_ask_for_nothing_split_evenly() {
        let page = PageSpec { arrange: Arrange::Beside, cells: vec![scatter().into(), scatter().into()] };
        let rects = placed(&page);
        assert!((rects[0].w() - rects[1].w()).abs() < 1e-9);
        assert!((rects[0].w() + rects[1].w() + CELL_GAP - 800.0).abs() < 1e-9);
    }

    /// The rule, at the pixel: the histogram's panel runs over exactly the
    /// scatter's, which is what makes a bar stand over the points it counts.
    #[test]
    fn a_shared_column_gives_the_two_panels_one_extent() {
        let (svg, _) = render(&marginal(), &data(), 800.0, 600.0);
        assert!(svg.starts_with("<svg"), "a page is an SVG document like any other");

        let root = Figure::Page(marginal());
        let mut cells = Vec::new();
        place(&root, Layout { x0: 0.0, y0: 0.0, x1: 800.0, y1: 600.0 }, &mut cells);
        let measured: Vec<Drawn> = cells.iter()
            .map(|c| SvgRenderer::for_theme(&c.spec.theme.resolved(), c.rect.w(), c.rect.h())
                .draw(c.spec, &data()))
            .collect();
        let mut fits = vec![Fit::free(); 3];
        let mut specs: Vec<PlotSpec> = cells.iter().map(|c| c.spec.clone()).collect();
        let mut diags = Vec::new();
        for channel in [Channel::X, Channel::Y] {
            for group in groups(&measured, &channel) {
                share(&group, &cells, &measured, &channel, &mut fits, &mut specs, &mut diags);
            }
        }

        let (hist, scat) = (fits[0].panel_x.expect("the histogram is fitted"),
                            fits[1].panel_x.expect("so is the scatter"));
        // Both are in their own cell's coordinates, and both cells start at x = 0.
        assert!((hist.0 - scat.0).abs() < 1e-9 && (hist.1 - scat.1).abs() < 1e-9,
                "the shared x axis runs over the same pixels in both plots");
        assert!(fits[1].draw_x_axis && !fits[0].draw_x_axis,
                "the x axis is drawn once, by the lower plot");
        assert!(fits[1].draw_y_axis && !fits[2].draw_y_axis,
                "and the shared y axis by the left one");
        assert!(fits[2].panel_y.is_some(), "the right-hand marginal is fitted to the scatter's rows");
        assert!(diags.is_empty(), "nothing was assumed: {diags:?}");
    }

    /// The scale half of the rule. Both plots read `speed`, so both axes end up
    /// stated — and stated identically, which is what makes the ticks agree.
    #[test]
    fn a_shared_column_gives_the_two_axes_one_scale() {
        let root = Figure::Page(marginal());
        let mut cells = Vec::new();
        place(&root, Layout { x0: 0.0, y0: 0.0, x1: 800.0, y1: 600.0 }, &mut cells);
        let measured: Vec<Drawn> = cells.iter()
            .map(|c| SvgRenderer::for_theme(&c.spec.theme.resolved(), c.rect.w(), c.rect.h())
                .draw(c.spec, &data()))
            .collect();
        let mut fits = vec![Fit::free(); 3];
        let mut specs: Vec<PlotSpec> = cells.iter().map(|c| c.spec.clone()).collect();
        let mut diags = Vec::new();
        for group in groups(&measured, &Channel::X) {
            share(&group, &cells, &measured, &Channel::X, &mut fits, &mut specs, &mut diags);
        }
        let limits = |s: &PlotSpec| s.x.as_ref().and_then(|d| d.limits);
        assert_eq!(limits(&specs[0]), limits(&specs[1]),
                   "one column, one domain — whatever each plot would have picked alone");
        assert!(limits(&specs[0]).is_some());
    }

    /// Two plots side by side on the same column: the same variable, so one
    /// scale — but two places, so neither gives up its axis. The intersection
    /// of their extents is empty and the rule notices rather than fitting both
    /// panels into a sliver.
    #[test]
    fn side_by_side_on_one_column_shares_the_scale_and_not_the_place() {
        let page = PageSpec {
            arrange: Arrange::Beside,
            cells: vec![scatter().into(), scatter().into()],
        };
        let root = Figure::Page(page);
        let mut cells = Vec::new();
        place(&root, Layout { x0: 0.0, y0: 0.0, x1: 800.0, y1: 600.0 }, &mut cells);
        let measured: Vec<Drawn> = cells.iter()
            .map(|c| SvgRenderer::for_theme(&c.spec.theme.resolved(), c.rect.w(), c.rect.h())
                .draw(c.spec, &data()))
            .collect();
        let mut fits = vec![Fit::free(); 2];
        let mut specs: Vec<PlotSpec> = cells.iter().map(|c| c.spec.clone()).collect();
        let mut diags = Vec::new();
        for group in groups(&measured, &Channel::X) {
            share(&group, &cells, &measured, &Channel::X, &mut fits, &mut specs, &mut diags);
        }
        assert!(fits[0].panel_x.is_none() && fits[1].panel_x.is_none(),
                "nothing to align: the panels are in different parts of the page");
        assert!(fits[0].draw_x_axis && fits[1].draw_x_axis, "both keep their own axis");
        assert!(specs[0].x.as_ref().unwrap().limits.is_some(), "and both are on one scale");
    }

    /// Two plots that share *nothing* are only arranged. This is the property
    /// that keeps composition presentational: no scale, no extent, no axis of
    /// one plot is decided by the other.
    #[test]
    fn unrelated_plots_are_only_arranged() {
        let other = PlotSpec::new().data("cars").x("dist").y("speed").layer(Layer::new(Mark::Point));
        let page = PageSpec { arrange: Arrange::Beside, cells: vec![scatter().into(), other.into()] };
        let root = Figure::Page(page);
        let mut cells = Vec::new();
        place(&root, Layout { x0: 0.0, y0: 0.0, x1: 800.0, y1: 600.0 }, &mut cells);
        let measured: Vec<Drawn> = cells.iter()
            .map(|c| SvgRenderer::for_theme(&c.spec.theme.resolved(), c.rect.w(), c.rect.h())
                .draw(c.spec, &data()))
            .collect();
        assert!(groups(&measured, &Channel::X).is_empty(), "different columns, different axes");
        assert!(groups(&measured, &Channel::Y).is_empty());
    }

    /// A page is one document, and each cell is a viewport inside it.
    #[test]
    fn the_cells_are_nested_viewports_at_their_own_corners() {
        let (svg, _) = render(&marginal(), &data(), 800.0, 600.0);
        assert_eq!(svg.matches("<svg").count(), 4, "the page, and one per cell");
        assert!(svg.contains(r#"<svg x="0.00" y="0.00""#), "the first cell is at the origin");
        assert!(svg.ends_with("</svg>\n"));
    }
}
