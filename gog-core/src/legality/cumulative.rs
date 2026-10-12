//! A running total: which marks draw one, and what it can add up.

use super::*;

// ---------------------------------------------------------------------------
// Cumulative — the running total, and what it can be a total of
//
// `cumulative` adds each position's value to the ones before it, along the axis
// the mark is measured against (spec §5, ruled 2026-10-11). It measures nothing
// itself, so two questions are its own: is the mark one that draws a measure, and
// is what the layer measures a number a total can be made of. Everything else it
// shares with the rest of the kernel: two transforms on its scale are
// `check_chain_jobs`'s, and a total with nothing to add up is the missing `y`.
// ---------------------------------------------------------------------------

/// Refuse `cumulative` on a mark that draws no single measure, in a space with no
/// one axis to add along, and beside a measurement whose values do not add up.
pub(super) fn check_cumulative(out: &mut Vec<Diagnostic>, spec: &PlotSpec, layer: &Layer) {
    let ts = &layer.transforms;
    if !crate::transform::accumulates(ts) {
        return;
    }
    let m = mark_name(&layer.mark);
    // A box takes no transform at all, and `check_box` says so in its own words.
    if layer.mark == Mark::Box {
        return;
    }
    if mark_takes_transform(&layer.mark, &Transform::Cumulative) == TransformLegality::None {
        let why = match layer.mark {
            Mark::Interval | Mark::Ribbon =>
                "draws a span between two values, and a total is one value at each position",
            Mark::Zone =>
                "fills cells with a color, and cells across a plane have no order to add up along",
            Mark::Path =>
                "keeps the order of the table's rows, and a total is added up in the order of the axis",
            Mark::Rule => "is placed by one column and measures nothing to add up",
            Mark::Edge => "joins two places a layout supplies and measures nothing to add up",
            Mark::Surface =>
                "is a sheet over a floor of cells, which have no order to add up along",
            _ => "does not draw a measure",
        };
        let instead = match layer.mark {
            Mark::Path => "`line` sorts its rows along the axis and draws the total: \
                           `line * cumulative + x(<a>) + y(<b>)`.".to_string(),
            _ => "The marks that draw a measure draw the total: `bar` and `area` from zero, \
                  `line`, `step` and `point` at the total, and `text` to print it, as in \
                  `line * cumulative + x(<a>) + y(<b>)`.".to_string(),
        };
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `cumulative` adds each position's value to the ones before it and \
                 draws the total, and `{m} * cumulative` has no total to draw: `{m}` {why}. \
                 {instead}"
            ),
        });
        return;
    }

    // **A total runs along one axis, and the plane and the circle each have one**
    // (2026-10-11). The cube has two floor positions with the measure standing on
    // `z`, so nothing says which one the total runs along; a packing sizes regions
    // by their measure, so the totals would be packed again and the areas would add
    // up to more than the whole; a map and a network place their rows by place and
    // by link. Each of these drew an empty or a false picture in silence before.
    let why = match space_of(spec) {
        SpaceKind::Flat | SpaceKind::Polar => None,
        SpaceKind::Space => Some("in `space()` the floor has two positions, `x` and `y`, and \
            the measure stands on `z`, so there is no one axis to add the total along"),
        SpaceKind::Nest => Some("`nest()` packs each region by its measure, so the totals \
            would be packed again, and the areas would add up to more than the whole"),
        SpaceKind::Map | SpaceKind::Globe => Some("a map places each row at its longitude \
            and latitude, so it has no axis to add a total along"),
        SpaceKind::Network => Some("a network places its nodes by their links, so it has \
            no axis to add a total along"),
    };
    if let Some(why) = why {
        let chain = std::iter::once(m).chain(ts.iter().map(transform_name))
            .collect::<Vec<_>>().join(" * ");
        // The flat sentence keeps the chain; it names a `y` only when the total adds
        // up a column, since a tally writes its own.
        let positions = match crate::transform::cell_measure(ts) {
            Some(_) => "x(<a>)",
            None => "x(<a>) + y(<b>)",
        };
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `{chain}` adds each position's value to the ones before it, along one \
                 axis, and {why}. Draw the running total flat, where it runs along `x`: \
                 `{chain} + {positions}`, with `color()` giving each group its own total."
            ),
        });
        return;
    }

    // What the total would be a total of. Each refusal names the measurement that
    // does add up, so the way out is a sentence that draws.
    let Some(t) = ts.iter().find(|t| matches!(t,
        Transform::Density | Transform::Smooth | Transform::SmoothBand
            | Transform::Range | Transform::Confidence | Transform::Deviation | Transform::Bounds
            | Transform::Partition))
    else { return };
    // A pair on a mark that does not draw one is refused by its own check.
    if is_pair_transform(t) && mark_takes_transform(&layer.mark, t) == TransformLegality::None {
        return;
    }
    let tn = transform_name(t);
    let message = match t {
        Transform::Density => format!(
            "gog: `{m} * density * cumulative` would add up the heights of an estimated \
             curve, and those heights are not counts of anything: the curve's total is the \
             area under it, which is 1. For the share of rows up to each value, cut the \
             rows into bins and add those: `{m} * bin * cumulative + x(<number>)` with its \
             `share` set to true."
        ),
        Transform::Smooth | Transform::SmoothBand => format!(
            "gog: `{m} * {tn} * cumulative` would add up the points of a fitted curve, and \
             how many points a curve is drawn with is not a fact about the data, so the \
             total would be too. Add up the values themselves: `{m} * cumulative + x(<a>) + \
             y(<b>)`, or `{m} * sum * cumulative` when a position has several rows."
        ),
        Transform::Partition => format!(
            "gog: `{m} * partition * cumulative` would add up the nodes of a hierarchy, \
             and a partition places them by their parents, not along an axis, so there \
             is no order to add them up in. A node's size already holds the total of the \
             nodes inside it. To add up a column along an axis, draw it with \
             `line * cumulative + x(<a>) + y(<b>)`."
        ),
        _ => format!(
            "gog: `{m} * {tn} * cumulative` would add up a pair, a low and a high value at \
             each position, and a total is one number. Add up one value per position: \
             `{m} * sum * cumulative` for the totals, or `{m} * mean * cumulative` for the \
             means."
        ),
    };
    out.push(Diagnostic { kind: DiagnosticKind::Illegal, message });
}

/// What to say when `cumulative` and a division fill the scale job together
/// (`proportion`, or a `stack` asked for shares). Each way out is a sentence that
/// draws, which depends on what the layer adds up: a statistic's measurement, the
/// column it names on the measure axis, or, with neither, the rows, which have to be
/// counted first. The share that ends at 1 is said with `cumulative`'s own knob,
/// since `proportion` divides by the whole plot.
pub(super) fn cumulative_scale_message(spec: &PlotSpec, layer: &Layer) -> String {
    let ts = &layer.transforms;
    let m = mark_name(&layer.mark);
    let chain = std::iter::once(m).chain(ts.iter().map(transform_name))
        .collect::<Vec<_>>().join(" * ");
    let before: String = ts.iter()
        .filter(|t| {
            let j = crate::transform::jobs(t, crate::transform::JobContext::default());
            j.measure || j.extent
        })
        .map(|t| format!(" * {}", transform_name(t)))
        .collect();
    // Nothing measures, and both positions are named: the layer adds up a column.
    let names_a_column = before.is_empty()
        && spec.position_for(layer, &Channel::X).is_some()
        && spec.position_for(layer, &Channel::Y).is_some();
    let measured = match (before.is_empty(), names_a_column) {
        (false, _) => before.clone(),
        (true, true) => String::new(),
        (true, false) => " * count".to_string(),
    };
    if ts.contains(&Transform::Proportion) {
        let whole = if names_a_column { " * sum".to_string() } else { before };
        format!(
            "gog: `{chain}` reads the measurement on two scales at once: `proportion` \
             divides it into shares of the whole plot, and `cumulative` adds each position \
             to the ones before it, so one of the two would be discarded. For running shares \
             that end at 1, write `{m}{measured} * cumulative` with its `share` set to true. \
             For each position's share of the whole plot, drop `cumulative`: \
             `{m}{whole} * proportion`."
        )
    } else {
        format!(
            "gog: `{chain}`, with the `stack` asked for shares (its `share` set to true), \
             reads the measurement on two scales at once: the `stack` divides each pile into \
             shares, and `cumulative` adds each position to the ones before it, so one of \
             the two would be discarded. To pile the running totals, keep the `stack` plain: \
             `{m}{measured} * cumulative * stack`. For running shares that end at 1 in each \
             group, write `{m}{measured} * cumulative` with its `share` set to true."
        )
    }
}
