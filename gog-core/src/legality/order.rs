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
