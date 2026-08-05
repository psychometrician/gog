//! Where every rectangle on the canvas comes from.
//!
//! The founding spec's pipeline puts a layout stage between the grammar and
//! the renderer, and for five milestones it did not exist — `svg.rs` computed
//! its margins inline, which was fine while there was exactly one panel.
//! M6 faceting is the feature that forced the stage into being: a faceted
//! plot is Wilkinson's frame of frames, and *some* single owner has to decide
//! how the outer frame divides into panels, where the strips sit, and which
//! panels touch the margins and so get tick labels. This module is that owner.
//! It computes rectangles and answers questions about them; it draws nothing.
//!
//! An unfaceted plot is the degenerate case on purpose: one panel, no strips,
//! no gaps, and the panel rectangle equals the outer rectangle to the pixel.
//! That identity is what keeps every existing plot byte-for-byte stable and
//! is pinned by the tests below.

use super::text::{estimate_cap_height, estimate_text_width};
use super::ticks::TickSpec;
use super::Layout;

/// Space between adjacent panels, in px. Enough to separate, not to strand.
pub(crate) const PANEL_GAP: f64 = 8.0;
/// Height of the strip naming each panel column, above the top row.
pub(crate) const STRIP_H: f64 = 20.0;
/// Width of the strip naming each panel row, right of the last column.
pub(crate) const STRIP_W: f64 = 20.0;

/// What a *page* has already decided about this plot's panels.
///
/// A composed plot no longer owns its own margins: two plots sharing an axis
/// share the extent their panels run over (`render::page`), so the panel area is
/// handed down rather than computed from the tick labels. Everything else about
/// the layout is unchanged, which is why this is four fields on the existing
/// computation and not a second layout engine.
///
/// [`Fit::free`] is the uncomposed plot — nothing decided elsewhere, every panel
/// where this module would have put it. Every existing plot renders through it,
/// and its layout is identical to the pixel.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Fit {
    /// The panel area's left and right edges, in this plot's own coordinates.
    pub(crate) panel_x: Option<(f64, f64)>,
    /// The panel area's top and bottom edges.
    pub(crate) panel_y: Option<(f64, f64)>,
    /// Does this plot draw the x axis — its ticks, its labels, its name?
    ///
    /// False when a plot beside or below it shares the axis and is nearer the
    /// edge it lives on. A shared axis is *one* axis, so it is drawn once, which
    /// is the rule a facet already follows for its own panels ([`labels_x`]).
    pub(crate) draw_x_axis: bool,
    pub(crate) draw_y_axis: bool,
}

impl Fit {
    /// A plot nobody has composed: it decides everything itself.
    pub(crate) const fn free() -> Fit {
        Fit { panel_x: None, panel_y: None, draw_x_axis: true, draw_y_axis: true }
    }
}

impl Default for Fit {
    fn default() -> Self {
        Fit::free()
    }
}

/// One cell of the facet grid — a frame with its place in the crossing.
pub(crate) struct Panel {
    pub(crate) rect: Layout,
    pub(crate) row: usize,
    pub(crate) col: usize,
    /// Which subset this panel shows, as an index into the plot's panel list.
    ///
    /// For a crossing that is `row * ncols + col`, which is what the renderer
    /// computed inline before wrapping existed. A folded ribbon numbers its
    /// panels *along the ribbon*, so the two disagree the moment a row is
    /// ragged — and the renderer must not have to know which it is looking at.
    pub(crate) slot: usize,
    /// The band naming this one panel — `Some` only in a folded ribbon.
    ///
    /// A crossing can name each column once above the grid, because every panel
    /// in a column shares that level. A ribbon has a different level in every
    /// cell, so the name belongs to the panel rather than to the column, and it
    /// costs a strip's height inside every cell rather than once at the top.
    pub(crate) strip: Option<Layout>,
}

/// The plot's frames: the outer rectangle the margins leave free, divided
/// into one panel per facet-category combination. Row-major, like reading.
pub(crate) struct PanelGrid {
    /// Everything inside the margins: panels, gaps, and strips.
    pub(crate) outer: Layout,
    pub(crate) panels: Vec<Panel>,
    pub(crate) nrows: usize,
    pub(crate) ncols: usize,
    /// Category per panel column — empty when the plot is not column-faceted.
    pub(crate) col_values: Vec<String>,
    /// Category per panel row — empty when the plot is not row-faceted.
    pub(crate) row_values: Vec<String>,
    /// The levels of a folded ribbon, in ribbon order — empty unless `wrap` is
    /// set. Indexed by [`Panel::slot`], and the authority for whether this grid
    /// is a ribbon at all.
    pub(crate) wrap_values: Vec<String>,
    /// The band naming the frame currently showing — `None` unless `play` is
    /// bound. A facet strip names one panel; this one names the whole plot,
    /// because a frame is the whole plot, so it spans the panel area and sits
    /// above every strip rather than beside one.
    pub(crate) play_strip: Option<Layout>,
    /// Which axes were freed, `(x, y)` — see [`PanelGrid::compute`]. Read by
    /// [`PanelGrid::labels_x`] and [`PanelGrid::labels_y`], which is the whole of
    /// what the layout does with it: a freed axis is ticked in every panel.
    pub(crate) free: (bool, bool),
}

impl PanelGrid {
    /// Compute the margins and divide what remains into panels.
    ///
    /// The margin arithmetic is exactly what the single-panel renderer always
    /// did: space for ticks, labels and the title, plus the legend panel on
    /// the right. Faceting adds only the strips — a row of names above the
    /// panels, a column of names beside them — and the division into cells.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn compute(
        width: f64,
        height: f64,
        fonts: (f64, f64, f64), // (sm, md, lg)
        x_ticks: &TickSpec,
        y_ticks: &TickSpec,
        x_label: &str,
        y_label: &str,
        has_title: bool,
        legend_extra_width: f64,
        col_values: Vec<String>,
        row_values: Vec<String>,
        // `theme(ratio = )` — the panel's width ÷ height, or `None` to fill the cell.
        ratio: Option<f64>,
        // `theme(tick_angle = )` — degrees the x tick labels are turned through.
        tick_angle: Option<f64>,
        // Is `play` bound? A frame sequence earns a strip the way a facet does,
        // and it costs the same band of height whatever the frame count is.
        has_play: bool,
        // `facet(g, wrap = n)` — fold the line of panels after `n` of them. The
        // *direction* is not passed because it is not a second decision: the
        // levels are in `col_values` when `|` made the facet and in `row_values`
        // when `/` did, so which list is non-empty is which way the line runs.
        wrap: Option<usize>,
        // Which axes were freed — `y(life, free = TRUE)` (spec §11). A freed axis
        // is a different scale in every panel, so every panel draws its own ticks
        // and every cell owes them room.
        free: (bool, bool),
        // How much room that is: (width on each cell's left for y labels, height
        // at each cell's foot for x labels). Measured in `svg.rs`, which owns text
        // metrics and has every panel's own labels in hand; `(0.0, 0.0)` whenever
        // nothing is freed, which is every plot drawn before free scales existed.
        cell_axis: (f64, f64),
        // What a page has already decided (see [`Fit`]). `Fit::free()` for a plot
        // drawn on its own, which is every plot that is not composed.
        fit: Fit,
    ) -> PanelGrid {
        let (font_sm, font_md, font_lg) = fonts;
        let tick_h = estimate_cap_height(font_sm);
        let label_h = estimate_cap_height(font_md);
        let title_h = estimate_cap_height(font_lg);

        let y_tick_w = y_ticks
            .labels
            .iter()
            .map(|l| estimate_text_width(l, font_sm))
            .fold(0.0_f64, f64::max);

        let pad_top = 16.0
            + if has_title { title_h + 12.0 } else { 0.0 }
            + if !y_label.is_empty() { label_h + 6.0 } else { 0.0 };

        // A turned x label is taller than an upright one by as much of its own
        // *width* as the angle borrows: height = w·sin θ + h·cos θ, the bounding
        // box of a rotated rectangle. Measured off the longest label, because the
        // margin has to hold the worst one — the whole reason to turn them is
        // that they are long.
        let x_tick_w = x_ticks
            .labels
            .iter()
            .map(|l| estimate_text_width(l, font_sm))
            .fold(0.0_f64, f64::max);
        let turned_tick_h = match tick_angle {
            Some(deg) if deg != 0.0 => {
                let t = deg.abs().to_radians();
                x_tick_w * t.sin() + tick_h * t.cos()
            }
            _ => tick_h,
        };

        // An axis with no tick labels reserves no room for them. Three plots
        // already ask for that — the cube, the circle and the packing all draw
        // their guides *inside* the panel and are handed an empty tick list —
        // and a page adds the fourth: a plot whose axis another plot on the page
        // is drawing (`Fit::draw_x_axis`). Without it, a marginal histogram
        // floats a tick label's height above the scatter it describes, and the
        // one thing a marginal plot has to do is touch.
        let tick_band = if x_ticks.labels.is_empty() { 0.0 } else { turned_tick_h + 8.0 };

        let pad_bottom = tick_band
            + if !x_label.is_empty() { label_h + 8.0 } else { 0.0 }
            + 10.0;

        let pad_left = y_tick_w + 24.0;

        // A turned label hangs *left* of its tick rather than straddling it (it
        // is anchored at its end), so the right margin no longer has to hold half
        // of the last one. Left unchanged rather than tightened: the y tick
        // labels already set `pad_left`, and a label leaning past the panel's
        // left edge is the one case this could make worse, not better.
        //
        // **An axis with no tick labels has no last one to hang past the panel**,
        // which is `tick_band`'s rule above on the other margin it governs, and
        // it reaches the same four plots: the cube, the circle and the packing
        // all draw their guides inside the panel, and so does a plot whose axis a
        // page-mate is drawing. The fallback here was 20px — half of a label that
        // is never written — and it dates from before any of those four existed.
        // On a full canvas it is a rounding error. In a *composed* cell it is not:
        // beside a key, a cube's panel had 234px of a 390px cell, and 20 of the
        // 131 it gave up were being held for nothing.
        let last_x_tick_half = if x_ticks.labels.is_empty() {
            0.0
        } else {
            match tick_angle {
                Some(deg) if deg != 0.0 => 8.0,
                _ => x_ticks
                    .labels
                    .last()
                    .map(|l| estimate_text_width(l, font_sm) / 2.0)
                    .unwrap_or(0.0),
            }
        };
        let pad_right = last_x_tick_half + 12.0 + legend_extra_width;

        let mut outer = Layout {
            x0: pad_left,
            y0: pad_top,
            x1: width - pad_right,
            y1: height - pad_bottom,
        };

        // Is this a ribbon to fold? Only a *one*-dimensional facet has a line of
        // panels; `check_facet` refuses `wrap` on a crossing and on no facet at
        // all, but `GOG_STRICT=0` downgrades an Illegal to a warning and draws
        // anyway, so the layout declines the fold rather than trusting the gate.
        let wrap = wrap.filter(|n| *n > 0 && col_values.is_empty() != row_values.is_empty());
        // Along the columns when `|` made the facet, down the rows when `/` did.
        let along_cols = !col_values.is_empty();
        let wrap_values: Vec<String> = match wrap {
            Some(_) if along_cols => col_values.clone(),
            Some(_) => row_values.clone(),
            None => Vec::new(),
        };

        // Folding replaces the line's own count with the rectangle's two. `wrap`
        // is the extent *in the direction the levels run*, and the other extent
        // is however many turns that takes — which is why one number is enough.
        let (nrows, ncols) = match wrap {
            Some(n) => {
                let levels = wrap_values.len().max(1);
                let along = n.min(levels).max(1);
                let across = levels.div_ceil(along);
                if along_cols { (across, along) } else { (along, across) }
            }
            None => (row_values.len().max(1), col_values.len().max(1)),
        };
        let faceted = !col_values.is_empty() || !row_values.is_empty();

        // Strips claim their band only on the axis that is actually faceted,
        // and the gap only exists when there is more than one frame to keep
        // apart — so the unfaceted panel *is* the outer rectangle.
        //
        // A folded ribbon claims neither band: its names do not belong to the
        // columns or to the rows, so they are not written once along an edge.
        // They cost a strip's height inside every cell instead (`cell_strip_h`).
        let ribbon = wrap.is_some();
        let strip_h = if col_values.is_empty() || ribbon { 0.0 } else { STRIP_H };
        let strip_w = if row_values.is_empty() || ribbon { 0.0 } else { STRIP_W };
        let cell_strip_h = if ribbon { STRIP_H } else { 0.0 };
        let gap = if faceted { PANEL_GAP } else { 0.0 };

        // The play strip claims its band on the same terms, and above the facet
        // strips rather than among them: a facet strip names one panel column,
        // this names every panel at once, because a frame is the whole plot. An
        // unplayed plot claims nothing, which is what keeps its layout identical.
        let play_h = if has_play { STRIP_H } else { 0.0 };

        // A page has the last word on the panel area, because two plots sharing
        // an axis must run over the same pixels of it or the shared axis is a
        // lie. The margins above still do all the *measuring* — the fit is an
        // intersection of what they asked for (`render::page`), so it only ever
        // takes room away, never puts a tick label somewhere it does not fit.
        //
        // `outer` moves with it: it is the rectangle the axis names and the
        // legend are placed against, and leaving it where the margins put it
        // would write them against an edge the panels no longer have.
        if let Some((x0, x1)) = fit.panel_x {
            outer.x0 = x0;
            outer.x1 = x1 + strip_w;
        }
        if let Some((y0, y1)) = fit.panel_y {
            outer.y0 = y0 - play_h - strip_h;
            outer.y1 = y1;
        }

        let area_x0 = outer.x0;
        let area_x1 = outer.x1 - strip_w;
        let area_y0 = outer.y0 + play_h + strip_h;
        let area_y1 = outer.y1;

        let play_strip = has_play.then(|| Layout {
            x0: area_x0,
            y0: outer.y0,
            x1: area_x1,
            y1: outer.y0 + STRIP_H,
        });

        let cell_w = ((area_x1 - area_x0) - gap * (ncols as f64 - 1.0)) / ncols as f64;
        let cell_h = ((area_y1 - area_y0) - gap * (nrows as f64 - 1.0)) / nrows as f64;
        // In a ribbon the cell holds a name *and* a panel, so the panel gets what
        // is left under the name. Everything below — the ratio, the inset, the
        // centering — then works on the panel's own room, exactly as it does when
        // there is no name to make room for.
        //
        // A freed axis takes its room the same way and for the same reason: what
        // a shared thing writes once along an edge, an unshared thing writes in
        // every cell. So `cell_axis` is subtracted here beside the strip, and the
        // panel is what is left inside its cell.
        let cell_h = cell_h - cell_strip_h - cell_axis.1;
        let cell_w = cell_w - cell_axis.0;

        // `theme(ratio = )` fixes the panel's width ÷ height. The cell keeps the
        // size the grid gave it and the *panel* shrinks inside it on whichever
        // axis has slack, then centers — so a ratio never changes what the plot
        // costs to place on a page, and a faceted row of circles gets round
        // circles rather than a re-negotiated layout. Applied per panel because
        // that is what the four callers waiting for it wanted (a circle, a
        // hexagon, a pile of dots, a cube), none of which is a claim about the
        // image (spec §7).
        let (panel_w, panel_h) = match ratio {
            Some(r) if r > 0.0 && cell_w > 0.0 && cell_h > 0.0 => {
                if cell_w / cell_h > r { (cell_h * r, cell_h) } else { (cell_w, cell_w / r) }
            }
            _ => (cell_w, cell_h),
        };
        let inset_x = (cell_w - panel_w) / 2.0;
        let inset_y = (cell_h - panel_h) / 2.0;

        let mut panels = Vec::with_capacity(nrows * ncols);
        for row in 0..nrows {
            for col in 0..ncols {
                // Which subset this cell shows. A crossing reads its two levels
                // off the cell's own coordinates, so every cell has one. A ribbon
                // is numbered along its own direction — across then down for `|`,
                // down then across for `/` — and the cells past the last level are
                // the slack the fold left over, not combinations with no rows, so
                // they get no panel at all.
                let slot = match wrap {
                    Some(_) if along_cols => row * ncols + col,
                    Some(_) => col * nrows + row,
                    None => row * ncols + col,
                };
                if wrap.is_some() && slot >= wrap_values.len() { continue }

                let cell_x0 = area_x0 + col as f64 * (cell_w + cell_axis.0 + gap);
                let cell_y0 = area_y0 + row as f64 * (cell_h + cell_strip_h + cell_axis.1 + gap);
                let x0 = cell_x0 + cell_axis.0 + inset_x;
                let y0 = cell_y0 + cell_strip_h + inset_y;
                panels.push(Panel {
                    rect: Layout { x0, y0, x1: x0 + panel_w, y1: y0 + panel_h },
                    row,
                    col,
                    slot,
                    // Spanning the *cell*, not the panel: a `theme(ratio = )`
                    // narrows the panel inside its cell, and a name that narrowed
                    // with it would drift away from the column of names above and
                    // below it.
                    strip: (cell_strip_h > 0.0).then(|| Layout {
                        x0: cell_x0 + cell_axis.0,
                        y0: cell_y0,
                        x1: cell_x0 + cell_axis.0 + cell_w,
                        y1: cell_y0 + cell_strip_h,
                    }),
                });
            }
        }

        PanelGrid { outer, panels, nrows, ncols, col_values, row_values, wrap_values,
                    play_strip, free }
    }

    /// Fixed scales mean one set of tick labels is enough — they are drawn
    /// only where a panel touches the margin they live in: x labels under the
    /// bottom row, y labels beside the left column.
    ///
    /// "The bottom row" stops being true the moment a ribbon is folded: with ten
    /// panels in a 4 × 3 rectangle the last row holds two, and the two above the
    /// gap have nothing below them either. So the rule is stated as *no panel
    /// sits directly below this one*, which is the same sentence one step more
    /// general — a full rectangle reduces to the bottom row exactly.
    /// A **freed** axis short-circuits both of these, and the reason is the one
    /// sentence above read backwards: one set of tick labels is enough only
    /// because one scale is. Give every panel its own scale and the numbers under
    /// the bottom row describe nothing but the bottom row.
    pub(crate) fn labels_x(&self, p: &Panel) -> bool {
        if self.free.0 {
            return true;
        }
        if self.wrap_values.is_empty() {
            return p.row == self.nrows - 1;
        }
        !self.panels.iter().any(|q| q.col == p.col && q.row == p.row + 1)
    }

    pub(crate) fn labels_y(&self, p: &Panel) -> bool {
        self.free.1 || p.col == 0
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::ticks::nice_ticks;

    fn grid(cols: Vec<&str>, rows: Vec<&str>) -> PanelGrid {
        grid_with(cols, rows, None, None)
    }

    fn grid_with(cols: Vec<&str>, rows: Vec<&str>, ratio: Option<f64>,
                 tick_angle: Option<f64>) -> PanelGrid {
        grid_played(cols, rows, ratio, tick_angle, false)
    }

    fn grid_played(cols: Vec<&str>, rows: Vec<&str>, ratio: Option<f64>,
                   tick_angle: Option<f64>, has_play: bool) -> PanelGrid {
        grid_fitted(cols, rows, ratio, tick_angle, has_play, Fit::free())
    }

    fn grid_fitted(cols: Vec<&str>, rows: Vec<&str>, ratio: Option<f64>,
                   tick_angle: Option<f64>, has_play: bool, fit: Fit) -> PanelGrid {
        grid_wrapped(cols, rows, ratio, tick_angle, has_play, fit, None)
    }

    fn grid_wrapped(cols: Vec<&str>, rows: Vec<&str>, ratio: Option<f64>,
                    tick_angle: Option<f64>, has_play: bool, fit: Fit,
                    wrap: Option<usize>) -> PanelGrid {
        grid_freed(cols, rows, ratio, tick_angle, has_play, fit, wrap,
                   (false, false), (0.0, 0.0))
    }

    #[allow(clippy::too_many_arguments)]
    fn grid_freed(cols: Vec<&str>, rows: Vec<&str>, ratio: Option<f64>,
                  tick_angle: Option<f64>, has_play: bool, fit: Fit,
                  wrap: Option<usize>, free: (bool, bool),
                  cell_axis: (f64, f64)) -> PanelGrid {
        let t = nice_ticks(0.0, 10.0, 5);
        PanelGrid::compute(
            800.0,
            600.0,
            (12.0, 14.0, 18.0),
            &t,
            &t,
            "X",
            "Y",
            false,
            0.0,
            cols.into_iter().map(String::from).collect(),
            rows.into_iter().map(String::from).collect(),
            ratio,
            tick_angle,
            has_play,
            wrap,
            free,
            cell_axis,
            fit,
        )
    }

    /// An axis with no tick labels reserves nothing on **either** margin.
    ///
    /// `tick_band` has zeroed the bottom for this case since the cube was built,
    /// and its comment names all four plots in it — the cube, the circle, the
    /// packing, and a plot whose axis a page-mate draws. The *right* margin kept
    /// a 20px fallback for half of a last label that none of the four writes, so
    /// "the cube takes the whole panel" was false by 20px the whole time.
    ///
    /// Pinned on both margins together, because one rule stated in two places is
    /// how the second one came to be missed.
    #[test]
    fn an_axis_with_no_tick_labels_reserves_neither_margin() {
        let empty = TickSpec { values: Vec::new(), labels: Vec::new(), step: 1.0 };
        let ticked = nice_ticks(0.0, 10.0, 5);
        let bare = |xt: &TickSpec, yt: &TickSpec, xl: &str, yl: &str| {
            PanelGrid::compute(800.0, 600.0, (12.0, 14.0, 18.0), xt, yt, xl, yl,
                               false, 0.0, vec![], vec![], None, None, false, None,
                               (false, false), (0.0, 0.0), Fit::free())
        };
        let cube = bare(&empty, &empty, "", "");
        let flat = bare(&ticked, &ticked, "X", "Y");

        // The bottom: already right, and asserted here so the pair cannot drift.
        assert!(cube.panels[0].rect.y1 > flat.panels[0].rect.y1,
                "no x tick labels means no band reserved under the panel");
        // The right: the defect. A cube's panel must reach further right than a
        // ticked plot's, not stop 20px short of the same place.
        assert!(cube.panels[0].rect.x1 > flat.panels[0].rect.x1,
                "no x tick labels means no half-label reserved beside the panel");
        // And exactly: nothing but the general 12px margin is held back.
        assert!((800.0 - cube.panels[0].rect.x1 - 12.0).abs() < 1e-9,
                "a cube gives up the margin and nothing else, got {}",
                800.0 - cube.panels[0].rect.x1);
    }

    /// The composed plot's whole promise, at the layout level: the page says
    /// where the panels run and the margins give up the difference. Without it
    /// a marginal histogram sits over the scatter it is supposed to describe by
    /// however much their tick labels happen to differ in width.
    #[test]
    fn a_fitted_panel_takes_the_extent_the_page_gives_it() {
        let free = grid(vec![], vec![]);
        let fit = Fit { panel_x: Some((120.0, 500.0)), ..Fit::free() };
        let fitted = grid_fitted(vec![], vec![], None, None, false, fit);
        let p = &fitted.panels[0].rect;
        assert!((p.x0 - 120.0).abs() < 1e-9 && (p.x1 - 500.0).abs() < 1e-9,
                "the panel runs exactly where the page put it");
        assert_eq!((p.y0, p.y1), (free.panels[0].rect.y0, free.panels[0].rect.y1),
                   "the axis nobody shared keeps the margins it measured for itself");
        assert!((fitted.outer.x0 - 120.0).abs() < 1e-9,
                "the axis name and the legend follow the panel, not the old margin");
    }

    /// Faceting inside a composed plot: the *area* the panels divide is what the
    /// page fixed, so the strips still get their band out of it rather than the
    /// page's arithmetic being redone per panel.
    #[test]
    fn a_fitted_facet_divides_the_extent_it_was_given() {
        let fit = Fit { panel_x: Some((100.0, 600.0)), ..Fit::free() };
        let g = grid_fitted(vec!["a", "b"], vec![], None, None, false, fit);
        assert!((g.panels[0].rect.x0 - 100.0).abs() < 1e-9);
        assert!((g.panels[1].rect.x1 - 600.0).abs() < 1e-9);
        assert!((g.panels[0].rect.y0 - (g.outer.y0 + STRIP_H)).abs() < 1e-9,
                "the strip band is still taken from inside the fitted area");
    }

    // `theme(ratio = )` is a statement about a *panel*, and the property worth
    // pinning is that it stays one when there are several: a faceted row of
    // circles wants every circle round, not one.
    #[test]
    fn ratio_fixes_every_panel_and_leaves_the_image_alone() {
        let plain = grid(vec![], vec![]);
        let square = grid_with(vec![], vec![], Some(1.0), None);
        assert!((square.panels[0].rect.w() - square.panels[0].rect.h()).abs() < 0.01,
                "ratio = 1 should give a square panel");
        assert!(square.panels[0].rect.w() < plain.panels[0].rect.w(),
                "the panel shrinks to meet the ratio, it does not grow");
        assert_eq!(plain.outer.w(), square.outer.w(),
                   "the image keeps the size it was given");

        let faceted = grid_with(vec!["a", "b", "c"], vec![], Some(1.0), None);
        for panel in &faceted.panels {
            assert!((panel.rect.w() - panel.rect.h()).abs() < 0.01,
                    "every panel is square, not just the first");
        }
    }

    // Turning the labels has to buy them the room they now need, or the plot
    // draws them over the axis title and says nothing.
    #[test]
    fn turned_tick_labels_earn_a_taller_bottom_margin() {
        let upright = grid(vec![], vec![]);
        let turned = grid_with(vec![], vec![], None, Some(45.0));
        assert!(turned.panels[0].rect.h() < upright.panels[0].rect.h(),
                "a turned label is taller, so the panel gives up the height");
    }

    #[test]
    fn an_unfaceted_plot_is_one_panel_equal_to_the_outer_frame() {
        let g = grid(vec![], vec![]);
        assert_eq!(g.panels.len(), 1);
        let (p, o) = (&g.panels[0].rect, &g.outer);
        assert_eq!((p.x0, p.y0, p.x1, p.y1), (o.x0, o.y0, o.x1, o.y1));
        assert!(g.play_strip.is_none(), "an unplayed plot claims no band");
    }

    /// A frame sequence earns a strip, and pays for it out of the panel — the
    /// same trade a facet's column names make. The point being pinned is the
    /// *unplayed* half: nothing above moves unless `play` is bound.
    #[test]
    fn play_claims_a_band_and_only_when_it_is_bound() {
        let plain = grid_played(vec![], vec![], None, None, false);
        let played = grid_played(vec![], vec![], None, None, true);

        let strip = played.play_strip.as_ref().expect("a played plot has a band");
        assert_eq!(strip.y0, played.outer.y0, "the band is the topmost thing inside the margins");
        assert!((strip.h() - STRIP_H).abs() < 1e-9);
        assert!((played.panels[0].rect.y0 - (plain.panels[0].rect.y0 + STRIP_H)).abs() < 1e-9,
                "the panel starts exactly one strip lower, and gives up exactly that height");
        assert_eq!(plain.panels[0].rect.y1, played.panels[0].rect.y1, "nothing else moves");
    }

    /// The band names the whole plot, so it sits *above* the names of the
    /// panels rather than among them — otherwise the facet's strip and the
    /// play strip would be written into the same pixels.
    #[test]
    fn a_played_facet_stacks_the_two_strips_rather_than_overlapping_them() {
        let g = grid_played(vec!["a", "b"], vec![], None, None, true);
        let strip = g.play_strip.as_ref().expect("still a band");
        assert_eq!(strip.y0, g.outer.y0);
        assert!(g.panels[0].rect.y0 >= strip.y1 + STRIP_H - 1e-9,
                "the facet's own strip fits between the play band and the panels");
    }

    #[test]
    fn column_facets_divide_the_width_equally_with_gaps_between() {
        let g = grid(vec!["a", "b", "c"], vec![]);
        assert_eq!((g.nrows, g.ncols, g.panels.len()), (1, 3, 3));
        let w0 = g.panels[0].rect.w();
        assert!(g.panels.iter().all(|p| (p.rect.w() - w0).abs() < 1e-9));
        assert!((g.panels[1].rect.x0 - g.panels[0].rect.x1 - PANEL_GAP).abs() < 1e-9);
        // The strip band sits above the panels, inside the outer frame.
        assert!((g.panels[0].rect.y0 - g.outer.y0 - STRIP_H).abs() < 1e-9);
        assert!((g.panels[2].rect.x1 - g.outer.x1).abs() < 1e-9);
    }

    #[test]
    fn a_crossed_grid_is_row_major_and_complete() {
        let g = grid(vec!["a", "b", "c"], vec!["u", "v"]);
        assert_eq!((g.nrows, g.ncols, g.panels.len()), (2, 3, 6));
        assert_eq!((g.panels[4].row, g.panels[4].col), (1, 1));
        // Row strips claim width on the right.
        assert!((g.panels[2].rect.x1 - (g.outer.x1 - STRIP_W)).abs() < 1e-9);
    }

    #[test]
    fn tick_labels_belong_to_the_margin_edges_only() {
        let g = grid(vec!["a", "b"], vec!["u", "v"]);
        let at = |r: usize, c: usize| &g.panels[r * g.ncols + c];
        assert!(g.labels_x(at(1, 0)) && !g.labels_x(at(0, 0)));
        assert!(g.labels_y(at(0, 0)) && !g.labels_y(at(0, 1)));
    }

    // -- wrap: the ribbon folded into a rectangle ---------------------------

    fn ten() -> Vec<&'static str> {
        vec!["a", "b", "c", "d", "e", "f", "g", "h", "i", "j"]
    }

    fn wrapped(cols: Vec<&str>, rows: Vec<&str>, n: usize) -> PanelGrid {
        grid_wrapped(cols, rows, None, None, false, Fit::free(), Some(n))
    }

    /// Ten panels across the page are ten slivers; folded at four they are a
    /// 4 × 3 rectangle. One number decides both extents, because the second is
    /// however many turns the first takes.
    #[test]
    fn a_wrapped_ribbon_folds_into_a_rectangle() {
        let g = wrapped(ten(), vec![], 4);
        assert_eq!((g.nrows, g.ncols), (3, 4));
        assert_eq!(g.panels.len(), 10, "ten levels are ten panels");
        // Numbered along the ribbon: across, then down.
        assert_eq!((g.panels[4].row, g.panels[4].col, g.panels[4].slot), (1, 0, 4));
    }

    /// The two cells after the last level are the slack the fold left over, not
    /// combinations with no rows — so unlike a crossing's empty panel they get
    /// no frame at all, and nothing indexes them.
    #[test]
    fn the_cells_past_the_last_level_are_not_panels() {
        let g = wrapped(ten(), vec![], 4);
        assert_eq!(g.nrows * g.ncols, 12);
        assert_eq!(g.panels.len(), 10);
        assert!(g.panels.iter().all(|p| p.slot < 10));
        // Nothing sits in the bottom row past the second column.
        assert!(!g.panels.iter().any(|p| p.row == 2 && p.col >= 2));
    }

    /// The direction is the operator's, never the count's: `/` runs the levels
    /// down and turns after four *rows*, which numbers the ribbon column-major.
    #[test]
    fn wrapping_down_runs_the_levels_down() {
        let g = wrapped(vec![], ten(), 4);
        assert_eq!((g.nrows, g.ncols), (4, 3));
        let at = |r: usize, c: usize| g.panels.iter()
            .find(|p| p.row == r && p.col == c).map(|p| p.slot);
        // Column 0 holds levels 0..3 top to bottom; column 1 starts at 4.
        assert_eq!((at(0, 0), at(1, 0), at(3, 0)), (Some(0), Some(1), Some(3)));
        assert_eq!(at(0, 1), Some(4));
    }

    /// A ribbon has a different level in every cell, so alignment cannot name
    /// them: each panel carries its own band, and it spans the cell rather than
    /// the panel so the names stay in a column when a ratio narrows the panels.
    #[test]
    fn every_panel_of_a_ribbon_carries_its_own_name() {
        let g = wrapped(ten(), vec![], 4);
        assert_eq!(g.wrap_values.len(), 10);
        assert!(g.panels.iter().all(|p| p.strip.is_some()));
        for p in &g.panels {
            let s = p.strip.as_ref().unwrap();
            assert!((s.y1 - p.rect.y0).abs() < 1e-9, "the band sits directly on its panel");
            assert!(s.x0 <= p.rect.x0 + 1e-9 && s.x1 >= p.rect.x1 - 1e-9);
        }
        // A crossing keeps naming its columns once, above the grid.
        assert!(grid(vec!["a", "b"], vec![]).panels.iter().all(|p| p.strip.is_none()));
    }

    /// "The bottom row" is the wrong sentence the moment a row is ragged: with
    /// ten panels in a 4 × 3 rectangle, the two above the gap have nothing below
    /// them either and would otherwise lose their axis.
    #[test]
    fn a_ragged_row_still_draws_its_axis() {
        let g = wrapped(ten(), vec![], 4);
        let at = |r: usize, c: usize| g.panels.iter()
            .find(|p| p.row == r && p.col == c).expect("panel");
        // Bottom row, and the two hanging over the gap it left.
        assert!(g.labels_x(at(2, 0)) && g.labels_x(at(2, 1)));
        assert!(g.labels_x(at(1, 2)) && g.labels_x(at(1, 3)));
        // Everything with a panel under it stays silent.
        assert!(!g.labels_x(at(0, 0)) && !g.labels_x(at(1, 0)) && !g.labels_x(at(0, 3)));
        assert!(g.labels_y(at(1, 0)) && !g.labels_y(at(1, 1)));
    }

    /// The generalized rule has to *reduce* to the old one, or it is a second
    /// rule wearing the same name. A full rectangle is the case where it does.
    #[test]
    fn a_full_rectangle_is_the_bottom_row_exactly() {
        let g = wrapped(vec!["a", "b", "c", "d"], vec![], 2);
        assert_eq!((g.nrows, g.ncols, g.panels.len()), (2, 2, 4));
        for p in &g.panels {
            assert_eq!(g.labels_x(p), p.row == g.nrows - 1);
        }
    }

    /// One set of tick labels is enough only because one scale is. Free the
    /// axis and the numbers under the bottom row describe nothing but the bottom
    /// row, so every panel draws its own — and only for the axis that asked.
    #[test]
    fn a_freed_axis_is_ticked_in_every_panel() {
        let free_y = grid_freed(vec!["a", "b", "c"], vec!["u", "v"], None, None,
                                false, Fit::free(), None, (false, true), (30.0, 0.0));
        assert!(free_y.panels.iter().all(|p| free_y.labels_y(p)),
                "a freed y is ticked beside every panel, not just the left column");
        assert!(free_y.panels.iter().any(|p| !free_y.labels_x(p)),
                "x was not freed, so it is still ticked only along the bottom");

        let free_x = grid_freed(vec!["a", "b", "c"], vec!["u", "v"], None, None,
                                false, Fit::free(), None, (true, false), (0.0, 20.0));
        assert!(free_x.panels.iter().all(|p| free_x.labels_x(p)));
        assert!(free_x.panels.iter().any(|p| !free_x.labels_y(p)));
    }

    /// The room those labels need is taken out of each cell, the way a ribbon's
    /// strip is — what a shared thing writes once along an edge, an unshared one
    /// writes in every cell — so the panels shrink and the image does not.
    #[test]
    fn a_freed_axis_takes_its_room_from_every_cell() {
        let plain = grid(vec!["a", "b", "c"], vec![]);
        let freed = grid_freed(vec!["a", "b", "c"], vec![], None, None,
                               false, Fit::free(), None, (false, true), (30.0, 0.0));
        assert_eq!(freed.outer.x0, plain.outer.x0, "the outer frame is unmoved");
        assert!(freed.panels[0].rect.w() < plain.panels[0].rect.w(),
                "each panel gives up the width its own labels need");
        // And it is given up inside the cell: the first panel starts a gutter's
        // width right of where it used to.
        assert!((freed.panels[0].rect.x0 - plain.panels[0].rect.x0 - 30.0).abs() < 1e-9);
    }

    /// `check_facet` refuses `wrap` on a crossing, but `GOG_STRICT=0` draws an
    /// Illegal anyway — so the layout must decline the fold rather than trust
    /// the gate and index a rectangle that was never built.
    #[test]
    fn a_crossing_ignores_a_wrap_it_was_never_allowed_to_have() {
        let crossed = grid(vec!["a", "b", "c"], vec!["u", "v"]);
        let same = wrapped(vec!["a", "b", "c"], vec!["u", "v"], 2);
        assert_eq!((same.nrows, same.ncols), (crossed.nrows, crossed.ncols));
        assert!(same.wrap_values.is_empty());
        assert!(same.panels.iter().all(|p| p.strip.is_none()));
    }
}
