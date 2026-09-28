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
//! **Panels in one row or column line up** (2026-09-26, ruled by the author from
//! before-and-after renders). After sharing, `align` moves each panel edge on a
//! common page line inward to the widest margin on that line, so a row shares a
//! top and a bottom and a column a left and a right, whether or not the plots
//! share a column. It is part of arranging, not a second kind of sharing: no
//! scale and no axis passes between the plots. Three kinds of edge are left
//! alone: an edge fixed by a shared column (unless every plot sharing it is on
//! the line), the side a plot gives up to a neighbor (which keeps a marginal
//! flush), and a plot whose measured panel `Fit` cannot hand back (a stated
//! `ratio`, a freed axis, a folded facet). The cost is room: the plot with the
//! narrower margin gives up the difference, up to a legend's width above or
//! below a neighbor with a key.
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
    let mut measured: Vec<Drawn> = cells
        .iter()
        .map(|c| {
            SvgRenderer::for_theme(&c.spec.theme.resolved(), c.rect.w(), c.rect.h())
                .draw(c.spec, data)
        })
        .collect();

    let sharing = |measured: &[Drawn], diagnostics: &mut Vec<Diagnostic>| {
        let mut fits: Vec<Fit> = vec![Fit::free(); cells.len()];
        let mut specs: Vec<PlotSpec> = cells.iter().map(|c| c.spec.clone()).collect();
        for channel in [Channel::X, Channel::Y] {
            for group in groups(measured, &channel) {
                share(&group, &cells, measured, &channel, &mut fits, &mut specs, diagnostics);
            }
        }
        (fits, specs)
    };

    // **An axis a plot gives up costs it no margin, so it is measured without
    // one.** Who draws a shared axis is decided by where the cells sit, not by
    // their panels, so it is known before any extent is fixed; a plot that gives
    // an axis up is measured again without its margin, and the extents are fixed
    // from that. Measured with it, the right plot of `a / (b | c)`, whose y axis
    // `b` draws, kept a 73 px blank strip once the shared x pinned its panel to
    // where it sat when measured alone.
    let (decided, _) = sharing(&measured, &mut Vec::new());
    for (i, cell) in cells.iter().enumerate() {
        let (draw_x, draw_y) = (decided[i].draw_x_axis, decided[i].draw_y_axis);
        if draw_x && draw_y {
            continue;
        }
        measured[i] = SvgRenderer {
            fit: Fit { draw_x_axis: draw_x, draw_y_axis: draw_y, ..Fit::free() },
            ..SvgRenderer::for_theme(&cell.spec.theme.resolved(), cell.rect.w(), cell.rect.h())
        }
        .draw(cell.spec, data);
    }

    let mut diagnostics = Vec::new();
    let (mut fits, specs) = sharing(&measured, &mut diagnostics);
    // Then the arrangement's own rule, for the plots that share nothing.
    align(&cells, &measured, &mut fits);

    // Pass two: each plot draws again, knowing what the page decided.
    let mut svg = String::with_capacity(96 * 1024);
    // A page is a group of images rather than one: each cell's own `<svg>` carries
    // its accessible name (`svg::accessible_name`), and a root role of `img` would
    // hide them all from a screen reader behind one name for the lot.
    let count = cells.len();
    svg.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{height}\" \
         viewBox=\"0 0 {width} {height}\" role=\"group\" aria-label=\"A page of {count} plots\">\n"
    ));
    // The page's own canvas fills the gaps between its cells, and it goes when
    // every cell's background paints nothing, for the reason a plot's does
    // (`svg::render`): a figure asked to be transparent is placed on a page whose
    // color it cannot know, and a white floor under it would hide that page. One
    // cell with a background of its own keeps the page white, since a floor is
    // one color and that cell has already said what it sits on.
    let see_through = cells.iter().all(|c| {
        c.spec.theme.resolved().background.as_deref().is_some_and(crate::color::paints_nothing)
    });
    if !see_through {
        svg.push_str(&format!(
            "  <rect width=\"{width}\" height=\"{height}\" fill=\"white\"/>\n"
        ));
    }
    for (i, cell) in cells.iter().enumerate() {
        let drawn = SvgRenderer {
            fit: fits[i].clone(),
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
        // A clustered panel decided this axis's order, and the shared axis
        // takes it (§9): the tree derived the order, so the panels beside it
        // read their slots in the same sequence — which is the whole marginal
        // figure. Authorities are the panels whose spec clusters this column
        // on this channel; their measured category lists are already in leaf
        // order. Page state, exactly like the shared extents below — nothing
        // enters any spec's wire form.
        let orders: Vec<Vec<String>> = group.iter()
            .filter(|&&i| crate::legality::cluster_orders(&specs[i], channel, &field))
            .filter_map(|&i| axis(&measured[i], channel).cats.clone())
            .collect();
        // Disagreeing derivations were refused by the figure check; reaching
        // them here means `GOG_STRICT=0`, where each panel keeps its own slots
        // and the mismatch is said below like any other.
        let agreed = orders.windows(2).all(|w| w[0] == w[1]);
        let orders = if agreed { orders } else { Vec::new() };
        let sets_agree = |order: &[String]| facts.iter().all(|f| {
            let mut a = f.cats.clone().unwrap_or_default();
            let mut b = order.to_vec();
            a.sort();
            b.sort();
            a == b
        });
        if let Some(order) = orders.first().filter(|o| sets_agree(o)) {
            for &i in group {
                if horizontal {
                    fits[i].cats_x = Some(order.clone());
                } else {
                    fits[i].cats_y = Some(order.clone());
                }
            }
        } else {
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
        }
    } else {
        // **A projected axis is not shared through a domain**, because `limits` does
        // two jobs at once — it selects rows in the column's own units and sets the
        // scale in the scale's — and a map is the one space where those differ.
        // Degrees select correctly and scale wrongly; projected numbers scale
        // correctly and exclude every row, which drew two empty maps and said
        // nothing, since a page writes this domain after `check_limit_rows` has run.
        //
        // Declining costs a reader nothing they would notice. A map's extent comes
        // from its projection rather than from a domain, so two maps over the same
        // columns already agree, and the extent and one-axis rules below still line
        // the cells up on the page. `AxisFacts::projected` carries the fact.
        let projected = facts.iter().any(|f| f.projected);
        let lo = facts.iter().map(|f| f.range.0).fold(f64::INFINITY, f64::min);
        let hi = facts.iter().map(|f| f.range.1).fold(f64::NEG_INFINITY, f64::max);
        // **And the span the ticks are chosen over**, which is not that range. The
        // range carries each plot's breathing margin, and a tick step rounded from
        // a span that includes margins can double: gdp's 49K rounds to a step of
        // 5K at twelve ticks, the shared range's 54K to 10K, so every composed page
        // drew coarser ticks than its plots alone. Each plot reported what its ticks
        // were chosen over; the union of those is what they are chosen over here,
        // so the shared axis is ticked the way either plot alone would tick it.
        let over = (
            facts.iter().map(|f| f.ticks_over.0).fold(f64::INFINITY, f64::min),
            facts.iter().map(|f| f.ticks_over.1).fold(f64::NEG_INFINITY, f64::max),
        );
        if !projected && lo.is_finite() && hi.is_finite() && hi > lo {
            for &i in group {
                // The range is in the units the *scale* works in; a stated
                // domain arrives in the data's own (spec §10), so a log axis
                // converts back out of decades before it says so.
                let base = axis(&measured[i], channel).log_base;
                let out = |v: f64| match base {
                    Some(b) => b.powf(v),
                    None => v,
                };
                // Only beside a domain the page stated: a plot whose caller stated
                // one keeps its own, ticks and all (spec §10).
                let stated = set_limits(&mut specs[i], channel, out(lo), out(hi));
                match horizontal {
                    true => fits[i].shared_domain_x |= stated,
                    false => fits[i].shared_domain_y |= stated,
                }
                if stated && over.0.is_finite() && over.1.is_finite() && over.1 >= over.0 {
                    match horizontal {
                        true => fits[i].ticks_x = Some(over),
                        false => fits[i].ticks_y = Some(over),
                    }
                }
            }
        }
    }

    // --- one extent, and one axis ------------------------------------------
    // In page coordinates, because the panels being compared are in different
    // cells. The intersection is what every plot in the group can reach, so
    // fitting to it only ever takes room away — no tick label is squeezed out of
    // a margin that was measured for it.
    //
    // Only for plots whose measured panel a fitted extent hands back on this
    // axis. A folded facet whose last row is short measured its panels as ending
    // in the first column, and two of them stacked on a shared column were each
    // squeezed into one column's width, the rest of the cell empty. Such a group
    // keeps the one scale above and each plot its own extent and axis, as two
    // plots side by side do.
    if group.iter().any(|&i| !measured_fits_on(cells[i].spec, horizontal)) {
        return;
    }
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

// ---------------------------------------------------------------------------
// Alignment — one row, one top and bottom; one column, one left and right
// ---------------------------------------------------------------------------

/// How near two cell edges have to be to count as one line of the page, in px.
const SAME_LINE: f64 = 0.5;

/// Line up the panels of cells that begin or end on the same line of the page.
///
/// Sharing lines up the plots that read one column, and only them. Two plots
/// that share nothing were placed and then left alone, so a stack whose y tick
/// labels differ in width had its panels' left edges wherever each plot's
/// labels put them. This is the arrangement's own rule: cells that begin on one
/// line put their panels' near edges on one line, and cells that end on one
/// line put their far edges on one, whether or not they share a column. A row
/// shares its top and bottom, a column its left and right, and a nested page
/// lines up along its outside edges. Each edge moves inward to the widest margin
/// on its line, so, like sharing, it only ever takes room away.
///
/// Three kinds of edge stay where they are. An edge a shared column fixed, unless
/// every plot sharing that extent stands on the line, in which case they move
/// together and stay one extent. An edge on the side of an axis the plot gives up
/// to a page-mate, since an axis a plot does not draw costs it no margin. And
/// every edge of a plot whose measured panels cannot be handed back as a fitted
/// extent ([`measured_is_fitted`]).
fn align(cells: &[Cell], measured: &[Drawn], fits: &mut [Fit]) {
    for channel in [Channel::X, Channel::Y] {
        let horizontal = channel == Channel::X;
        // The groups a shared column gave one extent on this axis.
        let fixed: Vec<Vec<usize>> = groups(measured, &channel)
            .into_iter()
            .filter(|g| extent(&fits[g[0]], horizontal).is_some())
            .collect();
        let origin = |i: usize| if horizontal { cells[i].rect.x0 } else { cells[i].rect.y0 };
        // Each panel's two edges on this axis, in page coordinates.
        let mut edges: Vec<(f64, f64)> = (0..cells.len())
            .map(|i| {
                let p = &measured[i].panel;
                let (a, b) = extent(&fits[i], horizontal)
                    .unwrap_or(if horizontal { (p.x0, p.x1) } else { (p.y0, p.y1) });
                (origin(i) + a, origin(i) + b)
            })
            .collect();
        let before = edges.clone();
        // The cells whose panels this lines up on the edge the y axis lives on,
        // for their names below.
        let mut lined_up: Vec<usize> = Vec::new();

        for far in [false, true] {
            let line = |i: usize| {
                let r = &cells[i].rect;
                match (horizontal, far) {
                    (true, false) => r.x0,
                    (true, true) => r.x1,
                    (false, false) => r.y0,
                    (false, true) => r.y1,
                }
            };
            // y is ticked down the left, x along the bottom.
            let own = |i: usize| {
                let gives_up = match (horizontal, far) {
                    (true, false) => !fits[i].draw_y_axis,
                    (false, true) => !fits[i].draw_x_axis,
                    _ => false,
                };
                !gives_up && measured_is_fitted(cells[i].spec)
            };
            let movable = |i: usize| {
                own(i)
                    && match fixed.iter().find(|g| g.contains(&i)) {
                        Some(g) => g.iter().all(|&j| own(j) && (line(j) - line(i)).abs() <= SAME_LINE),
                        None => true,
                    }
            };
            let mut lines: Vec<(f64, Vec<usize>)> = Vec::new();
            for i in (0..cells.len()).filter(|&i| movable(i)) {
                let at = line(i);
                match lines.iter_mut().find(|(l, _)| (*l - at).abs() <= SAME_LINE) {
                    Some((_, members)) => members.push(i),
                    None => lines.push((at, vec![i])),
                }
            }
            for (_, members) in lines.into_iter().filter(|(_, m)| m.len() > 1) {
                if horizontal && !far {
                    lined_up.extend(members.iter().copied());
                }
                let to = if far {
                    members.iter().map(|&i| edges[i].1).fold(f64::INFINITY, f64::min)
                } else {
                    members.iter().map(|&i| edges[i].0).fold(f64::NEG_INFINITY, f64::max)
                };
                for &i in &members {
                    let (lo, hi) = if far { (edges[i].0, to) } else { (to, edges[i].1) };
                    // A panel too narrow to reach the line keeps its own edge
                    // rather than collapsing onto it.
                    if hi - lo >= ALIGNABLE {
                        edges[i] = (lo, hi);
                    }
                }
            }
        }

        for i in 0..cells.len() {
            if edges[i] != before[i] {
                let fitted = Some((edges[i].0 - origin(i), edges[i].1 - origin(i)));
                if horizontal {
                    fits[i].panel_x = fitted;
                } else {
                    fits[i].panel_y = fitted;
                }
            }
        }

        // **A y name goes where its panel went**, unless the panel was lined up
        // with others that start on its line. Those keep one column of names at
        // the edge, which is what makes an aligned stack read as one figure. Any
        // other panel the page moved was moved to run under a plot on another
        // line, by as much as a column of the page, and its name moves with it.
        if horizontal {
            for i in 0..cells.len() {
                if lined_up.contains(&i) {
                    continue;
                }
                if let Some((x0, _)) = fits[i].panel_x {
                    fits[i].y_name_shift = (x0 - measured[i].panel.x0).max(0.0);
                }
            }
        }
    }
}

fn extent(fit: &Fit, horizontal: bool) -> Option<(f64, f64)> {
    if horizontal { fit.panel_x } else { fit.panel_y }
}

/// Is the rectangle this plot measured the one a fitted extent would hand back?
///
/// `Drawn::panel` runs from the first panel's corner to the last panel's, and a
/// fitted extent is read as the area the grid divides. The two agree for every
/// plot but three. A stated `ratio` insets its panel in the cell. A freed axis
/// puts each facet cell's own tick labels beside its panel, inside the area. A
/// folded ribbon puts a strip above each panel, and when its last row is short,
/// its last panel ends before the last column. Handing any of those measurements
/// back would shift or squeeze the grid, so those plots are left where they are.
fn measured_is_fitted(spec: &PlotSpec) -> bool {
    let freed = spec.x.iter().chain(spec.y.iter()).any(|d| d.free)
        || spec.layers.iter().any(|l| l.encodings.values().any(|d| d.free));
    let folded = spec.facet.as_ref().is_some_and(|f| f.wrap.is_some());
    spec.theme.resolved().ratio.is_none() && !freed && !folded
}

/// [`measured_is_fitted`], asked of one direction, for `share`.
///
/// A freed axis moves its tick labels inside the grid only across the other
/// direction: a freed `y` puts each panel's numbers to its left, which changes
/// where the panels start horizontally, and a freed `x` puts them underneath,
/// which changes where they end vertically. So two facets freed on `x` still
/// hand back their horizontal extent, and a shared `x` still lines them up.
/// A ratio and a fold disagree in both directions.
fn measured_fits_on(spec: &PlotSpec, horizontal: bool) -> bool {
    let freed = |channel: Channel| {
        let axis = if channel == Channel::X { &spec.x } else { &spec.y };
        axis.as_ref().is_some_and(|d| d.free)
            || spec.layers.iter().any(|l| l.encodings.get(&channel).is_some_and(|d| d.free))
    };
    let folded = spec.facet.as_ref().is_some_and(|f| f.wrap.is_some());
    let across = if horizontal { Channel::Y } else { Channel::X };
    spec.theme.resolved().ratio.is_none() && !folded && !freed(across)
}

/// State the domain of `channel` on the binding the axis is read from.
///
/// The same search [`PlotSpec::axis_def`] makes, because that is the definition
/// the renderer will consult: the plot's own binding when there is one, else the
/// first layer that names its own. Every layer that binds the channel gets it,
/// so a two-table plot cannot end up with one layer on the page's scale and
/// another on its own.
///
/// Returns whether the page stated anything: `false` when every binding already
/// carried a domain of the caller's own, which the page does not overrule.
fn set_limits(spec: &mut PlotSpec, channel: &Channel, lo: f64, hi: f64) -> bool {
    let limits = Some([Some(lo), Some(hi)]);
    let mut stated = false;
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
            stated = true;
        }
    }
    for layer in spec.layers.iter_mut() {
        if let Some(def) = layer.encodings.get_mut(channel) {
            if def.limits.is_none() {
                def.limits = limits;
                stated = true;
            }
        }
    }
    stated
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{FacetSpec, Layer, Mark, PageSpec, ThemeSpec, Transform};

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
                    theme: ThemeSpec::default(),
                }),
            ],
            theme: ThemeSpec::default(),
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

    /// A **page** nested as a cell asks with its own `theme()`, the way a plot
    /// does, and the two readings of the same statement agree: it is the figure's
    /// size at the top level and its cell's size one level in.
    ///
    /// The stated size beats the derivation from the cells, which is the case
    /// worth pinning because both answers exist here — the inner page's plots ask
    /// for nothing, so deriving would have returned `None` and split evenly.
    #[test]
    fn a_nested_page_asks_with_its_own_theme() {
        let inner = PageSpec {
            arrange: Arrange::Beside,
            cells: vec![scatter().into(), scatter().into()],
            theme: ThemeSpec { height: Some(180.0), ..ThemeSpec::default() },
        };
        let outer = PageSpec {
            arrange: Arrange::Below,
            cells: vec![Figure::Page(inner), scatter().into()],
            theme: ThemeSpec::default(),
        };
        let rects = placed(&outer);
        assert_eq!(rects.len(), 3, "two cells in the inner page, one below it");
        assert!((rects[0].h() - 180.0).abs() < 1e-9, "the nested page asked for 180px");
        assert!((rects[1].h() - 180.0).abs() < 1e-9, "and both of its cells got it");
        assert!((rects[2].h() - (600.0 - 180.0 - CELL_GAP)).abs() < 1e-9,
                "the plot below takes what is left, less the gap");
    }

    /// Cells that ask for nothing divide the page evenly — the ordinary page.
    #[test]
    fn cells_that_ask_for_nothing_split_evenly() {
        let page = PageSpec {
            arrange: Arrange::Beside,
            cells: vec![scatter().into(), scatter().into()],
            theme: ThemeSpec::default(),
        };
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

    /// Two maps composed draw two maps, and neither is handed a domain.
    ///
    /// **The one axis whose scale is not in its column's units**, and the page used
    /// to share it anyway: `limits` selects rows in degrees and scales in projected
    /// units, so writing the projected range excluded every row of a `lon` running
    /// -180 to 180 and drew two empty panels. Silently, because a page writes that
    /// domain after `check_limit_rows` — the check whose whole job is refusing it.
    ///
    /// Pinned on the **marks** rather than on the absence of `limits`, because the
    /// failure a reader met was an empty picture and a count is what sees it. The
    /// domain is asserted too, since that is the mechanism and a later change could
    /// empty the panels a different way.
    #[test]
    fn two_composed_maps_each_draw_and_neither_is_given_a_domain() {
        let world = DataFrame::new()
            .with_float("lon", vec![-160.0, -80.0, 0.0, 80.0, 160.0, -160.0])
            .with_float("lat", vec![-70.0, -20.0, 0.0, 20.0, 70.0, -70.0]);
        let data = HashMap::from([("w".to_string(), world)]);
        let map_of = |preserve: crate::ir::Preserve| {
            PlotSpec::new().data("w").x("lon").y("lat")
                .coord(crate::ir::CoordSpace::Map(crate::ir::MapView { preserve }))
                .layer(Layer::new(Mark::Path))
        };
        let page = PageSpec {
            arrange: Arrange::Beside,
            cells: vec![
                map_of(crate::ir::Preserve::Area).into(),
                map_of(crate::ir::Preserve::Angle).into(),
            ],
            theme: ThemeSpec::default(),
        };
        let (svg, _) = render(&page, &data, 800.0, 400.0);
        let drawn = svg.matches("<polyline").count() + svg.matches("<path d=").count();
        assert!(drawn >= 2, "each cell draws its own map, got {drawn} marks");

        // And the mechanism: a projected axis is never handed a stated domain.
        let root = Figure::Page(page);
        let mut cells = Vec::new();
        place(&root, Layout { x0: 0.0, y0: 0.0, x1: 800.0, y1: 400.0 }, &mut cells);
        let measured: Vec<Drawn> = cells.iter()
            .map(|c| SvgRenderer::for_theme(&c.spec.theme.resolved(), c.rect.w(), c.rect.h())
                .draw(c.spec, &data))
            .collect();
        assert!(measured[0].x.projected, "a map's x is not in its column's units");
        let mut fits = vec![Fit::free(); cells.len()];
        let mut specs: Vec<PlotSpec> = cells.iter().map(|c| c.spec.clone()).collect();
        let mut diags = Vec::new();
        for channel in [Channel::X, Channel::Y] {
            for group in groups(&measured, &channel) {
                share(&group, &cells, &measured, &channel, &mut fits, &mut specs, &mut diags);
            }
        }
        for s in &specs {
            assert!(s.x.as_ref().and_then(|d| d.limits).is_none(),
                    "a projected axis must not be shared through `limits`");
            assert!(s.y.as_ref().and_then(|d| d.limits).is_none());
        }
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
            theme: ThemeSpec::default(),
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

    /// **A shared axis is ticked the way each plot alone ticks it.** The page
    /// states the union of the plots' fitted ranges, margins included, and a step
    /// chosen over that span rounds up: gdp's 49K gives a step of 5K at twelve
    /// ticks and the shared range's 54K gave 10K, so a page putting three ticks
    /// beside twelve drew two beside six. The ticks are now chosen over what each
    /// plot chose its own over, and only the range is the page's.
    #[test]
    fn a_shared_axis_is_ticked_the_way_each_plot_alone_ticks_it() {
        let gdp = HashMap::from([("t".to_string(), DataFrame::new()
            .with_float("gdp", vec![277.55, 12000.0, 49357.19])
            .with_float("life", vec![40.0, 60.0, 82.0]))]);
        let plot = |n: usize| {
            let mut p = PlotSpec::new().data("t").x("gdp").y("life")
                .layer(Layer::new(Mark::Point));
            p.x = Some(crate::ir::ChannelDef::field("gdp").with_tick_count(n));
            p
        };
        let labels = |svg: &str| -> Vec<String> {
            svg.split("<text").skip(1)
                .filter_map(|t| t.split_once('>').and_then(|(_, r)| r.split_once("</text>")))
                .map(|(l, _)| l.to_string())
                .filter(|l| l.ends_with('K') || l == "0")
                .collect()
        };
        let page = PageSpec {
            arrange: Arrange::Beside,
            cells: vec![plot(3).into(), plot(12).into()],
            theme: ThemeSpec::default(),
        };
        let (svg, _) = render(&page, &gdp, 800.0, 600.0);
        // The page nests one `<svg>` per cell, in order.
        let cells: Vec<&str> = svg.split("<svg x=").skip(1).collect();
        assert_eq!(cells.len(), 2);
        for (cell, n) in cells.iter().zip([3, 12]) {
            let alone = SvgRenderer::for_theme(&ThemeSpec::default(), 390.0, 600.0)
                .render(&plot(n), &gdp);
            assert_eq!(labels(cell), labels(&alone), "tick_count = {n} on the page");
        }
        assert_eq!(labels(cells[1]).len(), 11, "{:?}", labels(cells[1]));
    }

    /// A page of transparent plots paints no floor under them, or the page it is
    /// placed on still never shows through; one plot with a background of its own
    /// keeps the white floor.
    #[test]
    fn a_page_of_transparent_plots_paints_no_floor() {
        let clear = |p: PlotSpec| PlotSpec {
            theme: ThemeSpec { background: Some("transparent".into()), ..Default::default() },
            ..p
        };
        let floor = r#"<rect width="800" height="600" fill="white"/>"#;
        let page = |cells: Vec<PlotSpec>| PageSpec {
            arrange: Arrange::Beside,
            cells: cells.into_iter().map(Into::into).collect(),
            theme: ThemeSpec::default(),
        };
        let (svg, _) = render(&page(vec![clear(scatter()), clear(scatter())]), &data(), 800.0, 600.0);
        assert!(!svg.contains(floor), "the page painted a floor under transparent plots");
        assert!(!svg.contains(r#"fill="white"/>"#), "a cell painted its own canvas");
        let (svg, _) = render(&page(vec![clear(scatter()), scatter()]), &data(), 800.0, 600.0);
        assert!(svg.contains(floor), "a plot with a background keeps the floor");
    }

    /// Two plots that share *nothing* are only arranged. This is the property
    /// A page is a group of named images: the root says how many plots it holds, and
    /// each cell's own `<svg>` carries that plot's name, which a root role of `img`
    /// would hide from a screen reader.
    #[test]
    fn a_page_is_a_group_of_named_plots() {
        let other = PlotSpec::new().data("cars").x("dist").y("speed").layer(Layer::new(Mark::Point));
        let page = PageSpec {
            arrange: Arrange::Beside,
            cells: vec![scatter().into(), other.into()],
            theme: ThemeSpec::default(),
        };
        let (svg, _) = render(&page, &data(), 800.0, 600.0);
        let root = svg.lines().next().unwrap_or("");
        assert!(root.contains(r#"role="group" aria-label="A page of 2 plots""#), "{root}");
        let cells: Vec<&str> = svg.lines().skip(1).filter(|l| l.trim_start().starts_with("<svg ")).collect();
        assert_eq!(cells.len(), 2, "{cells:?}");
        assert!(cells[0].contains(r#"role="img" aria-label="Points, x is speed, y is dist""#), "{cells:?}");
        assert!(cells[1].contains(r#"aria-label="Points, x is dist, y is speed""#), "{cells:?}");
    }

    /// Unrelated plots share no scale and no axis: neither decides the other's.
    /// Their panel edges still line up in the row (`align`), which is arranging,
    /// not a shared extent.
    #[test]
    fn unrelated_plots_are_only_arranged() {
        let other = PlotSpec::new().data("cars").x("dist").y("speed").layer(Layer::new(Mark::Point));
        let page = PageSpec {
            arrange: Arrange::Beside,
            cells: vec![scatter().into(), other.into()],
            theme: ThemeSpec::default(),
        };
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

    /// Plots that share no column still line up: a stack shares its left and
    /// right panel edges, a row its top and bottom, whatever each plot's tick
    /// labels asked for.
    #[test]
    fn plots_that_share_nothing_line_up_in_their_row_and_column() {
        let df = DataFrame::new()
            .with_float("speed", vec![4.0, 7.0, 8.0, 12.0])
            .with_float("dist", vec![2.0, 4.0, 16.0, 24.0])
            .with_float("big", vec![0.0012, 0.0031, 0.0054, 0.0087]);
        let data = HashMap::from([("cars".to_string(), df)]);
        let narrow = PlotSpec::new().data("cars").x("speed").y("dist").layer(Layer::new(Mark::Point));
        let wide = PlotSpec::new().data("cars").x("dist").y("big").layer(Layer::new(Mark::Point));
        let mut turned = wide.clone();
        turned.theme = ThemeSpec { tick_angle: Some(60.0), ..ThemeSpec::default() };

        // Where each panel's edges end up on the page: (left, right, top, bottom).
        let decide = |arrange: Arrange, a: &PlotSpec, b: &PlotSpec| {
            let root = Figure::Page(PageSpec {
                arrange,
                cells: vec![a.clone().into(), b.clone().into()],
                theme: ThemeSpec::default(),
            });
            let mut cells = Vec::new();
            place(&root, Layout { x0: 0.0, y0: 0.0, x1: 800.0, y1: 600.0 }, &mut cells);
            let measured: Vec<Drawn> = cells.iter()
                .map(|c| SvgRenderer::for_theme(&c.spec.theme.resolved(), c.rect.w(), c.rect.h())
                    .draw(c.spec, &data))
                .collect();
            let mut fits = vec![Fit::free(); 2];
            align(&cells, &measured, &mut fits);
            let edges = |i: usize, fitted: bool| {
                let (c, p) = (&cells[i].rect, &measured[i].panel);
                let (x0, x1) = fits[i].panel_x.filter(|_| fitted).unwrap_or((p.x0, p.x1));
                let (y0, y1) = fits[i].panel_y.filter(|_| fitted).unwrap_or((p.y0, p.y1));
                [c.x0 + x0, c.x0 + x1, c.y0 + y0, c.y0 + y1]
            };
            ([edges(0, false), edges(1, false)], [edges(0, true), edges(1, true)])
        };

        let (alone, page) = decide(Arrange::Below, &narrow, &wide);
        assert!((alone[0][0] - alone[1][0]).abs() > 1.0, "the premise: the y labels differ in width");
        assert!((page[0][0] - page[1][0]).abs() < 1e-9, "one left edge down the column");
        assert!((page[0][1] - page[1][1]).abs() < 1e-9, "and one right edge");
        assert!(page[0][0] >= alone[0][0] && page[1][0] >= alone[1][0], "an edge only moves inward");

        let (alone, page) = decide(Arrange::Beside, &narrow, &turned);
        assert!((alone[0][3] - alone[1][3]).abs() > 1.0, "the premise: the x labels differ in height");
        assert!((page[0][3] - page[1][3]).abs() < 1e-9, "one bottom edge along the row");
        assert!((page[0][2] - page[1][2]).abs() < 1e-9, "and one top edge");
    }

    /// A y name goes where its panel went. A panel moved to run under a plot on
    /// another line keeps its name beside it, where the name stayed at the cell's
    /// edge, 450px away; panels lined up on one line keep one column of names.
    #[test]
    fn a_y_name_follows_its_panel_unless_the_panels_are_lined_up() {
        // Each cell's panel left edge and its y name's x, in the cell's own terms.
        let names = |page: &PageSpec| -> Vec<(f64, Option<f64>)> {
            let (svg, _) = render(page, &data(), 800.0, 600.0);
            let first_number = |text: &str, after: &str, until: char| -> Option<f64> {
                text.split(after).nth(1)?.split(until).next()?.parse().ok()
            };
            svg.split("<svg ").skip(2).map(|cell| {
                let clip = cell.split("<clipPath").nth(1).expect("a clipped panel");
                let panel = first_number(clip, "<rect x=\"", '"').expect("a panel edge");
                (panel, first_number(cell, "<text transform=\"rotate(-90 ", ' '))
            }).collect()
        };
        let plot = |x: &str, y: &str| {
            PlotSpec::new().data("cars").x(x).y(y).layer(Layer::new(Mark::Point))
        };
        let histogram = PlotSpec::new().data("cars").x("speed")
            .layer(Layer::new(Mark::Bar).transform(Transform::Bin));
        let two = |arrange: Arrange, a: Figure, b: Figure| PageSpec {
            arrange, cells: vec![a, b], theme: ThemeSpec::default(),
        };

        // `(a | b) / c`, with `c` sharing `speed` with `b` alone.
        let top = two(Arrange::Beside, plot("dist", "speed").into(), plot("speed", "dist").into());
        let cells = names(&two(Arrange::Below, Figure::Page(top), histogram.clone().into()));
        let (panel, name) = cells[2];
        let name = name.expect("the histogram names its y axis");
        assert!(panel > 300.0, "the premise: the shared column moved the panel: {cells:?}");
        assert!(panel - name < 80.0, "the name sits beside its panel: {cells:?}");

        // Two stacked plots whose tick labels differ keep one column of names.
        let cells = names(&two(Arrange::Below, plot("speed", "dist").into(), histogram.into()));
        assert!((cells[0].0 - cells[1].0).abs() < 1e-9, "the panels are lined up: {cells:?}");
        assert_eq!(cells[0].1, cells[1].1, "and so are their names: {cells:?}");
    }

    /// **An axis a plot gives up costs it no margin, under a shared extent too.**
    /// In `hist / (a | b)`, `b` shares its `y` with `a`, which draws it, and its `x`
    /// with the histogram above. The shared `x` pinned `b`'s panel to where it sat
    /// when measured alone, y margin included, so a blank strip stood where the
    /// axis it gave up would have been. It now sits where the pair alone puts it.
    #[test]
    fn a_plot_that_gives_up_its_y_axis_keeps_no_margin_under_a_shared_x() {
        let panels = |page: &PageSpec| -> Vec<f64> {
            let (svg, _) = render(page, &data(), 800.0, 600.0);
            svg.split("<svg ").skip(2).map(|cell| {
                let clip = cell.split("<clipPath").nth(1).expect("a clipped panel");
                clip.split("<rect x=\"").nth(1).and_then(|r| r.split('"').next())
                    .and_then(|v| v.parse().ok()).expect("a panel edge")
            }).collect()
        };
        let plot = |x: &str, y: &str| {
            PlotSpec::new().data("cars").x(x).y(y).layer(Layer::new(Mark::Point))
        };
        let histogram = PlotSpec::new().data("cars").x("speed")
            .layer(Layer::new(Mark::Bar).transform(Transform::Bin));
        let two = |arrange: Arrange, a: Figure, b: Figure| PageSpec {
            arrange, cells: vec![a, b], theme: ThemeSpec::default(),
        };
        let pair = two(Arrange::Beside, plot("dist", "big").into(), plot("speed", "big").into());
        let alone = panels(&pair);
        let nested = panels(&two(Arrange::Below, histogram.into(), Figure::Page(pair)));
        assert!(alone[1] < alone[0], "the premise: the right plot draws no y axis: {alone:?}");
        assert!((nested[2] - alone[1]).abs() < 1e-9,
            "the right plot sits where it does alone: {nested:?} against {alone:?}");
    }

    /// A page shares an axis by writing its range into each plot as a domain,
    /// margins and all, and a bin is not cut on that: a histogram stacked over a
    /// scatter keeps the bins it has alone (Law 6). Cut on the shared range, its
    /// first bar started in the margin, at the panel's left edge.
    #[test]
    fn a_shared_axis_does_not_recut_a_histogram() {
        let df = DataFrame::new()
            .with_float("v", (1..=20).map(|i| i as f64).collect())
            .with_float("w", (1..=20).map(|i| (i * 7 % 11) as f64).collect());
        let data = HashMap::from([("t".to_string(), df)]);
        let hist = PlotSpec::new().data("t").x("v")
            .layer(Layer::new(Mark::Bar).transform(crate::ir::Transform::Bin));
        let scatter = PlotSpec::new().data("t").x("v").y("w").layer(Layer::new(Mark::Point));
        let page = PageSpec {
            arrange: Arrange::Below, cells: vec![hist.into(), scatter.into()], theme: ThemeSpec::default(),
        };
        let (svg, _) = render(&page, &data, 800.0, 600.0);
        let cell = svg.split("<svg ").nth(2).expect("the histogram's cell");
        let num = |s: &str, key: &str| s.split(key).nth(1)?.split('"').next()?.parse::<f64>().ok();
        let panel_left = cell.split("<clipPath").nth(1).and_then(|c| num(c, " x=\"")).unwrap();
        let first_bar = cell.lines()
            .filter(|l| l.contains("<rect") && l.contains("fill=\"#4e79a7\""))
            .filter_map(|l| num(l, " x=\""))
            .fold(f64::INFINITY, f64::min);
        assert!(first_bar > panel_left + 1.0,
            "the first bin starts at the data, not the page's margin: bar {first_bar}, panel {panel_left}");
    }

    /// A shared column gives its plots one extent only where each measured panel
    /// is the extent a fit hands back. Two folded facets stacked on one column
    /// measured their panels as ending in the first column, and each was squeezed
    /// into one column's width; two facets freed on `x` still line up on it.
    #[test]
    fn a_shared_column_leaves_a_folded_facet_its_own_width() {
        let df = DataFrame::new()
            .with_float("speed", vec![4.0, 7.0, 8.0, 12.0, 15.0, 18.0])
            .with_float("dist", vec![2.0, 4.0, 16.0, 24.0, 36.0, 56.0])
            .with_float("big", vec![0.0012, 0.0031, 0.0054, 0.0087, 0.0102, 0.0140])
            .with_str("g", ["a", "b", "c", "a", "b", "c"].iter().map(|s| s.to_string()).collect());
        let data = HashMap::from([("cars".to_string(), df)]);
        let faceted = |y: &str, facet: FacetSpec| {
            let mut spec = PlotSpec::new().data("cars").x("speed").y(y).layer(Layer::new(Mark::Point));
            spec.facet = Some(facet);
            spec
        };
        // Each cell's rightmost panel edge, in the cell's own terms.
        let right_edges = |page: PageSpec| -> Vec<f64> {
            let (svg, _) = render(&page, &data, 800.0, 600.0);
            svg.split("<svg ").skip(2).map(|cell| {
                cell.split("<clipPath").skip(1).filter_map(|c| {
                    let num = |key: &str| c.split(key).nth(1)?.split('"').next()?.parse::<f64>().ok();
                    Some(num(" x=\"")? + num(" width=\"")?)
                }).fold(f64::NEG_INFINITY, f64::max)
            }).collect()
        };
        let stack = |a: PlotSpec, b: PlotSpec| PageSpec {
            arrange: Arrange::Below, cells: vec![a.into(), b.into()], theme: ThemeSpec::default(),
        };

        let folded = FacetSpec { col: Some("g".into()), wrap: Some(2), ..Default::default() };
        let edges = right_edges(stack(faceted("dist", folded.clone()), faceted("speed", folded)));
        assert!(edges.iter().all(|&e| e > 600.0), "each folded facet keeps its width: {edges:?}");

        let mut free_x = faceted("dist", FacetSpec { col: Some("g".into()), ..Default::default() });
        free_x.x = Some(crate::ir::ChannelDef { free: true, ..crate::ir::ChannelDef::field("speed") });
        // Tick labels of another width, so the two only line up if the page does it.
        let mut other = free_x.clone();
        other.y = Some(crate::ir::ChannelDef::field("big"));
        let (svg, _) = render(&stack(free_x, other), &data, 800.0, 600.0);
        let lefts: Vec<&str> = svg.split("<svg ").skip(2)
            .filter_map(|cell| cell.split("<clipPath").nth(1)?.split(" x=\"").nth(1)?.split('"').next())
            .collect();
        assert_eq!(lefts.len(), 2);
        assert_eq!(lefts[0], lefts[1], "two facets freed on x still line up on it");
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
