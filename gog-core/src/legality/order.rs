//! Category order, and the axis it needs in order to mean anything.

use super::*;

// ---------------------------------------------------------------------------
// Order — and the axis it needs in order to mean anything
//
// `order()` sorts the plot's **categorical position axis** (spec §9), so a plot
// that has no such axis has nothing for it to sort, and until 2026-07-26 it was
// accepted and dropped in silence there — the §12 sin, sitting in plain sight
// because every sentence in the book that used it happened to have a categorical
// `x`. The packed space is what surfaced it: the one-level treemap binds no
// position at all, so `order()` on it looked like it was doing the sorting the
// packing deliberately does not do for you.
//
// Refused in every space rather than only in `nest`, because the rule is not the
// packing's — it is `order()`'s, and it was always true.
// ---------------------------------------------------------------------------

/// How each language **declares the order of a column's categories**, for a message
/// that asks for one. The engine reads a declared order off the table whichever
/// binding wrote it, so the direction names all four: "the column's factor levels"
/// was R's word, printed to Python, Julia and JavaScript as well.
pub(super) const DECLARE_LEVELS: &str = "a factor in R, `ordered()` in Python, Julia and JavaScript";

pub(super) fn check_order(out: &mut Vec<Diagnostic>, spec: &PlotSpec, data: &HashMap<String, DataFrame>) {
    let Some(order) = &spec.order else { return };
    let Some(df) = spec.data.as_ref().and_then(|n| data.get(n)) else { return };

    let categorical: Vec<&str> = [Channel::X, Channel::Y].iter()
        .filter_map(|ch| spec.axis_def(ch))
        .filter(|enc| actual_type(df, &enc.field) == Some(VarType::Discrete))
        .map(|enc| enc.field.as_str())
        .collect();
    let Some(axis) = categorical.first().copied() else {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `order({})` sorts a **categorical position axis**, and this plot has none — \
                 so there is nothing for it to put in order. Bind a category to `x` or `y`, or, if \
                 what you meant was the order of a color split, declare the column's levels in \
                 the table: {DECLARE_LEVELS}.",
                order.field
            ),
        });
        return;
    };
    // **A running total is added up in the categories' own order** (spec §5,
    // 2026-10-11), the one the table declares or the one its rows arrive in. The
    // transform runs before the axis is sorted, so a sorted axis would stand each
    // total at a category it was not added at: a line through the totals would
    // zigzag. Refused as not built, with the order the totals do follow.
    let accumulated = spec.layers.iter().find(|l| crate::transform::accumulates(&l.transforms)
        && l.data.as_ref().or(spec.data.as_ref()).and_then(|n| data.get(n))
            .is_some_and(|t| t.str_col(axis).is_some()));
    if let Some(layer) = accumulated {
        let chain = std::iter::once(mark_name(&layer.mark))
            .chain(layer.transforms.iter().map(transform_name))
            .collect::<Vec<_>>().join(" * ");
        out.push(Diagnostic {
            kind: DiagnosticKind::Unsupported,
            message: format!(
                "gog: `order({})` would sort the categories of `{axis}` after `{chain}` \
                 has added them up in their own order, so each total would stand at a \
                 category it was not added at. gog does not add up in a sorted order yet. \
                 Declare the order in the table instead ({DECLARE_LEVELS}), and the totals \
                 are added up in that order.",
                order.field
            ),
        });
        return;
    }
    // Ordering an axis by **itself** reads the column's own order, whatever it holds.
    if categorical.contains(&order.field.as_str()) {
        return;
    }
    // By **another** column, the key is read as numbers, one per category: the
    // renderer sorts the categories by value. A text column has no values to sort by
    // and a missing one has nothing at all, and both used to leave the axis in row
    // order without a word, the silent drop §12 forbids. The text case is what a
    // reader reaches for first (a `result` column of "gain" and "loss"), so its
    // direction names both ways to get an order: a number to sort by, or the axis
    // column's own declared order.
    let key = &order.field;
    // **A tally ranks by how many rows each category has** (2026-10-11, the second
    // consistency pass), so under `count` or `proportion` the column `order` reads
    // is the one the tally writes (spec §8, the column rules' first). `order(count)`
    // was refused as "this table has no `count` column", while `order(gdp)` drew
    // sorted by the count and never read `gdp`. The layer asked is the one the
    // renderer ranks by: the first whose table holds the axis's categories.
    let ranked = spec.layers.iter().find_map(|l| {
        let t = l.data.as_ref().or(spec.data.as_ref()).and_then(|n| data.get(n))?;
        t.str_col(axis)?;
        Some((l, t))
    });
    if let Some((layer, table)) = ranked {
        if let Some((kind, message)) = order_by_written(spec, layer, table, axis, key) {
            out.push(Diagnostic { kind, message });
            return;
        }
        if written_order_draws(layer, table, key) {
            return;
        }
    }
    let message = match actual_type(df, key) {
        Some(VarType::Continuous) | Some(VarType::Either) => return,
        Some(VarType::Discrete) => format!(
            "gog: `order({key})` sorts the categories of `{axis}` by the values in `{key}`, and \
             `{key}` holds text, which has no values to sort by. Order by a column of numbers, \
             such as a rank or a total, or declare the order of `{axis}`'s own categories and \
             write `order({axis})`."
        ),
        None => format!(
            "gog: `order({key})` sorts the categories of `{axis}` by the values in `{key}`, and \
             this table has no `{key}` column. Name a column of numbers from the table, or \
             write `order({axis})` to sort the axis by its own categories."
        ),
    };
    out.push(Diagnostic { kind: DiagnosticKind::Illegal, message });
}

/// Does `order(key)` name a column this layer's transforms write, which the
/// renderer ranks by? The tally's own name (`count` after `count`, `proportion`
/// after `proportion`), and the share a statistic writes (`sum * proportion`),
/// ranked by the statistic, since each share is its total over one sum.
fn written_order_draws(layer: &Layer, table: &DataFrame, key: &str) -> bool {
    let ts = &layer.transforms;
    let tally = crate::transform::ranks_by_tally(ts, layer.quantile.as_ref());
    (tally && crate::transform::cell_measure(ts) == Some(key))
        || (actual_type(table, key).is_none()
            && crate::transform::shares_a_statistic(ts)
            && key == crate::transform::CELL_SHARE)
}

/// What `order(key)` is told when the ranked layer writes columns of its own: a
/// number of the table a tally never reads, the tally's other name, a table column
/// that shares the written name, or a written column gog does not sort by yet.
/// `None` when none of these apply, and the ordinary checks decide.
fn order_by_written(
    spec: &PlotSpec, layer: &Layer, table: &DataFrame, axis: &str, key: &str,
) -> Option<(DiagnosticKind, String)> {
    use crate::transform::{cell_measure, CELL_COUNT, CELL_SHARE};
    let ts = &layer.transforms;
    let m = mark_name(&layer.mark);
    let chain = std::iter::once(m)
        .chain(ts.iter().map(transform_name))
        .collect::<Vec<_>>().join(" * ");
    let measure = cell_measure(ts);
    // The position a direction measures along: the one the categories are not on.
    let along = match spec.axis_def(&Channel::Y).is_some_and(|e| e.field == axis) {
        true => "x",
        false => "y",
    };
    if crate::transform::ranks_by_tally(ts, layer.quantile.as_ref()) {
        let writes = measure.unwrap_or(CELL_COUNT);
        // The written name over a table column of that name: the tally's, said so,
        // as a channel naming it is told (the column rules' fourth).
        if measure == Some(key) {
            return actual_type(table, key).is_some().then(|| (
                DiagnosticKind::Assumption,
                format!(
                    "gog: `order({key})` sorts the categories of `{axis}` by the `{key}` \
                     that `{chain}` writes, not by your table's `{key}` column. To sort by \
                     your table's `{key}`, add it up instead: `{m} * sum + {along}({key})`."
                ),
            ));
        }
        // The other name of the same tally: one ranks as the other does, and the
        // rows hold only the one the chain writes.
        if (key == CELL_COUNT || key == CELL_SHARE) && actual_type(table, key).is_none() {
            return Some((
                DiagnosticKind::Illegal,
                format!(
                    "gog: `order({key})` names a column the rows of `{chain}` do not hold: \
                     they hold `{writes}`. Write `order({writes})`, which ranks each \
                     category of `{axis}` by how many rows it has, as `{key}` would."
                ),
            ));
        }
        if matches!(actual_type(table, key), Some(VarType::Continuous | VarType::Either)) {
            return Some((
                DiagnosticKind::Illegal,
                format!(
                    "gog: `order({key})` would never read `{key}`. `{chain}` measures each \
                     category of `{axis}` by how many rows it has and reads no column, so \
                     the categories would be sorted by that number. To sort them that way, \
                     write `order({writes})`. To sort by `{key}`, draw it: `{m} * mean + \
                     {along}({key}) + order({key})` ranks each category by its mean `{key}`."
                ),
            ));
        }
        return None;
    }
    // Any other column a transform writes is a column (the column rules' first),
    // and not a misspelling; gog does not sort by it yet.
    if actual_type(table, key).is_none() && !written_order_draws(layer, table, key) {
        if let Some(VarType::Continuous) = written_column(spec, layer, key) {
            return Some((
                DiagnosticKind::Unsupported,
                format!(
                    "gog: `order({key})` names the `{key}` that `{chain}` writes. It is a \
                     column like any other, but gog does not sort the categories of `{axis}` \
                     by it yet. Sort by a column of numbers from the table, which `{chain}` \
                     ranks each category by, or write `order({axis})`."
                ),
            ));
        }
    }
    None
}
