//! The `nest` coordinate space — the panel packed with nested regions.
//!
//! Wilkinson's chapter 13 is *Space*, and §13.3.4 is "Mapping Nested Space to
//! Euclidean" — the same section family as §13.3.1.3, the sphere mapping that
//! `globe` and `map` will be. A treemap is what that mapping looks like when the
//! nested thing is a table's categories: every row's measure becomes an **area**,
//! and the areas partition the panel.
//!
//! **This is the one space that is not a map of the plane, and the module is
//! shaped by that.** `polar` and `project` both answer "where does this
//! coordinate land", so a mark asks them per vertex and nothing else changes.
//! There is no such question here — a packing has no coordinates, only regions —
//! so this module answers a different one, once per layer: *given these weights,
//! which rectangle does each row get?* Wilkinson is explicit about the cost
//! (§13.3.4.1): the two directions "have no intrinsic meaning related to the
//! data because they can be reordered", and adjacency is not a distance. So there
//! is no `map_x` here and there never will be one.
//!
//! **The packing is squarified** (Bruls, Huizing and van Wijk 2000), which is not
//! a choice offered to the caller and deliberately so (spec §18's `tri` ruling —
//! a second value is not added to finish a list). The naive alternative,
//! slice-and-dice, cuts one direction at a time and turns any long tail into
//! slivers a reader cannot compare; squarifying keeps each region as close to
//! square as the run allows, which is the *measured* defect that earns the
//! algorithm its place, exactly as `hex` earned its own.
//!
//! **The order is the axis's, not the algorithm's.** Published squarified
//! treemaps sort descending by value first, and this one does not: the order is
//! whatever the categorical axis already has (spec §10's ordering rule), so
//! `order(revenue, desc = TRUE)` produces the classic look and says so out loud.
//! Sorting silently would make `order()` a word with no effect in this space,
//! which is the silent drop §12 forbids — and it would be the algorithm deciding
//! what the grammar already has an atom for.

use crate::render::Layout;

/// One packed region, in screen pixels. The panel is one of these and so is
/// every cell inside it, which is what lets the two levels share one routine.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Cell {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) w: f64,
    pub(crate) h: f64,
}

/// The packing frame for one panel. Built once per panel like [`polar::Polar`],
/// for the same reason: the cells a mark draws and anything else reading the
/// panel must not be able to disagree about where a region is.
///
/// [`polar::Polar`]: crate::render::polar::Polar
pub(crate) struct Nest {
    panel: Cell,
}

impl Nest {
    pub(crate) fn new(l: &Layout) -> Self {
        Nest { panel: Cell { x: l.x0, y: l.y0, w: l.w(), h: l.h() } }
    }

    /// The whole panel as one region — the outer level's rectangle.
    pub(crate) fn panel(&self) -> Cell {
        self.panel
    }

    /// The two-level packing for one layer: a cell per row, and the outer region
    /// each group of rows was packed inside.
    ///
    /// **Two levels fall out of one routine run twice**, which is why the
    /// two-level treemap needed no vocabulary of its own (spec §15): the *domain
    /// axis* partitions the panel into one region per category, and each region is
    /// then partitioned among the rows standing in it. A plot with no domain axis
    /// has one slot, so the outer packing returns the whole panel and the inner one
    /// is the treemap — the same code, degenerate, exactly as an ungrouped
    /// transform is.
    ///
    /// **Both marks that draw in this space read it from here, which is what the
    /// struct is for.** A `bar` fills a region and a `text` names it, and two marks
    /// computing the same packing separately is two marks that can disagree about
    /// where a region is — the failure [`polar::Polar`] is built once per panel to
    /// prevent, one space over. A label that misses its own rectangle by a pixel is
    /// worse than no label, because nothing on the page says which cell it belongs
    /// to.
    ///
    /// `slots` are the domain positions, all equal when nothing is bound; `weights`
    /// is the measure. A negative measure is refused by legality, and clamped here
    /// so the downgraded run (`GOG_STRICT=0`) draws a region of no size rather than
    /// one that eats its neighbors' share.
    ///
    /// [`polar::Polar`]: crate::render::polar::Polar
    pub(crate) fn regions(&self, slots: &[f64], weights: &[f64]) -> (Vec<Cell>, Vec<Cell>) {
        let n = slots.len().min(weights.len());
        // A category sits at its own index, so ascending slot order *is* the axis
        // order — which is what keeps `order()` meaningful in this space rather
        // than the packing choosing for itself.
        let slot_of = |i: usize| (slots[i] * 1e6).round() as i64;
        let weight = |i: usize| weights[i].max(0.0);
        let mut keys: Vec<i64> = (0..n).map(slot_of).collect();
        keys.sort_unstable();
        keys.dedup();

        let outer: Vec<f64> = keys.iter()
            .map(|k| (0..n).filter(|&i| slot_of(i) == *k).map(weight).sum())
            .collect();
        let regions = Nest::pack(self.panel(), &outer);

        let mut cells = vec![Cell { x: 0.0, y: 0.0, w: 0.0, h: 0.0 }; n];
        for (ki, k) in keys.iter().enumerate() {
            let members: Vec<usize> = (0..n).filter(|&i| slot_of(i) == *k).collect();
            let inner: Vec<f64> = members.iter().map(|&i| weight(i)).collect();
            for (mi, c) in Nest::pack(regions[ki], &inner).into_iter().enumerate() {
                cells[members[mi]] = c;
            }
        }
        (cells, regions)
    }

    /// Pack `weights` into `rect`, in the order given, and return one cell per
    /// weight. A non-positive or non-finite weight gets a zero-area cell rather
    /// than being dropped, so the caller's row indices still line up — legality
    /// has already refused a negative measure outright, and a zero is a real
    /// value that happens to have no region.
    pub(crate) fn pack(rect: Cell, weights: &[f64]) -> Vec<Cell> {
        let mut out = vec![Cell { x: rect.x, y: rect.y, w: 0.0, h: 0.0 }; weights.len()];
        // Only the positive weights take part; the rest keep their empty cell.
        let live: Vec<usize> = (0..weights.len())
            .filter(|&i| weights[i].is_finite() && weights[i] > 0.0)
            .collect();
        let total: f64 = live.iter().map(|&i| weights[i]).sum();
        if live.is_empty() || total <= 0.0 || rect.w <= 0.0 || rect.h <= 0.0 {
            return out;
        }

        // Weights become areas in pixels², so the row arithmetic below is in the
        // units it actually lays out. The whole panel is the total by
        // construction, which is the property a treemap is read for.
        let scale = rect.w * rect.h / total;
        let areas: Vec<f64> = live.iter().map(|&i| weights[i] * scale).collect();

        let mut rest = rect;
        let mut i = 0usize;
        while i < areas.len() {
            let side = rest.w.min(rest.h);
            if side <= 0.0 {
                break;
            }
            // Grow the row while doing so improves the worst aspect ratio in it.
            // This is the whole of Bruls et al.: a row is finished the moment
            // adding one more would make its ugliest rectangle uglier.
            let mut end = i + 1;
            while end < areas.len() {
                if worst(&areas[i..end + 1], side) > worst(&areas[i..end], side) {
                    break;
                }
                end += 1;
            }
            let row = &areas[i..end];
            let row_sum: f64 = row.iter().sum();
            // The strip's thickness: its area spread along the shorter side.
            let thick = (row_sum / side).min(rest.w.max(rest.h));

            if rest.w >= rest.h {
                // The shorter side is the height, so the row is a column standing
                // on the left edge and the rest of the panel is what is right of it.
                let mut y = rest.y;
                for (k, a) in row.iter().enumerate() {
                    let h = a / thick;
                    out[live[i + k]] = Cell { x: rest.x, y, w: thick, h };
                    y += h;
                }
                rest = Cell { x: rest.x + thick, y: rest.y, w: (rest.w - thick).max(0.0), h: rest.h };
            } else {
                // The shorter side is the width: a band across the top.
                let mut x = rest.x;
                for (k, a) in row.iter().enumerate() {
                    let w = a / thick;
                    out[live[i + k]] = Cell { x, y: rest.y, w, h: thick };
                    x += w;
                }
                rest = Cell { x: rest.x, y: rest.y + thick, w: rest.w, h: (rest.h - thick).max(0.0) };
            }
            i = end;
        }
        out
    }
}

/// The worst aspect ratio in a row laid along `side` — the number the greedy
/// choice above is minimizing. A row of total area *s* laid along *w* is a strip
/// *s/w* thick, so a member of area *a* is `s/w` by `aw/s` and its ratio is
/// `max(s²/(w²a), w²a/s²)`; the row's worst comes from its smallest and largest
/// members, so only those two are needed.
fn worst(row: &[f64], side: f64) -> f64 {
    let s: f64 = row.iter().sum();
    if s <= 0.0 {
        return f64::INFINITY;
    }
    let hi = row.iter().copied().fold(f64::MIN, f64::max);
    let lo = row.iter().copied().fold(f64::MAX, f64::min);
    let w2 = side * side;
    let s2 = s * s;
    (w2 * hi / s2).max(s2 / (w2 * lo))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect() -> Cell {
        Cell { x: 0.0, y: 0.0, w: 600.0, h: 400.0 }
    }

    /// The property a treemap is read for: the regions **are** the panel. If the
    /// areas do not sum to it, every share the reader takes off the picture is
    /// wrong, and no amount of correct ordering rescues that.
    #[test]
    fn the_cells_fill_the_panel_exactly() {
        let w = [5.0, 3.0, 2.0, 8.0, 1.0, 13.0, 0.5];
        let cells = Nest::pack(rect(), &w);
        let area: f64 = cells.iter().map(|c| c.w * c.h).sum();
        assert!((area - 600.0 * 400.0).abs() < 1e-6, "packed area {area} is not the panel");
    }

    /// And each region is the share its own weight asked for — the sum being
    /// right would not catch two cells that swapped sizes.
    #[test]
    fn each_cell_is_its_own_share() {
        let w = [5.0, 3.0, 2.0, 8.0, 1.0, 13.0];
        let total: f64 = w.iter().sum();
        let cells = Nest::pack(rect(), &w);
        for (i, c) in cells.iter().enumerate() {
            let want = w[i] / total * 600.0 * 400.0;
            assert!((c.w * c.h - want).abs() < 1e-6, "cell {i}: {} wanted {want}", c.w * c.h);
        }
    }

    #[test]
    fn the_cells_stay_inside_the_panel_and_do_not_overlap() {
        let w = [4.0, 4.0, 4.0, 1.0, 9.0, 2.0, 2.0];
        let cells = Nest::pack(rect(), &w);
        for c in &cells {
            assert!(c.x >= -1e-9 && c.y >= -1e-9, "cell starts outside: {c:?}");
            assert!(c.x + c.w <= 600.0 + 1e-6 && c.y + c.h <= 400.0 + 1e-6, "cell runs out: {c:?}");
        }
        for (i, a) in cells.iter().enumerate() {
            for b in cells.iter().skip(i + 1) {
                let dx = (a.x + a.w).min(b.x + b.w) - a.x.max(b.x);
                let dy = (a.y + a.h).min(b.y + b.h) - a.y.max(b.y);
                assert!(dx <= 1e-6 || dy <= 1e-6, "{a:?} overlaps {b:?}");
            }
        }
    }

    /// Squarifying is the whole reason the algorithm was chosen over cutting one
    /// direction at a time, so the test is against slice-and-dice rather than
    /// against a constant: the same run, cut naively, has a far worse worst cell.
    #[test]
    fn squarifying_beats_slicing_on_the_worst_cell() {
        let w: Vec<f64> = (1..=24).map(|k| k as f64).collect();
        let squarified = Nest::pack(rect(), &w);
        let worst_sq = squarified.iter()
            .map(|c| (c.w / c.h).max(c.h / c.w))
            .fold(0.0f64, f64::max);
        // Slice-and-dice: every cell a full-height column of its own share.
        let total: f64 = w.iter().sum();
        let worst_slice = w.iter()
            .map(|v| {
                let cw = 600.0 * v / total;
                (cw / 400.0f64).max(400.0 / cw)
            })
            .fold(0.0f64, f64::max);
        assert!(worst_sq < 3.0, "squarified worst aspect {worst_sq} is not square enough");
        assert!(worst_sq < worst_slice / 4.0,
                "squarified {worst_sq} should beat sliced {worst_slice} by a distance");
    }

    /// The order is the caller's. Reversing the weights must reverse which cell
    /// is which, or `order()` would be a word with no effect in this space.
    #[test]
    fn the_given_order_is_kept() {
        let w = [1.0, 2.0, 3.0, 4.0];
        let fwd = Nest::pack(rect(), &w);
        let rev: Vec<f64> = w.iter().rev().copied().collect();
        let back = Nest::pack(rect(), &rev);
        assert!((fwd[0].w * fwd[0].h - back[3].w * back[3].h).abs() < 1e-6,
                "reversing the weights did not reverse the cells");
        // Every packing starts its first cell at the rectangle's own corner, so
        // the corner is not what tells the two apart — the *share* sitting in it is.
        assert!((fwd[0].w * fwd[0].h - back[0].w * back[0].h).abs() > 1.0,
                "the same region got the same size either way round");
    }

    #[test]
    fn a_zero_weight_gets_no_region_and_the_rest_still_fill() {
        let w = [3.0, 0.0, 5.0];
        let cells = Nest::pack(rect(), &w);
        assert_eq!(cells[1].w * cells[1].h, 0.0, "a zero weight took up room");
        let area: f64 = cells.iter().map(|c| c.w * c.h).sum();
        assert!((area - 600.0 * 400.0).abs() < 1e-6, "the rest did not fill the panel");
    }

    #[test]
    fn nothing_to_pack_draws_nothing_rather_than_panicking() {
        assert!(Nest::pack(rect(), &[]).is_empty());
        let cells = Nest::pack(rect(), &[0.0, 0.0]);
        assert!(cells.iter().all(|c| c.w * c.h == 0.0));
        let flat = Nest::pack(Cell { x: 0.0, y: 0.0, w: 0.0, h: 400.0 }, &[1.0, 2.0]);
        assert!(flat.iter().all(|c| c.w * c.h == 0.0));
    }
}
