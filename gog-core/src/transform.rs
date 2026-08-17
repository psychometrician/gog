/// Data transforms — applied to a DataFrame before rendering.
///
/// Each transform takes the bound data and produces a new DataFrame whose
/// column names are the same as the spec's x/y fields.  The renderer sees
/// only the transformed data and needs no special-casing per transform.
///
/// # Transforms
/// | Name      | Input needed           | What it produces              |
/// |-----------|------------------------|-------------------------------|
/// | `bin`     | x column (numeric)     | bin centers → x, counts → y  |
/// | `count`   | x column (any)         | unique values → x, counts → y |
/// | `smooth`  | x + y columns (numeric)| LOESS-smoothed x → x, y → y  |
/// | `density` | x column (numeric)     | eval points → x, KDE → y     |
use std::collections::HashMap;

use crate::data::DataFrame;
use crate::ir::{BinSpec, BoundsSpec, BoxSpec, ConfidenceSpec, DensitySpec, DeviationSpec,
                QuantileSpec, RangeSpec, StackSpec, Transform};

/// Apply a sequence of transforms to `df`.
///
/// Every transform here reads one axis and writes the other: `count` groups by
/// the key and writes tallies to the output, `bin` groups by the key and writes
/// counts, `smooth` fits the output against the key. Which *axis* plays which
/// role is not fixed — a horizontal bar chart groups by y and measures along x —
/// so the caller names them rather than assuming `(x, y)`.
///
/// `cut` is the bin layout for `key_field`, resolved by the caller from every
/// panel's rows at once (spec §11). `None` means "derive it here", which is what
/// an unfaceted caller and every test want — see [`BinCut`].
#[allow(clippy::too_many_arguments)]
pub fn apply(
    df: &DataFrame,
    transforms: &[Transform],
    key_field: &str,
    out_field: &str,
    bin_spec: Option<&BinSpec>,
    cut: Option<&BinLayout>,
    density_spec: Option<&DensitySpec>,
    range_spec: Option<&RangeSpec>,
    conf_spec: Option<&ConfidenceSpec>,
    dev_spec: Option<&DeviationSpec>,
    q_spec: Option<&QuantileSpec>,
    box_spec: Option<&BoxSpec>,
    bounds_spec: Option<&BoundsSpec>,
    stack_spec: Option<&StackSpec>,
    group_field: Option<&str>,
) -> DataFrame {
    // A statistic runs *within* each color/group when one is bound: a histogram
    // split by species is three histograms, not one combined bar with the split
    // silently dropped (the bug this closes). Every output row carries its group
    // label, so the renderer can color it and the legend agrees. No group means
    // one group — the whole frame — which is the same code, degenerate.
    let result = match group_field {
        Some(g) if df.str_col(g).is_some_and(|c| !c.is_empty()) => {
            apply_grouped(df, transforms, key_field, out_field, bin_spec, cut, density_spec, range_spec, conf_spec, dev_spec, q_spec, box_spec, bounds_spec, g)
        }
        _ => apply_seq(df, transforms, key_field, out_field, bin_spec, density_spec, range_spec, conf_spec, dev_spec, q_spec, box_spec, bounds_spec, cut),
    };

    // **`proportion` normalizes here, always** — the one pass that makes it a
    // *normalizer* rather than a statistic (spec §5). It divides the measurement
    // already in `out_field` by that column's total, and it has to run after the
    // groups recombine for the reason `stack` does: a share is a fraction of the
    // **whole frame**, and inside the split each group sees only its own rows.
    //
    // Until 2026-07-26 this ran only when the mark had no position axis, so a
    // *keyed* `proportion` normalized inside each group instead — `bar * proportion
    // + x(direction) + color(season)` drew two conditional distributions summing to
    // 1 each, a plot totaling 2. The word means one thing in every context (Law 6),
    // and §5 had said "over the whole frame" throughout while the code disagreed
    // wherever a `color` was bound. The conditional reading is the *facet*, which
    // partitions the data before any of this runs and so still sums to 1 per panel.
    let result = if transforms.contains(&Transform::Proportion) {
        normalize_shares(&result, out_field)
    } else {
        result
    };

    // `stack` runs here, *after* the per-group statistics have recombined, because
    // it is the one transform that reads across groups: each element's baseline is
    // the summed height of the groups stacked below it at the same position. Every
    // statistic before it partitioned the frame; stack re-reads the whole thing.
    // A no-op when the modifier is absent (the common case) or nothing splits.
    if transforms.contains(&Transform::Stack) {
        stack_frame(&result, key_field, out_field, group_field, stack_spec)
    } else {
        result
    }
}

/// `proportion`, in two dimensions — the same normalizer over a cell frame.
///
/// The plane's counterpart of the pass [`apply`] runs, and it exists so the word
/// means one thing in both readings (Law 6): whatever measured the cells,
/// `proportion` divides that measurement by its total across the whole frame. So
/// `zone * bin * proportion` is the relative-frequency heatmap on the mesh
/// `zone * bin` cuts, and `zone * mean * proportion` reads each cell's mean as a
/// share — the two-dimensional twins of the histogram and the summary bar.
///
/// A tally is *renamed* as it is divided ([`CELL_COUNT`] → [`CELL_SHARE`]), because
/// the legend titles itself from the column and a key reading `count` over a column
/// of fractions is the mislabeling this session existed to remove. A reduction
/// keeps the user's own column name — it is their quantity, rescaled in place.
///
/// Runs **after** the group split recombines, never inside it, which is the whole
/// frame rule [`apply`] follows and for the same reason.
pub fn share_cells(df: &DataFrame, transforms: &[Transform]) -> DataFrame {
    if !transforms.contains(&Transform::Proportion) { return df.clone() }
    // `count2d` already divided — it is handed `share` directly, being the one
    // reading where the tally and the normalization are one pass over one frame.
    if df.float_col(CELL_SHARE).is_some() { return df.clone() }

    if let Some(counts) = df.float_col(CELL_COUNT) {
        let total: f64 = counts.iter().filter(|v| v.is_finite()).sum();
        if !(total > 0.0) { return df.clone() }
        let shares: Vec<f64> = counts.iter().map(|c| c / total).collect();
        return df.clone().without_col(CELL_COUNT).with_float(CELL_SHARE, shares);
    }
    df.clone()
}

/// Divide `out_field` by its own total, so the column reads as shares of one.
fn normalize_shares(df: &DataFrame, out_field: &str) -> DataFrame {
    let Some(vals) = df.float_col(out_field) else { return df.clone() };
    let total: f64 = vals.iter().filter(|v| v.is_finite()).sum();
    if !(total > 0.0) { return df.clone(); }
    let shares: Vec<f64> = vals.iter().map(|v| v / total).collect();
    df.clone().with_float(out_field, shares)
}

// ---------------------------------------------------------------------------
// stack — the cross-group offset that piles marks along the measure axis
// ---------------------------------------------------------------------------

/// Pile the `color`/`group` split along the measure axis: rewrite `out_field` so
/// each element holds its cumulative **top**, and add a `stack_base` column with
/// its cumulative **bottom**. A `bar`/`area` element then spans `[stack_base,
/// out_field]` instead of `[0, value]`, and the axis — reading `out_field` — sees
/// the stacked total with no special plumbing (the same trick `range` uses to put
/// its extents on the ordinary y-domain).
///
/// The stacking order is category order (`categories_across`), so the first group
/// sits at the bottom and the legend reads bottom-to-top. The baseline is computed
/// directly — the sum of `out_field` over every row that shares this element's
/// position **and** belongs to an earlier group — rather than by trusting the row
/// order, so it is correct however the upstream statistic happened to emit its
/// rows. Expects one value per (position, group), the way the rest of the engine
/// treats pre-summarized data; feed it `bar * count * stack` or `area * sum * stack`
/// when the raw table has several rows per cell.
///
/// **`stack(share = true)` fills every pile to 1** — the 100% stacked bar. Each
/// element is divided by the total of the pile it sits in *before* the piling runs,
/// so the cumulative arithmetic below is untouched and every slot tops out at
/// exactly 1. It is a parameter here rather than a second normalizer beside
/// `proportion` because it divides by the **slot's** total where `proportion`
/// divides by the **frame's**, and because it composes with any measurement: a share
/// of summed revenue is `bar * sum * stack(share = true)`, which no reading of
/// `proportion` can say (spec §5, and [`crate::ir::StackSpec`]).
///
/// A pile summing to zero is left alone rather than divided — with nothing in the
/// slot there is no composition to show, and the alternative is a column of NaN.
fn stack_frame(df: &DataFrame, key_field: &str, out_field: &str, group_field: Option<&str>, spec: Option<&StackSpec>) -> DataFrame {
    let Some(outs) = df.float_col(out_field) else { return df.clone() };
    let n = outs.len();
    let share = spec.is_some_and(|s| s.share.unwrap_or(false));

    // With no split there is nothing to pile: every element sits on zero, which is
    // exactly what an un-stacked bar/area already does. Emit a zero baseline so the
    // renderer's stacked path is a no-op rather than a special case.
    let Some(gf) = group_field.and_then(|g| df.str_col(g)) else {
        return df.clone().with_float(STACK_BASE, vec![0.0; n]);
    };
    let order = crate::data::categories_across(&[df], group_field.unwrap());
    let rank = |g: &str| order.iter().position(|o| o == g).unwrap_or(usize::MAX);

    // Two elements share a position when their key columns match — string equality
    // for a categorical axis, a tolerance compare for a numeric one.
    let key_str = df.str_col(key_field);
    let key_num = df.float_col(key_field);
    let same_pos = |a: usize, b: usize| -> bool {
        // No key column *named* at all: the mark has no position axis, so every
        // element is in the one slot and the whole split piles into a single column
        // (the share-of-total bar, and the pie once the plane is bent — spec §15).
        //
        // Asked *before* the columns are consulted, and that order is the whole
        // point: a synthesizing transform with no `y()` writes its output under the
        // empty name too, so a lookup of `""` here finds the *counts* and reads
        // them as positions — which silently unstacked the pie until this was
        // moved up. A key that is named but absent is a different thing, a missing
        // column, already refused by `legality`.
        if key_field.is_empty() { return true; }
        if let Some(k) = key_str { return k[a] == k[b]; }
        if let Some(k) = key_num { return (k[a] - k[b]).abs() < 1e-9; }
        a == b
    };

    // `share` rescales each element to its own pile's total *before* the piling, so
    // the cumulative arithmetic below never learns the difference and every slot
    // tops out at 1. Dividing here rather than after is what keeps `stack_base`
    // right: a foot is a sum of tops, so scaling the tops scales the feet with them.
    let owned;
    let outs: &[f64] = if share {
        let totals: Vec<f64> = (0..n)
            .map(|i| (0..n).filter(|&j| same_pos(i, j)).map(|j| outs[j]).sum())
            .collect();
        owned = (0..n)
            .map(|i| if totals[i].abs() > 1e-12 { outs[i] / totals[i] } else { outs[i] })
            .collect::<Vec<f64>>();
        &owned
    } else {
        outs
    };

    let mut base = vec![0.0; n];
    let mut top = vec![0.0; n];
    for i in 0..n {
        let ri = rank(&gf[i]);
        // The foot: everything at this position that stacks below this group.
        let below: f64 = (0..n)
            .filter(|&j| j != i && same_pos(i, j) && rank(&gf[j]) < ri)
            .map(|j| outs[j])
            .sum();
        base[i] = below;
        top[i] = below + outs[i];
    }

    // **Where the pile hangs** — the other free choice, once the heights are fixed
    // (spec §5, [`crate::ir::StackSpec`]). Applied as one subtraction per position to
    // *both* the foot and the top, which is the whole of it: displacing a pile moves
    // it bodily and never changes a band's thickness, so every reading the plot
    // supports is untouched and only the origin is spent.
    let ranks: Vec<usize> = (0..n).map(|i| rank(&gf[i])).collect();
    let mut piles = positions_in_order(df, key_field, n);
    // Sorted bottom band first, so a pile can be read as a stack and — the part that
    // matters for `"wiggle"` — the same group can be found at the next position by its
    // rank rather than by where the table happened to put its row.
    for rows in piles.iter_mut() {
        rows.sort_by_key(|&i| ranks[i]);
    }
    if let Some(shift) = baseline_shift(spec, &piles, &ranks, &base, &top) {
        for i in 0..n {
            base[i] -= shift[i];
            top[i] -= shift[i];
        }
    }

    df.clone().with_float(out_field, top).with_float(STACK_BASE, base)
}

/// The distinct positions a pile can stand at, **in the order they are drawn** —
/// which is what a displaced baseline has to walk, since `"wiggle"` carries a running
/// offset from one position to the next.
///
/// Row order for a categorical key (the axis's own order, `categories_across`), and
/// ascending for a numeric one. Returned as one representative row index per position
/// so the caller can read any column at that position without a second lookup.
fn positions_in_order(df: &DataFrame, key_field: &str, n: usize) -> Vec<Vec<usize>> {
    // No key named: one pile holding everything (the share-of-total bar, and the pie).
    if key_field.is_empty() {
        return vec![(0..n).collect()];
    }
    let mut groups: Vec<(Option<f64>, String, Vec<usize>)> = Vec::new();
    for i in 0..n {
        let (num, name) = match (df.float_col(key_field), df.str_col(key_field)) {
            (Some(k), _) => (Some(k[i]), String::new()),
            (_, Some(k)) => (None, k[i].clone()),
            _ => (Some(i as f64), String::new()),
        };
        match groups.iter_mut().find(|(gn, gs, _)| match (gn, num) {
            (Some(a), Some(b)) => (a - &b).abs() < 1e-9,
            _ => *gs == name,
        }) {
            Some((_, _, rows)) => rows.push(i),
            None => groups.push((num, name, vec![i])),
        }
    }
    // A numeric key is a *position*, so it is walked left to right whatever order the
    // table happened to hold. A categorical key has no between, and its order is the
    // axis's, which is the order the rows arrived in.
    if groups.iter().all(|(n, _, _)| n.is_some()) {
        groups.sort_by(|a, b| a.0.unwrap().partial_cmp(&b.0.unwrap()).unwrap_or(std::cmp::Ordering::Equal));
    }
    groups.into_iter().map(|(_, _, rows)| rows).collect()
}

/// How far to lower each row, per `stack(baseline = )`. `None` for the default, so the
/// plain stacked bar does no arithmetic it did not do before.
///
/// **`"center"`** hangs every pile so its midpoint is at zero: subtract half the pile's
/// own total, one position at a time, nothing carried between them. That is the
/// ThemeRiver, and it is the *symmetric* layout rather than the streamgraph — a
/// distinction worth keeping, because the readability result that motivates this
/// parameter found the streamgraph better than both the floor and the ThemeRiver.
///
/// **`"wiggle"`** chooses the foot that leaves the bands as flat as it can: Byron and
/// Wattenberg's weighted-wiggle layout, the one a streamgraph means. Walking positions
/// left to right, the baseline moves by the *thickness-weighted mean* of how far each
/// band's own middle would otherwise travel — so a thick band, which the eye reads
/// first, is the one held steadiest, and thin bands absorb the movement. There is no
/// optimizer here: the minimizing offset has a closed form at each step, which is why
/// the whole layout is one pass.
fn baseline_shift(
    spec: Option<&StackSpec>,
    piles: &[Vec<usize>],
    ranks: &[usize],
    base: &[f64],
    top: &[f64],
) -> Option<Vec<f64>> {
    let kind = spec.and_then(|s| s.baseline.as_deref()).unwrap_or("zero");
    if kind == "zero" {
        return None;
    }
    let mut shift = vec![0.0; base.len()];
    // A band's own height, which is what it had before the cumulative sum went through
    // it. Read back rather than carried, so this function needs only the two arrays the
    // pile is already described by.
    let thick = |i: usize| top[i] - base[i];

    if kind == "center" {
        for rows in piles {
            // The pile's total is the sum of its bands. Taken as a sum rather than as
            // the last band's top so a pile the caller handed us out of order, or one
            // with a group missing, still centers on what it actually contains.
            let total: f64 = rows.iter().map(|&i| thick(i)).sum();
            for &i in rows { shift[i] = total / 2.0; }
        }
        return Some(shift);
    }

    // `"wiggle"` — Byron and Wattenberg's weighted-wiggle baseline, one pass. `foot` is
    // where the bottom band stands, carried from the previous position; each step moves
    // it by the thickness-weighted mean of how far the bands would otherwise travel, so
    // the thick bands (the ones the eye reads first) are the ones held steadiest.
    let mut foot = 0.0;
    for (p, rows) in piles.iter().enumerate() {
        if p > 0 {
            // The previous pile by *rank*, never by row order: a group's row can sit
            // anywhere in the table, and a group absent at one position must not slide
            // the whole pairing by one.
            let prev = &piles[p - 1];
            let was = |r: usize| prev.iter().find(|&&j| ranks[j] == r).map_or(0.0, |&j| thick(j));
            let (mut num, mut den) = (0.0, 0.0);
            let mut below = 0.0; // how far everything under this band has already moved
            for &i in rows {
                let t = thick(i);
                let grew = t - was(ranks[i]);
                // This band's own middle travels by half its own growth, on top of
                // every change stacked beneath it, which pushes it bodily.
                num += (below + grew / 2.0) * t;
                den += t;
                below += grew;
            }
            if den.abs() > 1e-12 {
                foot -= num / den;
            }
        }
        for &i in rows { shift[i] = -foot; }
    }
    Some(shift)
}

/// Spend a stacked span on *glyphs* instead of on length — the dot plot (spec §5).
///
/// `stack` hands every element the same thing whatever mark draws it: the span
/// `[stack_base, out_field]` along the measure axis. Each mark then draws that span
/// as its own geometry, and the marks divide by whether they can *stretch*. A bar
/// fills the span; an area fills it across x; a **point** cannot stretch, so it
/// spends the span on how many glyphs there are — one per unit of it. A row that
/// counted seven observations becomes seven rows at `base + 1 ..= base + 7`, and
/// the top one lands exactly where the un-piled `point * bin` drew its single
/// summary dot, with the pile filled in beneath.
///
/// **A row-level rewrite, not a render-stage one**, for the reason `stack` itself is
/// (see [`stack_frame`]): the pile reaches *down* to its foot, so it moves the
/// measure domain, and a renderer that expanded the glyphs on its own would draw
/// dots the axis had never heard of. Built render-side first, and that is exactly
/// what happened — `point * count * stack` on three categories counting 5, 3 and 7
/// put its bottom two dots below a y axis that started at 3, clipping them in
/// silence.
///
/// The unit is one row, which is why `legality::check_stack` requires a transform
/// that counts rows before it will accept `point * stack`: a pile of 3.7 dots means
/// nothing, so a mean or a proportion is refused there rather than rounded here.
pub fn pile(df: &DataFrame, out_field: &str) -> DataFrame {
    let Some(tops) = df.float_col(out_field) else { return df.clone() };
    let n = tops.len();
    let feet: Vec<f64> = match df.float_col(STACK_BASE) {
        Some(b) => (0..n).map(|i| b.get(i).copied().unwrap_or(0.0)).collect(),
        None    => vec![0.0; n],
    };
    // How many glyphs each row is worth, and where each of them sits. A count is a
    // whole number by construction, so rounding only undoes float arithmetic.
    let times: Vec<usize> = (0..n)
        .map(|i| {
            let (foot, top) = (feet[i], tops[i]);
            if !foot.is_finite() || !top.is_finite() { return 0 }
            (top - foot).round().max(0.0) as usize
        })
        .collect();
    let mut rungs: Vec<f64> = Vec::with_capacity(times.iter().sum());
    for (i, &k) in times.iter().enumerate() {
        rungs.extend((1..=k).map(|r| feet[i] + r as f64));
    }
    df.repeat_rows(&times).with_float(out_field, rungs)
}

/// Run the transform sequence separately inside each group of `group_field`,
/// then stack the results back into one frame tagged with the group.
#[allow(clippy::too_many_arguments)]
fn apply_grouped(
    df: &DataFrame,
    transforms: &[Transform],
    key_field: &str,
    out_field: &str,
    bin_spec: Option<&BinSpec>,
    cut: Option<&BinLayout>,
    density_spec: Option<&DensitySpec>,
    range_spec: Option<&RangeSpec>,
    conf_spec: Option<&ConfidenceSpec>,
    dev_spec: Option<&DeviationSpec>,
    q_spec: Option<&QuantileSpec>,
    box_spec: Option<&BoxSpec>,
    bounds_spec: Option<&BoundsSpec>,
    group_field: &str,
) -> DataFrame {
    // Bin edges are shared across the groups, or overlaid histograms would not
    // line up — the layout is computed once from every row, then each group
    // counts *its* rows into those fixed bins. No other statistic needs a shared
    // frame: a per-group mean or density stands on its own rows alone.
    //
    // A cut the caller already resolved wins, because it was resolved from *more*
    // rows than these: `df` here is one panel, and a facet shares its cut across
    // panels the same way it shares its scale (spec §11). Both splits then land on
    // the one lattice — a `color` split inside a `facet` gives every species in
    // every panel the same edges, which is the composition Law 1 promised and
    // neither split has to know about the other to keep.
    let shared = if transforms.iter().any(|t| *t == Transform::Bin) {
        cut.copied().or_else(|| df.float_col(key_field).and_then(|xs| bin_layout(xs, bin_spec)))
    } else {
        None
    };

    // Group order follows a declared factor when there is one, so colors and
    // the legend read in the same order the axis would.
    let levels = df.levels(group_field).map(<[String]>::to_vec);
    let groups = crate::data::categories_across(&[df], group_field);

    let parts: Vec<DataFrame> = groups.iter().filter_map(|gv| {
        let sub = df.filter_str_eq(group_field, gv);
        if sub.is_empty() { return None; }
        let res = apply_seq(&sub, transforms, key_field, out_field, bin_spec, density_spec, range_spec, conf_spec, dev_spec, q_spec, box_spec, bounds_spec, shared.as_ref());
        let n = res.len();
        if n == 0 { return None; }
        let tag = vec![gv.clone(); n];
        Some(match &levels {
            Some(lv) => res.with_levels(group_field, tag, lv.clone()),
            None     => res.with_str(group_field, tag),
        })
    }).collect();

    DataFrame::vconcat(&parts)
}

#[allow(clippy::too_many_arguments)]
fn apply_seq(
    df: &DataFrame,
    transforms: &[Transform],
    key_field: &str,
    out_field: &str,
    bin_spec: Option<&BinSpec>,
    density_spec: Option<&DensitySpec>,
    range_spec: Option<&RangeSpec>,
    conf_spec: Option<&ConfidenceSpec>,
    dev_spec: Option<&DeviationSpec>,
    q_spec: Option<&QuantileSpec>,
    box_spec: Option<&BoxSpec>,
    bounds_spec: Option<&BoundsSpec>,
    layout: Option<&BinLayout>,
) -> DataFrame {
    // **A cut is an extent description, and the tally was never `bin`'s to keep**
    // (spec §5). When something else in the sequence was handed a column to measure,
    // `bin` supplies only the cells: it rewrites the key to its cell's center and
    // keeps every row, so whatever measures next groups by the cut exactly as it
    // would group by categorical slots. Running the tally first is what silently
    // dropped the statistic — `bin` overwrote the named column with counts, and the
    // reduction then averaged one count per bin and gave it back unchanged, leaving
    // a histogram under an axis labeled for the column nobody had read.
    // **The cut runs first wherever it was written**, which is not this rule going
    // soft on order but the one ordering the rule allows: a cell has to *exist*
    // before anything can be measured in it, so an extent description is prior to a
    // measurement rather than earlier in a queue. `bar * mean * bin` therefore draws
    // what `bar * bin * mean` draws, and the two-dimensional reading already worked
    // this way — `svg.rs` dispatches on which transforms are present, never on their
    // order — so this is the two readings agreeing rather than a new liberty. Left
    // alone, the reversed spelling aggregated the raw rows into their own groups and
    // then cut *that*, drawing 1704 overlapping bars at ten positions.
    //
    // **This used to be a second code path, and that is what made it wrong.** The cut
    // ran in a `fold` of its own that filtered out `Bin` and nothing else, so the
    // `proportion` guard below — the twin loop's, six lines away — never applied to a
    // binned frame. `proportion` is `count` when it has nothing to rescale, so on the
    // hoisted path it re-tallied the frame the cut had just built: `bar * bin *
    // proportion * mean` threw the mean away and drew the histogram, and `bar * bin *
    // mean * proportion` computed the mean, overwrote it with a tally of one row per
    // cell, and drew every non-empty cell at 1/k — ten identical bars. Neither
    // spelling was a reading of anything, which is why this is one loop now: a guard
    // that holds on one path and not its twin is not a guard.
    let cut_first = transforms.contains(&Transform::Bin) && measures_a_column(transforms);

    // `proportion` makes the tally only when nothing else measured. It is a
    // **normalizer** (spec §5): its job is the division `apply` runs over the
    // recombined frame, and the tally is what it falls back to when there is no
    // measurement to rescale — which is every sentence that spells it alone, and so
    // is why the class change is invisible to `bar * proportion + x(<category>)`.
    // Composed with a statistic it steps aside here and lets that statistic write
    // `out_field`: `bar * bin * proportion` is `bin`'s own tally as shares (the
    // relative-frequency histogram, empty cells included, on the mesh `bar * bin`
    // cuts), and `bar * sum * proportion + y(<column>)` is each slot's summed
    // column as a share of the total.
    let measured = measures_beside_share(transforms);
    let mut current = if cut_first {
        bin_cut(df, key_field, bin_spec, layout)
    } else {
        df.clone()
    };
    for t in transforms {
        // The cut already ran, above, wherever it was written.
        if cut_first && *t == Transform::Bin { continue }
        if *t == Transform::Proportion && measured { continue }
        current = apply_one(&current, t, key_field, out_field, bin_spec, density_spec, range_spec, conf_spec, dev_spec, q_spec, box_spec, bounds_spec, layout);
    }
    current
}

/// Did some transform other than `proportion` make a measurement, so `proportion`
/// has something to rescale rather than a tally to make first?
///
/// The predicate that makes `proportion` a normalizer rather than a fifth
/// synthesizing transform (spec §5). `density`, `smooth` and the pair transforms
/// are deliberately absent: `legality::check_share_composition` refuses `proportion`
/// against all of them upstream, so naming them here would describe a state the
/// engine never reaches.
pub fn measures_beside_share(transforms: &[Transform]) -> bool {
    transforms.iter().any(|t| matches!(t, Transform::Bin | Transform::Count))
        || has_reduction(transforms)
}

/// Was some transform in this sequence handed a column to measure?
///
/// The question that decides who owns a composed measurement (spec §5). The five
/// value statistics reduce a named column to one number and the pair transforms
/// reduce it to two; either way the *user* named it, which is what makes the
/// measurement theirs and leaves `bin` with only its cut to contribute. `smooth` is
/// deliberately absent: it fits a curve rather than measuring cells, and the
/// composition rule refuses it upstream.
///
/// **Read off [`jobs`] rather than assembled here**, which is how `bounds` joined on
/// 2026-07-31. It had been missing, because the list was built from
/// [`reduces_column`] and [`pairs_a_column`] and the second deliberately excludes
/// `bounds` — for a good reason of its own (`bounds` names the pair instead of
/// computing it, so a cell has nothing to reduce) that was never a reason to leave it
/// out of *this* question. The cost was `line * bin * bounds`: the cut was not
/// hoisted, so written order ran the tallying `bin` first, which rebuilt the frame
/// and destroyed the two columns `bounds` was about to read, and the plot came out a
/// histogram. Reversed, it came out something else. One list, one answer.
pub fn measures_a_column(transforms: &[Transform]) -> bool {
    transforms.iter().any(|t| jobs(t, JobContext::default()).reads_a_column)
}

// ---------------------------------------------------------------------------
// Jobs — what a transform is *for*, and why two of a kind cannot compose
// ---------------------------------------------------------------------------

/// The four **jobs** a transform can do (spec §5).
///
/// Every transform in the kernel fills at least one of these, and two transforms
/// filling the same one contradict: the frame holds one answer per cell, so the
/// engine would have to discard one of them. That is the rule the whole family of
/// composition refusals derives from, and it was written three times by hand — once
/// per transform family — before it was written once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Job {
    /// **Where the cells are.** `bin` cuts an axis into bands; `bounds` names their
    /// sides from columns you already have; `partition` carves a rectangle into a
    /// hierarchy. Its default filler is the positions themselves — a categorical
    /// axis owns one slot per category and needs no transform to say so.
    Extent,
    /// **What is in them.** The tally, the reduction, the fitted curve, the pair.
    /// Its default filler is the tally, which is what lets `proportion` stand alone.
    Measure,
    /// **What scale the answer is read on.** `proportion` divides a measurement into
    /// shares of the whole; `stack(share = TRUE)` divides it into shares of its pile.
    Scale,
    /// **Where the marks sit** once everything above is settled. `dodge` puts
    /// colliding groups side by side, `stack` piles them, `jitter` scatters them
    /// inside their slot.
    Position,
}

/// What the mark and its settings contribute to reading a transform's job.
///
/// Two transforms answer differently depending on their surroundings, and both
/// answers are facts about the frame rather than about the mark, so they arrive as
/// plain bits rather than as a `Mark`. Keeping marks out of this module is the same
/// discipline that makes [`apply`] take `key_field`/`out_field` instead of working
/// out which axis is which.
#[derive(Debug, Clone, Copy, Default)]
pub struct JobContext {
    /// Does the mark carry its measurement on `color` rather than on an axis?
    ///
    /// A `zone` has no measure axis, so its `bounds(start, end)` names the sides of a
    /// rectangle — an extent. Every other mark reads `bounds(lower, upper)` as the
    /// low/high pair on the measure axis — a measurement. `legality::has_no_measure_axis`
    /// is the caller that answers this.
    pub measures_by_color: bool,
    /// Was `stack` given `share = TRUE`?
    ///
    /// A plain `stack` only piles, which is a position. A sharing one divides each
    /// element by its own pile's total first, which is a scale — and that is why
    /// `proportion` beside it is two divisions that cancel.
    pub stack_shares: bool,
}

/// Which jobs one transform fills, and on what terms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Jobs {
    /// Does it say where the cells are?
    pub extent: bool,
    /// Does it say what is in them?
    pub measure: bool,
    /// Was that measurement made from a column the **caller** named?
    ///
    /// The distinction [`Job::Measure`] turns on. `mean` was handed `life` and reduces
    /// it; `count` was handed nothing and tallies rows. Only the first kind can take
    /// the measure job over from `bin`, which is why `bar * bin * mean` composes and
    /// `bar * bin * count` does not.
    pub reads_a_column: bool,
    /// Does it hand the measure job over when something else was handed a column?
    ///
    /// True of `bin` alone. A cut *has* to describe an extent and only *happens* to
    /// tally, so the tally is the half it can give up; every other transform that
    /// measures was written to measure and has nothing to give.
    pub yields_measure: bool,
    /// Does it say what scale the answer is read on?
    pub scale: bool,
    /// Does it say where the marks sit?
    pub position: bool,
}

/// Which jobs this transform fills, given what the mark and settings contribute.
///
/// The single table the composition rule reads. Adding a transform without adding a
/// row here is caught by `every_transform_has_a_job`, on the same reasoning as
/// `legality::every_mark_channel_pair_has_a_rule`: a transform that arrives jobless
/// composes silently with everything, which is exactly how `range`, `confidence`,
/// `bounds` and the three collision modifiers spent the project's life outside every
/// composition check.
pub fn jobs(t: &Transform, ctx: JobContext) -> Jobs {
    let measure = |reads_a_column| Jobs { measure: true, reads_a_column, ..Jobs::default() };
    match t {
        // Cuts the axis into cells, and tallies them only because it can. The tally
        // is the half it yields — see [`bin_cut`], which is that yield in code.
        Transform::Bin => Jobs {
            extent: true, measure: true, yields_measure: true, ..Jobs::default()
        },
        // Fits a curve through the rows. The window it fits over is its own extent,
        // and the fit is its own measurement, and it can give up neither: a curve
        // sampled at somebody else's cells is not the curve.
        Transform::Smooth | Transform::Density => Jobs {
            extent: true, measure: true, ..Jobs::default()
        },
        // Carves one rectangle into a hierarchy of them, and measures each node.
        Transform::Partition => Jobs { extent: true, measure: true, ..Jobs::default() },
        // Lays out the whole diagram — slots, stacks, and the bands between them —
        // so it holds every job at once and composes with nothing. Deliberately
        // wider than `partition`'s claim: `flow * proportion` may one day divide
        // the measure axis into shares the way `partition * proportion` does, but
        // until that is built the refusal is honest and a relaxation is cheap,
        // where a composition that silently does nothing is §12's forbidden drop.
        Transform::Flow => Jobs {
            extent: true, measure: true, scale: true, position: true,
            ..Jobs::default()
        },
        // The graph layout holds every job for `flow`'s reason: it places its
        // own marks, measures nothing a caller could rename, and composes with
        // no other computation until a relaxation is argued for.
        Transform::Layout => Jobs {
            extent: true, measure: true, scale: true, position: true,
            ..Jobs::default()
        },
        // Tallies rows into whatever cells already exist. It was handed no column, so
        // it cannot take the measure job over from a cut.
        Transform::Count => measure(false),
        Transform::Sum | Transform::Mean | Transform::Median
            | Transform::Max | Transform::Min | Transform::Quantile => measure(true),
        // The pair transforms reduce a named column to two numbers rather than one.
        // Two numbers are still one answer per cell, so two of them still collide.
        Transform::Range | Transform::Confidence | Transform::Deviation
            | Transform::Box => measure(true),
        // The one entry that reads differently per mark: sides of a rectangle on a
        // mark that measures by color, the low/high pair on the measure axis
        // everywhere else.
        Transform::Bounds => if ctx.measures_by_color {
            Jobs { extent: true, measure: true, reads_a_column: true, ..Jobs::default() }
        } else {
            measure(true)
        },
        Transform::Proportion => Jobs { scale: true, ..Jobs::default() },
        Transform::Stack => Jobs {
            scale: ctx.stack_shares, position: true, ..Jobs::default()
        },
        // `repel` says where the marks sit for the same reason the other two offsets
        // do — it is the last word on where a label lands. It fills the job in page
        // units rather than data units, which changes when the answer is computed
        // and not what job it answers.
        Transform::Dodge | Transform::Jitter =>
            Jobs { position: true, ..Jobs::default() },
        // `repel` is **deliberately jobless**, and the distinction from the
        // accidental joblessness the comment above warns about is that this one
        // is argued: repel moves *ink*, at draw time, after every job in this
        // enum has already run — it does not decide where a mark sits, only
        // where its word rests once everything else has. So it contradicts
        // nothing, and in particular it composes with the whole-picture layouts
        // (`text * layout(from, to) * repel` is how a network's names come off
        // their dots), which claim every positional job precisely to refuse the
        // modifiers that *do* move marks. This was the relaxation the layout's
        // claim-everything entry said would be cheap when argued; a label
        // striking through its own node was the argument.
        Transform::Repel => Jobs::default(),
    }
}

/// The first pair in this sequence that fills the same job, if any.
///
/// **Two transforms that do one job contradict, and `bin` is the only one that can
/// step aside.** The frame holds one extent, one measurement, one scale and one
/// arrangement per cell, so a second filler is a request the engine can only answer
/// by throwing one of the two away — the silent drop §12 forbids.
///
/// Measure is asked first because it is the job most pairs collide on and its
/// messages are the ones that read best: `bin * smooth` is a collision on both extent
/// and measure, and *"`smooth` already averages locally as it goes"* is what the
/// reader needs, not *"two things cut the axis"*.
///
/// This finds the contradiction; `legality` says what to do about it. The split is
/// deliberate — a transform's job is a fact about frames, which is this module's
/// subject, while a refusal is a sentence addressed to a person, which is not.
pub fn job_conflict(ts: &[Transform], ctx: JobContext) -> Option<(Transform, Transform, Job)> {
    let of = |t: &Transform| jobs(t, ctx);

    // Measure. `bin` steps aside only when somebody else was handed a column, which
    // is why `bin * mean` composes, `bin * count` does not, and three measurements
    // never do however the yield falls.
    let measures: Vec<&Transform> = ts.iter().filter(|t| of(t).measure).collect();
    if measures.len() > 1 {
        let claimed = measures.iter().any(|t| of(t).reads_a_column);
        let standing: Vec<&Transform> = measures.iter().copied()
            .filter(|t| !(claimed && of(t).yields_measure))
            .collect();
        if standing.len() > 1 {
            return Some((standing[0].clone(), standing[1].clone(), Job::Measure));
        }
    }

    for (job, filled) in [
        (Job::Extent,   ts.iter().filter(|t| of(t).extent).collect::<Vec<_>>()),
        (Job::Scale,    ts.iter().filter(|t| of(t).scale).collect::<Vec<_>>()),
        (Job::Position, ts.iter().filter(|t| of(t).position).collect::<Vec<_>>()),
    ] {
        if filled.len() > 1 {
            return Some((filled[0].clone(), filled[1].clone(), job));
        }
    }
    None
}

#[allow(clippy::too_many_arguments)]
fn apply_one(df: &DataFrame, t: &Transform, key_field: &str, out_field: &str, bin_spec: Option<&BinSpec>, density_spec: Option<&DensitySpec>, range_spec: Option<&RangeSpec>, conf_spec: Option<&ConfidenceSpec>, dev_spec: Option<&DeviationSpec>, q_spec: Option<&QuantileSpec>, box_spec: Option<&BoxSpec>, bounds_spec: Option<&BoundsSpec>, layout: Option<&BinLayout>) -> DataFrame {
    match t {
        Transform::Bin     => bin(df, key_field, out_field, bin_spec, layout),
        Transform::Count   => count(df, key_field, out_field),
        Transform::Smooth  => smooth(df, key_field, out_field),
        Transform::Density => density(df, key_field, out_field, density_spec),
        Transform::Sum        => aggregate(df, key_field, out_field, AggFn::Sum),
        Transform::Mean       => aggregate(df, key_field, out_field, AggFn::Mean),
        Transform::Median     => aggregate(df, key_field, out_field, AggFn::Median),
        Transform::Max        => aggregate(df, key_field, out_field, AggFn::Max),
        Transform::Min        => aggregate(df, key_field, out_field, AggFn::Min),
        Transform::Quantile   => aggregate(df, key_field, out_field,
                                           AggFn::Quantile(quantile_p(q_spec))),
        Transform::Proportion => proportion(df, key_field, out_field),
        Transform::Range      => range(df, key_field, out_field, range_spec),
        Transform::Confidence => confidence(df, key_field, out_field, conf_spec),
        Transform::Deviation  => deviation(df, key_field, out_field, dev_spec),
        Transform::Box        => box_summary(df, key_field, out_field, box_spec),
        Transform::Bounds     => bounds(df, key_field, out_field, bounds_spec),
        // `dodge` is a collision modifier, not a statistic: it repositions the
        // groups at render time and synthesizes no rows, so here it is the
        // identity. It rides in the transform sequence only so `*` can carry it
        // uniformly; the renderer reads it off `layer.transforms` (spec §5).
        // `partition` is an *extent description*, not a keyed statistic: it reads
        // hierarchy columns the sequence here knows nothing about and emits one
        // row per node. Like the two-dimensional readings (`bin2d`, `count2d`) it
        // is dispatched by the renderer, which knows the mark and the bindings, so
        // reaching this one-key pipeline at all means a mark that does not take it
        // slipped past `mark_takes_transform`. Identity rather than a panic, on
        // §12's rule that the engine never invents a plot nobody asked for.
        Transform::Partition  => df.clone(),
        Transform::Flow       => df.clone(),
        Transform::Layout     => df.clone(),
        Transform::Dodge      => df.clone(),
        // `stack` is also a collision modifier, but its offset accumulates *across*
        // groups — group b sits on group a's height — so it cannot run inside a
        // single group's sequence, where each group sees only its own rows. Here it
        // is the per-group identity; the real accumulation is `stack_frame`, run by
        // `apply` once the groups have recombined (spec §5).
        Transform::Stack      => df.clone(),
        // `jitter` is `dodge`'s render-stage kin: it spreads overlapping points
        // within their categorical slot, an offset bounded to that slot so it never
        // moves the scale domain. Like `dodge` it synthesizes no rows here; the
        // `Jitter` helper in `render/marks/point.rs` computes each point's offset
        // (spec §5).
        Transform::Jitter     => df.clone(),
        // `repel` is the same shape one stage later. It moves labels off one
        // another, and what overlaps is *ink* rather than a position, so the offset
        // cannot be known until the glyphs have a size — `render/marks/text.rs`
        // computes it. No rows here either (spec §5).
        Transform::Repel      => df.clone(),
    }
}

/// Attach a transform's categorical key column while **carrying the input
/// column's declared factor order** onto the output.
///
/// Every summary/count transform rebuilds its key column from scratch, and doing
/// that with `with_str` drops the input's `levels` — so a factor axis
/// (`factor(cyl)` → 4, 6, 8) fell back to first-appearance order the moment it was
/// counted or aggregated (`bar * count + x(cyl)` drew 6, 8, 4). That breaks Law 4:
/// a factor declares its order once, and the transform must not lose it.
///
/// `categories_across` already orders present values by `levels` and drops absent
/// ones, so re-attaching the input's levels is the whole fix: present categories
/// read in declared order, and a level with no rows leaves no empty slot. A plain
/// (non-factor) string column has no levels and keeps first-appearance order, as
/// before — the numeric key branches sort ascending and never pass through here.
fn keyed(base: DataFrame, field: &str, keys: Vec<String>, src: &DataFrame) -> DataFrame {
    match src.levels(field) {
        Some(levels) => base.with_levels(field, keys, levels.to_vec()),
        None         => base.with_str(field, keys),
    }
}

// ---------------------------------------------------------------------------
// bin — histogram transform
// ---------------------------------------------------------------------------

/// Where a histogram's bins start, how wide they are, and how many. Split out of
/// `bin` so a grouped histogram can bin every color on the *same* edges: the
/// layout is computed once from all the rows, then each group counts into it. A
/// per-group layout would give each species its own edges, and overlaid bars
/// that do not line up.
///
/// **A facet split is that same sentence one dimension up** (spec §11). A cut is
/// an extent description and the tally was never `bin`'s to keep (spec §5), so
/// the two halves of a histogram land on opposite sides of the panel boundary:
/// the cut is shared across panels, exactly as the scale is, and each panel
/// tallies only its own rows into it. Derived per panel instead, `bar * bin +
/// x(life) | facet(continent)` gave every panel its own width — 5.5 years in
/// Asia against 1.7 in Europe on one shared axis — so the bars were counts of
/// different quantities drawn at the same height. [`BinCut`] is how the renderer
/// hands the shared answer down.
/// Opaque by construction: a caller resolves one and hands it back, and only this
/// module reads `mn`/`step`/`k`. That is what keeps "where the bins are" a single
/// answer rather than three numbers anyone may re-derive.
#[derive(Clone, Copy)]
pub struct BinLayout {
    mn: f64,
    step: f64,
    k: usize,
}

/// The cut a plot's `bin` makes on each domain axis, resolved **once** from every
/// panel's rows rather than per panel.
///
/// Indexed by the plot's real axes rather than by a transform's key/measure roles,
/// because that is what the renderer knows when it derives them and what the
/// two-dimensional readings need: a mesh cuts x and y, and which of the two a
/// one-dimensional `bin` groups by is the caller's exchange to resolve
/// ([`BinCut::axis`]).
///
/// `None` on an axis means "derive from the rows you were given", which stays the
/// honest answer for a caller holding all of them — a test, or any plot whose one
/// panel *is* the frame. So an unfaceted plot is byte-identical either way, and
/// only a facet split can tell the difference.
#[derive(Clone, Copy, Default)]
pub struct BinCut {
    pub x: Option<BinLayout>,
    pub y: Option<BinLayout>,
}

impl BinCut {
    /// The cut on one axis, named the way the caller already names it.
    pub fn axis(&self, is_x: bool) -> Option<&BinLayout> {
        if is_x { self.x.as_ref() } else { self.y.as_ref() }
    }
}

/// Choose the bin layout for `xs`. The count comes from `spec`: an explicit
/// `bins`, or a `width` (bins of exactly that many data units across the range),
/// or — the common case, `spec` absent or empty — Sturges' rule. `bins` and
/// `width` are mutually exclusive; if both somehow arrive (the R binding refuses
/// them upstream) `bins` wins. `None` when there is nothing finite to bin.
///
/// Reachable from the renderer because *when* this runs is the whole question: it
/// must see every panel's rows, so `svg.rs` calls it before the facet filters and
/// passes the answer back down. Both of its inputs are otherwise panel-sized —
/// `n` decides Sturges' `k`, the span decides the width — which is why an
/// explicit `bin(10)` did not rescue the faceted histogram either.
pub(crate) fn bin_layout(xs: &[f64], spec: Option<&BinSpec>) -> Option<BinLayout> {
    let n = xs.len();
    if n == 0 { return None; }
    let mn = xs.iter().cloned().fold(f64::INFINITY, f64::min);
    let mx = xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    if !mn.is_finite() { return None; }

    let span = (mx - mn).max(1e-12);
    let (k, step) = match spec {
        Some(BinSpec { bins: Some(b), .. }) => {
            let k = (*b).max(1);
            (k, span / k as f64)
        }
        Some(BinSpec { width: Some(w), .. }) if *w > 0.0 => {
            let k = (span / w).ceil().max(1.0) as usize;
            (k, *w)
        }
        _ => {
            let k = ((n as f64).log2().ceil() as usize + 1).max(2);
            (k, span / k as f64)
        }
    };
    Some(BinLayout { mn, step, k })
}

/// Bin `x_field` into equal-width bins and count observations per bin. Outputs
/// bin centers → `x_field`, counts → `y_field`.
///
/// `layout` is the shared bin frame for a grouped histogram — `Some` when this is
/// one color group of many, so every group lands on the same edges. `None` for a
/// plain histogram, which derives its own layout from these rows.
fn bin(df: &DataFrame, x_field: &str, y_field: &str, spec: Option<&BinSpec>, layout: Option<&BinLayout>) -> DataFrame {
    // A categorical key is refused by `legality::check_distribution_axis`, fatally,
    // before this runs. The guard stays so the transform is total under
    // `GOG_STRICT=0` — where the user has read the diagnostic and asked to draw
    // anyway — but it says nothing: printing here is what made the refusal
    // unrefusable, and repeating it would only double the message.
    if df.str_col(x_field).is_some() {
        return DataFrame::new();
    }
    let Some(xs) = df.float_col(x_field) else { return DataFrame::new() };

    let owned;
    let BinLayout { mn, step, k } = *match layout {
        Some(l) => l,
        None => match bin_layout(xs, spec) {
            Some(l) => { owned = l; &owned }
            None    => return DataFrame::new(),
        },
    };

    let mut counts = vec![0u32; k];
    for &v in xs {
        let mut b = ((v - mn) / step) as usize;
        if b >= k { b = k - 1; }
        counts[b] += 1;
    }

    let centers: Vec<f64> = (0..k).map(|i| mn + (i as f64 + 0.5) * step).collect();
    let ys: Vec<f64>      = counts.iter().map(|&c| c as f64).collect();

    DataFrame::new()
        .with_float(x_field, centers)
        .with_float(y_field, ys)
}

/// [`bin`] with the tally left off: cut `x_field` into the same cells and move every
/// row to its cell's center, keeping the rest of the frame intact.
///
/// **`bin`'s half of a composition** (spec §5). A `bin` supplies two things — where
/// the cells are, and how many rows fell in each — and only the first is what makes
/// it a `bin`. Composed with a transform that was handed a column, it gives the
/// tally up and keeps the cut, which leaves a frame the *next* transform can measure
/// with no knowledge that a cut happened at all: every row in a cell now shares one
/// key, so `aggregate` groups them exactly as it groups a category's rows. That is
/// why the binned mean needed no new statistic, only a `bin` that stops answering a
/// question nobody asked it.
///
/// **Rows are kept, not summarized** — the opposite of every other transform here,
/// and the reason this one is not `apply_one`'s business: it is an extent
/// description, so it belongs to the sequence rather than to a step in it.
///
/// **A non-finite key is dropped rather than binned**, which is [`bin2d`]'s rule
/// rather than [`bin`]'s. A NaN cast to an index saturates to zero in Rust, so the
/// tallying path silently counts missing values into the first cell; a cut cannot
/// afford that, because the row would then carry a real measurement at a position
/// its own data never named.
///
/// An empty cell simply has no rows, so it emits nothing — the absent-cell rule
/// [`bin2d`] and [`agg2d`] already share, arrived at here for free rather than
/// written a third time.
fn bin_cut(df: &DataFrame, x_field: &str, spec: Option<&BinSpec>, layout: Option<&BinLayout>) -> DataFrame {
    // As in `bin`: a categorical key is refused fatally upstream, and this guard is
    // only what keeps the transform total under `GOG_STRICT=0`.
    if df.str_col(x_field).is_some() { return DataFrame::new(); }
    let Some(xs) = df.float_col(x_field) else { return DataFrame::new() };

    let owned;
    let BinLayout { mn, step, k } = *match layout {
        Some(l) => l,
        None => match bin_layout(xs, spec) {
            Some(l) => { owned = l; &owned }
            None    => return DataFrame::new(),
        },
    };

    let keep: Vec<bool> = xs.iter().map(|v| v.is_finite()).collect();
    let centers: Vec<f64> = xs.iter().filter(|v| v.is_finite()).map(|&v| {
        let mut b = ((v - mn) / step) as usize;
        if b >= k { b = k - 1; }
        mn + (b as f64 + 0.5) * step
    }).collect();

    df.keep_rows(&keep).with_float(x_field, centers)
}

// ---------------------------------------------------------------------------
// bin, in two dimensions — the tiling
// ---------------------------------------------------------------------------

/// The columns a 2-D `bin` synthesizes: the cell's four edges, and the count it
/// measured inside them.
///
/// **The contract, stated once here because a second tiling has to keep it**
/// (spec §5). A 2-D bin emits one row per cell carrying that cell's **center**
/// on the two position columns, and its **extent** in synthesized columns the
/// tiling chooses. The split is not arbitrary: a center is what every tiling
/// has — a hexagon has one, a triangle has one — so it rides on `x`/`y` where
/// the axes and the ticks can see it. The extent is what tilings *differ* on: a
/// rectangle needs four edges, a hexagon needs a circumradius and a row parity.
/// So a mark asks the tiling what its cells look like rather than assuming four
/// sides, and hexagonal binning adds columns beside these instead of reopening
/// the contract.
pub const CELL_START: &str = "cell_start";

/// Where a stacked element's **foot** sits, in the measure column's units — the
/// bottom of the span whose top `stack` writes back into the measure column itself.
///
/// Plumbing of the same kind as [`CELL_START`], and it was a bare `"stack_base"` in
/// four modules until the axis became the fifth reader: `stack_frame` writes it, the
/// `bar` and `area` writers read it for each element's foot, `pile` reads it to know
/// where a dot column starts, and `build_axis` reads it so a **displaced** pile's feet
/// are inside the fitted range (spec §5's baseline ruling). A name that many places
/// share belongs in one of them.
pub const STACK_BASE: &str = "stack_base";
/// The cell's far edge along the domain axis. See [`CELL_START`].
pub const CELL_END: &str = "cell_end";
/// The cell's near edge along the measure axis. See [`CELL_START`].
pub const CELL_LOWER: &str = "cell_lower";
/// The cell's far edge along the measure axis. See [`CELL_START`].
pub const CELL_UPPER: &str = "cell_upper";
/// What a 2-D bin measured in each cell. Named plainly because it is not
/// internal: it is the column the color legend titles itself from, and the one
/// a user may name out loud as `color(count)`.
///
/// Shared with the two-dimensional `count` (the tile plot), and deliberately: both
/// tally rows into the cells of a mesh, and they differ only in where the mesh came
/// from — cut out of continuous axes, or handed over by the categories. A tally is a
/// tally, so it wears one name.
pub const CELL_COUNT: &str = "count";
/// The same tally as a **share of the whole** — what `zone * proportion` measures.
/// Its own name rather than `count`, because a legend reading "count" over numbers
/// summing to 1 would be a lie the reader cannot see.
pub const CELL_SHARE: &str = "proportion";

/// A hexagonal cell's center along the domain axis. See [`CELL_Y`].
pub const CELL_X: &str = "cell_x";
/// A hexagonal cell's center along the measure axis.
///
/// The center also rides on the plot's own `x`/`y` columns, as every tiling's
/// does — this pair is the same numbers under names the *tiling* chose. A
/// rectangle never needed them, because its center is implied by its four
/// edges; a hexagon is anchored at its center and has no edges to imply one. The
/// duplication buys the property that matters: a mark can draw any tiling
/// knowing only the names the tiling published, never the names the plot's axes
/// happen to be bound to.
pub const CELL_Y: &str = "cell_y";
/// A hexagonal cell's half-width, in the x column's own units. See [`CELL_DY`].
pub const CELL_DX: &str = "cell_dx";
/// A hexagonal cell's half-height, in the y column's own units.
///
/// The pair that the `hex` tiling emits **instead of** the four edges, and the
/// evidence that [`CELL_START`]'s contract held: hexagonal binning added two
/// columns beside the rectangular ones and reopened nothing. A hexagon has no
/// four edges to name, but it does have a center (which every tiling has, and
/// which rides on `x`/`y` as before) and a size, which is what these two carry.
/// The six vertices follow from them, so the parity of the staggered row never
/// has to be recorded: it is already in the center.
pub const CELL_DY: &str = "cell_dy";
/// The stage a flow row stands at (a node's own stage; a band's **left** stage),
/// holding the stage *column's name* as its value with the declared order of the
/// `flow(...)` atom as its levels. The domain axis reads this when nothing is
/// bound to `x`, exactly as an unbound radial axis under a `partition` reads
/// [`NODE_DEPTH`] — the axis, the scale and the mark all read one column.
pub const FLOW_STAGE: &str = "stage";
/// Which path a flow band row belongs to. The band projection is one row per
/// (path, stage) — the low/high-rows shape `range` established, one column
/// deeper — and consecutive rows sharing this key are one band's two ends. A
/// per-gap row carrying both ends was the first shape built, and it starved
/// the axis: only *left* stages appeared in [`FLOW_STAGE`], so a two-stage
/// flow drawn by `ribbon` alone had a one-category axis and no second end to
/// place. Per-stage rows put every stage in the data itself, which is what a
/// seen-levels axis reads.
pub const FLOW_PATH: &str = "path";
/// A `layout`'s computed positions, published under names of their own — the
/// axis fallbacks read these when nothing is bound, the [`NODE_DEPTH`] pattern
/// a third time. In the unit square flat, the unit cube when the network
/// states a view.
pub const LAYOUT_X: &str = "layout_x";
/// See [`LAYOUT_X`].
pub const LAYOUT_Y: &str = "layout_y";
/// See [`LAYOUT_X`].
pub const LAYOUT_Z: &str = "layout_z";
/// An edge row's second endpoint, beside [`LAYOUT_X`]'s first — synthesized
/// columns, never channels, which is the `ymin`/`ymax` ruling holding for a
/// second endpoint exactly as it held for a second extent.
pub const EDGE_X: &str = "edge_x";
/// See [`EDGE_X`].
pub const EDGE_Y: &str = "edge_y";
/// See [`EDGE_X`].
pub const EDGE_Z: &str = "edge_z";
/// A node's relation count — how many rows name it, parallel relations each
/// counted — published so `size(degree)` and `color(degree)` can name it: the
/// one fact an edge table implies about its nodes.
pub const NODE_DEGREE: &str = "degree";

/// The extent description a 2-D `bin` synthesizes, in the form a mark reads
/// extents in. Here rather than at either end so the transform that *writes*
/// these columns and the mark that *reads* them cannot drift apart — the
/// hand-maintained-list-beside-a-generated-one failure, avoided by there being
/// only one list.
pub fn cell_bounds() -> BoundsSpec {
    BoundsSpec {
        start: Some(CELL_START.to_string()),
        end: Some(CELL_END.to_string()),
        lower: Some(CELL_LOWER.to_string()),
        upper: Some(CELL_UPPER.to_string()),
    }
}

/// Cut **both** axes and count what lands in each cell — one `bin`, read in two
/// dimensions, which is the heatmap.
///
/// One row per **non-empty** cell. An empty cell is deliberately not a
/// zero-count rectangle: painting it the bottom of the ramp would claim a
/// measurement nobody made, where leaving it as panel says the data did not go
/// there. That is also what makes a tiling legible — the ragged edge of a
/// binned cloud *is* its support.
///
/// One `BinSpec` cuts both axes, so `bin(30)` means thirty bins each way. A
/// per-axis count is a second knob and is deliberately not invented here (spec
/// §5): the axes of a heatmap are usually the same kind of thing, and the one
/// case that wants them different can say `bin(width = )` in shared units.
///
/// `cut` carries the mesh a facet resolved across its panels, or `Default` to cut
/// these rows alone. Per panel the cells do not line up across the plot, and cells
/// that do not line up are not a mesh — [`bin2d_mixed`] states the rule and
/// [`BinCut`] is what finally makes faceting keep it.
pub fn bin2d(df: &DataFrame, x_field: &str, y_field: &str, spec: Option<&BinSpec>, cut: BinCut) -> DataFrame {
    // **The mixed mesh**, and it is `bin` cuts, `count` tallies read one axis at a
    // time. A `bin` does not need two continuous axes, it needs *something to
    // cut*: the axis that arrives already cut is left alone, because a category is
    // a cell. Dispatched on the column rather than on the plot's axis, since what
    // decides this is whether there is a width there to cut.
    match (df.str_col(x_field).is_some(), df.str_col(y_field).is_some()) {
        // Nothing to cut on either axis — refused fatally by
        // `legality::check_distribution_axis`, which names `count` as the transform
        // that tallies into cells the categories already are.
        (true, true) => return DataFrame::new(),
        (true, false) => return bin2d_mixed(df, x_field, y_field, spec, false, cut.y.as_ref()),
        (false, true) => return bin2d_mixed(df, y_field, x_field, spec, true, cut.x.as_ref()),
        (false, false) => {}
    }

    // Both guards mirror `bin`'s: a categorical axis is refused fatally by
    // `legality::check_distribution_axis` before this runs, and saying anything
    // here would only double a message the user has already read.
    let (Some(xs), Some(ys)) = (df.float_col(x_field), df.float_col(y_field)) else {
        return DataFrame::new();
    };
    let (Some(lx), Some(ly)) = (
        cut.x.or_else(|| bin_layout(xs, spec)),
        cut.y.or_else(|| bin_layout(ys, spec)),
    ) else {
        return DataFrame::new();
    };

    // The mesh. `rect` is the default and everything below is its case; `hex`
    // partitions the same plane a different way, so it is a different function
    // rather than a flag threaded through this one. The value is validated by
    // `legality::check_tiling` before it arrives.
    if spec.and_then(|s| s.tiling.as_deref()) == Some("hex") {
        return bin2d_hex(x_field, y_field, xs, ys, &lx, &ly);
    }

    let cell = |v: f64, l: &BinLayout| -> usize {
        let i = ((v - l.mn) / l.step) as usize;
        // The top edge belongs to the last cell, exactly as in one dimension:
        // the maximum value is *in* the data, so it cannot fall outside the mesh.
        i.min(l.k - 1)
    };

    let mut counts = vec![0u32; lx.k * ly.k];
    for (&vx, &vy) in xs.iter().zip(ys.iter()) {
        if !vx.is_finite() || !vy.is_finite() {
            continue;
        }
        counts[cell(vy, &ly) * lx.k + cell(vx, &lx)] += 1;
    }

    let mut cx = Vec::new();
    let mut cy = Vec::new();
    let mut start = Vec::new();
    let mut end = Vec::new();
    let mut lower = Vec::new();
    let mut upper = Vec::new();
    let mut n = Vec::new();
    for j in 0..ly.k {
        for i in 0..lx.k {
            let c = counts[j * lx.k + i];
            if c == 0 {
                continue;
            }
            let (x0, x1) = (lx.mn + i as f64 * lx.step, lx.mn + (i + 1) as f64 * lx.step);
            let (y0, y1) = (ly.mn + j as f64 * ly.step, ly.mn + (j + 1) as f64 * ly.step);
            cx.push(f64::midpoint(x0, x1));
            cy.push(f64::midpoint(y0, y1));
            start.push(x0);
            end.push(x1);
            lower.push(y0);
            upper.push(y1);
            n.push(c as f64);
        }
    }

    DataFrame::new()
        .with_float(x_field, cx)
        .with_float(y_field, cy)
        .with_float(CELL_START, start)
        .with_float(CELL_END, end)
        .with_float(CELL_LOWER, lower)
        .with_float(CELL_UPPER, upper)
        .with_float(CELL_COUNT, n)
}

/// Cut **one** axis and take the other's cells from the categories — the mixed
/// mesh, which is a distribution per category drawn as cells.
///
/// [`bin2d`] and [`count2d`] are its two ends: cut both axes, or cut neither. The
/// middle case needed no new rule, only the removal of an assumption nobody had
/// stated — that a layer's two axes describe their extents the *same* way. They
/// need not. `bin` cuts the axis with a width to cut and leaves alone the one that
/// arrives already cut, because a category **is** a cell (spec §5).
///
/// **The output contract holds by publishing less**, which is the third
/// combination of a split that already existed rather than a fifth extent
/// description. The cut axis publishes its two edges exactly as a rectangular
/// mesh does; the slotted axis publishes nothing at all, exactly as a tally does,
/// because `build_axis` has already put category *k* at *k* over `[k−½, k+½]`. So
/// the mark reads each axis's extent from wherever that axis's description is,
/// and there is no case in the middle to teach it.
///
/// **One layout across the whole column, never per category.** Per-category
/// cutpoints would give each slot its own edges, and cells that do not line up
/// across the plot are not a mesh — the same rule a grouped histogram already
/// obeys ([`bin_layout`] exists for it), and the same rule faceting obeys with
/// fixed shared scales. It is what lets a reader compare one column of cells
/// straight across.
///
/// That last clause was an aspiration until `cut` existed: faceting shared the
/// *scale* and cut per panel, so the mesh a reader was invited to compare straight
/// across was five different meshes. A panel is a column of cells one dimension
/// out, and it takes the rule this paragraph already states.
///
/// One row per **non-empty** cell, which is [`bin2d`]'s rule for [`bin2d`]'s
/// reason: an empty cell is an absence rather than a zero, and a category whose
/// rows never reach an interval should show panel there, not the floor of the ramp.
fn bin2d_mixed(
    df: &DataFrame, key_field: &str, num_field: &str, spec: Option<&BinSpec>, cut_is_x: bool,
    cut: Option<&BinLayout>,
) -> DataFrame {
    let (Some(keys), Some(vs)) = (df.str_col(key_field), df.float_col(num_field)) else {
        return DataFrame::new();
    };
    let Some(l) = cut.copied().or_else(|| bin_layout(vs, spec)) else {
        return DataFrame::new();
    };
    // The top edge belongs to the last cell, exactly as in one and two dimensions.
    let cell = |v: f64| -> usize { (((v - l.mn) / l.step) as usize).min(l.k - 1) };

    // First-seen order for the categories, re-leveled by `keyed` below — the same
    // treatment every keyed transform gives its key column, so a declared factor's
    // order survives being binned against.
    let mut cats: Vec<String> = Vec::new();
    let mut counts: Vec<u32> = Vec::new();
    for (k, &v) in keys.iter().zip(vs.iter()) {
        if !v.is_finite() {
            continue;
        }
        let ci = match cats.iter().position(|c| c == k) {
            Some(i) => i,
            None => {
                cats.push(k.clone());
                counts.resize(cats.len() * l.k, 0);
                cats.len() - 1
            }
        };
        counts[ci * l.k + cell(v)] += 1;
    }

    let mut key_out = Vec::new();
    let mut center = Vec::new();
    let mut lo = Vec::new();
    let mut hi = Vec::new();
    let mut n = Vec::new();
    for (ci, k) in cats.iter().enumerate() {
        for i in 0..l.k {
            let c = counts[ci * l.k + i];
            if c == 0 {
                continue;
            }
            let (a, b) = (l.mn + i as f64 * l.step, l.mn + (i + 1) as f64 * l.step);
            key_out.push(k.clone());
            center.push(f64::midpoint(a, b));
            lo.push(a);
            hi.push(b);
            n.push(c as f64);
        }
    }

    // Which pair of edge names the cut axis publishes is the only thing that turns
    // on *which* axis was cut — the mark asks the axis, so the names have to say.
    let (lo_name, hi_name) = match cut_is_x {
        true => (CELL_START, CELL_END),
        false => (CELL_LOWER, CELL_UPPER),
    };
    keyed(
        DataFrame::new()
            .with_float(num_field, center)
            .with_float(lo_name, lo)
            .with_float(hi_name, hi)
            .with_float(CELL_COUNT, n),
        key_field, key_out, df,
    )
}

/// Tally rows into the cells **two categorical axes already make** — one `count`,
/// read in two dimensions, which is the tile plot.
///
/// [`bin2d`]'s twin, and the pair says the whole rule: *`bin` cuts, `count`
/// tallies.* A continuous axis has to be cut into cells before anything can be
/// counted in them; a categorical axis arrives already cut, because a category **is**
/// a cell. So the two-dimensional readings divide exactly as the one-dimensional
/// ones always have — `bar * bin` against `bar * count` — and the second dimension
/// changes nothing but where the answer goes: a `bar` has a measure axis to write
/// its tally to, a `zone` does not, so the tally goes to `color` (spec §5).
///
/// **No extent columns, and that is the fourth extent description.** A rectangular
/// mesh publishes four edges, a hexagonal one a center and a half-extent, a level set
/// the curve itself — and a slot publishes **nothing at all**, because the axis
/// already knows: category *k* sits at *k* and owns `[k−½, k+½]`. There is no fact
/// about the cell for the transform to carry that the scale does not hold already,
/// so the honest output is the two key columns and the tally. The extent
/// description that costs no columns is still an extent description.
///
/// One row per **non-empty** pair, which is [`bin2d`]'s rule and for its reason: a
/// pair no row landed on is not a zero, it is an absence, and painting it the floor
/// of the ramp would claim a measurement nobody made. On a confusion matrix that is
/// the difference between "never confused these two" and "scored zero", and the blank
/// cell says the first.
///
/// `share` divides each tally by the total, which is `proportion` — the same rows
/// under [`CELL_SHARE`] instead of [`CELL_COUNT`].
pub fn count2d(df: &DataFrame, x_field: &str, y_field: &str, share: bool) -> DataFrame {
    // Both axes must be categorical, which `check_cell_keys` has already refused
    // otherwise — a number is a point and owns no slot to tally into. Saying
    // anything here would double a message the user has read.
    let (Some(xs), Some(ys)) = (df.str_col(x_field), df.str_col(y_field)) else {
        return DataFrame::new();
    };

    // First-seen order, then re-leveled by `keyed` — the same treatment every
    // keyed transform gives its key column, so a declared factor's order survives
    // being counted (the Law 4 fix `keyed` exists for), on **both** axes.
    let mut keys: Vec<(String, String)> = Vec::new();
    let mut n: Vec<f64> = Vec::new();
    for (a, b) in xs.iter().zip(ys.iter()) {
        match keys.iter().position(|(p, q)| p == a && q == b) {
            Some(i) => n[i] += 1.0,
            None => {
                keys.push((a.clone(), b.clone()));
                n.push(1.0);
            }
        }
    }

    let (measure, total) = match share {
        // Normalized over the **whole frame**, not within a row or a column of the
        // matrix. Which margin normalizes is a real question and it has a real
        // answer here: `proportion` has always meant *this key's share of every
        // row counted*, and a second dimension does not change what the word means.
        // A row-wise confusion matrix (each true class summing to 1) is the same
        // open question the 100% stacked bar asks, and is not answered by giving
        // one transform two meanings depending on the mark.
        true => (CELL_SHARE, n.iter().sum::<f64>().max(1e-12)),
        false => (CELL_COUNT, 1.0),
    };
    let vals: Vec<f64> = n.iter().map(|c| c / total).collect();

    let xk: Vec<String> = keys.iter().map(|(a, _)| a.clone()).collect();
    let yk: Vec<String> = keys.iter().map(|(_, b)| b.clone()).collect();
    keyed(
        keyed(DataFrame::new().with_float(measure, vals), x_field, xk, df),
        y_field, yk, df,
    )
}

/// Reduce `val_field` within each cell **two categorical axes already make** — a
/// value statistic grouped by a *pair* of keys instead of one.
///
/// [`count2d`]'s twin, and the pair says which half of §5's division each belongs
/// to: `count2d` was handed no column, so it tallies rows and publishes its answer
/// under a name of its own; this one *was* handed a column, so it reduces that column
/// and writes the answer back into it. One key or two is a fact about the mark — the
/// positions it does not measure with — never about what `mean` means, which is what
/// Law 2 asks of a transform that gains a dimension.
///
/// **The output has the same shape as [`count2d`]'s and for the same reasons.** Two
/// key columns and a measure; no extent columns at all, because a category owns its
/// slot and the axis already holds it (the fourth extent description); one row per
/// **non-empty** pair, because a pair no row landed on has no mean — not a zero, an
/// absence — and the blank cell says so. What differs is only the third column's
/// name: `val_field`, the column the user named, reduced in place.
pub fn agg2d(df: &DataFrame, x_field: &str, y_field: &str, val_field: &str, agg: AggFn) -> DataFrame {
    // Both axes categorical and the value column numeric — all three already
    // refused otherwise by `check_pair_summary`, so saying anything here would
    // double a message the user has read.
    let (Some(xs), Some(ys), Some(vs)) =
        (df.str_col(x_field), df.str_col(y_field), df.float_col(val_field)) else {
        return DataFrame::new();
    };

    // First-seen order, then re-leveled by `keyed` on **both** axes — the same
    // treatment every keyed transform gives its key column, so a declared factor's
    // order survives being summarized (the Law 4 fix `keyed` exists for).
    //
    // A non-finite value is dropped rather than poisoning its cell: one `NA` would
    // otherwise make the whole cell's mean `NaN` and paint it off the ramp. A cell
    // whose values were *all* non-finite has nothing to reduce, so it is an absent
    // pair like any other and gets no row.
    let mut keys: Vec<(String, String)> = Vec::new();
    let mut cells: Vec<Vec<f64>> = Vec::new();
    for ((a, b), &v) in xs.iter().zip(ys.iter()).zip(vs.iter()) {
        if !v.is_finite() { continue; }
        match keys.iter().position(|(p, q)| p == a && q == b) {
            Some(i) => cells[i].push(v),
            None => {
                keys.push((a.clone(), b.clone()));
                cells.push(vec![v]);
            }
        }
    }

    let vals: Vec<f64> = cells.iter_mut().map(|c| agg.reduce(c)).collect();
    let xk: Vec<String> = keys.iter().map(|(a, _)| a.clone()).collect();
    let yk: Vec<String> = keys.iter().map(|(_, b)| b.clone()).collect();
    keyed(
        keyed(DataFrame::new().with_float(val_field, vals), x_field, xk, df),
        y_field, yk, df,
    )
}

/// How one position axis says where its cells are — the four extent descriptions
/// (spec §5) reduced to the two a *mesh* can be built from, read one axis at a time.
///
/// A cut axis carries a layout and a number per row; a slotted one carries a category
/// per row and the ordered list of categories. Both answer the same two questions —
/// *how many cells* and *which cell is this row in* — so [`bin2d_agg`] below asks
/// them per axis and never learns which combination it was handed. That is what makes
/// the summary heatmap, the mixed summary mesh and the confusion matrix one function:
/// the extent description was always per axis, and only the code that read it was not.
enum CellAxis<'a> {
    Cut(&'a [f64], BinLayout),
    Slot(&'a [String], Vec<String>),
}

impl<'a> CellAxis<'a> {
    /// `cut` is the layout a facet resolved for this axis across its panels; a
    /// slotted axis ignores it, because its cells are the categories and
    /// `categories_across` already reads every panel's frames at once.
    fn of(df: &'a DataFrame, field: &str, spec: Option<&BinSpec>, cut: Option<&BinLayout>) -> Option<Self> {
        if let Some(c) = df.str_col(field) {
            return Some(CellAxis::Slot(c, crate::data::categories_across(&[df], field)));
        }
        let c = df.float_col(field)?;
        cut.copied().or_else(|| bin_layout(c, spec)).map(|l| CellAxis::Cut(c, l))
    }

    fn k(&self) -> usize {
        match self {
            CellAxis::Cut(_, l) => l.k,
            CellAxis::Slot(_, cats) => cats.len(),
        }
    }

    /// Which cell row `i` falls in, or `None` when it falls in none — a non-finite
    /// number, or a category outside the declared levels. Dropping the row is
    /// [`bin2d`]'s rule: the cell it would land in is not the one its data names.
    fn index(&self, i: usize) -> Option<usize> {
        match self {
            CellAxis::Cut(vals, l) => {
                let v = *vals.get(i)?;
                // The top edge belongs to the last cell, exactly as everywhere else:
                // the maximum is *in* the data, so it cannot fall outside the mesh.
                v.is_finite().then(|| (((v - l.mn) / l.step) as usize).min(l.k - 1))
            }
            CellAxis::Slot(vals, cats) => {
                let v = vals.get(i)?;
                cats.iter().position(|c| c == v)
            }
        }
    }
}

/// Cut a plane into cells and reduce a **named column** inside each — the summary
/// heatmap, and [`bin2d`]'s twin exactly as [`agg2d`] is [`count2d`]'s.
///
/// **A cut is an extent description, and the tally was never `bin`'s to keep**
/// (spec §5). `bin2d` answers *where are the cells* and *how many rows are in each*;
/// composed with a value statistic it keeps the first answer and gives up the second,
/// so this function is `bin2d` with `counts[…] += 1` replaced by `cells[…].push(v)`
/// and the same four edge columns published at the end. The mesh does not move — a
/// summary heatmap and the histogram of the same two columns cut identically, which
/// is what lets a reader compare them.
///
/// **One function for three meshes**, because [`CellAxis`] reads each axis's extent
/// description on its own: both axes cut is the summary heatmap, one cut and one
/// slotted is the mixed summary mesh, and neither cut is the tile plot — which is
/// [`agg2d`]'s job and never arrives here, since a layer with no `bin` takes the
/// other branch. The mixed mesh needed no second function this time, which is the
/// 2026-07-25 mixed-mesh ruling paying off rather than being re-derived.
///
/// One row per **non-empty** cell, and a non-finite value dropped rather than
/// poisoning its cell — [`agg2d`]'s two rules, kept for [`agg2d`]'s reasons. A cell
/// nothing landed in has no mean, which is an absence and not a zero.
#[allow(clippy::too_many_arguments)]
pub fn bin2d_agg(
    df: &DataFrame, x_field: &str, y_field: &str, val_field: &str,
    agg: AggFn, spec: Option<&BinSpec>, cut: BinCut,
) -> DataFrame {
    let Some(vs) = df.float_col(val_field) else { return DataFrame::new() };
    let (Some(ax), Some(ay)) = (
        CellAxis::of(df, x_field, spec, cut.x.as_ref()),
        CellAxis::of(df, y_field, spec, cut.y.as_ref()),
    ) else { return DataFrame::new() };

    // A different mesh puts different rows in different cells, so the tiling is
    // asked *before* anything is measured — the same order [`bin2d`] asks it in, and
    // the reason the tiling ruling put the parameter on `bin` rather than on the
    // mark. `legality::check_tiling` has already refused `hex` on anything but a
    // plane, so both axes are cut whenever this branch is reachable.
    if spec.and_then(|s| s.tiling.as_deref()) == Some("hex") {
        if let (CellAxis::Cut(xs, lx), CellAxis::Cut(ys, ly)) = (&ax, &ay) {
            return bin2d_hex_agg(x_field, y_field, val_field, xs, ys, vs, lx, ly, agg);
        }
    }

    let (kx, ky) = (ax.k(), ay.k());
    if kx == 0 || ky == 0 { return DataFrame::new() }

    let mut cells: Vec<Vec<f64>> = vec![Vec::new(); kx * ky];
    for (i, &v) in vs.iter().enumerate() {
        if !v.is_finite() { continue }
        let (Some(cx), Some(cy)) = (ax.index(i), ay.index(i)) else { continue };
        cells[cy * kx + cx].push(v);
    }

    // Two accumulators per axis: the center, which every extent description has and
    // which rides on the plot's own position column, and the pair of edges, which
    // only a *cut* axis publishes — a slot's bounds are already held by the scale
    // (`build_axis` puts category k at k over [k−½, k+½]), so publishing them here
    // would be a second opinion about where a category sits.
    let mut xs_out: Vec<f64> = Vec::new();
    let mut ys_out: Vec<f64> = Vec::new();
    let mut xk_out: Vec<String> = Vec::new();
    let mut yk_out: Vec<String> = Vec::new();
    let (mut start, mut end) = (Vec::new(), Vec::new());
    let (mut lower, mut upper) = (Vec::new(), Vec::new());
    let mut vals: Vec<f64> = Vec::new();

    for j in 0..ky {
        for i in 0..kx {
            let cell = &mut cells[j * kx + i];
            if cell.is_empty() { continue }
            match &ax {
                CellAxis::Cut(_, l) => {
                    let (a, b) = (l.mn + i as f64 * l.step, l.mn + (i + 1) as f64 * l.step);
                    xs_out.push(f64::midpoint(a, b));
                    start.push(a);
                    end.push(b);
                }
                CellAxis::Slot(_, cats) => xk_out.push(cats[i].clone()),
            }
            match &ay {
                CellAxis::Cut(_, l) => {
                    let (a, b) = (l.mn + j as f64 * l.step, l.mn + (j + 1) as f64 * l.step);
                    ys_out.push(f64::midpoint(a, b));
                    lower.push(a);
                    upper.push(b);
                }
                CellAxis::Slot(_, cats) => yk_out.push(cats[j].clone()),
            }
            vals.push(agg.reduce(cell));
        }
    }

    let mut out = DataFrame::new().with_float(val_field, vals);
    out = match &ax {
        CellAxis::Cut(..) => out.with_float(x_field, xs_out)
            .with_float(CELL_START, start).with_float(CELL_END, end),
        CellAxis::Slot(..) => keyed(out, x_field, xk_out, df),
    };
    match &ay {
        CellAxis::Cut(..) => out.with_float(y_field, ys_out)
            .with_float(CELL_LOWER, lower).with_float(CELL_UPPER, upper),
        CellAxis::Slot(..) => keyed(out, y_field, yk_out, df),
    }
}

/// The **two-dimensional form of the pair transforms** — [`agg2d`]'s twin, and the
/// piece that lets a whisker and a box stand in the cube (spec §15).
///
/// `agg2d` reduces a named column to **one** value per cell; these reduce it to a
/// *pair* — a low and a high — with whatever a particular statistic carries beside
/// them (a center for `confidence`, the quartiles and the outliers for `box`). The
/// grouping is identical, and so is the reason it can be: **one key or two is a fact
/// about the mark, never about what `range` means** (Law 2, and the sentence `AggFn`
/// already carries for the five reductions). The per-cell arithmetic is `extents_of`,
/// `ci_of` and `box_stat`, the same functions the one-key forms call.
///
/// Rows come out in the low/high pairing every pair transform uses, so `interval`,
/// `ribbon` and `box` read a cell exactly as they read a slot and no mark learns a
/// second output contract. `box` additionally appends its outlier rows, flagged by a
/// `NaN` in `middle`, which is that mark's existing sentinel.
///
/// A cell whose values are all non-finite has nothing to summarize and gets no row —
/// the absent-pair rule `agg2d` and `count2d` already follow.
pub fn pairs2d(
    df: &DataFrame, x_field: &str, y_field: &str, val_field: &str,
    kind: &Transform, conf: Option<&ConfidenceSpec>, bx: Option<&BoxSpec>,
    rng: Option<&RangeSpec>, dev: Option<&DeviationSpec>,
) -> DataFrame {
    // Both axes categorical and the value column numeric — all three already refused
    // otherwise by `check_pair_summary`, so saying anything here would double a
    // message the user has read.
    let (Some(xs), Some(ys), Some(vs)) =
        (df.str_col(x_field), df.str_col(y_field), df.float_col(val_field)) else {
        return DataFrame::new();
    };

    // First-seen order on both axes, re-leveled by `keyed` below — `agg2d`'s rule,
    // so a declared factor's order survives being summarized.
    let mut keys: Vec<(String, String)> = Vec::new();
    let mut cells: Vec<Vec<f64>> = Vec::new();
    for ((a, b), &v) in xs.iter().zip(ys.iter()).zip(vs.iter()) {
        if !v.is_finite() { continue; }
        match keys.iter().position(|(p, q)| p == a && q == b) {
            Some(i) => cells[i].push(v),
            None => { keys.push((a.clone(), b.clone())); cells.push(vec![v]); }
        }
    }

    let level = conf.and_then(|s| s.level).unwrap_or(0.95);
    let tukey = bx.and_then(|s| s.whiskers.as_deref()) != Some("range");

    let (mut xk, mut yk) = (Vec::new(), Vec::new());
    let (mut vals, mut ctr) = (Vec::new(), Vec::new());
    let (mut lower, mut middle, mut upper) = (Vec::new(), Vec::new(), Vec::new());
    for ((a, b), cell) in keys.iter().zip(cells.iter()) {
        let mut push = |v: f64| { xk.push(a.clone()); yk.push(b.clone()); vals.push(v); };
        match kind {
            Transform::Confidence => {
                let (lo, hi, c) = ci_of(cell, level);
                push(lo); ctr.push(c);
                push(hi); ctr.push(c);
            }
            // The same triple from the same function the one-key reading calls,
            // so a cube's spread band and a panel's report one number (Law 2).
            Transform::Deviation => {
                let (lo, hi, c) = sd_band_of(cell, dev.and_then(|s| s.multiplier).unwrap_or(1.0));
                push(lo); ctr.push(c);
                push(hi); ctr.push(c);
            }
            Transform::Box => {
                let st = box_stat(cell, tukey);
                push(st.w_lo); lower.push(st.q1); middle.push(st.median); upper.push(st.q3);
                push(st.w_hi); lower.push(st.q1); middle.push(st.median); upper.push(st.q3);
                for &o in &st.outliers {
                    push(o);
                    lower.push(f64::NAN); middle.push(f64::NAN); upper.push(f64::NAN);
                }
            }
            // `range`, and the default for anything else that pairs. The band is
            // `range_pair`'s, the same function the one-key reading calls, so a
            // cell and a slot cannot report different quartiles.
            _ => {
                let (lo, hi) = range_pair(cell, rng);
                push(lo);
                push(hi);
            }
        }
    }

    let mut out = DataFrame::new().with_float(val_field, vals);
    if !ctr.is_empty() { out = out.with_float("center", ctr); }
    if !lower.is_empty() {
        out = out.with_float("lower", lower).with_float("middle", middle).with_float("upper", upper);
    }
    keyed(keyed(out, x_field, xk, df), y_field, yk, df)
}

/// Cut the plane into **hexagons** and count what lands in each — Carr's tiling
/// (1987), and the reason it exists is a defect in the rectangular one.
///
/// A square mesh lines its cell centers up in rows *and* columns, and the eye
/// reads that regularity as if it were in the data. A hexagonal mesh staggers
/// alternate rows, so no such lattice is there to be seen. Wilkinson states it
/// exactly: "rectangular bins lead the eye to align bin centers and to see
/// regularity where there is none."
///
/// **Pointy-top hexagons**, matching Carr's `hexbin` and every implementation
/// since, so a reader who knows the plot recognizes it.
///
/// *The assignment is two interleaved rectangular lattices*, which is the
/// standard trick and is exact rather than approximate: hexagon centers are
/// precisely the union of a lattice at the integers and one offset by half a
/// step in both directions, so "which hexagon contains this point" is "which of
/// those two candidate centers is nearer". The metric weights the vertical
/// difference by 3 because the working space is stretched: one `sy` unit is
/// √3 `sx` units, which is what makes the two lattices interleave into a
/// *regular* hexagonal one rather than a squashed one.
///
/// *Regular on the page when the panel is square.* Both axes are normalized to
/// the same number of steps, so a hexagon is regular in that space and the panel
/// then stretches it by whatever the panel's own aspect is. This is what `hexbin`
/// exposes as `shape` and what ggplot2 does silently; the honest fix is a
/// square-panel hint in the layout, already parked for `polar`, which wants the
/// identical thing for its circles.
/// The hexagonal mesh itself — which cell a point falls in, and how big a cell is —
/// held apart from what gets *measured* in it.
///
/// Extracted when the summary heatmap gained its hexagonal reading (2026-07-26). A
/// tally and a reduction must land on the **same** mesh or the two plots of one pair
/// of columns cannot be compared, and two copies of this arithmetic is two chances to
/// disagree about where a hexagon is — the hand-maintained-list-beside-a-generated-one
/// failure, in geometry. `CELL_START`'s contract is the rectangular version of the
/// same care.
struct HexMesh {
    cx_unit: f64,
    cy_unit: f64,
    mn_x: f64,
    mn_y: f64,
}

impl HexMesh {
    fn of(lx: &BinLayout, ly: &BinLayout) -> Self {
        let sqrt3 = 3.0_f64.sqrt();
        // One step across x is one bin; one step up y is √3 bins, which is what puts
        // the two lattices at the right vertical spacing to interleave regularly.
        let k = lx.k.max(1) as f64;
        let x_span = (lx.step * lx.k as f64).max(1e-12);
        let y_span = (ly.step * ly.k as f64).max(1e-12);
        HexMesh {
            cx_unit: x_span / k,          // data units per `sx` step
            cy_unit: y_span * sqrt3 / k,  // data units per `sy` step
            mn_x: lx.mn,
            mn_y: ly.mn,
        }
    }

    /// Which hexagon `(vx, vy)` falls in, keyed on **doubled** lattice coordinates so
    /// a half-step is still an integer and the two interleaved lattices share one map
    /// with no rounding to argue about.
    fn key(&self, vx: f64, vy: f64) -> Option<(i64, i64)> {
        if !vx.is_finite() || !vy.is_finite() {
            return None;
        }
        let sx = (vx - self.mn_x) / self.cx_unit;
        let sy = (vy - self.mn_y) / self.cy_unit;

        // Lattice A: centers on the integers.
        let (ja, ia) = (sx.round(), sy.round());
        let da = (sx - ja).powi(2) + 3.0 * (sy - ia).powi(2);
        // Lattice B: centers on the half-steps of both axes.
        let (jb, ib) = (sx.floor() + 0.5, sy.floor() + 0.5);
        let db = (sx - jb).powi(2) + 3.0 * (sy - ib).powi(2);

        Some(if da <= db {
            ((ja * 2.0) as i64, (ia * 2.0) as i64)
        } else {
            ((jb * 2.0) as i64, (ib * 2.0) as i64)
        })
    }

    fn center(&self, (j, i): (i64, i64)) -> (f64, f64) {
        (self.mn_x + (j as f64 / 2.0) * self.cx_unit,
         self.mn_y + (i as f64 / 2.0) * self.cy_unit)
    }

    /// A hexagon's own size, in each column's own units. Half the horizontal
    /// spacing across, and a third of an `sy` step up — the pointy-top geometry,
    /// where the circumradius is width/√3 and the top vertex sits one of those
    /// above the center.
    fn half_extent(&self) -> (f64, f64) {
        (self.cx_unit / 2.0, self.cy_unit / 3.0)
    }

    /// Row-major, so the emitted order is stable across runs — a `HashMap`'s is not,
    /// and an unstable order would make two renders of one spec differ.
    fn ordered(keys: impl Iterator<Item = (i64, i64)>) -> Vec<(i64, i64)> {
        let mut ks: Vec<(i64, i64)> = keys.collect();
        ks.sort_unstable_by_key(|&(j, i)| (i, j));
        ks
    }

    /// The four columns every hexagonal cell publishes, whatever was measured in it.
    fn frame(&self, x_field: &str, y_field: &str, keys: &[(i64, i64)]) -> DataFrame {
        let (cxs, cys): (Vec<f64>, Vec<f64>) = keys.iter().map(|&k| self.center(k)).unzip();
        let (dx, dy) = self.half_extent();
        let n = keys.len();
        DataFrame::new()
            .with_float(x_field, cxs.clone())
            .with_float(y_field, cys.clone())
            .with_float(CELL_X, cxs)
            .with_float(CELL_Y, cys)
            .with_float(CELL_DX, vec![dx; n])
            .with_float(CELL_DY, vec![dy; n])
    }
}

fn bin2d_hex(
    x_field: &str, y_field: &str, xs: &[f64], ys: &[f64],
    lx: &BinLayout, ly: &BinLayout,
) -> DataFrame {
    let mesh = HexMesh::of(lx, ly);
    let mut cells: std::collections::HashMap<(i64, i64), u32> = std::collections::HashMap::new();
    for (&vx, &vy) in xs.iter().zip(ys.iter()) {
        if let Some(key) = mesh.key(vx, vy) {
            *cells.entry(key).or_insert(0) += 1;
        }
    }
    let keys = HexMesh::ordered(cells.keys().copied());
    let counts: Vec<f64> = keys.iter().map(|k| cells[k] as f64).collect();
    mesh.frame(x_field, y_field, &keys).with_float(CELL_COUNT, counts)
}

/// [`bin2d_hex`]'s reducing twin — the summary heatmap on a hexagonal mesh.
///
/// The same relation [`bin2d_agg`] has to [`bin2d`], one tiling over, and the
/// evidence that the tiling ruling still holds under composition: a different mesh
/// puts different rows in different cells, and it does that identically whether the
/// cell is then counted or reduced. So the mesh is [`HexMesh`] in both and only the
/// accumulator differs — `u32` against `Vec<f64>`.
///
/// One row per **non-empty** cell, and a non-finite value dropped rather than
/// poisoning its cell, exactly as in the rectangular reading.
fn bin2d_hex_agg(
    x_field: &str, y_field: &str, val_field: &str, xs: &[f64], ys: &[f64], vs: &[f64],
    lx: &BinLayout, ly: &BinLayout, agg: AggFn,
) -> DataFrame {
    let mesh = HexMesh::of(lx, ly);
    let mut cells: std::collections::HashMap<(i64, i64), Vec<f64>> = std::collections::HashMap::new();
    for ((&vx, &vy), &v) in xs.iter().zip(ys.iter()).zip(vs.iter()) {
        if !v.is_finite() { continue }
        if let Some(key) = mesh.key(vx, vy) {
            cells.entry(key).or_default().push(v);
        }
    }
    let keys = HexMesh::ordered(cells.keys().copied());
    let vals: Vec<f64> = keys.iter()
        .map(|k| agg.reduce(cells.get_mut(k).expect("key came from the map"))).collect();
    mesh.frame(x_field, y_field, &keys).with_float(val_field, vals)
}

// ---------------------------------------------------------------------------
// density in two dimensions — one field, and the two geometries a mark draws it as
//
// The dimensionality is read off the **mark**, never asked for (spec §5): a `line`
// leaves an axis free to measure the density along, so `line * density` estimates
// one; a `path` and a `zone` have no measure axis, so `density` cuts both and the
// measure goes to `color`. That is the same sentence `bin` is read by one section
// up, and the same rule decides both.
//
// What each mark then does with the field is the `step` ruling — the mark chooses
// the geometry, the transform stays constant. A `zone` draws a field as *cells*
// (`density2d_cells`, the smooth heatmap); a `path` draws it as its *iso-lines*
// (`density2d_contour`, the contour plot), because a path is the mark that strokes
// vertices in the order given, and a contour ring is a closed curve that doubles
// back in x. A `line` would sort that ring into a zigzag, which is why
// `legality::mark_takes_transform` sends the 2-D reading to `path` and not there.
// ---------------------------------------------------------------------------

/// What a 2-D `density` measured in each cell. `bin`'s [`CELL_COUNT`] one reading
/// over: named plainly because it is the column a color legend titles itself from,
/// and the one a user may say out loud as `color(density)`.
pub const FIELD_DENSITY: &str = "density";

/// The iso-value a contour ring was traced at.
///
/// **Numeric**, which is what makes it read correctly through the sequential ramp a
/// stroke's `color` takes when it is handed a measure: every vertex of one ring
/// carries the same level, so the ring comes out one uniform color off the ramp and
/// the legend is a color bar the reader decodes outward-in. A level *is* a quantity
/// — the density the line was cut at — so a categorical hue per level would be
/// throwing the ordering away and asking the reader to recover it from the legend's
/// row order.
pub const FIELD_LEVEL: &str = "level";

/// Which ring a contour vertex belongs to — plumbing, like [`CELL_START`] and
/// `stack_base`, shared by the transform that writes it and the mark that reads it.
///
/// It exists because *level* is not fine enough to stroke by: one level can enclose
/// two separate modes, and joining those two rings into one polyline would draw a
/// segment across the valley between them. So the ring is what breaks the stroke
/// and the level is what colors it — two different questions, which is why they
/// are two columns rather than one compound key.
pub const FIELD_RING: &str = "contour_ring";

/// How many grid points each axis of the field is evaluated on.
///
/// 64 is the resolution at which a traced ring stops looking polygonal at ordinary
/// panel sizes; the cost is 64² evaluations, each over a bounded window of rows, so
/// it does not scale with the data the way the naive product would.
const GRID: usize = 64;

/// Which column a two-dimensional reading measured its cells by — the name `color`
/// titles itself from, and the one a user may name out loud. `bin` and `count` tally
/// rows; `density` estimates a value; `proportion` is the tally as a share. One
/// function so the mark, the legend and the legality check cannot disagree about
/// what a heatmap is colored by.
///
/// **The four are one class, and it is not "distributional".** What they share is
/// that each **invents its own measurement**: none is handed a measured column to
/// reduce, so each publishes its answer under a name of its own (`count`, `density`,
/// `proportion`) that no column in the user's table had. Every *other* value
/// statistic (`mean`, `sum`, `median`, `max`, `min`) reduces a column the user
/// **names** and writes the answer back into it, so its measure column is the user's
/// own — which is why those five answer `None` here rather than being absent: this
/// function names a *synthesized* column, and they synthesize none.
///
/// The five are read in two dimensions too, by [`agg2d`] and [`reduces_column`]
/// beside it. What decides which of the two functions a layer asks is not the
/// dimension but the class: *were you handed a column to reduce?*
/// **A composed `bin` synthesizes nothing**, so it answers `None` here along with the
/// five. Cut and handed to a statistic it supplies only the extent, and the
/// measurement lands in the column the *user* named — which is what every caller of
/// this function is really asking for, since each of them wants to know *where the
/// number is*, not which transform is present. Reading it the other way is what let
/// `zone * bin * mean` route to the counting branch and paint a tally under a legend
/// titled for a column nobody had reduced. The other three cannot be composed at all
/// (`legality::check_chain_jobs`), so they need no such reading.
pub fn cell_measure(transforms: &[Transform]) -> Option<&'static str> {
    if transforms.contains(&Transform::Bin) && measures_a_column(transforms) {
        None
    // `proportion` is asked **first**, because it is a normalizer and so has the last
    // word on what the number is: composed with a `bin` or a `count` the cells hold
    // shares, not tallies, and reading the list in transform order gave `bin` the
    // answer and painted a legend titled `count` over a column of fractions. Composed
    // with one of the five it answers `None` along with them — the measurement is in
    // the user's own column, rescaled in place, so there is no synthesized name.
    } else if transforms.contains(&Transform::Proportion) {
        if measures_a_column(transforms) { None } else { Some(CELL_SHARE) }
    } else if transforms.contains(&Transform::Bin) || transforms.contains(&Transform::Count) {
        Some(CELL_COUNT)
    } else if transforms.contains(&Transform::Density) {
        Some(FIELD_DENSITY)
    } else {
        None
    }
}

/// Does this transform sequence measure **into cells**, so that on a mark with no
/// measure axis it reads in two dimensions? The question `svg.rs` asks to route a
/// layer, and `zone.rs` asks to know its measurement was made for it.
///
/// Named for the cells rather than for a *field*, which it was called until the tile
/// plot: a `bin` cuts a continuous plane into a mesh and a `density` estimates over
/// one, but a `count` tallies into cells the **categories** already are, and calling
/// that a field would be calling a confusion matrix a density estimate.
pub fn measures_cells(transforms: &[Transform]) -> bool {
    cell_measure(transforms).is_some()
}

/// Run a whole-frame transform separately inside each group, then stack the results
/// back into one frame tagged with the group.
///
/// [`apply_grouped`]'s shape, for the transforms that do **not** go through
/// [`apply`]: a two-dimensional reading is dispatched by the mark (see the note
/// above), so it never passes the per-group split every statistic in `apply` gets for
/// free. Without this a `group` binding on a contour was legal grammar that drew
/// nothing at all — the group column is absent from the transform's output, so the
/// mark writer found no series and returned. A silent drop, which §12 forbids.
///
/// A group's rings are numbered from one again, and that is deliberate: the mark
/// splits a stroke on *runs* of the ring column, and each group's rows are
/// contiguous, so ring 1 of one group can never be joined to ring 1 of another.
pub fn by_group(
    df: &DataFrame,
    group_field: Option<&str>,
    f: impl Fn(&DataFrame) -> DataFrame,
) -> DataFrame {
    let Some(g) = group_field.filter(|g| df.str_col(g).is_some_and(|c| !c.is_empty())) else {
        return f(df);
    };
    // A declared factor's order carries onto the output, as everywhere else.
    let levels = df.levels(g).map(<[String]>::to_vec);
    let parts: Vec<DataFrame> = crate::data::categories_across(&[df], g)
        .iter()
        .filter_map(|gv| {
            let sub = df.filter_str_eq(g, gv);
            if sub.is_empty() { return None; }
            let res = f(&sub);
            let n = res.len();
            if n == 0 { return None; }
            let tag = vec![gv.clone(); n];
            Some(match &levels {
                Some(lv) => res.with_levels(g, tag, lv.clone()),
                None => res.with_str(g, tag),
            })
        })
        .collect();
    DataFrame::vconcat(&parts)
}

/// A scalar field on a regular grid — what a two-dimensional `density` estimates,
/// before any mark has said how to draw it.
struct Field {
    nx: usize,
    ny: usize,
    x0: f64,
    y0: f64,
    dx: f64,
    dy: f64,
    /// Row-major in y: `z[j * nx + i]` is the density at grid point *(i, j)*.
    z: Vec<f64>,
}

impl Field {
    fn at(&self, i: usize, j: usize) -> f64 { self.z[j * self.nx + i] }
    fn x(&self, i: usize) -> f64 { self.x0 + i as f64 * self.dx }
    fn y(&self, j: usize) -> f64 { self.y0 + j as f64 * self.dy }
    fn max(&self) -> f64 { self.z.iter().copied().fold(0.0, f64::max) }
}

/// Estimate the density over the plane — a product Gaussian kernel on a
/// [`GRID`]×[`GRID`] lattice, with a per-axis Silverman bandwidth.
///
/// **The kernel is a product of the two axes' kernels**, which is what lets each
/// axis carry its own bandwidth in its own units. The alternative — one radial
/// kernel — would need a single length scale, and the two columns of a scatter
/// rarely measure the same quantity.
///
/// **The grid reaches 3 bandwidths past the data** on every side, as the 1-D curve
/// does, so the outermost iso-line closes inside the lattice instead of being cut
/// off at the data's own extreme.
///
/// **Each row is accumulated into a bounded window** rather than every grid point
/// summing over every row: past four bandwidths a Gaussian contributes less than a
/// ten-thousandth of its peak, so the window is where the arithmetic is, and the
/// cost stops being *grid × rows*.
fn kde2d(xs: &[f64], ys: &[f64], spec: Option<&DensitySpec>) -> Option<Field> {
    let rows: Vec<(f64, f64)> = xs.iter().zip(ys.iter())
        .filter(|(a, b)| a.is_finite() && b.is_finite())
        .map(|(&a, &b)| (a, b))
        .collect();
    if rows.len() < 2 { return None; }

    let col = |f: fn(&(f64, f64)) -> f64| -> Vec<f64> { rows.iter().map(f).collect() };
    let (cx, cy) = (col(|r| r.0), col(|r| r.1));
    let (hx, hy) = (bandwidth(&cx, 2, spec), bandwidth(&cy, 2, spec));

    let span = |v: &[f64], h: f64| -> (f64, f64) {
        let mn = v.iter().copied().fold(f64::INFINITY, f64::min) - 3.0 * h;
        let mx = v.iter().copied().fold(f64::NEG_INFINITY, f64::max) + 3.0 * h;
        (mn, mx)
    };
    let (x0, x1) = span(&cx, hx);
    let (y0, y1) = span(&cy, hy);
    if !(x1 > x0) || !(y1 > y0) { return None; }

    let (nx, ny) = (GRID, GRID);
    let dx = (x1 - x0) / (nx - 1) as f64;
    let dy = (y1 - y0) / (ny - 1) as f64;
    let mut z = vec![0.0_f64; nx * ny];

    // Four bandwidths of reach, in grid steps, on each axis.
    let wx = ((4.0 * hx / dx).ceil() as usize).max(1);
    let wy = ((4.0 * hy / dy).ceil() as usize).max(1);
    for &(px, py) in &rows {
        let ci = ((px - x0) / dx).round();
        let cj = ((py - y0) / dy).round();
        if !ci.is_finite() || !cj.is_finite() { continue; }
        let (ci, cj) = (ci as isize, cj as isize);
        let i_lo = (ci - wx as isize).max(0) as usize;
        let i_hi = ((ci + wx as isize).max(0) as usize).min(nx - 1);
        let j_lo = (cj - wy as isize).max(0) as usize;
        let j_hi = ((cj + wy as isize).max(0) as usize).min(ny - 1);
        for j in j_lo..=j_hi {
            let v = (y0 + j as f64 * dy - py) / hy;
            let ky = (-0.5 * v * v).exp();
            for i in i_lo..=i_hi {
                let u = (x0 + i as f64 * dx - px) / hx;
                z[j * nx + i] += ky * (-0.5 * u * u).exp();
            }
        }
    }

    // Normalize to a density: each kernel integrates to 2π·hx·hy, and there are
    // `n` of them. The scale never changes a contour's *shape* — the levels are
    // fractions of the peak — but it is what makes `color(density)` a number in
    // the two columns' own units rather than an arbitrary tally.
    let norm = rows.len() as f64 * 2.0 * std::f64::consts::PI * hx * hy;
    for v in &mut z { *v /= norm; }

    Some(Field { nx, ny, x0, y0, dx, dy, z })
}

/// How many iso-lines to trace, and at what values.
///
/// **Equal fractions of the peak**, `i/(k+1)` for *i* in 1..=k, which is the choice
/// that always yields exactly *k* non-empty rings: a level above the peak encloses
/// nothing, and a level at zero encloses the whole grid. Levels are returned
/// ascending, so the outermost ring is traced first and the legend reads outward-in.
fn contour_levels(peak: f64, count: usize) -> Vec<f64> {
    if !(peak > 0.0) { return Vec::new(); }
    (1..=count).map(|i| peak * i as f64 / (count + 1) as f64).collect()
}

/// One line segment of an iso-line, as two endpoints on cell edges.
type Seg = ((f64, f64), (f64, f64));

/// Marching squares over one cell of the field, at one level.
///
/// Each of the four edges is interpolated in a **canonical direction** — bottom and
/// top both run left-to-right, left and right both run bottom-to-top — so the
/// crossing two neighboring cells share is computed from the same two corners in
/// the same order, and comes out bit-identical. That is what lets [`chain`] join
/// segments by exact equality instead of by an epsilon nobody could justify.
///
/// The two **saddle** cases (opposite corners above the level) are resolved by the
/// cell's own average: it says whether the high ground connects through the middle
/// or the low ground does, which is the only information the cell holds. Guessing
/// one way always would put a visible notch in every ring that crossed a saddle.
fn cell_segments(f: &Field, i: usize, j: usize, level: f64) -> Vec<Seg> {
    let (va, vb, vc, vd) = (f.at(i, j), f.at(i + 1, j), f.at(i + 1, j + 1), f.at(i, j + 1));
    let mask = (va > level) as u8
        | (((vb > level) as u8) << 1)
        | (((vc > level) as u8) << 2)
        | (((vd > level) as u8) << 3);
    if mask == 0 || mask == 15 { return Vec::new(); }

    let (xi, xj) = (f.x(i), f.x(i + 1));
    let (yi, yj) = (f.y(j), f.y(j + 1));
    let lerp = |v1: f64, v2: f64| -> f64 {
        let d = v2 - v1;
        if d.abs() < 1e-300 { 0.5 } else { ((level - v1) / d).clamp(0.0, 1.0) }
    };
    let bottom = || (xi + lerp(va, vb) * (xj - xi), yi);
    let top    = || (xi + lerp(vd, vc) * (xj - xi), yj);
    let left   = || (xi, yi + lerp(va, vd) * (yj - yi));
    let right  = || (xj, yi + lerp(vb, vc) * (yj - yi));

    match mask {
        1 | 14 => vec![(left(), bottom())],
        2 | 13 => vec![(bottom(), right())],
        3 | 12 => vec![(left(), right())],
        4 | 11 => vec![(right(), top())],
        6 | 9  => vec![(bottom(), top())],
        7 | 8  => vec![(left(), top())],
        // The saddles. `mask == 5` is the low-left/high-right diagonal, `10` the
        // other; in both, whether the middle belongs to the high ground decides
        // which pair of corners the two segments cut off.
        _ => {
            let middle_high = (va + vb + vc + vd) / 4.0 > level;
            let joined = (mask == 5) == middle_high;
            if joined {
                vec![(bottom(), right()), (left(), top())]
            } else {
                vec![(left(), bottom()), (right(), top())]
            }
        }
    }
}

/// Join loose segments into polylines, following each one end to end.
///
/// Endpoints are matched on their **exact bits**, which is sound because
/// [`cell_segments`] interpolates every shared edge identically from both sides —
/// see its note. A walk that comes back to where it started is a closed ring; one
/// that runs out of segments is an open line ending at the grid's edge. Both are
/// legitimate contours, and the caller draws them the same way.
fn chain(segs: &[Seg]) -> Vec<Vec<(f64, f64)>> {
    let key = |p: (f64, f64)| (p.0.to_bits(), p.1.to_bits());
    let mut at: std::collections::HashMap<(u64, u64), Vec<usize>> = std::collections::HashMap::new();
    for (idx, &(p, q)) in segs.iter().enumerate() {
        at.entry(key(p)).or_default().push(idx);
        at.entry(key(q)).or_default().push(idx);
    }

    let mut used = vec![false; segs.len()];
    let mut out = Vec::new();
    for start in 0..segs.len() {
        if used[start] { continue; }
        used[start] = true;
        let mut poly = vec![segs[start].0, segs[start].1];

        // Forward, then backward from the other end — an open line can be entered
        // anywhere along it, so one direction is not enough.
        for forward in [true, false] {
            loop {
                let tip = if forward { *poly.last().unwrap() } else { poly[0] };
                if poly.len() > 2 && key(tip) == key(if forward { poly[0] } else { *poly.last().unwrap() }) {
                    break; // already closed
                }
                let Some(next) = at.get(&key(tip)).and_then(|v| v.iter().copied().find(|&i| !used[i]))
                else { break };
                used[next] = true;
                let (p, q) = segs[next];
                let far = if key(p) == key(tip) { q } else { p };
                if forward { poly.push(far) } else { poly.insert(0, far) }
                if key(far) == key(poly[0]) { break; }
            }
        }
        if poly.len() >= 2 { out.push(poly); }
    }
    out
}

/// Trace the field's iso-lines — the contour plot, and what a `path` draws a
/// two-dimensional `density` as.
///
/// One row per **vertex**, carrying the ring it belongs to ([`FIELD_RING`], which
/// breaks the stroke) and the level it was traced at ([`FIELD_LEVEL`], which
/// colors it). Adds **no channel** to the kernel: the level rides a synthesized
/// column exactly as a 2-D bin's count does, unbound by default and namable out
/// loud as `color(level)`.
pub fn density2d_contour(
    df: &DataFrame, x_field: &str, y_field: &str, spec: Option<&DensitySpec>,
) -> DataFrame {
    // A categorical axis is refused fatally by `legality::check_distribution_axis`
    // before this runs; saying it again here would only double a message the user
    // has read (the guard `bin2d` keeps, for the same reason).
    let (Some(xs), Some(ys)) = (df.float_col(x_field), df.float_col(y_field)) else {
        return DataFrame::new();
    };
    let Some(field) = kde2d(xs, ys, spec) else { return DataFrame::new() };

    let count = spec.and_then(|s| s.levels).unwrap_or(crate::ir::DEFAULT_LEVELS);
    let levels = contour_levels(field.max(), count);

    let (mut vx, mut vy, mut ring, mut at) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let mut rings = 0.0_f64;
    for &level in &levels {
        let mut segs = Vec::new();
        for j in 0..field.ny - 1 {
            for i in 0..field.nx - 1 {
                segs.extend(cell_segments(&field, i, j, level));
            }
        }
        for poly in chain(&segs) {
            rings += 1.0;
            for (px, py) in poly {
                vx.push(px);
                vy.push(py);
                ring.push(rings);
                at.push(level);
            }
        }
    }

    DataFrame::new()
        .with_float(x_field, vx)
        .with_float(y_field, vy)
        .with_float(FIELD_RING, ring)
        .with_float(FIELD_LEVEL, at)
}

/// Paint the field's cells — the smooth heatmap, and what a `zone` draws a
/// two-dimensional `density` as.
///
/// The rectangular extent contract, unchanged from `bin2d`: a center on the two
/// position columns and four edges beside it, read through [`cell_bounds`]. A grid
/// cell *is* a rectangle, so it takes the rectangular form rather than the
/// center-and-half-extent pair `hex` needed — which is the output contract doing
/// its job a third time.
///
/// **Every cell is emitted, including the faint ones**, and that is where it parts
/// from `bin2d` deliberately. A bin *counts rows*, so an empty cell means the data
/// did not go there and painting it the floor of the ramp would claim a measurement
/// nobody made. A density *estimates a value*, and the estimate exists at every
/// point of the plane — there is no such thing as a cell the estimator had no
/// opinion about, so leaving one out would be the omission rather than the honesty.
pub fn density2d_cells(
    df: &DataFrame, x_field: &str, y_field: &str, spec: Option<&DensitySpec>,
) -> DataFrame {
    let (Some(xs), Some(ys)) = (df.float_col(x_field), df.float_col(y_field)) else {
        return DataFrame::new();
    };
    let Some(f) = kde2d(xs, ys, spec) else { return DataFrame::new() };

    let n = (f.nx - 1) * (f.ny - 1);
    let mut cx = Vec::with_capacity(n);
    let mut cy = Vec::with_capacity(n);
    let mut start = Vec::with_capacity(n);
    let mut end = Vec::with_capacity(n);
    let mut lower = Vec::with_capacity(n);
    let mut upper = Vec::with_capacity(n);
    let mut val = Vec::with_capacity(n);
    for j in 0..f.ny - 1 {
        for i in 0..f.nx - 1 {
            let (x0, x1) = (f.x(i), f.x(i + 1));
            let (y0, y1) = (f.y(j), f.y(j + 1));
            cx.push(f64::midpoint(x0, x1));
            cy.push(f64::midpoint(y0, y1));
            start.push(x0);
            end.push(x1);
            lower.push(y0);
            upper.push(y1);
            // The cell's value is its four corners' mean — the field is sampled at
            // grid *points*, and a cell spans four of them.
            val.push((f.at(i, j) + f.at(i + 1, j) + f.at(i, j + 1) + f.at(i + 1, j + 1)) / 4.0);
        }
    }

    DataFrame::new()
        .with_float(x_field, cx)
        .with_float(y_field, cy)
        .with_float(CELL_START, start)
        .with_float(CELL_END, end)
        .with_float(CELL_LOWER, lower)
        .with_float(CELL_UPPER, upper)
        .with_float(FIELD_DENSITY, val)
}

// ---------------------------------------------------------------------------
// count — frequency aggregation
// ---------------------------------------------------------------------------

/// Count occurrences of each unique value in `x_field`.
/// Works on both string and numeric columns.
/// Outputs unique values → `x_field`, counts → `y_field`.
fn count(df: &DataFrame, x_field: &str, y_field: &str) -> DataFrame {
    // No position axis: one tally for the whole frame. `apply_grouped` has already
    // split by the color split when there is one, so "one value for these rows"
    // *is* one value per group — a keyless `bar * count + color(cut)` is one bar
    // per cut, all in the single slot (spec §15).
    if x_field.is_empty() {
        return DataFrame::new().with_float(y_field, vec![df.len() as f64]);
    }

    // String x column
    if let Some(xs) = df.str_col(x_field) {
        let mut entries: Vec<(String, f64)> = Vec::new();
        for x in xs {
            if let Some(e) = entries.iter_mut().find(|(k, _)| k == x) {
                e.1 += 1.0;
            } else {
                entries.push((x.clone(), 1.0));
            }
        }
        let keys:   Vec<String> = entries.iter().map(|(k, _)| k.clone()).collect();
        let counts: Vec<f64>    = entries.iter().map(|(_, c)| *c).collect();
        return keyed(DataFrame::new().with_float(y_field, counts), x_field, keys, df);
    }

    // Numeric x column
    if let Some(xs) = df.float_col(x_field) {
        let mut entries: Vec<(f64, f64)> = Vec::new();
        for &x in xs {
            if let Some(e) = entries.iter_mut().find(|(k, _)| (*k - x).abs() < 1e-12) {
                e.1 += 1.0;
            } else {
                entries.push((x, 1.0));
            }
        }
        entries.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        let keys:   Vec<f64> = entries.iter().map(|(k, _)| *k).collect();
        let counts: Vec<f64> = entries.iter().map(|(_, c)| *c).collect();
        return DataFrame::new().with_float(x_field, keys).with_float(y_field, counts);
    }

    df.clone()
}

// ---------------------------------------------------------------------------
// proportion — the tally a normalizer falls back to when nothing else measured
// ---------------------------------------------------------------------------

/// `proportion`'s fallback measurement: tally the rows, exactly as [`count`] does.
///
/// **The division is not here**, and that is the whole of what makes `proportion` a
/// normalizer rather than a fifth synthesizing transform (spec §5). It runs once in
/// [`apply`], over the recombined frame, so a share is a fraction of the **whole
/// frame** in every context — with a `color` split bound or without one, and
/// whether the number being rescaled is this tally or a `bin`'s or a `sum`'s.
///
/// Dividing *here* is what made the word mean two things: inside the per-group split
/// each group could see only its own rows, so a keyed `proportion` normalized within
/// its group and `bar * proportion + x(direction) + color(season)` drew a plot
/// summing to 2. The keyless reading had always deferred the division to `apply` for
/// exactly this reason; the keyed one now does the same thing, which is one rule
/// instead of two readings of it.
fn proportion(df: &DataFrame, x_field: &str, y_field: &str) -> DataFrame {
    // No position axis: the whole group is one number, its share of the frame.
    if x_field.is_empty() {
        return DataFrame::new().with_float(y_field, vec![df.len() as f64]);
    }
    count(df, x_field, y_field)
}

// ---------------------------------------------------------------------------
// smooth — LOESS (locally weighted scatter-plot smoother)
// ---------------------------------------------------------------------------

/// Fit a LOESS curve through `(x_field, y_field)` with span = 0.75 and
/// evaluate it at 100 evenly spaced x points.  Uses a tricube kernel and
/// local linear regression at each evaluation point.
fn smooth(df: &DataFrame, x_field: &str, y_field: &str) -> DataFrame {
    // Both axes categorical-checked by `legality::check_distribution_axis` (fatally,
    // and before this runs); see the note on `bin`. Returning the input unchanged is
    // the honest fallback for `GOG_STRICT=0` — but it is *why* this had to move: the
    // raw scatter drawn as if it were the fitted curve looks finished.
    if df.str_col(x_field).is_some() || df.str_col(y_field).is_some() {
        return df.clone();
    }
    let Some(xs) = df.float_col(x_field) else { return df.clone() };
    let Some(ys) = df.float_col(y_field) else { return df.clone() };
    let n = xs.len().min(ys.len());
    if n < 3 { return df.clone(); }

    // Sort by x
    let mut pts: Vec<(f64, f64)> = xs[..n].iter()
        .zip(ys[..n].iter())
        .map(|(&x, &y)| (x, y))
        .collect();
    pts.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    let span    = 0.75_f64;
    let window  = ((n as f64 * span).ceil() as usize).max(2).min(n);
    let x_lo    = pts[0].0;
    let x_hi    = pts[n - 1].0;
    let m       = 100_usize;

    let smooth_x: Vec<f64> = (0..m)
        .map(|i| x_lo + (i as f64 / (m - 1) as f64) * (x_hi - x_lo))
        .collect();

    let smooth_y: Vec<f64> = smooth_x.iter().map(|&xi| {
        // Find the `window` nearest points by |x − xi|
        let mut dists: Vec<(f64, usize)> = pts.iter()
            .enumerate()
            .map(|(i, (px, _))| ((px - xi).abs(), i))
            .collect();
        dists.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

        let h = dists[window - 1].0.max(1e-12);

        // Weighted linear regression: y = a + b·x, weights via tricube kernel
        let (mut sw, mut swx, mut swy, mut swxx, mut swxy) = (0.0, 0.0, 0.0, 0.0, 0.0);
        for &(_, idx) in &dists[..window] {
            let u = (pts[idx].0 - xi).abs() / h;
            let w = (1.0 - u.powi(3)).powi(3).max(0.0);
            let (px, py) = pts[idx];
            sw   += w;
            swx  += w * px;
            swy  += w * py;
            swxx += w * px * px;
            swxy += w * px * py;
        }

        let denom = sw * swxx - swx * swx;
        if denom.abs() < 1e-12 {
            swy / sw.max(1e-12)
        } else {
            let b = (sw * swxy - swx * swy) / denom;
            let a = (swy - b * swx) / sw;
            a + b * xi
        }
    }).collect();

    DataFrame::new()
        .with_float(x_field, smooth_x)
        .with_float(y_field, smooth_y)
}

// ---------------------------------------------------------------------------
// aggregate — generic group-by-x, summarize-y transform
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// range — per-group (min, max), the first transform to synthesize a *pair*
// ---------------------------------------------------------------------------

/// Group rows by `x_field` and reduce `y_field` to a **quantile band** within
/// each group, emitting **two rows per group** — the low, then the high — both in
/// the `y_field` column. Unparameterized the band is the whole group, so bare
/// `range` is the minimum and the maximum it always was.
///
/// That two-row encoding is deliberate: it lets an interval's extents ride the
/// ordinary single-column machinery. The axis domain reads both low and high
/// straight out of `y_field` (no special range plumbing), and `write_interval`
/// pairs the consecutive rows back into one whisker. Grouping matches the
/// aggregation family — string x keeps first-appearance order, numeric x sorts
/// ascending — so `interval * range` reads like `bar * mean`, one range where
/// the other has a point.
fn range(df: &DataFrame, x_field: &str, y_field: &str, spec: Option<&RangeSpec>) -> DataFrame {
    let Some(ys) = df.float_col(y_field) else { return df.clone() };

    let extents = |v: &[f64]| range_pair(v, spec);

    // String x: one (low, high) pair per group, groups in first-seen order.
    if let Some(xs) = df.str_col(x_field) {
        let mut groups: Vec<(String, Vec<f64>)> = Vec::new();
        for (x, &y) in xs.iter().zip(ys.iter()) {
            if let Some(g) = groups.iter_mut().find(|(k, _)| k == x) {
                g.1.push(y);
            } else {
                groups.push((x.clone(), vec![y]));
            }
        }
        let mut keys = Vec::with_capacity(groups.len() * 2);
        let mut vals = Vec::with_capacity(groups.len() * 2);
        for (k, v) in &groups {
            let (lo, hi) = extents(v);
            keys.push(k.clone()); vals.push(lo);
            keys.push(k.clone()); vals.push(hi);
        }
        return keyed(DataFrame::new().with_float(y_field, vals), x_field, keys, df);
    }

    // Numeric x: group by distinct value, ascending.
    if let Some(xs) = df.float_col(x_field) {
        let mut groups: Vec<(f64, Vec<f64>)> = Vec::new();
        for (&x, &y) in xs.iter().zip(ys.iter()) {
            if let Some(g) = groups.iter_mut().find(|(k, _)| *k == x) {
                g.1.push(y);
            } else {
                groups.push((x, vec![y]));
            }
        }
        groups.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        let mut kx = Vec::with_capacity(groups.len() * 2);
        let mut vals = Vec::with_capacity(groups.len() * 2);
        for (k, v) in &groups {
            let (lo, hi) = extents(v);
            kx.push(*k); vals.push(lo);
            kx.push(*k); vals.push(hi);
        }
        return DataFrame::new().with_float(x_field, kx).with_float(y_field, vals);
    }

    df.clone()
}

// ---------------------------------------------------------------------------
// bounds — reshape two pre-computed columns into the low/high pair (no compute)
// ---------------------------------------------------------------------------

/// The non-computing counterpart to [`range`]. Where `range` reduces raw `y` into
/// a per-group (low, high) pair, `bounds` takes two columns the caller **already
/// computed** — `bounds(lower, upper)` — and emits them as that same pair of rows
/// in `out_field`, keyed by `key_field`. So a pre-computed band (a model's SE, a
/// psychometric CSEM, a bootstrap interval) rides the identical pair-row machinery
/// `interval`/`ribbon` already read, with no `ymin`/`ymax` channels and no
/// statistics in the plot.
///
/// It **groups and reduces nothing**: one input row becomes one (low, high) pair,
/// in input order — the caller's table is already one row per position. Both rows
/// of a pair repeat the key, so they share a position the way `range`'s do.
fn bounds(df: &DataFrame, key_field: &str, out_field: &str, spec: Option<&BoundsSpec>) -> DataFrame {
    let Some(spec) = spec else { return df.clone() };
    // The measure pair only. A `zone`'s `start`/`end` name a *domain* extent, which
    // is not a pair of rows at a shared position — a zone never reaches this
    // function at all (`render::svg` passes its frame through untouched), so there
    // is nothing here to widen.
    let Some((lower, upper)) = spec.measure() else { return df.clone() };
    let (Some(lo), Some(hi)) = (df.float_col(lower), df.float_col(upper)) else {
        return df.clone();
    };

    // low then high, interleaved, into out_field.
    let interleave = |n: usize| -> Vec<f64> {
        let mut v = Vec::with_capacity(n * 2);
        for i in 0..n { v.push(lo[i]); v.push(hi[i]); }
        v
    };

    // String key (a categorical error bar) — carry declared factor order via `keyed`.
    if let Some(xs) = df.str_col(key_field) {
        let n = xs.len().min(lo.len()).min(hi.len());
        let mut keys = Vec::with_capacity(n * 2);
        for i in 0..n { keys.push(xs[i].clone()); keys.push(xs[i].clone()); }
        return keyed(DataFrame::new().with_float(out_field, interleave(n)), key_field, keys, df);
    }
    // Numeric key (a band across a score / time). Input order preserved.
    if let Some(xs) = df.float_col(key_field) {
        let n = xs.len().min(lo.len()).min(hi.len());
        let mut kx = Vec::with_capacity(n * 2);
        for i in 0..n { kx.push(xs[i]); kx.push(xs[i]); }
        return DataFrame::new().with_float(key_field, kx).with_float(out_field, interleave(n));
    }
    df.clone()
}

// ---------------------------------------------------------------------------
// confidence — the t-interval of the mean per group (a whisker with a center)
// ---------------------------------------------------------------------------

/// Inverse of the standard normal CDF — the z with lower-tail probability `p`.
/// Acklam's rational approximation, accurate to ~1.1e-9 over (0, 1); used by the
/// t-quantile below. No statistics dependency, in the spirit of the LOESS and
/// KDE numerics already in this file.
fn inv_norm(p: f64) -> f64 {
    const A: [f64; 6] = [-3.969683028665376e+01, 2.209460984245205e+02, -2.759285104469687e+02,
                          1.383577518672690e+02, -3.066479806614716e+01, 2.506628277459239e+00];
    const B: [f64; 5] = [-5.447609879822406e+01, 1.615858368580409e+02, -1.556989798598866e+02,
                          6.680131188771972e+01, -1.328068155288572e+01];
    const C: [f64; 6] = [-7.784894002430293e-03, -3.223964580411365e-01, -2.400758277161838e+00,
                         -2.549732539343734e+00, 4.374664141464968e+00, 2.938163982698783e+00];
    const D: [f64; 4] = [7.784695709041462e-03, 3.224671290700398e-01, 2.445134137142996e+00,
                         3.754408661907416e+00];
    const P_LOW: f64 = 0.02425;
    let p = p.clamp(1e-12, 1.0 - 1e-12);
    if p < P_LOW {
        let q = (-2.0 * p.ln()).sqrt();
        (((((C[0]*q+C[1])*q+C[2])*q+C[3])*q+C[4])*q+C[5]) / ((((D[0]*q+D[1])*q+D[2])*q+D[3])*q+1.0)
    } else if p <= 1.0 - P_LOW {
        let q = p - 0.5;
        let r = q * q;
        (((((A[0]*r+A[1])*r+A[2])*r+A[3])*r+A[4])*r+A[5]) * q
            / (((((B[0]*r+B[1])*r+B[2])*r+B[3])*r+B[4])*r+1.0)
    } else {
        let q = (-2.0 * (1.0 - p).ln()).sqrt();
        -(((((C[0]*q+C[1])*q+C[2])*q+C[3])*q+C[4])*q+C[5]) / ((((D[0]*q+D[1])*q+D[2])*q+D[3])*q+1.0)
    }
}

/// The two-sided critical value of Student's t: the positive `t` leaving
/// `two_tail` probability in the two tails at `df` degrees of freedom, so a 95%
/// interval passes `two_tail = 0.05`. Hill's Algorithm 396 (1970) for df ≥ 3,
/// with the df = 1 (Cauchy) and df = 2 closed forms. Verified against the
/// t-table in the tests.
fn t_quantile(two_tail: f64, df: f64) -> f64 {
    use std::f64::consts::PI;
    let p = two_tail.clamp(1e-12, 1.0 - 1e-12);
    if df <= 1.0 { return 1.0 / (p * PI / 2.0).tan(); }        // Cauchy
    if (df - 2.0).abs() < 1e-9 { return (2.0 / (p * (2.0 - p)) - 2.0).sqrt(); }
    let n = df;
    let a = 1.0 / (n - 0.5);
    let b = 48.0 / (a * a);
    let mut c = ((20700.0 * a / b - 98.0) * a - 16.0) * a + 96.36;
    let d = ((94.5 / (b + c) - 3.0) / b + 1.0) * (a * PI / 2.0).sqrt() * n;
    let x0 = d * p;
    let mut y = x0.powf(2.0 / n);
    if y > a + 0.05 {
        let x = inv_norm(p / 2.0);   // negative
        y = x * x;
        if n < 5.0 { c += 0.3 * (n - 4.5) * (x + 0.6); }
        c = (((0.05 * d * x - 5.0) * x - 7.0) * x - 2.0) * x + b + c;
        y = (((((0.4 * y + 6.3) * y + 36.0) * y + 94.5) / c - y - 3.0) / b + 1.0) * x;
        y = a * y * y;
        y = if y > 0.002 { y.exp() - 1.0 } else { 0.5 * y * y + y };
    } else {
        y = ((1.0 / (((n + 6.0) / (n * y) - 0.089 * d - 0.822) * (n + 2.0) * 3.0)
            + 0.5 / (n + 4.0)) * y - 1.0) * (n + 1.0) / (n + 2.0) + 1.0 / y;
    }
    (n * y).sqrt()
}

/// The confidence interval of the mean within each x group — mean ± t·se, at the
/// given `level` (0.95 default). Emits the low/high pair as two rows (like
/// `range`) **and** a `center` column (the mean, repeated on both rows), so
/// `interval` draws a whisker with a center dot: a pointrange. A group of n < 2
/// has no spread, so it collapses to a point (low = high = center = its value).
/// The t-interval of a cell's values: `(low, high, center)`.
///
/// Public to this module because it is read by **two** keyings rather than one, for
/// the reason [`AggFn`] is: [`confidence`] groups by a single key (the plane) and
/// [`pairs2d`] by a *pair* (a cell), and the arithmetic between them must be the
/// same function or the same words would mean two things (Law 2).
fn ci_of(vals: &[f64], level: f64) -> (f64, f64, f64) {
    let n = vals.len();
    let mean = vals.iter().sum::<f64>() / n as f64;
    if n < 2 { return (mean, mean, mean); }
    let var = vals.iter().map(|&v| (v - mean).powi(2)).sum::<f64>() / (n as f64 - 1.0);
    let se = (var / n as f64).sqrt();
    let m = t_quantile(1.0 - level, n as f64 - 1.0) * se;
    (mean - m, mean + m, mean)
}

/// The extents of a cell's values: `(min, max)`. [`ci_of`]'s sibling, shared by
/// [`range`] and [`pairs2d`] for the same reason.
fn extents_of(vals: &[f64]) -> (f64, f64) {
    (
        vals.iter().cloned().fold(f64::INFINITY, f64::min),
        vals.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
    )
}

/// The low/high pair `range` reduces one group to — **the one place the band is
/// decided**, called by both readings so a slot and a cell cannot disagree
/// (Law 2: how many keys a mark has is never a fact about what `range` means).
///
/// Unparameterized it is the extremes, and it takes the cheap path to say so:
/// a type-7 quantile at p = 0 and p = 1 *is* the minimum and the maximum, so the
/// two agree by arithmetic, and the fold is the same answer without a sort. The
/// test `bare_range_is_the_extremes_by_both_paths` holds the two together, which
/// is what makes this an optimization rather than a second opinion.
///
/// Anything else sorts and interpolates with [`quantile_sorted`], the function
/// `box` already uses, so `interval * range(0.25, 0.75)` and a box's body report
/// the same quartile from the same code.
fn range_pair(vals: &[f64], spec: Option<&RangeSpec>) -> (f64, f64) {
    let (lo_p, hi_p) = RangeSpec::probabilities(spec);
    if lo_p == 0.0 && hi_p == 1.0 {
        return extents_of(vals);
    }
    let mut s: Vec<f64> = vals.iter().cloned().filter(|v| v.is_finite()).collect();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    if s.is_empty() { return (f64::NAN, f64::NAN); }
    (quantile_sorted(&s, lo_p), quantile_sorted(&s, hi_p))
}

/// One group's spread band, mean ± k·sd, as the same *(low, high, center)*
/// triple [`ci_of`] returns — which is what lets `deviation` reuse every line
/// below rather than growing a second copy of the grouping.
///
/// The sample standard deviation, dividing by n−1, so it matches what a reader
/// gets from `sd(x)` and from the `var` inside `ci_of`. A group of one has no
/// spread to draw and collapses to its own value, exactly as `confidence` does
/// there, because the alternative is a band of width NaN that silently removes
/// the group from the axis domain.
/// The probability a `quantile` layer asks for. The one place `None` becomes a
/// number, so the transform and `reduces_column` cannot disagree about what an
/// unnamed probability means — and both are downstream of the refusal that stops
/// an unnamed one from getting here at all.
fn quantile_p(spec: Option<&QuantileSpec>) -> f64 {
    spec.and_then(|s| s.p).unwrap_or(0.5)
}

fn sd_band_of(vals: &[f64], multiplier: f64) -> (f64, f64, f64) {
    let n = vals.len();
    let mean = vals.iter().sum::<f64>() / n as f64;
    if n < 2 { return (mean, mean, mean); }
    let var = vals.iter().map(|&v| (v - mean).powi(2)).sum::<f64>() / (n as f64 - 1.0);
    let m = multiplier * var.sqrt();
    (mean - m, mean + m, mean)
}

/// `deviation` — the spread band per group. [`confidence`]'s twin, and the two
/// are one function apart on purpose: they emit the identical shape and differ
/// only in what the band *means*, which is the distinction `interval`'s chapter
/// exists to make visible.
fn deviation(df: &DataFrame, x_field: &str, y_field: &str, spec: Option<&DeviationSpec>) -> DataFrame {
    let k = spec.and_then(|s| s.multiplier).unwrap_or(1.0);
    centered_pairs(df, x_field, y_field, &move |vals: &[f64]| sd_band_of(vals, k))
}

fn confidence(df: &DataFrame, x_field: &str, y_field: &str, spec: Option<&ConfidenceSpec>) -> DataFrame {
    let level = spec.and_then(|s| s.level).unwrap_or(0.95);
    centered_pairs(df, x_field, y_field, &move |vals: &[f64]| ci_of(vals, level))
}

/// The emitting half of every *centered* pair transform: group by `x_field`,
/// reduce each group to a (low, high, center) triple, and write the pair as two
/// rows in `y_field` with the center repeated beside them.
///
/// Split out when `deviation` arrived, because the alternative was a second copy
/// of this grouping. Two copies of a grouping rule is how `confidence` and
/// `deviation` would come to disagree about which rows form a group, which is
/// the drift §14 records for the second renderer in a smaller place.
fn centered_pairs(
    df: &DataFrame, x_field: &str, y_field: &str,
    reduce: &dyn Fn(&[f64]) -> (f64, f64, f64),
) -> DataFrame {
    let Some(ys) = df.float_col(y_field) else { return df.clone() };

    let ci = reduce;

    // String x: one (low, high) pair + center per group, groups first-seen.
    if let Some(xs) = df.str_col(x_field) {
        let mut groups: Vec<(String, Vec<f64>)> = Vec::new();
        for (x, &y) in xs.iter().zip(ys.iter()) {
            if let Some(g) = groups.iter_mut().find(|(k, _)| k == x) { g.1.push(y); }
            else { groups.push((x.clone(), vec![y])); }
        }
        let mut keys = Vec::with_capacity(groups.len() * 2);
        let mut vals = Vec::with_capacity(groups.len() * 2);
        let mut ctr  = Vec::with_capacity(groups.len() * 2);
        for (k, v) in &groups {
            let (lo, hi, c) = ci(v);
            keys.push(k.clone()); vals.push(lo); ctr.push(c);
            keys.push(k.clone()); vals.push(hi); ctr.push(c);
        }
        return keyed(DataFrame::new().with_float(y_field, vals).with_float("center", ctr), x_field, keys, df);
    }

    // Numeric x: group by distinct value, ascending.
    if let Some(xs) = df.float_col(x_field) {
        let mut groups: Vec<(f64, Vec<f64>)> = Vec::new();
        for (&x, &y) in xs.iter().zip(ys.iter()) {
            if let Some(g) = groups.iter_mut().find(|(k, _)| *k == x) { g.1.push(y); }
            else { groups.push((x, vec![y])); }
        }
        groups.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        let mut kx = Vec::with_capacity(groups.len() * 2);
        let mut vals = Vec::with_capacity(groups.len() * 2);
        let mut ctr = Vec::with_capacity(groups.len() * 2);
        for (k, v) in &groups {
            let (lo, hi, c) = ci(v);
            kx.push(*k); vals.push(lo); ctr.push(c);
            kx.push(*k); vals.push(hi); ctr.push(c);
        }
        return DataFrame::new().with_float(x_field, kx).with_float(y_field, vals).with_float("center", ctr);
    }

    df.clone()
}

// ---------------------------------------------------------------------------
// box — the five-number summary per group (min, Q1, median, Q3, max)
// ---------------------------------------------------------------------------

/// The p-th quantile of an already-sorted slice by *type 7* (R's `quantile()`
/// default): linear interpolation between the two order statistics that straddle
/// `(n − 1)·p`. Chosen over Tukey's hinges because it is the quantile a reader
/// gets from `quantile(x)` — the box then matches the number they can compute.
fn quantile_sorted(sorted: &[f64], p: f64) -> f64 {
    let n = sorted.len();
    if n == 0 { return f64::NAN; }
    if n == 1 { return sorted[0]; }
    let h = (n as f64 - 1.0) * p.clamp(0.0, 1.0);
    let lo = h.floor() as usize;
    let frac = h - lo as f64;
    if lo + 1 < n { sorted[lo] + frac * (sorted[lo + 1] - sorted[lo]) } else { sorted[lo] }
}

/// The summary of one group: the two whisker ends, the three quartiles, and any
/// outlier values. `whiskers = "tukey"` (the default) runs the whiskers to the
/// most extreme datum **within 1.5·IQR of the box** and returns everything beyond
/// as outliers — the standard box plot; `"range"` runs them to the true min and
/// max with no outliers (the plain five-number summary).
struct BoxStat { w_lo: f64, w_hi: f64, q1: f64, median: f64, q3: f64, outliers: Vec<f64> }

fn box_stat(vals: &[f64], tukey: bool) -> BoxStat {
    let mut s: Vec<f64> = vals.iter().cloned().filter(|v| v.is_finite()).collect();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    if s.is_empty() {
        let n = f64::NAN;
        return BoxStat { w_lo: n, w_hi: n, q1: n, median: n, q3: n, outliers: Vec::new() };
    }
    let (q1, median, q3) = (quantile_sorted(&s, 0.25), quantile_sorted(&s, 0.50), quantile_sorted(&s, 0.75));
    let (min, max) = (s[0], s[s.len() - 1]);
    if !tukey {
        return BoxStat { w_lo: min, w_hi: max, q1, median, q3, outliers: Vec::new() };
    }
    // Tukey's fences: the whiskers stop at the last datum inside 1.5·IQR of the
    // box, and everything past the fence is an outlier drawn on its own.
    let iqr = q3 - q1;
    let (lo_fence, hi_fence) = (q1 - 1.5 * iqr, q3 + 1.5 * iqr);
    let w_lo = s.iter().cloned().find(|&v| v >= lo_fence).unwrap_or(min);
    let w_hi = s.iter().rev().cloned().find(|&v| v <= hi_fence).unwrap_or(max);
    let outliers = s.iter().cloned().filter(|&v| v < lo_fence || v > hi_fence).collect();
    BoxStat { w_lo, w_hi, q1, median, q3, outliers }
}

/// Group rows by `x_field` and reduce `y_field` to a **box-and-whisker summary**
/// per group — the whisker ends, the three quartiles, and (under the Tukey rule)
/// the outliers.
///
/// The encoding mirrors [`range`], and for the same reason: the two whisker ends
/// are emitted as a **low/high pair of rows** in `y_field`, so they ride the
/// ordinary y-domain machinery (`build_axis` already reads both out of `y_field`)
/// with no special plumbing. The three interior quartiles travel as extra columns
/// `lower`/`middle`/`upper`, repeated on both rows, the way [`confidence`] carries
/// its `center`. **Outliers append as their own rows** — `y` is the outlier value
/// (so the axis stretches to include it, which is the whole point of flagging it),
/// and `middle` is `NaN`, the sentinel that tells `write_box` "draw me as a point,
/// not part of a box." `write_box` partitions the rows on that sentinel.
///
/// Grouping matches the aggregation family: string x keeps first-appearance
/// order, numeric x sorts ascending. Read by the `box` mark, which injects this
/// transform; it is never composed by hand.
fn box_summary(df: &DataFrame, x_field: &str, y_field: &str, spec: Option<&BoxSpec>) -> DataFrame {
    let Some(ys) = df.float_col(y_field) else { return df.clone() };
    let tukey = spec.and_then(|s| s.whiskers.as_deref()) != Some("range");

    // Append one group's summary to the column builders: two box rows (whisker
    // ends, quartiles set), then one row per outlier (y = the value, quartiles NaN).
    let emit = |st: &BoxStat, key_lo: &mut dyn FnMut(),
                vals: &mut Vec<f64>, lower: &mut Vec<f64>, middle: &mut Vec<f64>, upper: &mut Vec<f64>| {
        for &(y, is_box) in &[(st.w_lo, true), (st.w_hi, true)] {
            key_lo();
            vals.push(y);
            if is_box { lower.push(st.q1); middle.push(st.median); upper.push(st.q3); }
        }
        for &o in &st.outliers {
            key_lo();
            vals.push(o); lower.push(f64::NAN); middle.push(f64::NAN); upper.push(f64::NAN);
        }
    };

    // String x: one summary per group, groups in first-seen order.
    if let Some(xs) = df.str_col(x_field) {
        let mut groups: Vec<(String, Vec<f64>)> = Vec::new();
        for (x, &y) in xs.iter().zip(ys.iter()) {
            if let Some(g) = groups.iter_mut().find(|(k, _)| k == x) { g.1.push(y); }
            else { groups.push((x.clone(), vec![y])); }
        }
        let (mut keys, mut vals) = (Vec::new(), Vec::new());
        let (mut lower, mut middle, mut upper) = (Vec::new(), Vec::new(), Vec::new());
        for (k, v) in &groups {
            let st = box_stat(v, tukey);
            let mut push_key = || keys.push(k.clone());
            emit(&st, &mut push_key, &mut vals, &mut lower, &mut middle, &mut upper);
        }
        return keyed(DataFrame::new().with_float(y_field, vals)
            .with_float("lower", lower).with_float("middle", middle).with_float("upper", upper), x_field, keys, df);
    }

    // Numeric x: group by distinct value, ascending.
    if let Some(xs) = df.float_col(x_field) {
        let mut groups: Vec<(f64, Vec<f64>)> = Vec::new();
        for (&x, &y) in xs.iter().zip(ys.iter()) {
            if let Some(g) = groups.iter_mut().find(|(k, _)| *k == x) { g.1.push(y); }
            else { groups.push((x, vec![y])); }
        }
        groups.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        let (mut kx, mut vals) = (Vec::new(), Vec::new());
        let (mut lower, mut middle, mut upper) = (Vec::new(), Vec::new(), Vec::new());
        for (k, v) in &groups {
            let st = box_stat(v, tukey);
            let mut push_key = || kx.push(*k);
            emit(&st, &mut push_key, &mut vals, &mut lower, &mut middle, &mut upper);
        }
        return DataFrame::new().with_float(x_field, kx).with_float(y_field, vals)
            .with_float("lower", lower).with_float("middle", middle).with_float("upper", upper);
    }

    df.clone()
}

/// The five reductions a user names — `sum`, `mean`, `median`, `max`, `min`.
///
/// Public because the arithmetic is now read by **two** keyings rather than one:
/// [`aggregate`] groups by a single key (the plane), [`agg2d`] by a *pair* (a cell).
/// The reduction itself is the same function in both, which is the point — one key
/// or two is a fact about the mark, never about what `mean` means (Law 2).
/// `Eq` is deliberately absent: `Quantile` carries the probability, and a float
/// has no total equality. Nothing compares an `AggFn`, so the derive was only
/// ever a convenience — where `Transform` keeps its `Eq` by pushing parameters
/// onto the layer, this enum is internal and can hold its one knob directly.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum AggFn { Sum, Mean, Median, Max, Min, Quantile(f64) }

impl AggFn {
    /// Reduce one group's values to one number. Takes `&mut` because the median
    /// sorts in place; the caller owns a scratch vector per group either way.
    ///
    /// Non-finite values are dropped **here**, in the one place every keying
    /// passes through: filtered only on the keyless path, one NaN poisoned a
    /// keyed group's whole bar while the same transform without an `x` quietly
    /// skipped it — the two-ways behavior Law 2 forbids. A group with nothing
    /// finite reduces to NaN, which no mark draws.
    fn reduce(self, vals: &mut Vec<f64>) -> f64 {
        vals.retain(|v| v.is_finite());
        if vals.is_empty() {
            return f64::NAN;
        }
        match self {
            AggFn::Sum    => vals.iter().sum(),
            AggFn::Mean   => vals.iter().sum::<f64>() / vals.len() as f64,
            AggFn::Max    => vals.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
            AggFn::Min    => vals.iter().cloned().fold(f64::INFINITY,     f64::min),
            AggFn::Median => {
                vals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                let n = vals.len();
                if n % 2 == 1 { vals[n / 2] } else { (vals[n / 2 - 1] + vals[n / 2]) / 2.0 }
            }
            // Type 7, the same `quantile_sorted` the box and the band call, so
            // `bar * quantile(0.5)` and `bar * median` report the same middle
            // and the Assumption that names one for the other stays true.
            AggFn::Quantile(p) => {
                vals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                quantile_sorted(vals, p)
            }
        }
    }
}

/// Which reduction this transform sequence asks for, if any — the five that reduce
/// a column the user names, as against the four that invent their own measurement
/// ([`cell_measure`]).
///
/// The two classes are what §5 divides the value statistics by, and the division is
/// a property of the transform rather than a list anyone maintains: *were you handed
/// a column to reduce?* `count` was not, so it tallies rows and publishes its answer
/// under a name of its own; `mean` was, so it reduces that column and writes the
/// answer back into it.
/// Does this layer's sequence reduce a named column to a **pair** rather than to one
/// value — `range`, `confidence`, or the summary a `box` injects?
///
/// [`reduces_column`]'s sibling, and the two are asked together wherever the question
/// is *is there a reduction here* (`legality::reads_two_dimensions`). `bounds` is
/// **not** one of them: it names two columns that already hold the pair rather than
/// computing one from a group, so there is nothing for a cell to reduce.
pub fn pairs_a_column(transforms: &[Transform]) -> bool {
    transforms.iter().any(|t| matches!(t, Transform::Range | Transform::Confidence
                                          | Transform::Deviation | Transform::Box))
}

/// A statistic that reduces the data to **one value per x-group** (or per bin),
/// which any *locus* mark can then draw. "One `bin`, three marks" (spec §5)
/// generalized: `point`/`line`/`area`/`bar`/`step` all draw a value at each x.
///
/// Lives here rather than in `legality` because it answers a question about a
/// *transform*, and two modules need it: the Mark × Transform grid asks which
/// pairs are legal, and `line` asks whether connecting its rows in x order was
/// intended. `line` kept its own copy of this list until 2026-08-05 and had
/// fallen two members behind — `quantile` never joined it, and `bin` never had,
/// so the frequency polygon warned about a zigzag it cannot draw.
pub fn is_value_statistic(t: &Transform) -> bool {
    matches!(
        t,
        Transform::Bin | Transform::Smooth | Transform::Count | Transform::Density
            | Transform::Proportion | Transform::Sum | Transform::Mean | Transform::Median
            | Transform::Max | Transform::Min | Transform::Quantile
    )
}

/// Is this one transform a member of the **aggregation family**?
///
/// [`reduces_column`] answers *which* reduction to run, so it takes the first it
/// finds and stops. This answers *how many were asked for*, which is a different
/// question and the one `legality::check_chain_jobs` needs: two reductions in
/// one layer measure the cell twice, exactly as two synthesizers do.
///
/// Until 2026-07-30 nothing asked it, and `bar * sum * mean` drew the sum and
/// discarded the mean without a word — the silent drop §12 forbids, sitting in the
/// engine rather than in the book. The two lists are bound by
/// `the_reduction_family_is_one_list`, so a sixth statistic cannot join `AggFn`
/// below and skip the rule above.
pub fn is_reduction(t: &Transform) -> bool {
    matches!(t, Transform::Sum | Transform::Mean | Transform::Median
                | Transform::Max | Transform::Min | Transform::Quantile)
}

/// **Is** there a reduction in this sequence — the classification question.
///
/// Split from [`reduces_column`] when `quantile` arrived, because the two
/// questions stopped having one answer. Knowing *that* a layer reduces needs no
/// parameter; knowing *which* reduction to run needs the probability when the
/// answer is a quantile. Every caller that only wanted the first question now
/// asks it directly, so no site can obtain an `AggFn` without the knob that
/// completes it.
pub fn has_reduction(transforms: &[Transform]) -> bool {
    transforms.iter().any(is_reduction)
}

/// **Which** reduction, ready to run. `q` supplies `quantile`'s probability and
/// is ignored by the other five; a `quantile` layer that names no probability is
/// refused by `legality::check_quantile_params` before this is reached, and the
/// 0.5 fallback here exists only so a `GOG_STRICT=0` draw has a defined answer
/// rather than a NaN that would quietly empty the axis.
pub fn reduces_column(transforms: &[Transform], q: Option<&QuantileSpec>) -> Option<AggFn> {
    transforms.iter().find_map(|t| match t {
        Transform::Sum      => Some(AggFn::Sum),
        Transform::Mean     => Some(AggFn::Mean),
        Transform::Median   => Some(AggFn::Median),
        Transform::Max      => Some(AggFn::Max),
        Transform::Min      => Some(AggFn::Min),
        Transform::Quantile => Some(AggFn::Quantile(q.and_then(|s| s.p).unwrap_or(0.5))),
        _ => None,
    })
}

/// Group rows by `x_field`, apply `agg_fn` to `y_field` within each group.
/// Works on both string and numeric x columns.
/// String x: preserves first-appearance order.
/// Numeric x: groups sorted ascending.
fn aggregate(df: &DataFrame, x_field: &str, y_field: &str, agg_fn: AggFn) -> DataFrame {
    let Some(ys) = df.float_col(y_field) else { return df.clone() };

    let apply = |vals: &mut Vec<f64>| -> f64 { agg_fn.reduce(vals) };

    // No position axis: one summary for the whole frame, the keyless form `count`
    // takes above. `bar * sum * stack + y(amount) + color(region)` is the total per
    // region in a single column, which is the most common pie there is.
    if x_field.is_empty() {
        let mut vals: Vec<f64> = ys.iter().copied().filter(|v| v.is_finite()).collect();
        if vals.is_empty() { return DataFrame::new(); }
        return DataFrame::new().with_float(y_field, vec![apply(&mut vals)]);
    }

    // String x column
    if let Some(xs) = df.str_col(x_field) {
        let mut groups: Vec<(String, Vec<f64>)> = Vec::new();
        for (x, &y) in xs.iter().zip(ys.iter()) {
            if let Some(g) = groups.iter_mut().find(|(k, _)| k == x) {
                g.1.push(y);
            } else {
                groups.push((x.clone(), vec![y]));
            }
        }
        let keys:   Vec<String> = groups.iter().map(|(k, _)| k.clone()).collect();
        let values: Vec<f64>    = groups.iter_mut().map(|(_, v)| apply(v)).collect();
        return keyed(DataFrame::new().with_float(y_field, values), x_field, keys, df);
    }

    // Numeric x column
    if let Some(xs) = df.float_col(x_field) {
        let mut groups: Vec<(f64, Vec<f64>)> = Vec::new();
        for (&x, &y) in xs.iter().zip(ys.iter()) {
            if let Some(g) = groups.iter_mut().find(|(k, _)| (*k - x).abs() < 1e-12) {
                g.1.push(y);
            } else {
                groups.push((x, vec![y]));
            }
        }
        groups.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        let keys:   Vec<f64> = groups.iter().map(|(k, _)| *k).collect();
        let values: Vec<f64> = groups.iter_mut().map(|(_, v)| apply(v)).collect();
        return DataFrame::new().with_float(x_field, keys).with_float(y_field, values);
    }

    df.clone()
}

/// Estimate the probability density of `x_field` using a Gaussian kernel.
/// Bandwidth is chosen by Silverman's rule-of-thumb (IQR-adjusted).
/// Evaluates at 256 points spanning [min − 3h, max + 3h].
/// Silverman's rule-of-thumb bandwidth for `vals`, in `dims` dimensions, with the
/// layer's overrides applied.
///
/// Split out of `density` so the one-dimensional curve and the two-dimensional
/// field cannot drift apart on *how smooth* is decided — they differ in one
/// exponent and nothing else.
///
/// **The robust scale** is the smaller of the sd and the IQR spread, but only when
/// the IQR is real. With many tied values the IQR is zero while the data still has
/// spread (a column of one repeated value plus a few outliers), and taking the min
/// would collapse the bandwidth to ~0: the density then becomes a picket of spikes
/// that integrates to 6·10⁸ instead of 1. R's `bw.nrd0` guards the same way — an
/// empty IQR falls back to the sd.
///
/// **The exponent is where the dimension enters**, and it is derived rather than
/// tuned: Silverman's rule is *n*^(−1/(d+4)), so a curve smooths at *n*^(−1/5) and
/// a field at *n*^(−1/6). One rule, read in the number of dimensions the mark asked
/// for (spec §5) — the same shape as the transform itself.
///
/// **Only `adjust` reaches the two-dimensional reading.** An absolute `bandwidth`
/// is a length in *one* column's units, and a field has two columns carrying
/// different quantities, so one number cannot mean both — the lesson `hex` learned
/// when its circumradius came out as a half-width/half-height pair. So
/// `legality::check_density_params` refuses it there and points at `adjust`, which
/// is dimensionless and means the same thing on either axis.
fn bandwidth(vals: &[f64], dims: u32, spec: Option<&DensitySpec>) -> f64 {
    let n = vals.len();
    if n == 0 { return 1.0; }
    let mean = vals.iter().sum::<f64>() / n as f64;
    let sd = (vals.iter().map(|&v| (v - mean).powi(2)).sum::<f64>() / n as f64).sqrt();

    let mut sorted: Vec<f64> = vals.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let (q1, q3) = (sorted[n / 4], sorted[(3 * n) / 4]);
    let robust = if q3 > q1 { (q3 - q1) / 1.34 } else { f64::INFINITY };
    let sigma = sd.min(robust);

    let h_auto = (0.9 * sigma * (n as f64).powf(-1.0 / (dims as f64 + 4.0))).max(1e-12);
    // An absolute `bandwidth` in the data's units wins outright; otherwise an
    // `adjust` multiplier scales the automatic one (`density(2)` → twice as
    // smooth). Both floored away from zero so a mistaken tiny value cannot spike
    // the estimate.
    match (spec.and_then(|s| s.bandwidth), spec.and_then(|s| s.adjust)) {
        (Some(bw), _) if dims == 1 => bw.max(1e-12),
        (_, Some(a)) => (h_auto * a).max(1e-12),
        _ => h_auto,
    }
}

/// The column a violin's half-extent rides in — the density, per slot, **unscaled**.
///
/// Not normalized here, and that is the division of labor rather than an omission:
/// what the widest violin should measure across the panel is a question about the
/// *page*, so `render::marks::violin` divides by the frame's maximum and the
/// transform stays a statistic. It also has to be that way round for a split
/// violin to be honest — `apply_grouped` runs this once per color and sees only
/// that color's rows, so a maximum taken here would rescale each group against
/// itself and quietly make two unequal groups look alike.
pub const SLOT_WIDTH: &str = "width";

/// The **slot reading** of `density` — the violin (spec §5).
///
/// Handed a categorical key and a continuous measure, `density` estimates the
/// distribution of the measure *within each category*, and the mark draws the
/// estimate as a width across the category's slot. It is the same estimator as the
/// curve (Law 2 — same Gaussian kernel, same Silverman bandwidth, same three-
/// bandwidth extension past the extremes), read per group; only what the answer is
/// drawn *as* differs, which is the mark's business and not the transform's.
///
/// Every group is estimated on **its own** evaluation grid, because a violin is a
/// conditional distribution and a shared grid would draw each group's estimate
/// across the whole panel's range, giving every violin the same flat tails.
///
/// `compare` is the one knob (spec §5): the weight each group's estimate carries.
/// A density integrates to 1 whatever the group's size, so `"count"` (the default —
/// see [`crate::ir::DEFAULT_COMPARE`] for why it is the default and not the
/// convention) multiplies by the group's row count and the areas come out
/// proportional to how many rows each group has; `"shape"` leaves the estimate
/// alone and every violin ends up with the same area. The choice is made here and
/// read nowhere else — the renderer only ever divides by the maximum it is given.
fn slot_density(df: &DataFrame, key_field: &str, val_field: &str, spec: Option<&DensitySpec>) -> DataFrame {
    let (Some(keys), Some(vals)) = (df.str_col(key_field), df.float_col(val_field))
        else { return DataFrame::new() };

    let by_count = spec.and_then(|s| s.compare.as_deref())
        .unwrap_or(crate::ir::DEFAULT_COMPARE) == crate::ir::COMPARE_COUNT;

    // Groups in first-seen order — `box_summary`'s rule, so a violin and a box drawn
    // over the same column stand in the same slots.
    let mut groups: Vec<(String, Vec<f64>)> = Vec::new();
    for (k, &v) in keys.iter().zip(vals.iter()) {
        if !v.is_finite() { continue }
        if let Some(g) = groups.iter_mut().find(|(g, _)| g == k) { g.1.push(v); }
        else { groups.push((k.clone(), vec![v])); }
    }

    let (mut out_keys, mut evals, mut widths) = (Vec::new(), Vec::new(), Vec::new());
    for (k, v) in &groups {
        // One point estimates nothing — a kernel needs a spread to be a spread of.
        // Dropped rather than drawn as a spike, the same silence `density` keeps on
        // an empty frame.
        if v.len() < 2 { continue }
        let h = bandwidth(v, 1, spec);
        let mn = v.iter().cloned().fold(f64::INFINITY, f64::min) - 3.0 * h;
        let mx = v.iter().cloned().fold(f64::NEG_INFINITY, f64::max) + 3.0 * h;
        let weight = if by_count { v.len() as f64 } else { 1.0 };

        let m = SLOT_SAMPLES;
        let two_pi_sqrt = (2.0 * std::f64::consts::PI).sqrt();
        for i in 0..m {
            let at = mn + (i as f64 / (m - 1) as f64) * (mx - mn);
            let d: f64 = v.iter()
                .map(|&xj| { let u = (at - xj) / h; (-0.5 * u * u).exp() / two_pi_sqrt })
                .sum::<f64>() / (v.len() as f64 * h);
            out_keys.push(k.clone());
            evals.push(at);
            widths.push(d * weight);
        }
    }

    keyed(DataFrame::new().with_float(val_field, evals).with_float(SLOT_WIDTH, widths),
          key_field, out_keys, df)
}

/// How many points each violin's outline is traced from.
///
/// Fewer than the curve's 256 because a violin is drawn at slot width rather than
/// panel width — a fifth of the page for a handful of categories — so the extra
/// vertices land inside the same pixel and cost bytes for nothing.
const SLOT_SAMPLES: usize = 128;

fn density(df: &DataFrame, x_field: &str, y_field: &str, spec: Option<&DensitySpec>) -> DataFrame {
    // A categorical key with a measure beside it is the **violin** (spec §5): the
    // estimate spreads along the measure axis and is drawn across the category's
    // slot. Without that measure there is nothing to estimate along, which
    // `legality::check_distribution_axis` refuses before this runs; see `bin`.
    if df.str_col(x_field).is_some() {
        return if df.float_col(y_field).is_some() {
            slot_density(df, x_field, y_field, spec)
        } else {
            DataFrame::new()
        };
    }
    let Some(xs) = df.float_col(x_field) else { return df.clone() };
    let n = xs.len();
    if n == 0 { return DataFrame::new(); }

    let mn = xs.iter().cloned().fold(f64::INFINITY, f64::min);
    let mx = xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    if !mn.is_finite() { return DataFrame::new(); }

    let h = bandwidth(xs, 1, spec);

    let m = 256_usize;
    let eval_xs: Vec<f64> = (0..m)
        .map(|i| (mn - 3.0 * h) + (i as f64 / (m - 1) as f64) * ((mx + 3.0 * h) - (mn - 3.0 * h)))
        .collect();

    let two_pi_sqrt = (2.0 * std::f64::consts::PI).sqrt();
    let eval_ys: Vec<f64> = eval_xs.iter().map(|&xi| {
        let sum: f64 = xs.iter()
            .map(|&xj| {
                let u = (xi - xj) / h;
                (-0.5 * u * u).exp() / two_pi_sqrt
            })
            .sum();
        sum / (n as f64 * h)
    }).collect();

    DataFrame::new()
        .with_float(x_field, eval_xs)
        .with_float(y_field, eval_ys)
}

// ---------------------------------------------------------------------------
// partition — a whole apportioned among nested parts
// ---------------------------------------------------------------------------

/// A node's own name, at its own level: `"Housing"` on the inner ring,
/// `"Energy"` on the rim. Named plainly because it is not internal — it is the
/// column a caller writes out loud as `label(name)`, the way a binned count is
/// named on `color(count)`.
pub const NODE_NAME: &str = "name";

/// Which ring a node sits on, counting the innermost as 1. The *measure* axis of
/// a partition, synthesized exactly as a tally is: nothing in the table carries
/// it, so the transform publishes it, and `y(depth, limits = c(0, 4))` is what
/// the caller then writes to hollow out the middle (spec §15 — a hole is a
/// stretch of the radial axis with nothing standing on it).
pub const NODE_DEPTH: &str = "depth";

/// Where each leaf's path stops, and whether any interior node carries a value.
///
/// Split out from [`partition`] because **`legality` asks the same question
/// first** and must answer it fatally: a table where a parent has a number of
/// its own is genuinely ambiguous (plotly spells the two readings
/// `branchvalues="total"` and `"remainder"`), and §12 says refuse rather than
/// guess. One function so the check and the layout cannot disagree about what
/// the tree is — the hand-written-list-beside-a-generated-one failure again.
pub struct Paths {
    /// Per row, how many leading levels are non-empty. `0` means the row names no
    /// root and takes no part in the picture.
    pub depth: Vec<usize>,
    /// Per row, the level values down to `depth`, then empty strings.
    pub path: Vec<Vec<String>>,
    /// A prefix that both **ends** at some node and **continues** past it, which
    /// is an interior node carrying its own value. Carries the offending path,
    /// for the diagnostic.
    pub interior: Option<Vec<String>>,
    /// A row whose levels resume after a gap (`group` set, `item` empty, `detail`
    /// set). A hole in the middle of a path is not a ragged rim: the rim is where
    /// a branch *stops*, and nothing below a stop can be reached.
    pub gap: Option<Vec<String>>,
}

/// Read the hierarchy out of the level columns, without laying anything out.
pub fn paths(df: &DataFrame, levels: &[String]) -> Paths {
    let cols: Vec<Option<&Vec<String>>> =
        levels.iter().map(|l| df.str_col(l)).collect();
    let n = df.len();

    let mut depth = vec![0usize; n];
    let mut path = vec![Vec::<String>::new(); n];
    let mut gap = None;

    for r in 0..n {
        let mut p: Vec<String> = Vec::with_capacity(levels.len());
        let mut d = 0usize;
        let mut stopped = false;
        for c in cols.iter() {
            // A missing column reads as empty rather than as an error: `legality`
            // has already refused a level that is not there, so reaching here with
            // one means the check is being bypassed and an empty branch is the
            // least surprising answer.
            let v = c.and_then(|col| col.get(r)).cloned().unwrap_or_default();
            // Empty **only**. A missing value already arrives here as the empty
            // string (`gog-cli` unwraps a wire `null` to `String::default()`), so
            // there is nothing left for a spelling to catch — and catching one
            // would be a silent coercion of real data: `"NA"` is North America,
            // and a hierarchy of continents would have lost it. Found 2026-07-27
            // running plotly's repeated-labels example, whose first column is
            // exactly that.
            let v = v.trim().to_string();
            if v.is_empty() {
                stopped = true;
                p.push(String::new());
                continue;
            }
            // A value *after* a stop is a hole in the path, not a deeper branch.
            if stopped {
                if gap.is_none() {
                    let mut shown = p.clone();
                    shown.push(v.clone());
                    gap = Some(shown);
                }
                p.push(String::new());
                continue;
            }
            p.push(v);
            d += 1;
        }
        depth[r] = d;
        path[r] = p;
    }

    // An interior value: some row's path *ends* at a prefix that other rows
    // continue past. Checked prefix by prefix rather than node by node, because
    // the ambiguity is about the prefix and not about either row.
    let mut interior = None;
    'outer: for d in 1..levels.len() {
        let mut seen: Vec<(Vec<String>, bool, bool)> = Vec::new();
        for r in 0..n {
            if depth[r] < d {
                continue;
            }
            let key: Vec<String> = path[r][..d].to_vec();
            let ends = depth[r] == d;
            match seen.iter_mut().find(|(k, _, _)| *k == key) {
                Some((_, e, c)) => {
                    *e |= ends;
                    *c |= !ends;
                }
                None => seen.push((key, ends, !ends)),
            }
        }
        if let Some((k, _, _)) = seen.iter().find(|(_, e, c)| *e && *c) {
            interior = Some(k.clone());
            break 'outer;
        }
    }

    Paths { depth, path, interior, gap }
}

/// Apportion a whole among nested parts: one row per **node** of the hierarchy,
/// carrying that node's arc and its ring.
///
/// **Two ideas, and neither of them walks the tree.** Lay every leaf end to end
/// in proportion to its measure, which is a running total; a node's arc is then
/// nothing more than the **extent of its own leaves**, which one minimum and one
/// maximum per group answer. The traversal a hierarchy seems to demand is only
/// needed when descendants are scattered, so the rows are ordered first — by each
/// level's *first appearance* in the table, never alphabetically, so that the
/// caller's row order still decides the sweep round the circle and `order()`
/// remains the way to change it.
///
/// **The output is the rectangular extent description, unchanged** ([`CELL_START`]):
/// four edges plus the center on the position columns. That is what makes this a
/// transform rather than a coordinate space — `zone` needed no new branch to draw
/// it, and `polar` needed no new reading, because a bent rectangle was already a
/// sector.
///
/// `measure` is the bound `x` column when there is one; with none, every leaf
/// weighs 1 and the partition tallies, which is what `count`/`proportion` already
/// do when nothing else measured.
///
/// **`cross` is which way the levels run**, and it is the whole of the difference
/// between two families of plot. Nested (the default) sends every level down the
/// *same* axis and steps the other by one ring: flat that is the icicle, bent the
/// sunburst. Crossed alternates — the first level divides the width, the second
/// divides the height *within* each of those columns, the third the width again —
/// which is Wilkinson's crossed frame (ch. 11 §11.3.5.5) and draws the **mosaic**.
/// One parameter, one orthogonal meaning (§5): everything else here is shared, and
/// the evidence is that the first level's extents come out identical either way.
///
/// Two consequences follow from crossing rather than being chosen alongside it.
/// The **depth axis is spent** — both directions now carry the hierarchy, so there
/// is no ring to step and no hole to hollow — and only the **leaves** are drawn,
/// because a parent's region is exactly the union of its children's and would be
/// nothing but ink they paint over.
pub fn partition(
    df: &DataFrame,
    levels: &[String],
    measure: Option<&str>,
    measure_out: &str,
    depth_out: &str,
    cross: bool,
) -> DataFrame {
    let n = df.len();
    if levels.is_empty() || n == 0 {
        return DataFrame::new();
    }
    let Paths { depth, path, .. } = paths(df, levels);

    // First-appearance rank per level, so ordering the rows groups each branch
    // without imposing an alphabet on it. A level a row does not reach sorts last,
    // which only ever separates a ragged rim's short branches from long ones
    // sharing their prefix — and never reorders the branches themselves.
    let ranks: Vec<HashMap<String, usize>> = (0..levels.len())
        .map(|d| {
            let mut m = HashMap::new();
            for r in 0..n {
                if depth[r] > d {
                    let k = &path[r][d];
                    let next = m.len();
                    m.entry(k.clone()).or_insert(next);
                }
            }
            m
        })
        .collect();

    let mut rows: Vec<usize> = (0..n).filter(|&r| depth[r] > 0).collect();
    rows.sort_by_key(|&r| {
        (0..levels.len())
            .map(|d| {
                if depth[r] > d {
                    ranks[d].get(&path[r][d]).copied().unwrap_or(usize::MAX)
                } else {
                    usize::MAX
                }
            })
            .collect::<Vec<_>>()
    });

    // Every leaf, laid end to end. The axis runs 0 .. total in the measure's own
    // units, so `* proportion` normalizes it exactly as it normalizes a tally,
    // and no `share =` knob has to exist.
    let weight = |r: usize| -> f64 {
        match measure.and_then(|m| df.float_col(m)) {
            Some(v) => v.get(r).copied().filter(|x| x.is_finite() && *x >= 0.0).unwrap_or(0.0),
            None => 1.0,
        }
    };
    let mut lo = Vec::with_capacity(rows.len());
    let mut hi = Vec::with_capacity(rows.len());
    let mut at = 0.0;
    for &r in &rows {
        lo.push(at);
        at += weight(r);
        hi.push(at);
    }
    if at <= 0.0 {
        return DataFrame::new();
    }

    // One region per level; a node is a run of consecutive rows sharing a prefix,
    // and its extent is that run's.
    let mut name = Vec::new();
    let mut x_center = Vec::new();
    let mut y_center = Vec::new();
    let mut ring = Vec::new();
    let mut c_start = Vec::new();
    let mut c_end = Vec::new();
    let mut c_lower = Vec::new();
    let mut c_upper = Vec::new();
    let mut level_vals: Vec<Vec<String>> = vec![Vec::new(); levels.len()];

    // The box a node has to divide, and the span of leaves that box was drawn for,
    // carried **per leaf row** rather than in a tree. A level overwrites the rows
    // underneath it before the next one reads them, so a node always finds its
    // parent's rectangle at its own first row and nothing ever walks back up — the
    // same trick the running total plays for the arcs, one dimension over.
    // Crossed only: the nested reading's other axis is the ring, which needs no
    // parent at all.
    //
    // **The two axes carry different quantities, and the root says so.** Width runs
    // `0 .. total` in the measure's own units, exactly as the nested reading's does,
    // so `* proportion` turns it into shares and no knob had to exist. Height runs
    // `0 .. 1` and cannot do otherwise: every column fills it, so what a height
    // reads is that cell's share **of its own column**, and the total measure is not
    // the thing being divided. Drawn `0 .. total` it invited the axis to be read as
    // a count, which it is not for any column but the whole table.
    let mut parent = vec![(0.0, at, 0.0, 1.0, 0.0, at); rows.len()];

    for d in 1..=levels.len() {
        let mut i = 0;
        while i < rows.len() {
            if depth[rows[i]] < d {
                i += 1;
                continue;
            }
            let key = &path[rows[i]][..d];
            let mut j = i;
            while j + 1 < rows.len()
                && depth[rows[j + 1]] >= d
                && path[rows[j + 1]][..d] == *key
            {
                j += 1;
            }
            let (a0, a1) = (lo[i], hi[j]);
            // A **leaf** is a node no row goes deeper than. It is not the same as
            // "sits on the last level": a branch may stop early, which is what
            // gives a hierarchy its ragged rim, and those short branches are leaves
            // where they stop.
            let leaf = rows[i..=j].iter().all(|&r| depth[r] == d);
            let rect = if cross {
                // The node's share **of its parent**, which is the one number the
                // crossed reading needs and the nested one does not. Odd levels
                // divide the width and even levels the height, so a level always
                // cuts across the one above it; that alternation is the whole of
                // what `cross` means.
                let (px0, px1, py0, py1, ps0, ps1) = parent[i];
                let span = ps1 - ps0;
                let (f0, f1) = match span > 0.0 {
                    true => ((a0 - ps0) / span, (a1 - ps0) / span),
                    false => (0.0, 1.0),
                };
                let r = match d % 2 == 1 {
                    true => (px0 + f0 * (px1 - px0), px0 + f1 * (px1 - px0), py0, py1),
                    false => (px0, px1, py0 + f0 * (py1 - py0), py0 + f1 * (py1 - py0)),
                };
                for p in parent[i..=j].iter_mut() {
                    *p = (r.0, r.1, r.2, r.3, a0, a1);
                }
                r
            } else {
                (a0, a1, d as f64, d as f64 + 1.0)
            };
            if cross && !leaf {
                i = j + 1;
                continue;
            }
            name.push(key[d - 1].clone());
            x_center.push((rect.0 + rect.1) / 2.0);
            y_center.push((rect.2 + rect.3) / 2.0);
            ring.push(d as f64 + 0.5);
            c_start.push(rect.0);
            c_end.push(rect.1);
            c_lower.push(rect.2);
            c_upper.push(rect.3);
            // Every level column is carried, filled down to this node's own depth
            // and empty below it, so `color(group)` colors a rim node by the
            // branch it belongs to rather than by itself.
            for (l, col) in level_vals.iter_mut().enumerate() {
                col.push(if l < d { key[l].clone() } else { String::new() });
            }
            i = j + 1;
        }
    }

    // The ring rides on **both** names, and the duplication is the same one a
    // hexagonal mesh already makes (see [`CELL_Y`]). `depth` is the name a caller
    // says out loud — `y(depth, limits = c(0, 4))` is what hollows out the middle
    // — while `depth_out` is whatever column the plot's measure axis is reading,
    // which is the empty name when nothing is bound there. Writing only the first
    // left an unbound axis with no column to fit and every ring crushed against
    // the rim; writing only the second made the hole unsayable. A tally has the
    // same two duties and answers them the same way.
    let mut out = DataFrame::new()
        .with_str(NODE_NAME, name)
        .with_float(NODE_DEPTH, ring.clone())
        .with_float(CELL_START, c_start)
        .with_float(CELL_END, c_end)
        .with_float(CELL_LOWER, c_lower)
        .with_float(CELL_UPPER, c_upper)
        .with_float(measure_out, x_center);
    if depth_out != NODE_DEPTH && depth_out != measure_out {
        // Crossed, the second axis holds a **place** like the first one does, so a
        // `text` layer reads its label's height from it. Nested, it holds the ring,
        // which is that reading's whole vertical story.
        out = out.with_float(depth_out, if cross { y_center } else { ring });
    }
    for (l, col) in level_vals.into_iter().enumerate() {
        // A level column keeps its declared order, so a factor's levels survive
        // being partitioned — the same courtesy `keyed` does every key column.
        out = match df.levels(&levels[l]) {
            Some(lv) => out.with_levels(levels[l].clone(), col, lv.to_vec()),
            None => out.with_str(levels[l].clone(), col),
        };
    }
    out
}

/// The flow diagram's shared layout — one computation, projected per reading
/// mark exactly as `partition` feeds a rectangle and a name (spec §15, the flow
/// entry). One input row is one path through every stage; rows sharing a path
/// are aggregated, and a row missing a stage value is dropped (`check_flow`
/// counts what that costs, so the drop is never silent).
///
/// Everything here is deterministic by construction: slot order per stage is
/// the declared level order, first appearance otherwise; paths at a stage sort
/// node-first and then by their full path, so a node's paths are contiguous and
/// two layers reading one table land on identical numbers. The stacks are
/// **contiguous** — no padding row is invented between nodes — so the measure
/// axis reads true cumulative magnitude and keeps its ticks, which is half of
/// what parts this mark's ink from the funnel connector §18 refuses.
struct FlowLayout {
    /// The stage columns, in the atom's order.
    stages: Vec<String>,
    /// Distinct paths, each with its per-stage category ranks, values, weight,
    /// and per-stage running offset (the path's interval at stage `k` is
    /// `offset[k] .. offset[k] + weight`).
    paths: Vec<FlowPath>,
    /// Per stage: category names in slot order.
    cats: Vec<Vec<String>>,
}

struct FlowPath {
    values: Vec<String>,
    ranks: Vec<usize>,
    weight: f64,
    offsets: Vec<f64>,
}

fn flow_layout(df: &DataFrame, stages: &[String], measure: Option<&str>) -> Option<FlowLayout> {
    let n = df.len();
    if stages.len() < 2 || n == 0 {
        return None;
    }
    let cols: Vec<&Vec<String>> = stages.iter()
        .map(|s| df.str_col(s))
        .collect::<Option<Vec<_>>>()?;
    let weight = |r: usize| -> f64 {
        match measure.and_then(|m| df.float_col(m)) {
            Some(v) => v.get(r).copied().filter(|x| x.is_finite() && *x >= 0.0).unwrap_or(0.0),
            None => 1.0,
        }
    };

    // Slot order per stage: the declared levels where the column has them,
    // first appearance otherwise — `categories_across`'s own rule, restated here
    // because the layout needs the rank as a number before any axis exists.
    let cats: Vec<Vec<String>> = stages.iter().enumerate()
        .map(|(k, s)| match df.levels(s) {
            Some(lv) => lv.iter()
                .filter(|c| cols[k].iter().any(|v| v == *c))
                .cloned().collect(),
            None => {
                let mut seen = Vec::new();
                for v in cols[k] {
                    if !v.is_empty() && !seen.contains(v) {
                        seen.push(v.clone());
                    }
                }
                seen
            }
        })
        .collect();

    // Aggregate rows into paths. A path is the full tuple of stage values; rows
    // sharing one add their weights, which is what quietly marginalizes any
    // column the atom did not name.
    let mut paths: Vec<FlowPath> = Vec::new();
    for r in 0..n {
        let values: Vec<String> = cols.iter().map(|c| c[r].clone()).collect();
        if values.iter().any(|v| v.is_empty()) {
            continue;
        }
        let Some(ranks) = values.iter().enumerate()
            .map(|(k, v)| cats[k].iter().position(|c| c == v))
            .collect::<Option<Vec<usize>>>()
        else {
            continue;
        };
        let w = weight(r);
        match paths.iter_mut().find(|p| p.values == values) {
            Some(p) => p.weight += w,
            None => paths.push(FlowPath { values, ranks, weight: w, offsets: Vec::new() }),
        }
    }
    paths.retain(|p| p.weight > 0.0);
    if paths.is_empty() {
        return None;
    }

    // Stack each stage: paths sort node-first (so a node's interval is one
    // contiguous run) and then by their whole path, one total order for the
    // tie so the bands leave a node in the order they will arrive at the next.
    for k in 0..stages.len() {
        let mut order: Vec<usize> = (0..paths.len()).collect();
        order.sort_by(|&a, &b| {
            (paths[a].ranks[k], &paths[a].ranks).cmp(&(paths[b].ranks[k], &paths[b].ranks))
        });
        // Each path is visited exactly once per stage, so `offsets[k]` lands on
        // the right index by construction.
        let mut at = 0.0;
        for &i in &order {
            paths[i].offsets.push(at);
            at += paths[i].weight;
        }
    }

    Some(FlowLayout { stages: stages.to_vec(), paths, cats })
}

/// The node projection: one row per (stage, category), read by `zone` (the
/// slot) and `text` (the name at its center, `label(name)` as under
/// `partition`). Publishes the measure pair only — the domain axis is the
/// slotted one, which is the mixed mesh's shape: "the cut axis's two edges,
/// nothing for the slotted one."
pub fn flow_nodes(
    df: &DataFrame, stages: &[String], measure: Option<&str>, measure_out: &str,
) -> DataFrame {
    let Some(fl) = flow_layout(df, stages, measure) else {
        return DataFrame::new();
    };
    let mut stage_col = Vec::new();
    let mut name = Vec::new();
    let mut lower = Vec::new();
    let mut upper = Vec::new();
    let mut center = Vec::new();
    for (k, stage) in fl.stages.iter().enumerate() {
        for cat in &fl.cats[k] {
            let mut lo = f64::INFINITY;
            let mut hi = f64::NEG_INFINITY;
            for p in fl.paths.iter().filter(|p| &p.values[k] == cat) {
                lo = lo.min(p.offsets[k]);
                hi = hi.max(p.offsets[k] + p.weight);
            }
            if lo >= hi {
                continue;
            }
            stage_col.push(stage.clone());
            name.push(cat.clone());
            lower.push(lo);
            upper.push(hi);
            center.push((lo + hi) / 2.0);
        }
    }
    let out = DataFrame::new()
        .with_levels(FLOW_STAGE, stage_col, fl.stages.clone())
        .with_str(NODE_NAME, name)
        .with_float(CELL_LOWER, lower)
        .with_float(CELL_UPPER, upper);
    match measure_out.is_empty() || measure_out == FLOW_STAGE {
        true => out,
        false => out.with_float(measure_out, center),
    }
}

/// The band projection: one row per (path, stage), read by `ribbon`. Rows
/// sharing a [`FLOW_PATH`] key are one path in stage order, and each
/// consecutive pair is one band's two ends — the low/high-rows shape `range`
/// established, one column deeper. [`CELL_LOWER`]/[`CELL_UPPER`] carry the
/// path's interval at that row's stage, and [`FLOW_STAGE`] holds every stage,
/// so the axis a bands-only plot fits from is already complete. Every stage
/// column rides along with its declared levels, so `color(<any stage>)`
/// colors a band by the category its path holds there — well defined
/// precisely because a band is a whole path's slice, never a merged
/// aggregate.
pub fn flow_bands(
    df: &DataFrame, stages: &[String], measure: Option<&str>, measure_out: &str,
) -> DataFrame {
    let Some(fl) = flow_layout(df, stages, measure) else {
        return DataFrame::new();
    };
    // Paths in one global order, each contributing its full run of stages, so
    // the writer pairs consecutive rows and the painting order is the path
    // order.
    let mut order: Vec<usize> = (0..fl.paths.len()).collect();
    order.sort_by(|&a, &b| fl.paths[a].ranks.cmp(&fl.paths[b].ranks));
    let mut path_key = Vec::new();
    let mut stage_col = Vec::new();
    let mut lo = Vec::new();
    let mut hi = Vec::new();
    let mut center = Vec::new();
    let mut carried: Vec<Vec<String>> = vec![Vec::new(); fl.stages.len()];
    for (slot, &i) in order.iter().enumerate() {
        let p = &fl.paths[i];
        for k in 0..fl.stages.len() {
            path_key.push(format!("p{slot}"));
            stage_col.push(fl.stages[k].clone());
            lo.push(p.offsets[k]);
            hi.push(p.offsets[k] + p.weight);
            center.push(p.offsets[k] + p.weight / 2.0);
            for (s, col) in carried.iter_mut().enumerate() {
                col.push(p.values[s].clone());
            }
        }
    }
    let mut out = DataFrame::new()
        .with_str(FLOW_PATH, path_key)
        .with_levels(FLOW_STAGE, stage_col, fl.stages.clone())
        .with_float(CELL_LOWER, lo)
        .with_float(CELL_UPPER, hi);
    if !measure_out.is_empty() && measure_out != FLOW_STAGE {
        out = out.with_float(measure_out, center);
    }
    for (s, col) in carried.into_iter().enumerate() {
        out = match df.levels(&fl.stages[s]) {
            Some(lv) => out.with_levels(fl.stages[s].clone(), col, lv.to_vec()),
            None => out.with_str(fl.stages[s].clone(), col),
        };
    }
    out
}

/// The graph layout's shared computation — one deterministic placement,
/// projected per reading mark exactly as `flow`'s is (spec §15, the network
/// entry). Nodes derive from the endpoint union of an edge table; positions
/// come from a hash-seeded start refined by a budgeted spring relaxation.
///
/// **Every arithmetic step is `+ − × ÷ √` or the [`crate::render::hash01`]
/// hash.** No transcendental is called, because the browser engine is a second
/// compiled artifact and an iterative layout amplifies a last-bit `libm`
/// difference into visible drift between the CLI's picture and the first
/// interactive frame. The budget is `repel`'s
/// (`clamp(40_000_000 / n², 24, 240)` — it can only cost quality, never
/// honesty), and the tie-broken start is seeded from each node's own name, so
/// one table is one picture, every run, on every target.
struct GraphLayout {
    /// Node names, first-appearance order across (from, to).
    nodes: Vec<String>,
    /// Neighbor count per node, parallel to `nodes`.
    degree: Vec<f64>,
    /// Positions per node, `dims` values each, normalized to the unit range.
    pos: Vec<Vec<f64>>,
    /// Edge endpoint indices per surviving input row, with the row's index in
    /// the input frame so carried columns can be read back.
    edges: Vec<(usize, usize, usize)>,
}

fn graph_layout(df: &DataFrame, from: &str, to: &str, dims: usize) -> Option<GraphLayout> {
    let n_rows = df.len();
    let (a, b) = (df.str_col(from)?, df.str_col(to)?);
    let mut nodes: Vec<String> = Vec::new();
    let mut index = |name: &str, nodes: &mut Vec<String>| -> Option<usize> {
        if name.is_empty() {
            return None;
        }
        match nodes.iter().position(|x| x == name) {
            Some(i) => Some(i),
            None => {
                nodes.push(name.to_string());
                Some(nodes.len() - 1)
            }
        }
    };
    let mut edges: Vec<(usize, usize, usize)> = Vec::new();
    for r in 0..n_rows {
        let (Some(i), Some(j)) = (index(&a[r], &mut nodes), index(&b[r], &mut nodes)) else {
            continue;
        };
        if i != j {
            edges.push((i, j, r));
        }
    }
    let n = nodes.len();
    if n < 2 || edges.is_empty() {
        return None;
    }
    let mut degree = vec![0.0; n];
    for &(i, j, _) in &edges {
        degree[i] += 1.0;
        degree[j] += 1.0;
    }

    // The seeded start: each coordinate from the node's own name, `jitter`'s
    // seeded-from-data rule. A name-derived seed rather than an index-derived
    // one, so reordering the table's rows cannot move a node.
    let seed_of = |name: &str| -> u64 {
        let mut s: u64 = 0;
        for byte in name.bytes() {
            s = s.wrapping_mul(31).wrapping_add(byte as u64);
        }
        s
    };
    let mut pos: Vec<Vec<f64>> = nodes.iter()
        .map(|name| {
            let s = seed_of(name);
            (0..dims)
                .map(|d| crate::render::hash01(s.wrapping_add(1_000_003 * d as u64)))
                .collect()
        })
        .collect();

    // Fruchterman-Reingold springs under repel's budget. `k` is the ideal
    // spring length in the unit volume; the step cap `t` cools by ×0.97, which
    // keeps every operation inside the bit-deterministic subset.
    let k = (1.0 / n as f64).sqrt();
    let iters = (40_000_000_usize / (n * n)).clamp(24, 240);
    let mut t = 0.10;
    for _ in 0..iters {
        let mut disp = vec![vec![0.0; dims]; n];
        for i in 0..n {
            for j in (i + 1)..n {
                let d: Vec<f64> = (0..dims).map(|c| pos[i][c] - pos[j][c]).collect();
                let dist2: f64 = d.iter().map(|v| v * v).sum::<f64>() + 1e-9;
                let push = k * k / dist2;
                for c in 0..dims {
                    disp[i][c] += d[c] * push;
                    disp[j][c] -= d[c] * push;
                }
            }
        }
        for &(i, j, _) in &edges {
            let d: Vec<f64> = (0..dims).map(|c| pos[i][c] - pos[j][c]).collect();
            let dist = (d.iter().map(|v| v * v).sum::<f64>() + 1e-9).sqrt();
            let pull = dist / k;
            for c in 0..dims {
                disp[i][c] -= d[c] * pull;
                disp[j][c] += d[c] * pull;
            }
        }
        for i in 0..n {
            let dist = (disp[i].iter().map(|v| v * v).sum::<f64>() + 1e-9).sqrt();
            let step = if dist < t { dist } else { t };
            for c in 0..dims {
                pos[i][c] += disp[i][c] / dist * step;
            }
        }
        t *= 0.97;
    }

    // Normalize each dimension to the unit range; a degenerate spread centers.
    for c in 0..dims {
        let lo = pos.iter().map(|p| p[c]).fold(f64::INFINITY, f64::min);
        let hi = pos.iter().map(|p| p[c]).fold(f64::NEG_INFINITY, f64::max);
        for p in pos.iter_mut() {
            p[c] = if hi > lo { (p[c] - lo) / (hi - lo) } else { 0.5 };
        }
    }

    Some(GraphLayout { nodes, degree, pos, edges })
}

/// The node projection: one row per node — its `name`, its `degree`, and its
/// position under [`LAYOUT_X`]/[`LAYOUT_Y`] (and [`LAYOUT_Z`] in the cube).
/// Read by `point` and by `text + label(name)` through the ordinary writers,
/// which is the leak test the design set: no writer learns a network reading.
pub fn layout_nodes(df: &DataFrame, from: &str, to: &str, dims: usize) -> DataFrame {
    let Some(g) = graph_layout(df, from, to, dims) else {
        return DataFrame::new();
    };
    let mut out = DataFrame::new()
        .with_str(NODE_NAME, g.nodes.clone())
        .with_float(NODE_DEGREE, g.degree.clone())
        .with_float(LAYOUT_X, g.pos.iter().map(|p| p[0]).collect())
        .with_float(LAYOUT_Y, g.pos.iter().map(|p| p[1]).collect());
    if dims > 2 {
        out = out.with_float(LAYOUT_Z, g.pos.iter().map(|p| p[2]).collect());
    }
    out
}

/// The edge projection: one row per surviving input row, both endpoints as
/// synthesized columns, every input column carried through — an edge is 1:1
/// with its row, so `color(<any edge column>)` needs no rule of its own.
pub fn layout_edges(df: &DataFrame, from: &str, to: &str, dims: usize) -> DataFrame {
    let Some(g) = graph_layout(df, from, to, dims) else {
        return DataFrame::new();
    };
    let rows: Vec<usize> = g.edges.iter().map(|&(_, _, r)| r).collect();
    let mut out = DataFrame::new()
        .with_float(LAYOUT_X, g.edges.iter().map(|&(i, _, _)| g.pos[i][0]).collect())
        .with_float(LAYOUT_Y, g.edges.iter().map(|&(i, _, _)| g.pos[i][1]).collect())
        .with_float(EDGE_X, g.edges.iter().map(|&(_, j, _)| g.pos[j][0]).collect())
        .with_float(EDGE_Y, g.edges.iter().map(|&(_, j, _)| g.pos[j][1]).collect());
    if dims > 2 {
        out = out
            .with_float(LAYOUT_Z, g.edges.iter().map(|&(i, _, _)| g.pos[i][2]).collect())
            .with_float(EDGE_Z, g.edges.iter().map(|&(_, j, _)| g.pos[j][2]).collect());
    }
    // Sorted, not iterated: the column map has no order, and while nothing
    // downstream reads columns positionally, the determinism rule is cheaper
    // to keep total than to argue per consumer.
    let mut names: Vec<String> = df.column_names().map(String::from).collect();
    names.sort();
    for name in names {
        if let Some(vals) = df.str_col(&name) {
            let col: Vec<String> = rows.iter().map(|&r| vals[r].clone()).collect();
            out = match df.levels(&name) {
                Some(lv) => out.with_levels(name.clone(), col, lv.to_vec()),
                None => out.with_str(name.clone(), col),
            };
        } else if let Some(vals) = df.float_col(&name) {
            out = out.with_float(name.clone(), rows.iter().map(|&r| vals[r]).collect());
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Tests
//
// The estimators (`smooth`, `density`) are pinned by invariants a *correct*
// implementation must satisfy rather than by the exact floats this code happens
// to emit — LOESS of a straight line is that line; a density integrates to one.
// A test that only echoes today's output would pass a wrong rewrite too, which
// is the failure mode `scale.rs` recorded (the bar-width test that held with the
// pipeline reversed). The exact transforms assert exact numbers.
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;

    fn flow_frame() -> DataFrame {
        DataFrame::new()
            .with_levels("a", vec!["p".into(), "p".into(), "q".into(), "q".into(), "p".into()],
                         vec!["p".into(), "q".into()])
            .with_str("b", vec!["u".into(), "v".into(), "u".into(), "v".into(), "u".into()])
            .with_float("n", vec![2.0, 3.0, 4.0, 1.0, 5.0])
    }

    /// **A flow conserves its total at every stage.** Each stage's node intervals
    /// tile `0 .. total` exactly — contiguous stacks, no invented padding — which
    /// is what keeps the measure axis honest, and the alluvial identity (a middle
    /// node's inflow equals its outflow) holds by construction.
    #[test]
    fn a_flow_conserves_its_total_at_every_stage() {
        let stages = vec!["a".to_string(), "b".to_string()];
        let nodes = flow_frame();
        let out = flow_nodes(&nodes, &stages, Some("n"), "count");
        let stage = out.str_col(FLOW_STAGE).unwrap();
        let lo = out.float_col(CELL_LOWER).unwrap();
        let hi = out.float_col(CELL_UPPER).unwrap();
        for s in ["a", "b"] {
            let mut spans: Vec<(f64, f64)> = (0..stage.len())
                .filter(|&r| stage[r] == s)
                .map(|r| (lo[r], hi[r]))
                .collect();
            spans.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap());
            assert_eq!(spans.first().unwrap().0, 0.0, "stage `{s}` starts at zero");
            assert_eq!(spans.last().unwrap().1, 15.0, "stage `{s}` ends at the total");
            for w in spans.windows(2) {
                assert_eq!(w[0].1, w[1].0, "stage `{s}` stacks contiguously");
            }
        }
    }

    /// **Rows sharing a path are one band.** The two `p → u` rows (weights 2 and
    /// 5) aggregate to one path of 7, which quietly marginalizes any column the
    /// atom did not name. The projection is one row per (path, stage) — every
    /// stage present in [`FLOW_STAGE`], so a bands-only plot fits a complete
    /// axis — and a path's rows all carry its one thickness, which is the
    /// property that parts this ink from the funnel connector's.
    #[test]
    fn a_flow_aggregates_paths_and_keeps_thickness_at_both_ends() {
        let stages = vec!["a".to_string(), "b".to_string()];
        let out = flow_bands(&flow_frame(), &stages, Some("n"), "count");
        let key = out.str_col(FLOW_PATH).unwrap();
        let stage = out.str_col(FLOW_STAGE).unwrap();
        let lo = out.float_col(CELL_LOWER).unwrap();
        let hi = out.float_col(CELL_UPPER).unwrap();
        assert_eq!(key.len(), 8, "four distinct paths, one row per stage each");
        assert!(stage.iter().any(|s| s == "a") && stage.iter().any(|s| s == "b"),
            "every stage appears in the band frame's own rows");
        let mut widths: Vec<f64> = (0..key.len()).step_by(2).map(|r| hi[r] - lo[r]).collect();
        widths.sort_by(|x, y| x.partial_cmp(y).unwrap());
        assert_eq!(widths, vec![1.0, 3.0, 4.0, 7.0]);
        for r in (0..key.len()).step_by(2) {
            assert_eq!(key[r], key[r + 1], "a path's rows are consecutive");
            assert_eq!(hi[r] - lo[r], hi[r + 1] - lo[r + 1],
                "band {r} is as thick at both ends");
        }
    }

    /// **The stage column carries the atom's order as its levels**, so the axis
    /// draws the stages in reading order; a band's carried columns keep their
    /// declared levels; and two runs over one table emit identical frames —
    /// the determinism two layers reading one computation stand on.
    #[test]
    fn a_flow_is_ordered_by_the_atom_and_deterministic() {
        let stages = vec!["a".to_string(), "b".to_string()];
        let nodes = flow_nodes(&flow_frame(), &stages, Some("n"), "count");
        assert_eq!(nodes.levels(FLOW_STAGE).unwrap(), &["a".to_string(), "b".to_string()]);
        let bands = flow_bands(&flow_frame(), &stages, Some("n"), "count");
        assert_eq!(bands.levels("a").unwrap(), &["p".to_string(), "q".to_string()],
            "a carried stage keeps its declared levels");
        let again = flow_bands(&flow_frame(), &stages, Some("n"), "count");
        for col in [CELL_LOWER, CELL_UPPER] {
            assert_eq!(bands.float_col(col), again.float_col(col),
                "one table, one layout, every run (`{col}`)");
        }
        assert_eq!(bands.str_col("a"), again.str_col("a"));
        assert_eq!(bands.str_col(FLOW_PATH), again.str_col(FLOW_PATH));
    }

    /// **A row missing a stage value is not a path** and is left out here; the
    /// count a reader is owed for that comes from `check_flow`, which is the
    /// legality layer's half of the same rule (§12: never a silent drop).
    #[test]
    fn a_flow_drops_incomplete_rows_and_tallies_when_unweighted() {
        let stages = vec!["a".to_string(), "b".to_string()];
        let df = DataFrame::new()
            .with_str("a", vec!["p".into(), "p".into(), "".into()])
            .with_str("b", vec!["u".into(), "u".into(), "u".into()]);
        let out = flow_bands(&df, &stages, None, "count");
        let hi = out.float_col(CELL_UPPER).unwrap();
        assert_eq!(hi.len(), 2, "one path survives, one row per stage");
        assert_eq!(hi[0], 2.0, "the tally weighs each surviving row 1");
    }

    fn edge_frame() -> DataFrame {
        DataFrame::new()
            .with_str("a", vec!["p".into(), "p".into(), "q".into(), "r".into()])
            .with_str("b", vec!["q".into(), "r".into(), "r".into(), "s".into()])
    }

    /// **One table is one placement, every run, in either dimension.** The
    /// layout is seeded from node names and budgeted, so two computations agree
    /// to the bit — the guarantee three layers reading one table stand on — and
    /// every position lands in the unit range the space states.
    #[test]
    fn a_layout_is_deterministic_and_lands_in_the_unit_range() {
        for dims in [2usize, 3] {
            let one = layout_nodes(&edge_frame(), "a", "b", dims);
            let two = layout_nodes(&edge_frame(), "a", "b", dims);
            for col in [LAYOUT_X, LAYOUT_Y] {
                assert_eq!(one.float_col(col), two.float_col(col),
                    "one table, one placement ({dims}-D, `{col}`)");
                for v in one.float_col(col).unwrap() {
                    assert!((0.0..=1.0).contains(v), "{col} in the unit range");
                }
            }
            assert_eq!(one.float_col(LAYOUT_Z).is_some(), dims == 3,
                "the third coordinate exists exactly in the cube");
        }
    }

    /// **Nodes derive from the endpoint union**, in first-appearance order, with
    /// the degree the edges imply — the two columns `point` and `text` read.
    #[test]
    fn a_layout_derives_its_nodes_and_their_degrees()  {
        let nodes = layout_nodes(&edge_frame(), "a", "b", 2);
        assert_eq!(nodes.str_col(NODE_NAME).unwrap(),
            &["p".to_string(), "q".into(), "r".into(), "s".into()]);
        assert_eq!(nodes.float_col(NODE_DEGREE).unwrap(), &[2.0, 2.0, 3.0, 1.0]);
    }

    /// **An edge row is 1:1 with its input row**, both endpoints synthesized and
    /// every input column carried — which is what lets `color`/`opacity` map any
    /// edge column with no rule of their own. The two projections agree on every
    /// shared node position, asserted across frames rather than assumed.
    #[test]
    fn a_layout_edge_row_carries_its_columns_and_agrees_with_the_nodes() {
        let df = edge_frame().with_float("w", vec![1.0, 2.0, 3.0, 4.0]);
        let nodes = layout_nodes(&df, "a", "b", 2);
        let edges = layout_edges(&df, "a", "b", 2);
        assert_eq!(edges.float_col("w").unwrap(), &[1.0, 2.0, 3.0, 4.0]);
        let names = nodes.str_col(NODE_NAME).unwrap();
        let nx = nodes.float_col(LAYOUT_X).unwrap();
        let from = edges.str_col("a").unwrap();
        let ex = edges.float_col(LAYOUT_X).unwrap();
        for r in 0..from.len() {
            let i = names.iter().position(|n| n == &from[r]).unwrap();
            assert_eq!(ex[r], nx[i], "edge {r} starts where its node stands");
        }
    }

    /// **The aggregation family is one list, named in two places, and they must agree.**
    ///
    /// [`is_reduction`] gates the refusal in `legality::check_chain_jobs`;
    /// [`reduces_column`] picks the `AggFn` that actually runs. A statistic added to
    /// the second and forgotten in the first would walk straight back into the silent
    /// drop this pair was written to close on 2026-07-30, and nothing else would fail.
    /// Every variant is named on purpose rather than two by hand — the same discipline
    /// as `border_spans_*`, and for the same reason: a list that does not name the
    /// whole family can be widened without the assertion moving.
    #[test]
    fn the_reduction_family_is_one_list() {
        let every = [
            Transform::Bin, Transform::Smooth, Transform::Count, Transform::Density,
            Transform::Sum, Transform::Mean, Transform::Median, Transform::Max,
            Transform::Min, Transform::Proportion, Transform::Range,
            Transform::Confidence, Transform::Box, Transform::Bounds, Transform::Dodge,
            Transform::Stack, Transform::Jitter, Transform::Partition,
        ];
        for t in &every {
            assert_eq!(
                is_reduction(t),
                has_reduction(std::slice::from_ref(t)),
                "{t:?}: `is_reduction` and `reduces_column` disagree, so the refusal and \
                 the arithmetic are reading different lists"
            );
        }
        // And the family is the five it is supposed to be, so widening it is a
        // deliberate edit here rather than a side effect somewhere else.
        let family: Vec<_> = every.iter().filter(|t| is_reduction(t)).collect();
        assert_eq!(family.len(), 5, "the aggregation family is sum/mean/median/max/min: {family:?}");
    }

    /// Every transform the kernel has, named here on purpose. `every_transform_has_a_job`
    /// and the table below both walk it, so a nineteenth variant fails to compile
    /// against this array before it can reach either rule.
    const EVERY_TRANSFORM: [Transform; 18] = [
        Transform::Bin, Transform::Smooth, Transform::Count, Transform::Density,
        Transform::Sum, Transform::Mean, Transform::Median, Transform::Max,
        Transform::Min, Transform::Proportion, Transform::Range, Transform::Confidence,
        Transform::Box, Transform::Bounds, Transform::Dodge, Transform::Stack,
        Transform::Jitter, Transform::Partition,
    ];

    /// **A transform with no job composes silently with everything, so there is no
    /// such thing.**
    ///
    /// The totality rule, in the family of `legality::every_mark_channel_pair_has_a_rule`.
    /// Six transforms — `range`, `confidence`, `bounds`, `dodge`, `stack`, `jitter` —
    /// sat outside every composition check for the project's life, not because anyone
    /// decided they should but because the checks named the families they knew and
    /// nobody widened them. A jobless transform is that state, and this is what makes
    /// it impossible to reach by accident.
    #[test]
    fn every_transform_has_a_job() {
        for t in &EVERY_TRANSFORM {
            for ctx in [
                JobContext::default(),
                JobContext { measures_by_color: true, stack_shares: true },
            ] {
                let j = jobs(t, ctx);
                assert!(
                    j.extent || j.measure || j.scale || j.position,
                    "{t:?} fills no job, so nothing can tell what it collides with"
                );
                assert!(
                    !j.reads_a_column || j.measure,
                    "{t:?} claims to read a column without measuring one"
                );
                assert!(
                    !j.yields_measure || j.measure,
                    "{t:?} yields a measure job it does not fill"
                );
            }
        }
        // `bin` is the only transform that yields, and the rule is one sentence only
        // because that stays true. A second yielder would need the rule rewritten,
        // not the list widened.
        let yielders: Vec<_> = EVERY_TRANSFORM.iter()
            .filter(|t| jobs(t, JobContext::default()).yields_measure).collect();
        assert_eq!(yielders, vec![&Transform::Bin], "only `bin`'s measurement is a by-product");
    }

    /// **The job table reproduces every composition the engine already judges.**
    ///
    /// The test that says this is one rule rather than a fourth hand-written list.
    /// Each row below is a verdict the engine reached before jobs existed, by one of
    /// three separate enumerated checks; `job_conflict` has to reach the same one from
    /// the table alone. The rows marked *new* are the defects the enumeration missed —
    /// each one a chain that draws today while ignoring something the caller wrote.
    #[test]
    fn the_job_table_reaches_the_verdicts_the_engine_already_reached() {
        let flat = JobContext::default();
        let zone = JobContext { measures_by_color: true, ..JobContext::default() };
        let share = JobContext { stack_shares: true, ..JobContext::default() };
        use Transform::*;
        let cases: &[(&[Transform], JobContext, bool, &str)] = &[
            // Already refused, and the rule has to keep refusing them.
            (&[Bin, Count],        flat, false, "a cut and a tally each invent a measurement"),
            (&[Bin, Density],      flat, false, "two things cut the axis"),
            (&[Bin, Smooth],       flat, false, "a curve is not sampled at somebody else's cells"),
            (&[Count, Mean],       flat, false, "a tally cannot hand its cells to a reduction"),
            (&[Density, Mean],     flat, false, "same, one estimator over"),
            (&[Mean, Sum],         flat, false, "two reductions of the named column"),
            (&[Bounds, Bin],       zone, false, "a zone's sides said twice"),
            (&[Bounds, Mean],      zone, false, "a bounded zone has no cells to reduce within"),
            // The defects. Every one of these draws today, ignoring one of the two.
            (&[Smooth, Mean],      flat, false, "new: `smooth` already averages as it goes"),
            (&[Range, Confidence], flat, false, "new: two pairs, and the last one written wins"),
            (&[Sum, Range],        flat, false, "new: a reduction and a pair of the same column"),
            (&[Bounds, Mean],      flat, false, "new: mis-scoped until now, silent off a zone"),
            (&[Dodge, Stack],      flat, false, "new: side by side and piled at once"),
            (&[Proportion, Stack], share, false, "new: two divisions that cancel"),
            (&[Partition, Bin],    zone, false, "new: a hierarchy and a cut"),
            // Legal, and the rule must not take them away — every one is in the book.
            (&[Bin, Mean],         flat, true, "the cut yields its tally to the reduction"),
            (&[Bin, Range],        flat, true, "and to a pair just the same"),
            (&[Bin, Bounds],       flat, true, "off a zone, `bounds` is the measurement"),
            (&[Bin, Proportion],   flat, true, "the histogram read as shares"),
            (&[Sum, Proportion],   flat, true, "each slot's sum as a share of the total"),
            (&[Count, Proportion], flat, true, "redundant, but the plot is right — a warning, not a refusal"),
            (&[Sum, Dodge],        flat, true, "measure then arrange"),
            (&[Sum, Stack],        flat, true, "and the other arrangement"),
            (&[Bin, Stack],        flat, true, "the dot plot"),
            (&[Mean, Dodge],       flat, true, "the grouped bar chart"),
            (&[Partition, Proportion], zone, true, "a treemap read as shares"),
            (&[Bin, Mean, Dodge],  flat, true, "three jobs, three transforms"),
            (&[Bin, Mean, Proportion, Dodge], flat, true, "all four, which is the ceiling"),
        ];
        for (ts, ctx, legal, why) in cases {
            let got = job_conflict(ts, *ctx);
            assert_eq!(
                got.is_none(), *legal,
                "{ts:?}: expected {}, got {got:?} — {why}",
                if *legal { "legal" } else { "a conflict" }
            );
        }
    }

    /// **One filler per job caps a chain at four, and that is where the ceiling comes
    /// from.**
    ///
    /// Not a number anyone chose. Four jobs means a fifth transform has to repeat one,
    /// and repeating one is the contradiction above — so the limit derives instead of
    /// being remembered, which is what keeps it out of the reference card.
    #[test]
    fn a_legal_chain_is_at_most_four_transforms_long() {
        use Transform::*;
        assert!(job_conflict(&[Bin, Mean, Proportion, Dodge], JobContext::default()).is_none());
        for fifth in &EVERY_TRANSFORM {
            let mut chain = vec![Bin, Mean, Proportion, Dodge];
            chain.push(fifth.clone());
            assert!(
                job_conflict(&chain, JobContext::default()).is_some(),
                "a fifth transform ({fifth:?}) has to repeat a job, so no legal chain is five long"
            );
        }
    }

    // A unit test holds every row it is testing, so "derive the cut from the rows
    // you were given" is exactly what it means — the `None` case of [`BinCut`].
    // These three shadow the real entry points with that answer filled in, so the
    // tests below say what they are testing rather than restating a parameter that
    // only a faceted renderer has an opinion about. The shared-cut path is covered
    // deliberately instead, in `a_shared_cut_*` below and in `svg.rs`.
    #[allow(clippy::too_many_arguments)]
    fn apply(
        df: &DataFrame, transforms: &[Transform], key_field: &str, out_field: &str,
        bin_spec: Option<&BinSpec>, density_spec: Option<&DensitySpec>,
        conf_spec: Option<&ConfidenceSpec>, box_spec: Option<&BoxSpec>,
        bounds_spec: Option<&BoundsSpec>, stack_spec: Option<&StackSpec>,
        group_field: Option<&str>,
    ) -> DataFrame {
        super::apply(df, transforms, key_field, out_field, bin_spec, None,
                     density_spec, None, conf_spec, None, None, box_spec, bounds_spec,
                     stack_spec, group_field)
    }
    fn bin2d(df: &DataFrame, x_field: &str, y_field: &str, spec: Option<&BinSpec>) -> DataFrame {
        super::bin2d(df, x_field, y_field, spec, BinCut::default())
    }
    fn bin2d_agg(
        df: &DataFrame, x_field: &str, y_field: &str, val_field: &str,
        agg: AggFn, spec: Option<&BinSpec>,
    ) -> DataFrame {
        super::bin2d_agg(df, x_field, y_field, val_field, agg, spec, BinCut::default())
    }

    fn num(name: &str, vals: &[f64]) -> DataFrame {
        DataFrame::new().with_float(name, vals.to_vec())
    }
    fn xy(xs: &[f64], ys: &[f64]) -> DataFrame {
        DataFrame::new().with_float("x", xs.to_vec()).with_float("y", ys.to_vec())
    }
    fn cat_y(cats: &[&str], ys: &[f64]) -> DataFrame {
        DataFrame::new()
            .with_str("g", cats.iter().map(|s| s.to_string()).collect())
            .with_float("y", ys.to_vec())
    }
    fn col<'a>(df: &'a DataFrame, name: &str) -> &'a Vec<f64> {
        df.float_col(name).expect("numeric column")
    }

    // -- the keyless statistics: one value, no position axis ----------------
    //
    // A `bar` whose split *is* its segmentation has no `x` (spec §15), so the
    // statistic answers "one value for these rows" rather than "one value per x".
    // These pin what the pie and the share-of-total column are made of.

    fn split(gs: &[&str]) -> DataFrame {
        DataFrame::new().with_str("g", gs.iter().map(|s| s.to_string()).collect())
    }

    #[test]
    fn a_keyless_count_is_one_tally_per_group() {
        let df = split(&["a", "a", "a", "b", "b", "c"]);
        let out = apply(&df, &[Transform::Count], "", "n", None, None, None, None, None, None, Some("g"));
        assert_eq!(col(&out, "n"), &vec![3.0, 2.0, 1.0]);
        assert_eq!(out.str_col("g").unwrap(), &vec!["a".to_string(), "b".into(), "c".into()]);
    }

    #[test]
    fn a_keyless_stack_piles_the_whole_split_into_one_slot() {
        // Every element is at the same position because there is no position, so
        // the groups lay end to end and the last one carries the total. This is
        // what makes the pie close: the full turn is the sum.
        let df = split(&["a", "a", "a", "b", "b", "c"]);
        let out = apply(&df, &[Transform::Count, Transform::Stack], "", "n",
                        None, None, None, None, None, None, Some("g"));
        assert_eq!(col(&out, "n"), &vec![3.0, 5.0, 6.0], "cumulative tops");
        assert_eq!(col(&out, "stack_base"), &vec![0.0, 3.0, 5.0], "each sits on the one below");
    }

    /// The trap this cost an hour to find: with no `y()` the synthesized output
    /// column is *also* named `""`, so a position lookup of the empty key finds the
    /// counts and reads them as positions. Every group then looks like it is at a
    /// different place and nothing stacks — a pie drawn as overlapping wedges. The
    /// emptiness of the key must be asked before any column is consulted.
    #[test]
    fn a_keyless_stack_is_not_fooled_by_an_unnamed_output_column() {
        let df = split(&["a", "a", "a", "b", "b", "c"]);
        let out = apply(&df, &[Transform::Count, Transform::Stack], "", "",
                        None, None, None, None, None, None, Some("g"));
        assert_eq!(col(&out, ""), &vec![3.0, 5.0, 6.0]);
        assert_eq!(col(&out, "stack_base"), &vec![0.0, 3.0, 5.0]);
    }

    /// `pile` is `stack` spent on glyphs: the tally becomes one row per observation,
    /// each a rung above the one below (spec §5, the dot plot). The count is the
    /// *number of rows*, so the top rung equals the tally the un-piled frame carried.
    #[test]
    fn a_pile_turns_a_tally_back_into_one_row_per_observation() {
        let df = DataFrame::new()
            .with_float("x", vec![1.0, 2.0, 3.0])
            .with_float("n", vec![3.0, 0.0, 2.0]);
        let out = pile(&df, "n");

        assert_eq!(out.len(), 5, "three plus none plus two");
        assert_eq!(col(&out, "n"), &vec![1.0, 2.0, 3.0, 1.0, 2.0], "each pile counts up from its foot");
        // The x column comes along, so a dot knows which bin it is standing in —
        // and the empty bin drops out entirely rather than drawing a dot at zero.
        assert_eq!(col(&out, "x"), &vec![1.0, 1.0, 1.0, 3.0, 3.0]);
    }

    /// The pile reads the same `stack_base` every other mark reads, so a split piles
    /// group *b* on top of group *a* — a stacked bar drawn in dots.
    #[test]
    fn a_split_pile_starts_where_the_group_below_it_stopped() {
        let df = split(&["a", "a", "b", "b", "b"]);
        let counted = apply(&df, &[Transform::Count, Transform::Stack], "", "n",
                            None, None, None, None, None, None, Some("g"));
        assert_eq!(col(&counted, "n"), &vec![2.0, 5.0], "cumulative tops, as ever");

        let out = pile(&counted, "n");
        assert_eq!(col(&out, "n"), &vec![1.0, 2.0, 3.0, 4.0, 5.0],
            "five observations, five rungs, no gap and no overlap at the join");
        assert_eq!(out.str_col("g").unwrap(),
            &vec!["a".to_string(), "a".into(), "b".into(), "b".into(), "b".into()],
            "each dot keeps its group, so the colors pile in order");
    }

    /// The pile's floor is what made this a data-space rewrite rather than a
    /// renderer trick: the rungs reach *below* the tally the axis was fitted to, so
    /// on a frame whose smallest tally is 3 a render-stage pile drew dots at 1 and 2
    /// outside the panel and clipped them in silence.
    #[test]
    fn a_pile_reaches_below_the_tally_that_summarized_it() {
        let df = DataFrame::new().with_float("x", vec![1.0, 2.0]).with_float("n", vec![5.0, 3.0]);
        let lowest = col(&pile(&df, "n"), "n").iter().cloned().fold(f64::INFINITY, f64::min);
        assert_eq!(lowest, 1.0, "the bottom dot of every pile sits at one, not at the tally");
    }

    #[test]
    fn a_keyless_sum_totals_each_group() {
        let df = split(&["a", "a", "b"]).with_float("v", vec![2.0, 3.0, 10.0]);
        let out = apply(&df, &[Transform::Sum], "", "v", None, None, None, None, None, None, Some("g"));
        assert_eq!(col(&out, "v"), &vec![5.0, 10.0]);
    }

    /// A share is a fraction of the *total*, and inside the split each group can
    /// only see its own rows — so it counts, and the normalization happens once the
    /// groups have recombined. Without that the answer would be 1.0 everywhere.
    #[test]
    fn a_keyless_proportion_is_a_share_of_the_whole_not_of_the_group() {
        let df = split(&["a", "a", "a", "b"]);
        let out = apply(&df, &[Transform::Proportion], "", "p",
                        None, None, None, None, None, None, Some("g"));
        assert_eq!(col(&out, "p"), &vec![0.75, 0.25]);
        let total: f64 = col(&out, "p").iter().sum();
        assert!((total - 1.0).abs() < 1e-12, "shares must sum to one");
    }

    // -- bin ---------------------------------------------------------------

    #[test]
    fn bin_uses_sturges_number_of_bins() {
        // Sturges: k = ⌈log₂ n⌉ + 1. For n = 100 that is ⌈6.64⌉ + 1 = 8.
        let xs: Vec<f64> = (0..100).map(|i| i as f64).collect();
        let out = bin(&num("x", &xs), "x", "count", None, None);
        assert_eq!(col(&out, "x").len(), 8, "n=100 → 8 Sturges bins");
    }

    #[test]
    fn bin_counts_every_row_exactly_once() {
        // The clamp that folds the maximum into the last bin is what makes this
        // correct: without it the max overflows the count vector. Summing the counts back
        // to n is what proves no row was dropped or double-counted.
        let xs: Vec<f64> = (0..10).map(|i| i as f64).collect();
        let out = bin(&num("x", &xs), "x", "count", None, None);
        let total: f64 = col(&out, "count").iter().sum();
        assert_eq!(total, 10.0, "counts must sum to n");
    }

    #[test]
    fn bin_centers_are_evenly_spaced() {
        let xs: Vec<f64> = (0..10).map(|i| i as f64).collect();
        let out = bin(&num("x", &xs), "x", "count", None, None);
        let c = col(&out, "x");
        let step = c[1] - c[0];
        for w in c.windows(2) {
            assert!((w[1] - w[0] - step).abs() < 1e-9, "uneven bin centers: {c:?}");
        }
    }

    #[test]
    fn bin_refuses_a_categorical_column() {
        let df = DataFrame::new().with_str("x", vec!["a".into(), "b".into()]);
        let out = bin(&df, "x", "count", None, None);
        assert!(out.is_empty(), "a categorical column cannot be binned");
    }

    #[test]
    fn bin_honors_an_explicit_count() {
        // `bar * bin(30)` → exactly 30 bins, whatever Sturges would have said
        // (for n = 100 Sturges gives 8).
        let xs: Vec<f64> = (0..100).map(|i| i as f64).collect();
        let spec = BinSpec { bins: Some(30), width: None, tiling: None };
        let out = bin(&num("x", &xs), "x", "count", Some(&spec), None);
        assert_eq!(col(&out, "x").len(), 30, "explicit count overrides Sturges");
        // Every row still lands in exactly one bin.
        assert_eq!(col(&out, "count").iter().sum::<f64>(), 100.0);
    }

    #[test]
    fn bin_honors_an_explicit_width() {
        // `bar * bin(width = 10)` over 0..=99 (span 99) → ceil(99/10) = 10 bins,
        // and the centers sit exactly one width apart.
        let xs: Vec<f64> = (0..100).map(|i| i as f64).collect();
        let spec = BinSpec { bins: None, width: Some(10.0), tiling: None };
        let out = bin(&num("x", &xs), "x", "count", Some(&spec), None);
        let centers = col(&out, "x");
        assert_eq!(centers.len(), 10, "span 99 / width 10 → 10 bins");
        assert!((centers[1] - centers[0] - 10.0).abs() < 1e-9,
            "centers one width apart, got {}", centers[1] - centers[0]);
        assert_eq!(col(&out, "count").iter().sum::<f64>(), 100.0);
    }

    // -- bin in two dimensions (the tiling) --------------------------------

    #[test]
    fn bin2d_counts_every_row_into_exactly_one_cell() {
        // The 1-D invariant, one dimension up, and it is the one that matters: a
        // cell is a *partition*, so every row belongs to one and only one. Summing
        // the counts back to n is what proves neither clamp dropped a row nor
        // double-counted the corner where both maxima meet.
        let xs: Vec<f64> = (0..40).map(|i| (i % 8) as f64).collect();
        let ys: Vec<f64> = (0..40).map(|i| (i / 8) as f64).collect();
        let out = bin2d(&xy(&xs, &ys), "x", "y", None);
        assert_eq!(col(&out, CELL_COUNT).iter().sum::<f64>(), 40.0, "counts must sum to n");
    }

    #[test]
    fn bin2d_omits_the_cells_no_row_fell_in() {
        // A cell with no rows is not a zero, it is a place the data did not go —
        // and painting it the bottom of the ramp would claim a measurement nobody
        // made. Two tight clumps in opposite corners of a 4x4 mesh leave most of
        // it untouched, so a full grid of cells would be unmistakable.
        let xs = [0.0, 0.1, 9.9, 10.0];
        let ys = [0.0, 0.1, 9.9, 10.0];
        let spec = BinSpec { bins: Some(4), width: None, tiling: None };
        let out = bin2d(&xy(&xs, &ys), "x", "y", Some(&spec));
        assert_eq!(out.len(), 2, "two clumps, two cells — not the 16 of a full mesh");
        assert_eq!(col(&out, CELL_COUNT), &vec![2.0, 2.0]);
    }

    #[test]
    fn bin2d_puts_the_center_on_the_axes_and_the_extent_beside_it() {
        // The output contract a second tiling has to keep (spec §5): the position
        // columns carry the cell's **center**, which every tiling has, and the
        // extent rides in synthesized columns, which is where tilings differ.
        // Pinned as a relation rather than by echoing numbers — the center must be
        // the midpoint of the edges, and the edges must meet with no gap and no
        // overlap, which is what makes the cells a partition rather than a scatter
        // of rectangles that happen to be near each other.
        let xs: Vec<f64> = (0..20).map(|i| i as f64).collect();
        let ys: Vec<f64> = (0..20).map(|i| (i * 3) as f64).collect();
        let spec = BinSpec { bins: Some(5), width: None, tiling: None };
        let out = bin2d(&xy(&xs, &ys), "x", "y", Some(&spec));

        let (cx, cy) = (col(&out, "x"), col(&out, "y"));
        let (s, e) = (col(&out, CELL_START), col(&out, CELL_END));
        let (lo, hi) = (col(&out, CELL_LOWER), col(&out, CELL_UPPER));
        for i in 0..out.len() {
            assert!((cx[i] - (s[i] + e[i]) / 2.0).abs() < 1e-9, "x is the cell's center");
            assert!((cy[i] - (lo[i] + hi[i]) / 2.0).abs() < 1e-9, "y is the cell's center");
            assert!(e[i] > s[i] && hi[i] > lo[i], "a cell has positive extent");
        }
        // Every cell is one of five widths and one of five heights, all equal —
        // an equal-interval mesh, which is what `rect` binning means.
        let w = e[0] - s[0];
        let h = hi[0] - lo[0];
        for i in 0..out.len() {
            assert!((e[i] - s[i] - w).abs() < 1e-9, "one mesh width");
            assert!((hi[i] - lo[i] - h).abs() < 1e-9, "one mesh height");
        }
    }

    #[test]
    fn bin2d_cuts_each_axis_on_its_own_scale() {
        // One `BinSpec` cuts both axes into the same *number* of bins, not the same
        // width: the two axes are different quantities and a shared width would be
        // meaningless. Here y spans ten times what x does, so its cells must be ten
        // times as tall and exactly as many.
        let xs: Vec<f64> = (0..50).map(|i| i as f64).collect();
        let ys: Vec<f64> = (0..50).map(|i| (i * 10) as f64).collect();
        let spec = BinSpec { bins: Some(5), width: None, tiling: None };
        let out = bin2d(&xy(&xs, &ys), "x", "y", Some(&spec));
        let w = col(&out, CELL_END)[0] - col(&out, CELL_START)[0];
        let h = col(&out, CELL_UPPER)[0] - col(&out, CELL_LOWER)[0];
        assert!((h / w - 10.0).abs() < 1e-6, "y's cells are ten times as tall, got {}", h / w);
    }

    #[test]
    fn bin2d_refuses_two_categorical_axes_by_drawing_nothing() {
        // The fatal refusal is `legality::check_distribution_axis`'s, upstream of
        // here. This is the guard that keeps the transform *total* under
        // `GOG_STRICT=0`, where the user has read the diagnostic and asked to draw
        // anyway — it must return empty rather than panic on the missing column.
        //
        // *Two* categorical axes, since one is the mixed mesh and draws.
        let df = DataFrame::new()
            .with_str("g", vec!["a".into(), "b".into(), "c".into()])
            .with_str("h", vec!["p".into(), "q".into(), "p".into()]);
        assert_eq!(bin2d(&df, "g", "h", None).len(), 0);
        assert_eq!(bin2d(&df, "h", "g", None).len(), 0);
    }

    #[test]
    fn a_mixed_mesh_cuts_the_axis_with_a_width_and_slots_the_other() {
        // The extent description is per axis: the cut axis publishes its two edges
        // exactly as a rectangular mesh does, and the slotted one publishes nothing
        // at all exactly as a tally does. Which pair of names appears is the only
        // thing that turns on which axis was cut, because the mark asks the axis.
        let df = DataFrame::new()
            .with_float("v", vec![0.0, 1.0, 2.0, 3.0])
            .with_str("g", vec!["a".into(), "a".into(), "b".into(), "b".into()]);
        let spec = BinSpec { bins: Some(2), width: None, tiling: None };

        let out = bin2d(&df, "v", "g", Some(&spec));
        assert!(out.float_col(CELL_START).is_some() && out.float_col(CELL_END).is_some(),
            "the cut axis publishes its edges");
        assert!(out.float_col(CELL_LOWER).is_none() && out.float_col(CELL_UPPER).is_none(),
            "and the slotted axis publishes nothing — the axis already holds it");
        assert!(out.str_col("g").is_some(), "the category rides its own column");

        // The mirror, and the names swap with it.
        let out = bin2d(&df, "g", "v", Some(&spec));
        assert!(out.float_col(CELL_LOWER).is_some() && out.float_col(CELL_UPPER).is_some(),
            "cutting y publishes the measure axis's pair");
        assert!(out.float_col(CELL_START).is_none() && out.float_col(CELL_END).is_none());
    }

    #[test]
    fn a_mixed_mesh_cuts_every_category_on_the_same_edges() {
        // Per-category cutpoints would give each slot its own edges, and cells that
        // do not line up across the plot are not a mesh — the reason a grouped
        // histogram shares one `bin_layout`, one dimension along. Group "a" spans
        // 0..1 and "b" spans 8..9, so a per-group layout would give both the same
        // two cells; a shared one puts them at opposite ends of ten.
        let df = DataFrame::new()
            .with_float("v", vec![0.0, 1.0, 8.0, 9.0])
            .with_str("g", vec!["a".into(), "a".into(), "b".into(), "b".into()]);
        let spec = BinSpec { bins: Some(10), width: None, tiling: None };
        let out = bin2d(&df, "v", "g", Some(&spec));

        let (gs, starts) = (out.str_col("g").unwrap(), col(&out, CELL_START));
        let lo = |k: &str| gs.iter().zip(starts.iter())
            .filter(|(g, _)| *g == k).map(|(_, s)| *s).fold(f64::INFINITY, f64::min);
        assert!((lo("b") - lo("a")) > 7.0,
            "the two groups sit in different cells of one mesh, not the same cell of two: {} vs {}",
            lo("a"), lo("b"));
        // Every edge is a multiple of the shared step, which is what "one mesh" means.
        let step = (9.0 - 0.0) / 10.0;
        for s in starts { assert!((s / step - (s / step).round()).abs() < 1e-9, "edge {s} is off the mesh"); }
    }

    #[test]
    fn a_mixed_mesh_counts_every_row_into_exactly_one_cell() {
        // `bin2d`'s invariant, inherited: a tally loses nothing and double-counts
        // nothing, whichever axis was cut. Empty cells stay absent, which is why
        // this counts rows rather than cells.
        let df = DataFrame::new()
            .with_float("v", vec![1.0, 1.5, 2.0, 7.0, 9.0, 9.5, 3.0])
            .with_str("g", vec!["a".into(), "b".into(), "a".into(), "b".into(),
                                "a".into(), "a".into(), "b".into()]);
        for (a, b) in [("v", "g"), ("g", "v")] {
            let out = bin2d(&df, a, b, None);
            let total: f64 = col(&out, CELL_COUNT).iter().sum();
            assert_eq!(total, 7.0, "every row lands in exactly one cell ({a}, {b})");
        }
    }

    #[test]
    fn the_cell_columns_are_named_in_one_place() {
        // The transform writes these and the `zone` mark reads them, so the two
        // ends agree only because they consult the same list. Pinned because the
        // failure mode is silent: a renamed constant on one side draws an empty
        // panel, not an error.
        let b = cell_bounds();
        assert_eq!(b.domain(), Some((CELL_START, CELL_END)));
        assert_eq!(b.measure(), Some((CELL_LOWER, CELL_UPPER)));

        let out = bin2d(&xy(&[0.0, 1.0, 2.0], &[0.0, 1.0, 2.0]), "x", "y", None);
        for c in [CELL_START, CELL_END, CELL_LOWER, CELL_UPPER, CELL_COUNT] {
            assert!(out.float_col(c).is_some(), "`{c}` must be written by the transform");
        }
    }

    // -- bin in two dimensions, hexagonal ----------------------------------

    fn hex_spec(bins: usize) -> BinSpec {
        BinSpec { bins: Some(bins), width: None, tiling: Some("hex".into()) }
    }

    /// `hex_spec`'s rectangular sibling — a stated cell count, so a test can name the
    /// mesh it means instead of depending on Sturges' rule.
    fn bins(n: usize) -> BinSpec {
        BinSpec { bins: Some(n), width: None, tiling: None }
    }

    /// A cloud dense enough to fill a mesh, deterministic so the tests are.
    fn cloud(n: usize) -> DataFrame {
        let (mut xs, mut ys) = (Vec::new(), Vec::new());
        let (mut a, mut b) = (12345u64, 6789u64);
        for _ in 0..n {
            a = a.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            b = b.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            xs.push((a >> 11) as f64 / (1u64 << 53) as f64);
            ys.push((b >> 11) as f64 / (1u64 << 53) as f64);
        }
        xy(&xs, &ys)
    }

    #[test]
    fn hex_counts_every_row_into_exactly_one_cell() {
        // The partition invariant, and for a hexagonal mesh it is the one that
        // actually tests the geometry: the cells are found by asking which of two
        // interleaved lattices holds the nearer center, so a wrong metric shows up
        // here as points falling into cells that overlap or leave gaps.
        let out = bin2d(&cloud(2000), "x", "y", Some(&hex_spec(12)));
        assert_eq!(col(&out, CELL_COUNT).iter().sum::<f64>(), 2000.0, "counts must sum to n");
    }

    #[test]
    fn hex_centers_lie_on_two_interleaved_lattices() {
        // The signature of a hexagonal mesh, and the thing that makes it worth
        // having: **alternate rows are staggered**, so there is no aligned grid of
        // centers for the eye to read as structure. Stated as a parity fact, which
        // is exactly what "two lattices, one offset by half a step in both
        // directions" means — a center sits either on both whole steps or on both
        // half steps, never on one of each. A rectangular mesh would put every
        // center on a whole step in both, and would fail this.
        let out = bin2d(&cloud(2000), "x", "y", Some(&hex_spec(10)));
        let (cx, cy) = (col(&out, CELL_X), col(&out, CELL_Y));
        let (dx, dy) = (col(&out, CELL_DX)[0], col(&out, CELL_DY)[0]);
        // One step across is two half-widths; one step up is three half-heights.
        let (x_step, y_step) = (dx * 2.0, dy * 3.0);
        let x0 = cx.iter().cloned().fold(f64::INFINITY, f64::min);
        let y0 = cy.iter().cloned().fold(f64::INFINITY, f64::min);

        let mut staggered = false;
        for i in 0..out.len() {
            let j2 = (cx[i] - x0) / x_step * 2.0;
            let i2 = (cy[i] - y0) / y_step * 2.0;
            let (jr, ir) = (j2.round(), i2.round());
            assert!((j2 - jr).abs() < 1e-6 && (i2 - ir).abs() < 1e-6,
                "every center sits on a half step of the lattice: got ({j2}, {i2})");
            assert_eq!((jr as i64).rem_euclid(2), (ir as i64).rem_euclid(2),
                "a center is on both whole steps or both half steps, never one of each");
            if (jr as i64).rem_euclid(2) == 1 { staggered = true; }
        }
        assert!(staggered, "a hex mesh must actually use its offset rows, or it is a grid");
    }

    #[test]
    fn hex_cells_are_all_one_size() {
        // A mesh, not a scatter of hexagons: every cell is the same, which is what
        // lets the renderer read one half-extent pair and what makes the count
        // comparable from cell to cell.
        let out = bin2d(&cloud(1500), "x", "y", Some(&hex_spec(8)));
        let (dx, dy) = (col(&out, CELL_DX), col(&out, CELL_DY));
        assert!(dx.iter().all(|v| (v - dx[0]).abs() < 1e-12));
        assert!(dy.iter().all(|v| (v - dy[0]).abs() < 1e-12));
    }

    #[test]
    fn hex_cells_are_regular_in_the_space_they_are_cut_in() {
        // Pointy-top and *regular*: the height is 2/√3 of the width, which is the
        // hexagon's own proportion and the reason the mesh tessellates at all.
        // Measured in normalized units — both axes scaled to the same number of
        // steps — because that is the space the lattice is built in. On the page a
        // non-square panel then stretches it, which is `hexbin`'s `shape` question
        // and is answered in the book rather than pretended away.
        let df = cloud(1500);
        let out = bin2d(&df, "x", "y", Some(&hex_spec(10)));
        let (dx, dy) = (col(&out, CELL_DX)[0], col(&out, CELL_DY)[0]);

        // The half-extents come back in each column's **own** units, so comparing
        // them raw measures the data's aspect rather than the hexagon's. Divide
        // each by its axis's span and the units cancel, which is the normalized
        // space the lattice is actually built in. (Getting this wrong is the whole
        // subtlety of the mesh: a first version of this test asserted the raw
        // ratio and failed by 0.14%, which is the cloud's two spans differing.)
        let span = |f: &str| {
            let c = df.float_col(f).unwrap();
            c.iter().cloned().fold(f64::NEG_INFINITY, f64::max)
                - c.iter().cloned().fold(f64::INFINITY, f64::min)
        };
        let ratio = (dy / span("y")) / (dx / span("x"));
        assert!((ratio - 2.0 / 3.0_f64.sqrt()).abs() < 1e-9,
            "height/width must be 2/√3 for a regular pointy-top hexagon, got {ratio}");
    }

    #[test]
    fn hex_emits_a_stable_order() {
        // The cells are collected in a hash map, whose iteration order is not
        // stable across runs — so they are sorted before they leave. Without this
        // two renders of one spec would differ, which breaks the promise the
        // jitter seed already makes: one spec is one picture.
        let a = bin2d(&cloud(800), "x", "y", Some(&hex_spec(9)));
        let b = bin2d(&cloud(800), "x", "y", Some(&hex_spec(9)));
        assert_eq!(col(&a, CELL_X), col(&b, CELL_X));
        assert_eq!(col(&a, CELL_COUNT), col(&b, CELL_COUNT));
    }

    #[test]
    fn the_tilings_describe_their_own_cells() {
        // The output contract (spec §5), tested from both sides: each tiling
        // publishes the columns a mark needs to draw it, and neither has to know
        // what the plot's axes are called. This is the claim that let `hex` be
        // added without reopening anything — rect kept its four edges untouched
        // and hex put a center and a half-extent beside them.
        let df = cloud(500);
        let rect = bin2d(&df, "x", "y", None);
        for c in [CELL_START, CELL_END, CELL_LOWER, CELL_UPPER, CELL_COUNT] {
            assert!(rect.float_col(c).is_some(), "rect publishes `{c}`");
        }
        assert!(rect.float_col(CELL_DX).is_none(), "a rectangle has no half-extent to publish");

        let hex = bin2d(&df, "x", "y", Some(&hex_spec(10)));
        for c in [CELL_X, CELL_Y, CELL_DX, CELL_DY, CELL_COUNT] {
            assert!(hex.float_col(c).is_some(), "hex publishes `{c}`");
        }
        assert!(hex.float_col(CELL_START).is_none(), "a hexagon has no edges to name");
    }

    // -- the composed cut: `bin` supplies cells, a statistic measures them ---

    /// A frame whose answer is known by arithmetic rather than by a previous run:
    /// `v` is a constant per quadrant, so the mean in each cell of a 2×2 mesh *is*
    /// that constant.
    fn quadrants() -> DataFrame {
        let (mut xs, mut ys, mut vs) = (Vec::new(), Vec::new(), Vec::new());
        for i in 0..20 {
            for j in 0..20 {
                let (x, y) = (i as f64 * 5.0, j as f64 * 5.0);
                xs.push(x);
                ys.push(y);
                // 1, 2 / 3, 4 by quadrant of a 100×100 square.
                vs.push(1.0 + if x > 47.5 { 1.0 } else { 0.0 } + if y > 47.5 { 2.0 } else { 0.0 });
            }
        }
        DataFrame::new().with_float("x", xs).with_float("y", ys).with_float("v", vs)
    }

    #[test]
    fn a_composed_bin_cuts_and_keeps_its_rows_rather_than_tallying_them() {
        // The 1-D half of the composition ruling (spec §5). Alone, `bin` answers
        // *where are the cells* and *how many rows are in each*; composed, it answers
        // only the first — so every row survives, moved to its cell's center, and the
        // statistic that follows groups them exactly as it groups a category's rows.
        let df = DataFrame::new()
            .with_float("x", (0..100).map(|i| i as f64).collect())
            .with_float("y", (0..100).map(|i| (i / 25) as f64).collect());

        let tallied = apply(&df, &[Transform::Bin], "x", "y", Some(&bins(4)), None, None, None, None, None, None);
        assert_eq!(tallied.len(), 4, "a bin alone emits one row per cell");
        assert_eq!(tallied.float_col("y").unwrap(), &vec![25.0; 4], "…carrying the tally");

        // Composed, the same four cells — but `y` now holds the *mean of y*, which by
        // construction is 0, 1, 2, 3 rather than the count 25.
        let reduced = apply(&df, &[Transform::Bin, Transform::Mean], "x", "y", Some(&bins(4)), None, None, None, None, None, None);
        assert_eq!(reduced.len(), 4, "the mesh does not move when the measurement changes");
        assert_eq!(reduced.float_col("y").unwrap(), &vec![0.0, 1.0, 2.0, 3.0],
            "the named column is reduced within each cell, not overwritten by a tally");
        assert_eq!(reduced.float_col("x").unwrap(), tallied.float_col("x").unwrap(),
            "both readings cut the same axis into the same cells");
    }

    #[test]
    fn the_cut_runs_first_wherever_it_was_written() {
        // **`*` commutes among the transforms, and this pair was the first case of
        // it rather than the exception it was written as.** The comment here used to
        // say the opposite — "non-commutative in general and stays so, 49 of the 169
        // legal two-transform compositions draw differently reversed" — and the
        // measurement was right while the conclusion was not. Every one of those 49
        // was a chain where reversing it changed which transform got silently
        // discarded, so what the reversal exposed was the drop rather than a meaning.
        // Once the contradictions are refused (`legality::check_chain_jobs`), a legal
        // chain has at most one transform actually running in the sequence, and one
        // transform has no order: across 253 legal chains on every mark, on both a
        // categorical and a continuous domain, every permutation now renders
        // identically. The two here answer *different* questions, so there is nothing
        // for an order to decide. A cell has to exist before anything can be measured
        // in it, which makes the cut prior rather than merely earlier.
        //
        // The two-dimensional reading already worked this way (`svg.rs` dispatches on
        // which transforms are present, never on their order), so this is the two
        // readings agreeing. Left alone, `mean * bin` aggregated the raw rows into
        // their own groups and then cut *that*, drawing one bar per row.
        let df = DataFrame::new()
            .with_float("x", (0..100).map(|i| i as f64).collect())
            .with_float("y", (0..100).map(|i| (i / 25) as f64).collect());
        let forward = apply(&df, &[Transform::Bin, Transform::Mean], "x", "y", Some(&bins(4)), None, None, None, None, None, None);
        let reverse = apply(&df, &[Transform::Mean, Transform::Bin], "x", "y", Some(&bins(4)), None, None, None, None, None, None);
        assert_eq!(forward.len(), reverse.len(), "the same cells either way");
        assert_eq!(forward.float_col("y").unwrap(), reverse.float_col("y").unwrap(),
            "…measured the same way");
        assert_eq!(forward.float_col("x").unwrap(), reverse.float_col("x").unwrap(),
            "…at the same places");
    }

    #[test]
    fn a_composed_bin_does_not_silently_drop_the_statistic() {
        // The bug this ruling closed, pinned so it cannot come back. Until
        // 2026-07-26 every one-dimensional composition ran `bin` first, which
        // overwrote the named column with its own tally; the reduction then averaged
        // one count per cell and handed it straight back. The geometry was
        // byte-identical to a plain histogram and only the axis *title* changed — so
        // the plot read `Life` over a column of counts. A silent drop (§12) that
        // rendered and exited 0.
        let df = DataFrame::new()
            .with_float("x", (0..100).map(|i| i as f64).collect())
            .with_float("y", (0..100).map(|i| (i / 25) as f64).collect());
        let plain   = apply(&df, &[Transform::Bin], "x", "y", Some(&bins(4)), None, None, None, None, None, None);
        let reduced = apply(&df, &[Transform::Bin, Transform::Mean], "x", "y", Some(&bins(4)), None, None, None, None, None, None);
        assert_ne!(plain.float_col("y").unwrap(), reduced.float_col("y").unwrap(),
            "a composed statistic must change what is measured, or it was dropped");
    }

    #[test]
    fn the_summary_heatmap_reduces_the_named_column_inside_each_cut_cell() {
        // The two-dimensional reading, checked against arithmetic: `v` is constant
        // within each quadrant, so a 2×2 mesh must report exactly those constants.
        let out = bin2d_agg(&quadrants(), "x", "y", "v", AggFn::Mean, Some(&bins(2)));
        assert_eq!(out.len(), 4, "four cells, one per quadrant");
        let mut got = out.float_col("v").unwrap().clone();
        got.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert_eq!(got, vec![1.0, 2.0, 3.0, 4.0], "each cell reports its own quadrant's value");

        // And it publishes the extent a cut axis owes, exactly as `bin2d` does —
        // the output contract holding across the composition rather than being
        // restated for it.
        for c in [CELL_START, CELL_END, CELL_LOWER, CELL_UPPER] {
            assert!(out.float_col(c).is_some(), "a cut cell publishes `{c}`");
        }
        assert!(out.float_col(CELL_COUNT).is_none(),
            "a reduced cell measures the named column, so it synthesizes no tally");
    }

    #[test]
    fn a_mixed_summary_mesh_cuts_one_axis_and_slots_the_other() {
        // The extent description is per axis (spec §5), and composition does not
        // change that: cut x, slot y, and the output publishes edges for the first
        // and nothing at all for the second.
        let q = quadrants();
        let cats: Vec<String> = q.float_col("y").unwrap().iter()
            .map(|&v| if v > 47.5 { "high".to_string() } else { "low".to_string() }).collect();
        let df = DataFrame::new()
            .with_float("x", q.float_col("x").unwrap().clone())
            .with_str("y", cats)
            .with_float("v", q.float_col("v").unwrap().clone());

        let out = bin2d_agg(&df, "x", "y", "v", AggFn::Mean, Some(&bins(2)));
        assert_eq!(out.len(), 4, "two cut cells × two slots");
        let mut got = out.float_col("v").unwrap().clone();
        got.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert_eq!(got, vec![1.0, 2.0, 3.0, 4.0]);
        assert!(out.float_col(CELL_START).is_some(), "the cut axis publishes its edges");
        assert!(out.float_col(CELL_LOWER).is_none(),
            "the slotted axis publishes nothing — the scale already holds its bounds");
        assert!(out.str_col("y").is_some(), "…and keeps its categories");
    }

    #[test]
    fn a_reduced_cell_that_nothing_landed_in_is_absent_rather_than_zero() {
        // `bin2d`'s and `agg2d`'s shared rule, inherited rather than re-argued: a
        // cell no row reached has no mean, which is an absence. Painting it the floor
        // of the ramp would claim a measurement nobody made.
        // Two clumps on the diagonal — both positions low, or both high — so of the
        // sixteen cells a 4×4 mesh cuts, exactly two are ever reached.
        let df = DataFrame::new()
            .with_float("x", vec![0.0, 1.0, 98.0, 99.0])
            .with_float("y", vec![0.0, 1.0, 98.0, 99.0])
            .with_float("v", vec![5.0, 5.0, 9.0, 9.0]);
        let out = bin2d_agg(&df, "x", "y", "v", AggFn::Mean, Some(&bins(4)));
        assert_eq!(out.len(), 2, "only the cells rows actually reached");
        assert_eq!(out.float_col("v").unwrap(), &vec![5.0, 9.0],
            "each clump reports its own value, and the fourteen empty cells report nothing");
    }

    #[test]
    fn a_non_finite_value_is_dropped_rather_than_poisoning_its_cell() {
        // `agg2d`'s rule, kept: one NaN would otherwise make the whole cell's mean
        // NaN and paint it off the ramp entirely.
        let df = DataFrame::new()
            .with_float("x", vec![1.0, 2.0, 3.0])
            .with_float("y", vec![1.0, 1.0, 1.0])
            .with_float("v", vec![2.0, f64::NAN, 4.0]);
        let out = bin2d_agg(&df, "x", "y", "v", AggFn::Mean, Some(&bins(1)));
        assert_eq!(out.float_col("v").unwrap(), &vec![3.0],
            "the finite values are averaged and the NaN ignored");
    }

    #[test]
    fn a_tally_and_a_reduction_land_on_the_same_hexagonal_mesh() {
        // The tiling ruling under composition: a different mesh puts different rows
        // in different cells, and it must do that *identically* whether the cell is
        // then counted or reduced — otherwise the hexbin of a pair of columns and the
        // summary hexbin of the same pair cannot be compared. One `HexMesh` serves
        // both, and this is the assertion that keeps it that way.
        let df = cloud(500);
        let vs: Vec<f64> = (0..df.len()).map(|i| (i % 7) as f64).collect();
        let df = df.with_float("v", vs);

        let tallied = bin2d(&df, "x", "y", Some(&hex_spec(6)));
        let reduced = bin2d_agg(&df, "x", "y", "v", AggFn::Mean, Some(&hex_spec(6)));

        assert_eq!(tallied.len(), reduced.len(), "the same cells are occupied");
        assert_eq!(tallied.float_col(CELL_X).unwrap(), reduced.float_col(CELL_X).unwrap(),
            "…at the same centers");
        assert_eq!(tallied.float_col(CELL_DX).unwrap(), reduced.float_col(CELL_DX).unwrap(),
            "…and the same size");
        assert!(reduced.float_col("v").is_some(), "the reduction rides on the named column");
        assert!(reduced.float_col(CELL_COUNT).is_none(), "…and synthesizes no tally");
    }

    // -- grouped statistics (the split-by-color fix) ----------------------

    fn num_grp(xs: &[f64], groups: &[&str]) -> DataFrame {
        DataFrame::new()
            .with_float("x", xs.to_vec())
            .with_str("g", groups.iter().map(|s| s.to_string()).collect())
    }

    #[test]
    fn a_grouped_bin_shares_edges_and_tags_each_row() {
        // The heart of the overlaid histogram: two groups over one range must bin
        // on the *same* edges, or their bars would not line up when drawn on top
        // of each other. Each output row also carries its group label so the
        // renderer can color it — the column the old code dropped in silence.
        let df = num_grp(
            &[0.0, 1.0, 2.0,   0.0, 1.0, 2.0, 3.0, 4.0],
            &["a", "a", "a",   "b", "b", "b", "b", "b"],
        );
        let out = apply(&df, &[Transform::Bin], "x", "count", None, None, None, None, None, None, Some("g"));
        let g       = out.str_col("g").expect("group column carried through");
        let centers = out.float_col("x").expect("bin centers");
        let of = |want: &str| -> Vec<f64> {
            centers.iter().zip(g).filter(|(_, gg)| *gg == want).map(|(c, _)| *c).collect()
        };
        assert_eq!(of("a"), of("b"), "both groups share one set of bin edges");
        // No row lost or double-counted across the split.
        assert_eq!(out.float_col("count").unwrap().iter().sum::<f64>(), 8.0);
    }

    #[test]
    fn a_grouped_aggregation_splits_by_key_and_group() {
        // mean of y within each (x, group) cell — the grouped bar case. Group `a`
        // first (its keys in first-seen order), then group `b`.
        let df = DataFrame::new()
            .with_str("x", ["p", "p", "q", "q"].iter().map(|s| s.to_string()).collect())
            .with_float("y", vec![10.0, 20.0, 100.0, 200.0])
            .with_str("g", ["a", "b", "a", "b"].iter().map(|s| s.to_string()).collect());
        let out = apply(&df, &[Transform::Mean], "x", "y", None, None, None, None, None, None, Some("g"));
        assert_eq!(out.len(), 4, "2 keys × 2 groups");
        assert_eq!(out.str_col("x").unwrap(), &["p", "q", "p", "q"]);
        assert_eq!(out.str_col("g").unwrap(), &["a", "a", "b", "b"]);
        assert_eq!(out.float_col("y").unwrap(), &[10.0, 100.0, 20.0, 200.0]);
    }

    #[test]
    fn a_declared_group_order_survives_the_split() {
        // A factor's levels order the groups, so colors read Low, High even when
        // the rows arrive High first — the same rule the axis follows.
        let df = DataFrame::new()
            .with_float("x", vec![0.0, 1.0, 0.0, 1.0])
            .with_levels(
                "g",
                ["high", "high", "low", "low"].iter().map(|s| s.to_string()).collect(),
                vec!["low".into(), "high".into()],
            );
        let out = apply(&df, &[Transform::Bin], "x", "count", None, None, None, None, None, None, Some("g"));
        let g = out.str_col("g").unwrap();
        assert_eq!(g.first().map(String::as_str), Some("low"), "declared order wins");
        assert_eq!(out.levels("g"), Some(["low".to_string(), "high".to_string()].as_slice()));
    }

    #[test]
    fn without_a_group_a_transform_is_the_whole_frame() {
        // The degenerate case: no group means one group, byte-for-byte the old
        // path, and no stray group column invented.
        let xs: Vec<f64> = (0..10).map(|i| i as f64).collect();
        let grouped = apply(&num("x", &xs), &[Transform::Bin], "x", "count", None, None, None, None, None, None, None);
        let direct  = bin(&num("x", &xs), "x", "count", None, None);
        assert_eq!(grouped.float_col("x"), direct.float_col("x"));
        assert_eq!(grouped.float_col("count"), direct.float_col("count"));
        assert!(grouped.column_names().count() == 2, "no group column when none asked for");
    }

    // -- stack (the cross-group collision modifier) ------------------------

    /// A frame of `g` bands over `p` positions, thicknesses given band-major.
    fn bands(p: usize, g: usize, v: &[f64]) -> DataFrame {
        let mut key = Vec::new();
        let mut grp = Vec::new();
        for b in 0..g {
            for i in 0..p {
                key.push(i as f64);
                grp.push(format!("b{b}"));
            }
        }
        DataFrame::new().with_float("t", key).with_str("g", grp).with_float("v", v.to_vec())
    }

    /// The quantity a streamgraph exists to make small: how far each band's own middle
    /// travels from one position to the next, squared, weighted by how thick the band
    /// is there. Byron and Wattenberg's "wiggle", computed off the laid-out spans so it
    /// measures the *drawing* rather than the formula that produced it.
    fn wiggle_of(out: &DataFrame) -> f64 {
        let (top, base) = (out.float_col("v").unwrap(), out.float_col(STACK_BASE).unwrap());
        let g = out.str_col("g").unwrap();
        let t = out.float_col("t").unwrap();
        let mut total = 0.0;
        for b in g.iter().collect::<std::collections::BTreeSet<_>>() {
            let mut rows: Vec<usize> = (0..g.len()).filter(|&i| &g[i] == b).collect();
            rows.sort_by(|&a, &c| t[a].partial_cmp(&t[c]).unwrap());
            let mid = |i: usize| (top[i] + base[i]) / 2.0;
            for w in rows.windows(2) {
                let thick = top[w[1]] - base[w[1]];
                total += thick * (mid(w[1]) - mid(w[0])).powi(2);
            }
        }
        total
    }

    #[test]
    fn the_wiggle_baseline_actually_wiggles_least_and_center_sits_on_zero() {
        // The parameter's whole claim, checked as a *measurement* rather than as a
        // picture: `"wiggle"` exists only because the floor-anchored pile has a defect
        // the eye can see, where every band above the bottom one rides the sum of the
        // ones below it. If this ordering ever reverses, the layout is wrong however
        // plausible the plot looks.
        let df = bands(6, 3, &[
            4.0, 9.0, 3.0, 8.0, 2.0, 7.0,   // a band that swings hard
            5.0, 5.0, 5.0, 5.0, 5.0, 5.0,   // one that does not move at all
            2.0, 3.0, 9.0, 2.0, 8.0, 3.0,   // and one that swings the other way
        ]);
        let run = |b: Option<&str>| {
            let spec = b.map(|b| StackSpec { share: None, baseline: Some(b.to_string()) });
            apply(&df, &[Transform::Stack], "t", "v", None, None, None, None, None,
                  spec.as_ref(), Some("g"))
        };
        let (zero, center, wiggle) = (run(None), run(Some("center")), run(Some("wiggle")));

        let (wz, wc, ww) = (wiggle_of(&zero), wiggle_of(&center), wiggle_of(&wiggle));
        assert!(ww < wc && wc < wz,
            "the three layouts should order wiggle < center < zero, got \
             wiggle={ww:.2} center={wc:.2} zero={wz:.2}");

        // `"center"` is the *symmetric* layout, which is a different promise and worth
        // pinning separately: every pile straddles zero, whatever its total.
        let (top, base) = (center.float_col("v").unwrap(), center.float_col(STACK_BASE).unwrap());
        let t = center.float_col("t").unwrap();
        for pos in 0..6 {
            let rows: Vec<usize> = (0..t.len()).filter(|&i| t[i] == pos as f64).collect();
            let lo = rows.iter().map(|&i| base[i]).fold(f64::INFINITY, f64::min);
            let hi = rows.iter().map(|&i| top[i]).fold(f64::NEG_INFINITY, f64::max);
            assert!((lo + hi).abs() < 1e-9, "pile {pos} straddles zero: {lo} .. {hi}");
        }

        // And displacing never changes a *thickness*: it moves the pile bodily, so
        // every reading the plot supports survives it and only the origin is spent.
        for other in [&center, &wiggle] {
            let (t2, b2) = (other.float_col("v").unwrap(), other.float_col(STACK_BASE).unwrap());
            let (t0, b0) = (zero.float_col("v").unwrap(), zero.float_col(STACK_BASE).unwrap());
            for i in 0..t0.len() {
                assert!(((t2[i] - b2[i]) - (t0[i] - b0[i])).abs() < 1e-9,
                    "row {i} changed thickness under a displaced baseline");
            }
        }
    }

    #[test]
    fn stack_piles_groups_and_records_each_baseline() {
        // Two groups over two categories. `stack` rewrites `y` to each element's
        // cumulative *top* and adds `stack_base` with its cumulative *bottom*, the
        // first group (category order: a, then b) on the floor. The pairing is what
        // lets a bar span [base, top] and the axis read the stacked total off `y`.
        let df = DataFrame::new()
            .with_str("x", ["p", "q", "p", "q"].iter().map(|s| s.to_string()).collect())
            .with_float("y", vec![10.0, 30.0, 5.0, 7.0])
            .with_str("g", ["a", "a", "b", "b"].iter().map(|s| s.to_string()).collect());
        let out = apply(&df, &[Transform::Stack], "x", "y", None, None, None, None, None, None, Some("g"));
        // apply_grouped emits group-major in category order: (a,p),(a,q),(b,p),(b,q).
        assert_eq!(out.float_col("stack_base").unwrap(), &[0.0, 0.0, 10.0, 30.0],
            "group a sits on zero; group b sits on a's height at the same x");
        assert_eq!(out.float_col("y").unwrap(), &[10.0, 30.0, 15.0, 37.0],
            "each top is its baseline plus its own value");
        // The measure axis reads `y`, so the tallest stack (37) is the domain top —
        // taller than any single value (30), which is the whole point of stacking.
        assert_eq!(out.float_col("y").unwrap().iter().cloned().fold(0.0, f64::max), 37.0);
        assert!(out.str_col("g").is_some(), "the split column survives so the renderer can color it");
    }

    #[test]
    fn stack_of_one_group_rests_on_zero() {
        // No split → nothing to pile: every element keeps its value and sits on a
        // zero baseline, byte-identical to an un-stacked bar. (`check_stack` refuses
        // this upstream; the transform stays a no-op rather than inventing an offset.)
        let out = apply(&num("x", &[1.0, 2.0, 3.0]).with_float("y", vec![4.0, 5.0, 6.0]),
            &[Transform::Stack], "x", "y", None, None, None, None, None, None, None);
        assert_eq!(out.float_col("y").unwrap(), &[4.0, 5.0, 6.0]);
        assert_eq!(out.float_col("stack_base").unwrap(), &[0.0, 0.0, 0.0]);
    }

    // -- factor order survives a summary -----------------------------------

    #[test]
    fn a_summary_carries_the_factor_order_onto_its_key() {
        // `factor(cyl)` declares 4, 6, 8; the rows arrive 6, 4, 8 (first-seen).
        // Before the fix `count` rebuilt the key with `with_str` and dropped the
        // levels, so the axis read 6, 4, 8. It must carry the declared order through.
        let df = DataFrame::new().with_levels(
            "cyl",
            ["6", "4", "8", "6", "4"].iter().map(|s| s.to_string()).collect(),
            vec!["4".into(), "6".into(), "8".into()],
        );
        let out = count(&df, "cyl", "n");
        assert_eq!(
            crate::data::categories_across(&[&out], "cyl"),
            vec!["4".to_string(), "6".to_string(), "8".to_string()],
            "count must preserve the factor's declared order, not first-appearance"
        );

        // A plain (non-factor) string column has no levels → first-appearance order,
        // exactly as before: the fix bites only where a declared order exists to keep.
        let plain = DataFrame::new().with_str("g", ["b", "a", "b"].iter().map(|s| s.to_string()).collect());
        assert_eq!(
            crate::data::categories_across(&[&count(&plain, "g", "n")], "g"),
            vec!["b".to_string(), "a".to_string()],
            "a non-factor column keeps first-appearance order"
        );
    }

    #[test]
    fn apply_keeps_factor_order_through_both_paths() {
        // The render path calls `apply`, not `count` directly, so the levels must
        // survive the wrapper — ungrouped (apply_seq) and grouped (apply_grouped +
        // vconcat). This is the case the mtcars axis exercises.
        let lv = || vec!["4".to_string(), "6".to_string(), "8".to_string()];
        let ungrouped = DataFrame::new().with_levels(
            "cyl", ["6", "4", "8", "6", "4"].iter().map(|s| s.to_string()).collect(), lv());
        let out = apply(&ungrouped, &[Transform::Count], "cyl", "n", None, None, None, None, None, None, None);
        assert_eq!(out.levels("cyl"), Some(lv().as_slice()), "ungrouped apply dropped the key levels");

        let grouped = DataFrame::new()
            .with_levels("cyl", ["6", "4", "8", "6", "4"].iter().map(|s| s.to_string()).collect(), lv())
            .with_str("vs", ["0", "1", "0", "1", "0"].iter().map(|s| s.to_string()).collect());
        let outg = apply(&grouped, &[Transform::Count], "cyl", "n", None, None, None, None, None, None, Some("vs"));
        assert_eq!(crate::data::categories_across(&[&outg], "cyl"), lv(), "grouped apply lost the key order");
    }

    #[test]
    fn a_summary_drops_a_factor_level_with_no_rows() {
        // Declared 4, 5, 6, 8 but no 5s present: the empty level leaves no slot (the
        // ragged-factor choice), while the order among the present ones holds. This
        // falls out of `categories_across`, which orders by levels and drops absent.
        let df = DataFrame::new()
            .with_levels(
                "cyl",
                ["8", "4", "6"].iter().map(|s| s.to_string()).collect(),
                vec!["4".into(), "5".into(), "6".into(), "8".into()],
            )
            .with_float("y", vec![1.0, 2.0, 3.0]);
        let out = aggregate(&df, "cyl", "y", AggFn::Sum);
        assert_eq!(
            crate::data::categories_across(&[&out], "cyl"),
            vec!["4".to_string(), "6".to_string(), "8".to_string()],
            "declared order kept; the empty `5` level leaves no slot"
        );
    }

    // -- count -------------------------------------------------------------

    #[test]
    fn count_tallies_each_category_in_first_seen_order() {
        let df = DataFrame::new().with_str(
            "g",
            ["b", "a", "b", "b", "a"].iter().map(|s| s.to_string()).collect(),
        );
        let out = count(&df, "g", "count");
        assert_eq!(out.str_col("g").unwrap(), &vec!["b".to_string(), "a".to_string()]);
        assert_eq!(col(&out, "count"), &vec![3.0, 2.0]);
    }

    #[test]
    fn count_sorts_numeric_keys_ascending() {
        let out = count(&num("x", &[3.0, 1.0, 3.0, 2.0, 1.0, 3.0]), "x", "count");
        assert_eq!(col(&out, "x"), &vec![1.0, 2.0, 3.0]);
        assert_eq!(col(&out, "count"), &vec![2.0, 1.0, 3.0]);
    }

    // -- proportion --------------------------------------------------------

    // Both tests go through `apply` rather than calling `proportion` directly,
    // and that is the point rather than a detail: since 2026-07-26 the division
    // is `apply`'s, run once over the recombined frame, and `proportion` itself
    // only tallies. Testing the private function would now assert the tally and
    // say nothing at all about the word's meaning.
    fn shares(df: &DataFrame, key: &str, group: Option<&str>) -> Vec<f64> {
        let out = apply(df, &[Transform::Proportion], key, "p",
                        None, None, None, None, None, None, group);
        col(&out, "p").clone()
    }

    #[test]
    fn proportion_is_each_count_over_the_row_total() {
        // Six rows: Europe 3, Asia 2, Africa 1 → 0.5, 0.333…, 0.166…
        let df = DataFrame::new().with_str(
            "g",
            ["Europe", "Asia", "Europe", "Africa", "Europe", "Asia"]
                .iter().map(|s| s.to_string()).collect(),
        );
        let p = shares(&df, "g", None);
        assert!((p[0] - 0.5).abs() < 1e-12);
        assert!((p[1] - 2.0 / 6.0).abs() < 1e-12);
        assert!((p[2] - 1.0 / 6.0).abs() < 1e-12);
    }

    #[test]
    fn proportions_sum_to_one() {
        let p = shares(&num("x", &[1.0, 1.0, 2.0, 3.0, 3.0, 3.0, 4.0]), "x", None);
        let s: f64 = p.iter().sum();
        assert!((s - 1.0).abs() < 1e-12, "proportions summed to {s}");
    }

    /// The measured defect this session closed, pinned so it cannot come back.
    ///
    /// A `color` split used to normalize *inside* each group, so two seasons of
    /// wind directions each summed to 1 and the plot summed to 2 — the conditional
    /// distribution twice over, where the word had always been defined (§5) as a
    /// share of the whole frame. Law 6: one meaning in every context.
    #[test]
    fn a_split_does_not_give_each_group_its_own_denominator() {
        let df = DataFrame::new()
            .with_str("dir", ["N", "N", "S", "N", "S", "S", "S", "N"]
                .iter().map(|s| s.to_string()).collect())
            .with_str("season", ["Su", "Su", "Su", "Wi", "Wi", "Wi", "Su", "Wi"]
                .iter().map(|s| s.to_string()).collect());

        let split: f64 = shares(&df, "dir", Some("season")).iter().sum();
        assert!((split - 1.0).abs() < 1e-12, "split plot summed to {split}, not 1");

        // And the unsplit reading is unchanged, which is what makes it one rule
        // rather than a second one for the split case.
        let plain: f64 = shares(&df, "dir", None).iter().sum();
        assert!((plain - 1.0).abs() < 1e-12, "unsplit plot summed to {plain}, not 1");
    }

    /// The relative-frequency histogram: `bin` keeps its cut *and* its tally, and
    /// `proportion` rescales the tally it finds (spec §5).
    ///
    /// Refused for one day as "two synthesizing transforms", on the strength of a
    /// plot that had come out twelve equal bars at 1/12 — which was the sequencing
    /// (`proportion` re-counting the binned frame, where every cell appears once)
    /// and not the sentence. So the assertion that matters is that the bars are
    /// **not** all equal, and that they are the histogram's own counts divided by n.
    #[test]
    fn a_binned_proportion_is_the_histogram_read_as_fractions() {
        let xs: Vec<f64> = (0..40).map(|i| (i % 7) as f64 + (i / 20) as f64 * 0.5).collect();
        let df = num("x", &xs);
        let counts = col(&apply(&df, &[Transform::Bin], "x", "y",
                                None, None, None, None, None, None, None), "y").clone();
        let shares = col(&apply(&df, &[Transform::Bin, Transform::Proportion], "x", "y",
                                None, None, None, None, None, None, None), "y").clone();

        assert_eq!(counts.len(), shares.len(), "the mesh moved when the measurement did");
        let n: f64 = counts.iter().sum();
        for (c, s) in counts.iter().zip(&shares) {
            assert!((c / n - s).abs() < 1e-12, "{c}/{n} != {s}");
        }
        let total: f64 = shares.iter().sum();
        assert!((total - 1.0).abs() < 1e-12, "shares summed to {total}");
        // The defect that started this, stated as itself: twelve equal bars.
        let first = shares[0];
        assert!(shares.iter().any(|s| (s - first).abs() > 1e-9),
                "every bar the same height — the 1/12 plot is back: {shares:?}");
    }

    /// `stack(share = true)` fills every pile to exactly 1, whatever measured it.
    #[test]
    fn a_filled_pile_reaches_one_in_every_slot() {
        let df = DataFrame::new()
            .with_str("k", ["a", "a", "b", "b"].iter().map(|s| s.to_string()).collect())
            .with_str("g", ["x", "y", "x", "y"].iter().map(|s| s.to_string()).collect())
            .with_float("v", vec![3.0, 1.0, 5.0, 15.0]);
        let spec = StackSpec { share: Some(true), baseline: None };
        let out = apply(&df, &[Transform::Sum, Transform::Stack], "k", "v",
                        None, None, None, None, None, Some(&spec), Some("g"));
        let tops = col(&out, "v");
        let base = col(&out, "stack_base");
        // Each slot's tallest top is its pile's total, and every one of them is 1.
        for (t, b) in tops.iter().zip(base) {
            assert!(*t <= 1.0 + 1e-12, "a filled pile overshot 1: {t}");
            assert!(*b >= -1e-12 && *b < 1.0, "a foot left the unit interval: {b}");
        }
        let mut totals: Vec<f64> = tops.clone();
        totals.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert!((totals[totals.len() - 1] - 1.0).abs() < 1e-12,
                "no pile reached 1: {tops:?}");
        // 3:1 and 5:15 are different compositions, so the filled bars differ —
        // a fill that ignored the values would put every boundary in one place.
        assert!(tops.iter().any(|t| (t - tops[0]).abs() > 1e-9),
                "every segment identical — the fill lost the composition: {tops:?}");
    }

    // -- aggregate family --------------------------------------------------

    #[test]
    fn sum_and_mean_group_y_by_x() {
        let df = cat_y(&["a", "b", "a", "b", "a"], &[1.0, 10.0, 2.0, 20.0, 3.0]);
        let s = aggregate(&df, "g", "y", AggFn::Sum);
        assert_eq!(s.str_col("g").unwrap(), &vec!["a".to_string(), "b".to_string()]);
        assert_eq!(col(&s, "y"), &vec![6.0, 30.0]); // a:1+2+3, b:10+20
        let m = aggregate(&df, "g", "y", AggFn::Mean);
        assert_eq!(col(&m, "y"), &vec![2.0, 15.0]); // a:6/3, b:30/2
    }

    #[test]
    fn median_averages_the_two_middles_on_an_even_count() {
        // The even-count midpoint is where a naive median goes wrong.
        let df = cat_y(&["a", "a", "a", "a"], &[10.0, 2.0, 8.0, 4.0]);
        // sorted 2,4,8,10 → (4+8)/2 = 6
        let out = aggregate(&df, "g", "y", AggFn::Median);
        assert_eq!(col(&out, "y"), &vec![6.0]);
    }

    #[test]
    fn median_of_an_odd_count_is_the_middle_value() {
        let df = cat_y(&["a", "a", "a"], &[7.0, 1.0, 5.0]);
        let out = aggregate(&df, "g", "y", AggFn::Median); // sorted 1,5,7 → 5
        assert_eq!(col(&out, "y"), &vec![5.0]);
    }

    #[test]
    fn max_and_min_pick_the_extremes_per_group() {
        let df = cat_y(&["a", "b", "a", "b"], &[3.0, -1.0, -5.0, 9.0]);
        assert_eq!(col(&aggregate(&df, "g", "y", AggFn::Max), "y"), &vec![3.0, 9.0]);
        assert_eq!(col(&aggregate(&df, "g", "y", AggFn::Min), "y"), &vec![-5.0, -1.0]);
    }

    /// One NaN in a keyed group must not poison the group's number: the keyless
    /// reading filters non-finite values, and Law 2 says the keyed readings do
    /// exactly the same. A group with nothing finite reduces to NaN, which no
    /// mark draws.
    #[test]
    fn a_non_finite_value_is_dropped_from_a_keyed_group_as_the_keyless_reading_drops_it() {
        let df = cat_y(
            &["a", "a", "b", "b"],
            &[2.0, f64::NAN, f64::NAN, f64::NAN],
        );
        let m = aggregate(&df, "g", "y", AggFn::Mean);
        assert_eq!(m.str_col("g").unwrap(), &vec!["a".to_string(), "b".to_string()]);
        let got = col(&m, "y");
        assert_eq!(got[0], 2.0, "the finite value alone is the group's mean");
        assert!(got[1].is_nan(), "a group with nothing finite reduces to NaN");
        // The numeric-x keying goes through the same reduction.
        let df = DataFrame::new()
            .with_float("x", vec![1.0, 1.0, 2.0])
            .with_float("y", vec![4.0, f64::INFINITY, 6.0]);
        let m = aggregate(&df, "x", "y", AggFn::Sum);
        assert_eq!(col(&m, "y"), &vec![4.0, 6.0]);
    }

    // -- smooth ------------------------------------------------------------

    #[test]
    fn smooth_evaluates_at_100_points_across_the_data_span() {
        let xs: Vec<f64> = (0..20).map(|i| i as f64).collect();
        let ys: Vec<f64> = xs.iter().map(|&x| x * x).collect();
        let out = smooth(&xy(&xs, &ys), "x", "y");
        let sx = col(&out, "x");
        assert_eq!(sx.len(), 100);
        assert!((sx[0] - 0.0).abs() < 1e-9 && (sx[99] - 19.0).abs() < 1e-9,
                "spans [min, max]");
    }

    #[test]
    fn smooth_of_a_straight_line_returns_that_line() {
        // The invariant that catches a broken LOESS: local *linear* regression
        // over globally collinear points reproduces the line exactly, at every
        // evaluation point including the one-sided boundaries.
        let xs: Vec<f64> = (0..30).map(|i| i as f64 * 0.5).collect();
        let ys: Vec<f64> = xs.iter().map(|&x| 2.0 * x + 1.0).collect();
        let out = smooth(&xy(&xs, &ys), "x", "y");
        let (sx, sy) = (col(&out, "x"), col(&out, "y"));
        for (&x, &y) in sx.iter().zip(sy.iter()) {
            assert!((y - (2.0 * x + 1.0)).abs() < 1e-6, "LOESS bent a line at x={x}: y={y}");
        }
    }

    #[test]
    fn smooth_needs_at_least_three_points() {
        let out = smooth(&xy(&[0.0, 1.0], &[0.0, 1.0]), "x", "y");
        assert_eq!(col(&out, "x").len(), 2, "too few points → data returned unchanged");
    }

    // -- density -----------------------------------------------------------

    fn trapezoid(xs: &[f64], ys: &[f64]) -> f64 {
        xs.windows(2).zip(ys.windows(2))
            .map(|(x, y)| 0.5 * (y[0] + y[1]) * (x[1] - x[0]))
            .sum()
    }

    #[test]
    fn density_evaluates_at_256_non_negative_points() {
        let xs: Vec<f64> = (0..50).map(|i| (i as f64 * 0.3).sin() * 10.0 + 20.0).collect();
        let out = density(&num("x", &xs), "x", "d", None);
        assert_eq!(col(&out, "x").len(), 256);
        assert!(col(&out, "d").iter().all(|&d| d >= 0.0), "a density is never negative");
    }

    #[test]
    fn density_integrates_to_about_one() {
        // The defining property of a probability density. A KDE over ℝ integrates
        // to exactly 1; over the finite [min−3h, max+3h] grid it loses only the
        // far tails, so the trapezoid rule should land within a hair of 1.
        let xs: Vec<f64> = (0..80).map(|i| ((i * 37) % 50) as f64).collect();
        let out = density(&num("x", &xs), "x", "d", None);
        let area = trapezoid(col(&out, "x"), col(&out, "d"));
        assert!((area - 1.0).abs() < 0.03, "density integrated to {area}, expected ≈ 1");
    }

    // ---------------------------------------------------------------------
    // density in two dimensions — the field, the contour, the cells
    //
    // Pinned by properties a *correct* implementation must have, on the same
    // principle as the 1-D estimators above: a field integrates to one, a ring
    // closes, a mesh tiles. A test that echoed today's vertex floats would pass a
    // wrong rewrite just as happily.
    // ---------------------------------------------------------------------

    /// Two well-separated Gaussian-ish blobs — the shape that makes a contour worth
    /// drawing, and the one case where a level encloses more than one ring.
    fn two_modes() -> DataFrame {
        let mut xs = Vec::new();
        let mut ys = Vec::new();
        for i in 0..60 {
            let t = i as f64;
            xs.push(2.0 + (t * 0.7).sin());
            ys.push(2.0 + (t * 1.3).cos());
            xs.push(12.0 + (t * 0.9).cos());
            ys.push(12.0 + (t * 1.1).sin());
        }
        xy(&xs, &ys)
    }

    #[test]
    fn a_field_integrates_to_about_one() {
        // The defining property of a *density* in two dimensions, and the reason the
        // estimate is normalized at all: `color(density)` has to be a number in the
        // two columns' own units. Summing the cell values times the cell area is the
        // 2-D counterpart of the trapezoid check on the curve.
        let df = two_modes();
        let cells = density2d_cells(&df, "x", "y", None);
        let v = col(&cells, FIELD_DENSITY);
        let (s, e) = (col(&cells, CELL_START), col(&cells, CELL_END));
        let (lo, hi) = (col(&cells, CELL_LOWER), col(&cells, CELL_UPPER));
        let mass: f64 = (0..v.len())
            .map(|i| v[i] * (e[i] - s[i]) * (hi[i] - lo[i]))
            .sum();
        assert!((mass - 1.0).abs() < 0.05, "the field integrated to {mass}, expected ≈ 1");
    }

    #[test]
    fn the_fields_cells_tile_the_plane_with_no_gap_and_no_overlap() {
        // A mesh is only a mesh if the cells meet. Each row of the grid must hand its
        // right edge to the next cell's left edge exactly — which is also what makes
        // the emitted rectangles seamless on the page instead of hairline-striped.
        let cells = density2d_cells(&two_modes(), "x", "y", None);
        let (s, e) = (col(&cells, CELL_START), col(&cells, CELL_END));
        let (lo, hi) = (col(&cells, CELL_LOWER), col(&cells, CELL_UPPER));
        let width = GRID - 1;
        assert_eq!(cells.len(), width * width, "one row per cell of the grid");
        for i in 0..cells.len() {
            // Not the last in its row: the next cell continues from this one's edge.
            if (i + 1) % width != 0 {
                assert!((e[i] - s[i + 1]).abs() < 1e-9, "gap in x at cell {i}");
                assert!((lo[i] - lo[i + 1]).abs() < 1e-9, "row {i} is not level");
            }
            // The cell one row up starts where this one's top edge is.
            if i + width < cells.len() {
                assert!((hi[i] - lo[i + width]).abs() < 1e-9, "gap in y at cell {i}");
            }
            assert!(e[i] > s[i] && hi[i] > lo[i], "cell {i} has no area");
        }
    }

    #[test]
    fn a_contour_traces_as_many_levels_as_were_asked_for() {
        // The `levels` knob, and the reason the cutpoints are fractions of the *peak*
        // rather than pretty numbers: every level then encloses something, so asking
        // for four gives four and not "four unless one missed the data".
        for want in [1_usize, 4, 9] {
            let spec = DensitySpec { levels: Some(want), ..Default::default() };
            let out = density2d_contour(&two_modes(), "x", "y", Some(&spec));
            let mut seen: Vec<f64> = col(&out, FIELD_LEVEL).to_vec();
            seen.sort_by(|a, b| a.partial_cmp(b).unwrap());
            seen.dedup_by(|a, b| (*a - *b).abs() < 1e-12);
            assert_eq!(seen.len(), want, "asked for {want} levels, traced {}", seen.len());
        }
    }

    #[test]
    fn every_contour_ring_closes_on_itself() {
        // A traced iso-line of a field whose grid reaches past the data on every side
        // never runs off the edge, so each walk must come back to where it began.
        // This is what `chain`'s exact-bit endpoint matching buys: joined on an
        // epsilon, rings fray into open arcs instead.
        let out = density2d_contour(&two_modes(), "x", "y", None);
        let ring = col(&out, FIELD_RING);
        let (xs, ys) = (col(&out, "x"), col(&out, "y"));
        let mut start = 0;
        let mut rings = 0;
        for i in 0..=ring.len() {
            if i == ring.len() || (i > start && ring[i] != ring[start]) {
                let last = i - 1;
                assert!(
                    (xs[start] - xs[last]).abs() < 1e-9 && (ys[start] - ys[last]).abs() < 1e-9,
                    "ring {} runs from ({}, {}) to ({}, {}) without closing",
                    ring[start], xs[start], ys[start], xs[last], ys[last],
                );
                assert!(last - start >= 3, "ring {} has too few vertices to enclose anything", ring[start]);
                rings += 1;
                start = i;
            }
        }
        assert!(rings >= crate::ir::DEFAULT_LEVELS, "expected at least one ring per level, got {rings}");
    }

    #[test]
    fn one_level_over_two_modes_is_two_rings_which_is_why_the_ring_column_exists() {
        // The claim `FIELD_RING` is *for*. Two separated modes share every level, so
        // splitting the stroke by level alone would join the two rings with a segment
        // straight across the empty valley between them. At least one level must
        // therefore carry more than one ring.
        let out = density2d_contour(&two_modes(), "x", "y", None);
        let (level, ring) = (col(&out, FIELD_LEVEL), col(&out, FIELD_RING));
        let mut worst = 0;
        for i in 0..level.len() {
            let rings: std::collections::HashSet<u64> = (0..level.len())
                .filter(|&j| (level[j] - level[i]).abs() < 1e-12)
                .map(|j| ring[j].to_bits())
                .collect();
            worst = worst.max(rings.len());
        }
        assert!(worst >= 2, "no level enclosed two modes separately; ring is doing nothing");
    }

    #[test]
    fn a_contour_ring_is_not_in_x_order_which_is_why_a_line_cannot_draw_it() {
        // The finding that sent the contour to `path` instead of `line`. A ring
        // doubles back in x by construction, so the sorted order `write_line` imposes
        // is *not* the traversal order — sorting it would replace the ring with a
        // zigzag between its two halves.
        let out = density2d_contour(&two_modes(), "x", "y", None);
        let (xs, ring) = (col(&out, "x"), col(&out, FIELD_RING));
        let first: Vec<f64> = (0..xs.len()).filter(|&i| ring[i] == ring[0]).map(|i| xs[i]).collect();
        let mut sorted = first.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert_ne!(first, sorted, "a ring that is already in x order is not a ring");
        // And it really does turn back: some vertex is left of its predecessor.
        assert!(first.windows(2).any(|w| w[1] < w[0]), "the ring never doubles back");
    }

    #[test]
    fn a_group_split_contours_each_category_and_never_joins_two_of_them() {
        // `group` runs the whole estimate per category, which is the split every
        // statistic in `apply` gets for free and this reading had to be given (it is
        // dispatched by the mark, so it never passes through `apply`). Two claims: the
        // group column survives onto the output — without it the mark writer finds no
        // series and silently draws nothing — and each group is numbered from one, so
        // the mark's run-based split cannot weld one group's ring to another's.
        let mut df = two_modes();
        let n = df.len();
        let tag: Vec<String> = (0..n).map(|i| if i % 2 == 0 { "a".into() } else { "b".into() }).collect();
        df = df.with_str("g", tag);

        let split = by_group(&df, Some("g"), |sub| density2d_contour(sub, "x", "y", None));
        let groups = split.str_col("g").expect("the group column rides onto the output");
        assert!(groups.iter().any(|g| g == "a") && groups.iter().any(|g| g == "b"),
            "both groups are present");

        // Inside each group the rings are contiguous runs, so a run never spans two
        // groups — the property the renderer's split depends on.
        let ring = col(&split, FIELD_RING);
        for i in 1..ring.len() {
            if groups[i] != groups[i - 1] {
                assert_ne!(ring[i], ring[i - 1],
                    "a run of one ring id must not straddle the group boundary at {i}");
            }
        }
        // And each group restarts its numbering, which is what makes that safe.
        assert_eq!(ring[0], 1.0, "the first group's first ring is ring 1");
    }

    #[test]
    fn a_smoother_field_spreads_its_contours_wider() {
        // `adjust` is the knob that reaches the 2-D reading, and more smoothing means
        // a broader estimate — so the outermost ring must enclose more of the plane.
        let span = |adjust: Option<f64>| {
            let spec = DensitySpec { adjust, ..Default::default() };
            let out = density2d_contour(&two_modes(), "x", "y", Some(&spec));
            let xs = col(&out, "x");
            xs.iter().copied().fold(f64::NEG_INFINITY, f64::max)
                - xs.iter().copied().fold(f64::INFINITY, f64::min)
        };
        assert!(span(Some(2.0)) > span(Some(0.5)),
            "a wider bandwidth must give wider contours");
    }

    #[test]
    fn density_survives_tied_data_with_a_zero_iqr() {
        // Most values identical, a few outliers: the interquartile range is zero
        // while the data plainly has spread. The bandwidth must not collapse to a
        // spike, or the estimate integrates to nothing.
        let mut xs = vec![5.0; 40];
        xs.extend([80.0, 82.0, 85.0, 88.0]);
        let out = density(&num("x", &xs), "x", "d", None);
        let area = trapezoid(col(&out, "x"), col(&out, "d"));
        assert!((area - 1.0).abs() < 0.05, "zero-IQR density integrated to {area}");
    }

    #[test]
    fn density_refuses_a_categorical_column() {
        // A category with **no measure beside it** is still nothing to estimate
        // along. What changed with the violin is only that a measure makes it the
        // slot reading; a bare categorical key is the same empty answer it always
        // was, refused upstream by `check_distribution_axis`.
        let df = DataFrame::new().with_str("x", vec!["a".into(), "b".into()]);
        assert!(density(&df, "x", "d", None).is_empty());
    }

    // -- the violin: the slot reading of `density` (spec §5) -------------------

    /// Three groups of very different size and spread, so both readings of
    /// `compare` have something to disagree about.
    fn violin_frame() -> DataFrame {
        let mut cats: Vec<String> = Vec::new();
        let mut vals: Vec<f64> = Vec::new();
        for (g, n) in [("a", 40usize), ("b", 10), ("c", 4)] {
            for i in 0..n {
                cats.push(g.to_string());
                vals.push(i as f64);
            }
        }
        DataFrame::new().with_str("g", cats).with_float("v", vals)
    }

    fn violin(spec: Option<&DensitySpec>) -> DataFrame {
        density(&violin_frame(), "g", "v", spec)
    }

    #[test]
    fn a_violin_is_one_block_of_samples_per_category() {
        let out = violin(None);
        // Three groups, each traced from the same number of points, and the slot
        // column survives so the renderer knows which slot each block stands in.
        assert_eq!(out.len(), 3 * SLOT_SAMPLES);
        let keys = out.str_col("g").expect("the slot column");
        assert_eq!(keys[0], "a");
        assert_eq!(keys[SLOT_SAMPLES], "b");
        assert_eq!(keys[2 * SLOT_SAMPLES], "c");
        // Blocks are contiguous and ascending in the measure — the renderer traces
        // the outline in emission order and would fold it over otherwise.
        let vs = col(&out, "v");
        assert!(vs[..SLOT_SAMPLES].windows(2).all(|w| w[1] > w[0]),
                "each block must ascend along the measure");
    }

    #[test]
    fn a_violin_leaves_its_widths_unnormalized() {
        // The renderer divides by the frame maximum, so what arrives here is a
        // density in the measure's own reciprocal units, not a fraction. Pinning
        // this keeps the normalization in exactly one place: were the transform to
        // scale to [0, 1] as well, a split violin would be rescaled per color and
        // two unequal groups would come out looking alike.
        let out = violin(None);
        let w = col(&out, SLOT_WIDTH);
        assert!(w.iter().all(|&v| v >= 0.0), "a density is never negative");
        assert!(w.iter().cloned().fold(f64::NEG_INFINITY, f64::max) != 1.0,
                "widths must not arrive pre-scaled to a maximum of 1");
    }

    #[test]
    fn compare_count_weights_each_violin_by_its_group_size() {
        // The default. Group `a` has ten times `c`'s rows over a wider spread, so
        // its estimate carries ten times the mass — the ratio of the *areas*, which
        // is what the reading promises. Areas rather than peaks, since the peak also
        // moves with the spread.
        let area = |out: &DataFrame, block: usize| -> f64 {
            let w = out.float_col(SLOT_WIDTH).unwrap();
            let v = out.float_col("v").unwrap();
            let lo = block * SLOT_SAMPLES;
            let step = v[lo + 1] - v[lo];
            w[lo..lo + SLOT_SAMPLES].iter().sum::<f64>() * step
        };
        let counted = violin(None);
        let (a, c) = (area(&counted, 0), area(&counted, 2));
        assert!((a / c - 10.0).abs() < 0.05,
                "areas should be proportional to 40:4 rows, got {a} and {c}");

        // `compare = "shape"` is the same three estimates with the weight taken
        // back off: every violin integrates to 1, so every area is equal.
        let spec = DensitySpec { compare: Some("shape".into()), ..Default::default() };
        let shaped = violin(Some(&spec));
        let (a, c) = (area(&shaped, 0), area(&shaped, 2));
        assert!((a - c).abs() < 0.02, "shapes should be equal in area, got {a} and {c}");
    }

    #[test]
    fn a_violin_drops_a_group_too_small_to_estimate() {
        // One row is not a spread, and a kernel over it is a spike asserting a
        // precision the data does not have. Dropped rather than drawn — the silence
        // `density` already keeps on an empty frame, not a silent *substitution*.
        let df = DataFrame::new()
            .with_str("g", vec!["a".into(), "a".into(), "a".into(), "lone".into()])
            .with_float("v", vec![1.0, 2.0, 3.0, 9.0]);
        let out = density(&df, "g", "v", None);
        assert_eq!(out.len(), SLOT_SAMPLES);
        assert!(out.str_col("g").unwrap().iter().all(|k| k == "a"));
    }

    #[test]
    fn density_bandwidth_sets_an_absolute_scale() {
        // An explicit bandwidth replaces Silverman's choice outright. The grid
        // runs [min − 3h, max + 3h], so with h pinned to 4 on data spanning
        // 0..=39 the ends are exactly −12 and 51 — a direct read on the bandwidth.
        let xs: Vec<f64> = (0..40).map(|i| i as f64).collect();
        let spec = DensitySpec { adjust: None, bandwidth: Some(4.0), levels: None , compare: None, reach: None };
        let out = density(&num("x", &xs), "x", "d", Some(&spec));
        let ex = col(&out, "x");
        assert!((ex[0] - (0.0 - 12.0)).abs() < 1e-9, "grid starts at min − 3·bandwidth");
        assert!((ex[255] - (39.0 + 12.0)).abs() < 1e-9, "grid ends at max + 3·bandwidth");
    }

    #[test]
    fn density_adjust_scales_the_automatic_bandwidth() {
        // `adjust` multiplies whatever Silverman picks: 2× widens the [min−3h,
        // max+3h] grid, 0.5× narrows it, both against the un-adjusted default. The
        // exact bandwidth is data-dependent, so assert the ordering, not a value.
        let xs: Vec<f64> = (0..60).map(|i| (i as f64 * 0.5).cos() * 8.0 + 25.0).collect();
        let span = |spec: Option<&DensitySpec>| {
            let out = density(&num("x", &xs), "x", "d", spec);
            let ex = col(&out, "x");
            ex[ex.len() - 1] - ex[0]
        };
        let auto     = span(None);
        let wider    = span(Some(&DensitySpec { adjust: Some(2.0), bandwidth: None, levels: None , compare: None, reach: None }));
        let narrower = span(Some(&DensitySpec { adjust: Some(0.5), bandwidth: None, levels: None , compare: None, reach: None }));
        assert!(wider > auto,    "adjust = 2 widens the grid ({wider} vs {auto})");
        assert!(narrower < auto, "adjust = 0.5 narrows the grid ({narrower} vs {auto})");
    }

    // -- range -------------------------------------------------------------

    #[test]
    fn range_emits_low_then_high_per_group() {
        // Two rows per group — low first, high second — groups in first-seen
        // order. This is the pairing `write_interval` reconstructs.
        let df = DataFrame::new()
            .with_str("g", ["a", "a", "a", "b", "b"].iter().map(|s| s.to_string()).collect())
            .with_float("v", vec![3.0, 1.0, 2.0, 9.0, 5.0]);
        let out = range(&df, "g", "v", None);
        assert_eq!(out.str_col("g").unwrap(), &["a", "a", "b", "b"]);
        assert_eq!(out.float_col("v").unwrap(), &[1.0, 3.0, 5.0, 9.0]);
    }

    #[test]
    fn range_over_numeric_x_sorts_ascending() {
        let df = DataFrame::new()
            .with_float("x", vec![2.0, 2.0, 1.0, 1.0])
            .with_float("v", vec![8.0, 4.0, 30.0, 10.0]);
        let out = range(&df, "x", "v", None);
        assert_eq!(out.float_col("x").unwrap(), &[1.0, 1.0, 2.0, 2.0]);
        assert_eq!(out.float_col("v").unwrap(), &[10.0, 30.0, 4.0, 8.0]);
    }

    #[test]
    fn bare_range_is_the_extremes_by_both_paths() {
        // `range_pair` shortcuts the default pair to a fold, on the claim that a
        // type-7 quantile at p = 0 and p = 1 *is* the minimum and the maximum.
        // If that ever stops holding, the shortcut becomes a second opinion and
        // bare `range` quietly disagrees with `range(0, 1)` — the two-copies
        // failure the second renderer died of, one function down. This is what
        // holds them together, and it is why the parameter could be added
        // without touching what bare `range` draws.
        for vals in [
            vec![3.0, 1.0, 2.0],
            vec![9.0, 5.0],
            vec![42.0],
            vec![1.0, 1.0, 1.0],
            (1..=10).map(|i| i as f64).collect::<Vec<_>>(),
        ] {
            let mut s = vals.clone();
            s.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            assert_eq!(
                range_pair(&vals, None),
                (quantile_sorted(&s, 0.0), quantile_sorted(&s, 1.0)),
                "bare range disagreed with range(0, 1) on {vals:?}"
            );
        }
    }

    #[test]
    fn range_takes_a_quantile_band() {
        // 1..=10 by type 7: Q1 = 3.25 and Q3 = 7.75 — the numbers R's
        // `quantile()` gives, so a reader can reproduce the band by hand.
        let df = DataFrame::new()
            .with_str("g", vec!["a".to_string(); 10])
            .with_float("v", (1..=10).map(|i| i as f64).collect());
        let spec = RangeSpec { low: Some(0.25), high: Some(0.75) };
        let out = range(&df, "g", "v", Some(&spec));
        assert_eq!(out.float_col("v").unwrap(), &[3.25, 7.75]);
    }

    #[test]
    fn the_band_and_the_box_body_agree() {
        // `interval * range(0.25, 0.75)` *is* the box's body, so the two must
        // report the same quartiles. They share `quantile_sorted`; this says so
        // out loud, because sharing it is a choice a later edit could undo.
        let vals: Vec<f64> = vec![2.0, 7.0, 1.0, 9.0, 4.0, 6.0, 3.0];
        let st = box_stat(&vals, true);
        let spec = RangeSpec { low: Some(0.25), high: Some(0.75) };
        assert_eq!(range_pair(&vals, Some(&spec)), (st.q1, st.q3));
    }

    #[test]
    fn one_sided_range_defaults_the_other_end() {
        // An unset side is that side's extreme, which is what makes bare `range`
        // the degenerate case of the parameterized one rather than a rule beside
        // it. Type 7 at p = 0.9 over 1..=10 is 9.1.
        let vals: Vec<f64> = (1..=10).map(|i| i as f64).collect();
        let (lo, hi) = range_pair(&vals, Some(&RangeSpec { low: None, high: Some(0.9) }));
        assert_eq!(lo, 1.0);
        assert!((hi - 9.1).abs() < 1e-12, "expected 9.1, got {hi}");
    }

    /// **The two classes have to nest, and this is what says so.**
    ///
    /// `line` asks `is_value_statistic` whether connecting its rows in x order
    /// was intended, and every reduction leaves one value per x, so a reduction
    /// that is not a value statistic is a member of a class behaving unlike its
    /// siblings. That is exactly what happened: `quantile` joined the
    /// aggregation family, `line` kept a written-out copy of the list, and
    /// `line * quantile(0.9)` warned about a zigzag it cannot draw. The copy is
    /// gone; this keeps the two definitions from drifting apart again.
    #[test]
    fn every_reduction_is_also_a_value_statistic() {
        for t in crate::legality::USER_TRANSFORMS {
            if is_reduction(&t) {
                assert!(
                    is_value_statistic(&t),
                    "{t:?} reduces a column but is not a value statistic, so `line` \
                     will warn about a zigzag it cannot draw"
                );
            }
        }
    }

    #[test]
    fn deviation_is_the_mean_plus_and_minus_k_standard_deviations() {
        // 2,4,4,4,5,5,7,9: mean 5, sample sd 2.138... (n−1). The band is
        // mean ± k·sd, and the center is the mean whatever k is.
        let vals = vec![2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0];
        let sd = (vals.iter().map(|v| (v - 5.0f64).powi(2)).sum::<f64>() / 7.0).sqrt();
        let (lo, hi, c) = sd_band_of(&vals, 1.0);
        assert!((c - 5.0).abs() < 1e-12);
        assert!((lo - (5.0 - sd)).abs() < 1e-12);
        assert!((hi - (5.0 + sd)).abs() < 1e-12);
        // The multiplier scales the half-width and moves nothing else.
        let (lo2, hi2, c2) = sd_band_of(&vals, 2.0);
        assert!((c2 - c).abs() < 1e-12);
        assert!((hi2 - c) - 2.0 * (hi - c) < 1e-12);
        assert!((c - lo2) - 2.0 * (c - lo) < 1e-12);
    }

    #[test]
    fn a_group_of_one_has_no_spread_and_collapses_to_a_point() {
        // The sample sd of one value is undefined, and a band of width NaN would
        // quietly drop the group out of the axis domain. `confidence` collapses
        // there for the same reason, so `deviation` matches it.
        let (lo, hi, c) = sd_band_of(&[7.0], 1.0);
        assert_eq!((lo, hi, c), (7.0, 7.0, 7.0));
    }

    #[test]
    fn quantile_at_the_three_plain_points_is_those_plain_atoms() {
        // The Assumption at p = 0, 0.5 and 1 tells a reader to write `min`,
        // `median` and `max` instead. That direction is only honest if the
        // numbers actually agree, and they have to agree on an even-length group
        // too, where the median interpolates. This is what makes the message true.
        for vals in [
            vec![3.0, 1.0, 4.0, 1.0, 5.0],           // odd
            vec![3.0, 1.0, 4.0, 1.0, 5.0, 9.0],      // even, so the median averages
        ] {
            let mut a = vals.clone();
            let mut b = vals.clone();
            assert_eq!(AggFn::Quantile(0.0).reduce(&mut a), AggFn::Min.reduce(&mut b));
            let (mut a, mut b) = (vals.clone(), vals.clone());
            assert_eq!(AggFn::Quantile(0.5).reduce(&mut a), AggFn::Median.reduce(&mut b));
            let (mut a, mut b) = (vals.clone(), vals.clone());
            assert_eq!(AggFn::Quantile(1.0).reduce(&mut a), AggFn::Max.reduce(&mut b));
        }
    }

    #[test]
    fn quantile_reduces_a_group_by_type_seven() {
        // 1..=10 at p = 0.9 is 9.1, the number R's `quantile()` returns, and the
        // same `quantile_sorted` the box and the band call.
        let df = DataFrame::new()
            .with_str("g", vec!["a".to_string(); 10])
            .with_float("v", (1..=10).map(|i| i as f64).collect());
        let spec = QuantileSpec { p: Some(0.9) };
        let out = aggregate(&df, "g", "v", AggFn::Quantile(quantile_p(Some(&spec))));
        let got = out.float_col("v").unwrap()[0];
        assert!((got - 9.1).abs() < 1e-12, "expected 9.1, got {got}");
    }

    #[test]
    fn a_cell_takes_the_same_spread_band_as_a_slot() {
        // Law 2 again, for the second pair transform: one key or two is a fact
        // about the mark, never about what `deviation` means.
        let vals: Vec<f64> = vec![2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0];
        let df = DataFrame::new()
            .with_str("a", vec!["p".to_string(); 8])
            .with_str("b", vec!["q".to_string(); 8])
            .with_float("v", vals.clone());
        let spec = DeviationSpec { multiplier: Some(2.0) };
        let out = pairs2d(&df, "a", "b", "v", &Transform::Deviation, None, None, None, Some(&spec));
        let (lo, hi, _) = sd_band_of(&vals, 2.0);
        assert_eq!(out.float_col("v").unwrap(), &[lo, hi]);
    }

    #[test]
    fn a_cell_takes_the_same_band_as_a_slot() {
        // Law 2: how many keys a mark has is a fact about the mark, never about
        // what `range` means. Both readings call `range_pair`, so the cube's
        // whisker and the panel's report one band.
        let df = DataFrame::new()
            .with_str("a", vec!["p".to_string(); 10])
            .with_str("b", vec!["q".to_string(); 10])
            .with_float("v", (1..=10).map(|i| i as f64).collect());
        let spec = RangeSpec { low: Some(0.25), high: Some(0.75) };
        let out = pairs2d(&df, "a", "b", "v", &Transform::Range, None, None, Some(&spec), None);
        assert_eq!(out.float_col("v").unwrap(), &[3.25, 7.75]);
    }

    #[test]
    fn bounds_reshapes_precomputed_columns_without_reducing() {
        // The non-computing counterpart to `range`: each input row (x, lo, hi)
        // becomes two rows — the low then the high in out_field — one pair per
        // row, in input order, with nothing grouped or reduced.
        let df = DataFrame::new()
            .with_float("x", vec![1.0, 2.0, 3.0])
            .with_float("lo", vec![10.0, 20.0, 30.0])
            .with_float("hi", vec![15.0, 25.0, 35.0]);
        let spec = BoundsSpec { lower: Some("lo".into()), upper: Some("hi".into()), ..Default::default() };
        let out = bounds(&df, "x", "y", Some(&spec));
        assert_eq!(out.float_col("y").unwrap(), &[10.0, 15.0, 20.0, 25.0, 30.0, 35.0]);
        assert_eq!(out.float_col("x").unwrap(), &[1.0, 1.0, 2.0, 2.0, 3.0, 3.0]);
    }

    // -- confidence --------------------------------------------------------

    #[test]
    fn t_quantile_matches_the_table() {
        // Two-sided 95% (two_tail = 0.05) and 99% critical values, standard table.
        let close = |a: f64, b: f64| (a - b).abs() < 5e-3;
        assert!(close(t_quantile(0.05, 1.0), 12.706), "df=1: {}", t_quantile(0.05, 1.0));
        assert!(close(t_quantile(0.05, 2.0), 4.303),  "df=2: {}", t_quantile(0.05, 2.0));
        assert!(close(t_quantile(0.05, 5.0), 2.571),  "df=5: {}", t_quantile(0.05, 5.0));
        assert!(close(t_quantile(0.05, 10.0), 2.228), "df=10: {}", t_quantile(0.05, 10.0));
        assert!(close(t_quantile(0.05, 30.0), 2.042), "df=30: {}", t_quantile(0.05, 30.0));
        assert!(close(t_quantile(0.01, 10.0), 3.169), "99% df=10: {}", t_quantile(0.01, 10.0));
        // Large df approaches the normal 1.96.
        assert!((t_quantile(0.05, 100000.0) - 1.960).abs() < 1e-2);
    }

    #[test]
    fn confidence_gives_mean_plus_minus_t_se() {
        // One group of five: mean 5, sd sqrt(2.5)=1.5811, se 0.7071,
        // t(0.975, 4)=2.776, margin 1.963.
        let df = DataFrame::new()
            .with_str("g", vec!["a".to_string(); 5])
            .with_float("y", vec![3.0, 4.0, 5.0, 6.0, 7.0]);
        let out = confidence(&df, "g", "y", None);
        let v = out.float_col("y").unwrap();
        let c = out.float_col("center").unwrap();
        assert_eq!(v.len(), 2, "one low/high pair");
        assert!((c[0] - 5.0).abs() < 1e-9, "center is the mean");
        assert!((v[0] - (5.0 - 1.963)).abs() < 1e-2, "low = mean - t*se, got {}", v[0]);
        assert!((v[1] - (5.0 + 1.963)).abs() < 1e-2, "high = mean + t*se, got {}", v[1]);
    }

    #[test]
    fn confidence_level_widens_the_interval() {
        let df = DataFrame::new()
            .with_str("g", vec!["a".to_string(); 8])
            .with_float("y", (0..8).map(|i| i as f64).collect());
        let width = |lvl: f64| {
            let o = confidence(&df, "g", "y", Some(&ConfidenceSpec { level: Some(lvl) }));
            let v = o.float_col("y").unwrap().clone();
            v[1] - v[0]
        };
        assert!(width(0.99) > width(0.95), "99% wider than 95%");
    }

    #[test]
    fn confidence_collapses_a_singleton_to_a_point() {
        // n = 1 per group has no spread: low == high == center.
        let df = DataFrame::new()
            .with_str("g", vec!["a".to_string(), "b".to_string()])
            .with_float("y", vec![10.0, 20.0]);
        let out = confidence(&df, "g", "y", None);
        assert_eq!(out.float_col("y").unwrap(), &[10.0, 10.0, 20.0, 20.0]);
    }

    // -- box (five-number summary) ----------------------------------------
    #[test]
    fn box_matches_r_type7_quantiles() {
        // 1..=9: R's `quantile(1:9, c(0,.25,.5,.75,1))` gives 1, 3, 5, 7, 9 — the
        // type-7 numbers a reader can reproduce. The extents ride y (two rows),
        // the quartiles ride the interior columns.
        let df = DataFrame::new()
            .with_str("g", vec!["a".to_string(); 9])
            .with_float("v", (1..=9).map(|i| i as f64).collect());
        let out = box_summary(&df, "g", "v", None);
        assert_eq!(out.float_col("v").unwrap(), &[1.0, 9.0], "min, max as low/high rows");
        assert_eq!(out.float_col("lower").unwrap(), &[3.0, 3.0], "Q1 repeated on both rows");
        assert_eq!(out.float_col("middle").unwrap(), &[5.0, 5.0], "median");
        assert_eq!(out.float_col("upper").unwrap(), &[7.0, 7.0], "Q3");
    }

    #[test]
    fn box_interpolates_between_order_statistics() {
        // 1..=10: type-7 Q1 = 3.25, median = 5.5, Q3 = 7.75 — the interpolation,
        // not a raw order statistic (which the hinge method would give).
        let df = DataFrame::new()
            .with_str("g", vec!["a".to_string(); 10])
            .with_float("v", (1..=10).map(|i| i as f64).collect());
        let out = box_summary(&df, "g", "v", None);
        let (lo, mid, up) = (out.float_col("lower").unwrap(), out.float_col("middle").unwrap(), out.float_col("upper").unwrap());
        assert!((lo[0] - 3.25).abs() < 1e-9, "Q1 = 3.25, got {}", lo[0]);
        assert!((mid[0] - 5.5).abs() < 1e-9, "median = 5.5, got {}", mid[0]);
        assert!((up[0] - 7.75).abs() < 1e-9, "Q3 = 7.75, got {}", up[0]);
    }

    #[test]
    fn box_summarizes_each_group_independently() {
        // Two groups, first-seen order preserved (like `range`): a's spread and
        // b's are computed apart, two rows each.
        let df = DataFrame::new()
            .with_str("g", vec!["a".into(), "a".into(), "a".into(), "b".into(), "b".into(), "b".into()])
            .with_float("v", vec![1.0, 2.0, 3.0, 10.0, 20.0, 30.0]);
        let out = box_summary(&df, "g", "v", None);
        assert_eq!(out.str_col("g").unwrap(), &["a", "a", "b", "b"]);
        assert_eq!(out.float_col("v").unwrap(), &[1.0, 3.0, 10.0, 30.0], "min/max per group");
        assert_eq!(out.float_col("middle").unwrap(), &[2.0, 2.0, 20.0, 20.0], "each group's median");
    }

    #[test]
    fn box_tukey_pulls_whiskers_to_the_fence_and_splits_off_outliers() {
        // 0..=8 then a far outlier at 100. Q1=2, Q3=6, IQR=4, upper fence =
        // 6 + 1.5*4 = 12, so 100 is an outlier and the high whisker stops at 8.
        // The outlier appends as a third row with a NaN `middle` sentinel.
        let v: Vec<f64> = (0..=8).map(|i| i as f64).chain(std::iter::once(100.0)).collect();
        let df = DataFrame::new().with_str("g", vec!["a".to_string(); v.len()]).with_float("v", v);
        let out = box_summary(&df, "g", "v", None);
        let (y, mid) = (out.float_col("v").unwrap(), out.float_col("middle").unwrap());
        assert_eq!(y.len(), 3, "two whisker rows + one outlier");
        assert_eq!(y[0], 0.0, "low whisker = min (no low outlier)");
        assert_eq!(y[1], 8.0, "high whisker pulled to the fence, not 100");
        assert_eq!(y[2], 100.0, "the outlier rides as its own row");
        assert!(mid[0].is_finite() && mid[1].is_finite(), "box rows keep the median");
        assert!(mid[2].is_nan(), "outlier row's middle is the NaN sentinel");
    }

    #[test]
    fn box_range_mode_keeps_the_extremes_and_draws_no_outliers() {
        // The same data under `whiskers = "range"`: whiskers reach 100, no outlier
        // row — the plain five-number summary, two rows only.
        let v: Vec<f64> = (0..=8).map(|i| i as f64).chain(std::iter::once(100.0)).collect();
        let df = DataFrame::new().with_str("g", vec!["a".to_string(); v.len()]).with_float("v", v);
        let spec = BoxSpec { whiskers: Some("range".into()) };
        let out = box_summary(&df, "g", "v", Some(&spec));
        let y = out.float_col("v").unwrap();
        assert_eq!(y, &[0.0, 100.0], "range whiskers reach the true extremes, no outlier split");
    }

    // -- apply -------------------------------------------------------------

    #[test]
    fn apply_with_no_transforms_returns_the_data_unchanged() {
        let df = xy(&[1.0, 2.0], &[3.0, 4.0]);
        let out = apply(&df, &[], "x", "y", None, None, None, None, None, None, None);
        assert_eq!(col(&out, "x"), &vec![1.0, 2.0]);
    }

    #[test]
    fn apply_runs_transforms_in_sequence() {
        // bin then count: bin makes a count column, and counting *that* tallies
        // how many bins share each count. Proves apply threads one output into
        // the next rather than re-reading the original.
        let xs: Vec<f64> = (0..10).map(|i| i as f64).collect();
        let out = apply(&num("x", &xs), &[Transform::Bin], "x", "count", None, None, None, None, None, None, None);
        assert_eq!(col(&out, "x").len(), 5); // ⌈log₂10⌉+1 = 5
    }

    // -- partition ---------------------------------------------------------
    //
    // Pinned by the invariants a *correct* layout must satisfy rather than by the
    // floats this code happens to emit, which is the rule the estimators above
    // follow and for the same reason: a test that echoed today's numbers would
    // pass a wrong rewrite too. The invariants are the tree's own — children fill
    // their parent exactly, siblings do not overlap, and the first ring tiles the
    // whole — and they are what the *grammar* cannot check for a caller who lays
    // a table out by hand, which is the argument this atom was built on.

    fn tree(levels: &[&[&str]], amount: &[f64]) -> DataFrame {
        let mut df = DataFrame::new().with_float("amount", amount.to_vec());
        for (i, col) in levels.iter().enumerate() {
            df = df.with_str(
                format!("l{i}"),
                col.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
            );
        }
        df
    }
    fn names(n: usize) -> Vec<String> {
        (0..n).map(|i| format!("l{i}")).collect()
    }

    #[test]
    fn a_node_is_exactly_the_extent_of_its_own_leaves() {
        // The whole design in one assertion. Two levels, uneven weights, and a
        // branch order that is not alphabetical.
        let df = tree(
            &[&["B", "B", "A", "A", "A"], &["x", "y", "p", "q", "r"]],
            &[3.0, 1.0, 2.0, 2.0, 6.0],
        );
        let out = partition(&df, &names(2), Some("amount"), "amount", "depth", false);
        let (s, e) = (col(&out, CELL_START), col(&out, CELL_END));
        let d = col(&out, CELL_LOWER);
        let n = out.str_col(NODE_NAME).unwrap();

        // The first ring tiles the whole, with no gap and no overlap.
        let mut ring1: Vec<(f64, f64)> = (0..n.len())
            .filter(|&i| d[i] == 1.0)
            .map(|i| (s[i], e[i]))
            .collect();
        ring1.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        assert_eq!(ring1.first().unwrap().0, 0.0, "the turn starts at zero");
        assert_eq!(ring1.last().unwrap().1, 14.0, "and ends at the total");
        for w in ring1.windows(2) {
            assert_eq!(w[0].1, w[1].0, "siblings meet exactly: {ring1:?}");
        }

        // Every child lies inside a parent — the invariant a hand-laid table can
        // break silently, and the reason this is an atom.
        for i in 0..n.len() {
            if d[i] == 1.0 {
                continue;
            }
            let inside = (0..n.len()).any(|p| {
                d[p] == d[i] - 1.0 && s[p] <= s[i] + 1e-9 && e[p] >= e[i] - 1e-9
            });
            assert!(inside, "`{}` escapes its parent", n[i]);
        }
    }

    #[test]
    fn two_nodes_with_one_name_under_different_parents_stay_apart() {
        // plotly's `ids` example, which exists because a sunburst keyed on the
        // *label* merges these two into one wedge. Keying on the whole path is
        // what keeps them separate, and this is the cheapest test that says so.
        let df = tree(
            &[&["North", "North", "South", "South"], &["Alpha", "Beta", "Alpha", "Beta"]],
            &[10.0, 20.0, 30.0, 40.0],
        );
        let out = partition(&df, &names(2), Some("amount"), "amount", "depth", false);
        let n = out.str_col(NODE_NAME).unwrap();
        let d = col(&out, CELL_LOWER);
        let alphas: Vec<usize> = (0..n.len()).filter(|&i| n[i] == "Alpha").collect();
        assert_eq!(alphas.len(), 2, "one `Alpha` per parent, not one in total");
        let widths: Vec<f64> = alphas.iter()
            .map(|&i| col(&out, CELL_END)[i] - col(&out, CELL_START)[i])
            .collect();
        assert_eq!(widths, vec![10.0, 30.0], "each keeps its own parent's weight");
        assert!(d[alphas[0]] == 2.0 && d[alphas[1]] == 2.0);
    }

    #[test]
    fn a_branch_that_stops_early_leaves_the_rim_blank() {
        // The ragged rim, which is what a real hierarchy looks like: `A` reaches
        // the third level and `B` does not, so the third ring has one node.
        let df = tree(
            &[&["A", "B"], &["p", "q"], &["deep", ""]],
            &[4.0, 6.0],
        );
        let out = partition(&df, &names(3), Some("amount"), "amount", "depth", false);
        let d = col(&out, CELL_LOWER);
        assert_eq!(d.iter().filter(|v| **v == 1.0).count(), 2);
        assert_eq!(d.iter().filter(|v| **v == 2.0).count(), 2);
        assert_eq!(d.iter().filter(|v| **v == 3.0).count(), 1, "one node on the rim");
    }

    #[test]
    fn with_nothing_to_weigh_by_every_leaf_weighs_one() {
        // The tally, and it is the same courtesy `count`/`proportion` do when
        // nothing else measured: bind no `x` and the picture is the tree's shape
        // rather than its size.
        let df = tree(&[&["A", "A", "B"], &["p", "q", "r"]], &[99.0, 1.0, 1.0]);
        let out = partition(&df, &names(2), None, "amount", "depth", false);
        let n = out.str_col(NODE_NAME).unwrap();
        let i = n.iter().position(|s| s == "A").unwrap();
        let w = col(&out, CELL_END)[i] - col(&out, CELL_START)[i];
        assert_eq!(w, 2.0, "two leaves, two units — the 99 is not read");
    }

    #[test]
    fn the_tables_own_order_decides_the_sweep() {
        // Ordering is by **first appearance**, never alphabetical, so the caller's
        // row order still decides where each branch lands and `order()` stays the
        // way to change it. Alphabetical sorting would put `Apple` first here.
        let df = tree(&[&["Zebra", "Apple"], &["z", "a"]], &[1.0, 1.0]);
        let out = partition(&df, &names(2), Some("amount"), "amount", "depth", false);
        let n = out.str_col(NODE_NAME).unwrap();
        let s = col(&out, CELL_START);
        let z = n.iter().position(|x| x == "Zebra").unwrap();
        let a = n.iter().position(|x| x == "Apple").unwrap();
        assert!(s[z] < s[a], "the table put Zebra first, so the circle does too");
    }

    #[test]
    fn scattered_rows_are_gathered_before_they_are_laid_out() {
        // The reason the layout needs no traversal is that descendants end up
        // contiguous — which is the sort's doing, not the table's. A table whose
        // branches are interleaved must give the same answer as one already
        // grouped, or the "extent of its own leaves" shortcut would be a bug.
        let grouped = tree(&[&["A", "A", "B", "B"], &["p", "q", "r", "s"]],
                           &[1.0, 2.0, 3.0, 4.0]);
        let mixed = tree(&[&["A", "B", "A", "B"], &["p", "r", "q", "s"]],
                         &[1.0, 3.0, 2.0, 4.0]);
        let width = |df: &DataFrame, want: &str| -> f64 {
            let out = partition(df, &names(2), Some("amount"), "amount", "depth", false);
            let n = out.str_col(NODE_NAME).unwrap();
            let i = n.iter().position(|s| s == want).unwrap();
            col(&out, CELL_END)[i] - col(&out, CELL_START)[i]
        };
        assert_eq!(width(&grouped, "A"), 3.0);
        assert_eq!(width(&mixed, "A"), 3.0, "an interleaved table lays out the same");
    }

    #[test]
    fn the_letters_n_a_are_a_category_and_not_a_missing_value() {
        // "NA" is North America. A missing level already reaches this code as the
        // empty string, so treating the *spelling* as missing coerced real data —
        // and it took plotly's repeated-labels example, whose first column is
        // exactly `["NA", "NA", "EU", "EU"]`, to notice. Both continents must keep
        // their branch.
        let df = tree(
            &[&["NA", "NA", "EU", "EU"], &["Football", "Hockey", "Football", "Hockey"]],
            &[5.0, 3.0, 8.0, 2.0],
        );
        let out = partition(&df, &names(2), Some("amount"), "amount", "depth", false);
        let n = out.str_col(NODE_NAME).unwrap();
        assert_eq!(n.iter().filter(|s| *s == "NA").count(), 1, "NA is a continent: {n:?}");
        assert_eq!(out.len(), 6, "two branches of two, plus the two branches");
    }

    #[test]
    fn an_interior_value_is_reported_rather_than_resolved() {
        // The one genuine ambiguity in the set — plotly spells the two readings
        // `branchvalues="total"` and `"remainder"` — so `paths` reports it and
        // `legality` refuses. Here only that it is *seen*; the wording is pinned
        // in `legality`.
        let df = tree(&[&["A", "A"], &["", "p"]], &[5.0, 5.0]);
        assert_eq!(paths(&df, &names(2)).interior, Some(vec!["A".to_string()]));
        // And a plain ragged rim is not one: `B` simply stops.
        let ok = tree(&[&["A", "B"], &["p", ""]], &[5.0, 5.0]);
        assert_eq!(paths(&ok, &names(2)).interior, None);
    }

    // -----------------------------------------------------------------------
    // partition, crossed — the mosaic
    // -----------------------------------------------------------------------

    /// The contingency table the crossed tests read, small enough to check by hand.
    /// Column totals 20 / 70, so the first column is 2/9 of the width; within it the
    /// split is 10:10, and within the second 30:40.
    fn crossed_table() -> DataFrame {
        tree(
            &[&["P", "P", "Q", "Q"], &["u", "v", "u", "v"]],
            &[10.0, 10.0, 30.0, 40.0],
        )
    }

    #[test]
    fn crossing_leaves_the_first_level_exactly_where_nesting_put_it() {
        // The claim the whole feature was built on: a mosaic's column widths are a
        // *marginal total*, and laying leaves end to end already computes those. So
        // the two readings must agree about level one and differ only below it. If
        // this ever fails, `cross` has stopped being one parameter on one layout and
        // become a second layout wearing its name.
        let df = crossed_table();
        let nested = partition(&df, &names(2), Some("amount"), "amount", "depth", false);
        let crossed = partition(&df, &names(2), Some("amount"), "amount", "depth", true);

        // Level one, nested: the two rows on the inner ring. Crossed: nothing is
        // emitted for it at all (it is not a leaf), so the columns are read off the
        // *leaves* that tile each one.
        let ns = col(&nested, CELL_START);
        let ne = col(&nested, CELL_END);
        let nd = col(&nested, CELL_LOWER);
        let top: Vec<(f64, f64)> = (0..nested.len()).filter(|&i| nd[i] == 1.0)
            .map(|i| (ns[i], ne[i])).collect();
        assert_eq!(top, vec![(0.0, 20.0), (20.0, 90.0)], "marginal totals, nested");

        let cs = col(&crossed, CELL_START);
        let ce = col(&crossed, CELL_END);
        let mut cols: Vec<(f64, f64)> = (0..crossed.len()).map(|i| (cs[i], ce[i])).collect();
        cols.dedup();
        assert_eq!(cols, vec![(0.0, 20.0), (20.0, 90.0)],
            "and the identical marginal totals, crossed");
    }

    #[test]
    fn a_crossed_cell_is_its_share_of_its_own_column() {
        // The other half of the mosaic: height is *conditional*. Both columns fill
        // 0..1 whatever they weigh, which is what makes two columns of wildly
        // different totals comparable at all — and is why the second axis cannot run
        // in the measure's units the way the first does.
        let out = partition(&crossed_table(), &names(2), Some("amount"), "amount", "depth", true);
        let (lo, hi) = (col(&out, CELL_LOWER), col(&out, CELL_UPPER));
        let (s, n) = (col(&out, CELL_START), out.str_col(NODE_NAME).unwrap());

        assert_eq!(out.len(), 4, "four leaves, and only the leaves");
        for (i, name) in n.iter().enumerate() {
            let (want_lo, want_hi) = match (s[i] == 0.0, name.as_str()) {
                (true, "u")  => (0.0, 0.5),          // 10 of P's 20
                (true, "v")  => (0.5, 1.0),
                (false, "u") => (0.0, 30.0 / 70.0),  // 30 of Q's 70
                (false, "v") => (30.0 / 70.0, 1.0),
                _ => unreachable!("unexpected node {name}"),
            };
            assert!((lo[i] - want_lo).abs() < 1e-9 && (hi[i] - want_hi).abs() < 1e-9,
                "`{name}` in the column at {}: got {}..{}, want {want_lo}..{want_hi}",
                s[i], lo[i], hi[i]);
        }
    }

    #[test]
    fn a_shallower_crossed_partition_lands_its_columns_in_the_same_places() {
        // The property the labeling idiom rests on, carried over from the sunburst:
        // `text * partition(a, cross = TRUE)` under `zone * partition(a, b, cross =
        // TRUE)` names the columns without filtering anything, because a column's
        // width is decided by the leaves under it and naming fewer levels does not
        // move them.
        let df = crossed_table();
        let deep = partition(&df, &names(2), Some("amount"), "amount", "depth", true);
        let shallow = partition(&df, &names(1), Some("amount"), "amount", "depth", true);
        let (ds, de) = (col(&deep, CELL_START), col(&deep, CELL_END));
        let (ss, se) = (col(&shallow, CELL_START), col(&shallow, CELL_END));

        assert_eq!(shallow.len(), 2, "one node per column");
        for i in 0..shallow.len() {
            assert!((0..deep.len()).any(|j| (ds[j] - ss[i]).abs() < 1e-9
                                         && (de[j] - se[i]).abs() < 1e-9),
                "the column at {}..{} has no cell tiling it", ss[i], se[i]);
        }
        // A single level fills the height, which is the spine plot — and the reason
        // the root's height is 1 rather than the total.
        let (lo, hi) = (col(&shallow, CELL_LOWER), col(&shallow, CELL_UPPER));
        assert!(lo.iter().all(|v| *v == 0.0) && hi.iter().all(|v| *v == 1.0));
    }

    #[test]
    fn a_ragged_branch_crossed_is_drawn_where_it_stops() {
        // A branch that ends early is a leaf at *its* depth, not a missing cell on
        // the last one — so it fills its whole column rather than vanishing. The
        // nested reading shows the same rows as a short ring; here it is the
        // difference between a mosaic that tiles the panel and one with a hole in it.
        let df = tree(&[&["A", "B", "B"], &["", "p", "q"]], &[4.0, 3.0, 1.0]);
        let out = partition(&df, &names(2), Some("amount"), "amount", "depth", true);
        let n = out.str_col(NODE_NAME).unwrap();
        let (lo, hi) = (col(&out, CELL_LOWER), col(&out, CELL_UPPER));

        assert_eq!(n.len(), 3, "A itself, plus B's two children: {n:?}");
        let a = n.iter().position(|s| s == "A").unwrap();
        assert_eq!((lo[a], hi[a]), (0.0, 1.0), "`A` stops at level one and fills its column");

        // And the whole panel is tiled: every column's cells cover 0..1 exactly.
        let (s, e) = (col(&out, CELL_START), col(&out, CELL_END));
        let mut area = 0.0;
        for i in 0..n.len() {
            area += (e[i] - s[i]) * (hi[i] - lo[i]);
        }
        assert!((area - 8.0).abs() < 1e-9, "the cells tile 8 wide by 1 high, got {area}");
    }
}

