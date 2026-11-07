//! A change on a running total: which marks draw one, and what it walks.

use super::*;

// ---------------------------------------------------------------------------
// Change — each row's step on a running total, the waterfall (spec §5)
//
// `change` walks a categorical position in its declared order, keeps a running
// total of the measure, and writes each row as a span from the total before it to
// the total after (ruled 2026-11-06). The table holds the items only, each with its
// own amount, and `totals` names the next total after each item; the transform
// computes every total, as `count` computes a count. So its questions are its own: a
// mark that draws a span on a slot, one category to walk and one number to add, a
// totals column of names, each total's items standing together, and one walk per
// panel.
// ---------------------------------------------------------------------------

/// Refuse `change` where it has no span to draw, nothing to walk, or no place to put
/// a total it computes.
pub(super) fn check_change(
    out: &mut Vec<Diagnostic>,
    spec: &PlotSpec,
    df: Option<&DataFrame>,
    layer: &Layer,
) {
    let ts = &layer.transforms;
    if !ts.contains(&Transform::Change) || layer.mark == Mark::Box {
        return;
    }
    let m = mark_name(&layer.mark);
    let illegal = |out: &mut Vec<Diagnostic>, message: String| {
        out.push(Diagnostic { kind: DiagnosticKind::Illegal, message });
    };
    if mark_takes_transform(&layer.mark, &Transform::Change) == TransformLegality::None {
        let instead = if layer.mark == Mark::Bar {
            " A bar is drawn from zero, and a running total drawn from zero is `bar * cumulative`."
        } else {
            ""
        };
        illegal(out, format!(
            "gog: `change` draws each row as a span on a running total, from the total \
             before it to the total after, and `{m} * change` has no such span to draw. \
             Draw the blocks with `zone * change` and their amounts with `text * change` \
             and `label(<amount>)`.{instead}"
        ));
        return;
    }
    if let Some(other) = ts.iter().find(|t| **t != Transform::Change) {
        let o = transform_name(other);
        let chain = std::iter::once(m).chain(ts.iter().map(transform_name))
            .collect::<Vec<_>>().join(" * ");
        illegal(out, format!(
            "gog: `change` walks each row of the table as it is, so it stands alone in its \
             layer, and `{chain}` adds `{o}` to it. Give the table one row per line \
             before the plot, and write `{m} * change`."
        ));
        return;
    }
    if space_of(spec) != SpaceKind::Flat {
        illegal(out, format!(
            "gog: `{m} * change` walks a categorical axis and draws each step along the \
             other, which needs the plane. Remove the space and draw it flat."
        ));
        return;
    }
    let Some(df) = df else { return };

    // One category to walk, one number to add, on the two positions.
    let field = |ch: Channel| spec.position_for(layer, &ch).map(|d| d.field.as_str());
    let category = |f: Option<&str>| f.is_some_and(|f| df.str_col(f).is_some() && df.time_unit(f).is_none());
    let number = |f: Option<&str>| f.is_some_and(|f| df.float_col(f).is_some());
    let (fx, fy) = (field(Channel::X), field(Channel::Y));
    let walk = match (category(fx), number(fy), category(fy), number(fx)) {
        (true, true, _, _) => fx.zip(fy),
        (_, _, true, true) => fy.zip(fx),
        _ => None,
    };
    let Some((key, _)) = walk else {
        illegal(out, format!(
            "gog: `{m} * change` walks the categories of one position in their declared \
             order and adds up a number on the other, so it needs one of each: \
             `{m} * change + x(<line item>) + y(<amount>)`. A number on both positions \
             has no steps to walk, and two categories have no amount to add."
        ));
        return;
    };

    // The totals column names a total on every row. A column of TRUE and FALSE is a
    // statement that already holds its own sums: the transform computes them.
    let totals = layer.change.as_ref().and_then(|c| c.totals.as_deref());
    let mut names: Option<&Vec<String>> = None;
    if let Some(t) = totals {
        if df.str_col(t).is_none() && df.float_col(t).is_none() {
            illegal(out, format!(
                "gog: `change(totals = {t})` refers to a column that is not in the data. \
                 Check the spelling of `{t}`."
            ));
            return;
        }
        if crate::transform::total_flags(df, t).is_some() {
            illegal(out, format!(
                "gog: `change(totals = {t})` names, for each item, the next total after it, \
                 such as `Net income`, and `{t}` holds true and false. `change` computes each \
                 total from the items before it, so the table needs no rows for them: leave \
                 out the rows that state a total, and write in `{t}` the name of the next \
                 total after each item."
            ));
            return;
        }
        let Some(col) = df.str_col(t) else {
            illegal(out, format!(
                "gog: `change(totals = {t})` names, for each item, the next total after it, \
                 and `{t}` holds numbers. Give it the totals' names, such as `Net income`."
            ));
            return;
        };
        let keys = df.str_col(key);
        if let Some(r) = col.iter().position(|v| v.trim().is_empty()) {
            let line = keys.and_then(|k| k.get(r)).map_or(String::new(), |k| format!(" on `{k}`"));
            illegal(out, format!(
                "gog: `change(totals = {t})` draws each total after the last item that names \
                 it, and `{t}` is empty{line}. Name the next total after each item."
            ));
            return;
        }
        if let Some(n) = col.iter().find(|n| keys.is_some_and(|k| k.contains(n))) {
            illegal(out, format!(
                "gog: `change(totals = {t})` draws a block named `{n}` after the last item \
                 that names it, and `{key}` already has an item named `{n}`, so the two would \
                 share one slot. Give the total a name that no item has."
            ));
            return;
        }
        names = Some(col);
    }

    // One walk per panel: a split by a column of the table would lay several walks
    // over one axis. The written `direction` is not a split of the table.
    if let Some((ch, f)) = [Channel::Color, Channel::Group, Channel::Pattern, Channel::Shape].iter()
        .filter_map(|ch| {
            layer.encodings.get(ch).or_else(|| spec.channels.get(ch)).map(|d| (ch, d.field.as_str()))
        })
        .find(|(_, f)| *f != crate::transform::CHANGE_DIRECTION && df.str_col(f).is_some())
    {
        let c = channel_name(ch);
        illegal(out, format!(
            "gog: `{m} * change` walks one running total in each panel, and `{c}({f})` \
             would lay one walk for each `{f}` over the same axis. Draw one walk per \
             `{f}` with `| facet({f})`, or color the steps with `color(direction)`."
        ));
        return;
    }

    // Each total is drawn after the last item naming it, so its items are next to each other
    // in the walk. The walk is per panel and per frame, as the renderer walks it: a
    // facet or `play` splits the rows before any transform runs, so two statements in
    // one table are two walks.
    let (Some(t), Some(names)) = (totals, names) else { return };
    let keys: Vec<&str> = spec.facet.as_ref().into_iter()
        .flat_map(|f| [f.col.as_deref(), f.row.as_deref()])
        .flatten()
        .chain(layer.encodings.get(&Channel::Play).or_else(|| spec.channels.get(&Channel::Play))
            .map(|d| d.field.as_str()))
        .filter(|k| df.str_col(k).is_some())
        .collect();
    let mut cells: Vec<(Vec<String>, Vec<usize>)> = Vec::new();
    for r in 0..df.len() {
        let cell: Vec<String> = keys.iter()
            .map(|k| df.str_col(k).and_then(|c| c.get(r).cloned()).unwrap_or_default())
            .collect();
        match cells.iter_mut().find(|(c, _)| *c == cell) {
            Some((_, rows)) => rows.push(r),
            None => cells.push((cell, vec![r])),
        }
    }
    let items = df.str_col(key);
    for (_, rows) in cells {
        let taken;
        let part = if keys.is_empty() { df } else { taken = df.take_rows(&rows); &taken };
        let back = |i: usize| if keys.is_empty() { i } else { rows[i] };
        let walk = crate::transform::change_order(part, key);
        let mut closed: Vec<&str> = Vec::new();
        let mut last: Option<usize> = None;
        for &i in &walk {
            let n = names[back(i)].as_str();
            if let Some(p) = last {
                let prev = names[back(p)].as_str();
                if prev != n {
                    if closed.contains(&n) {
                        let item = |r: usize| items.and_then(|k| k.get(back(r)).cloned()).unwrap_or_default();
                        illegal(out, format!(
                            "gog: `change(totals = {t})` draws each total after the last item that \
                             names it, so the items that name `{n}` must be next to each other in \
                             the order of `{key}`. `{a}` names `{n}`, but comes after `{b}`, which \
                             names `{prev}`. Declare the order of `{key}` so that the items of each \
                             total are next to each other.",
                            a = item(i), b = item(p),
                        ));
                        return;
                    }
                    closed.push(prev);
                }
            }
            last = Some(i);
        }
    }
}
