//! The names a message uses for the atoms, and the lists it joins them into.

use super::*;

/// "a bar", but "an area".
///
/// Every mark name went into these templates behind a hardcoded "a" until
/// `area` shipped and three diagnostics started reading "a area has no size to
/// set". A message that tells you what to do should not also be the reason you
/// stop trusting it.
pub(super) fn article(word: &str) -> &'static str {
    match word.chars().next() {
        Some('a' | 'e' | 'i' | 'o' | 'u') => "an",
        _ => "a",
    }
}

pub(crate) fn mark_name(mark: &Mark) -> &'static str {
    match mark {
        Mark::Point => "point",
        Mark::Line => "line",
        Mark::Area => "area",
        Mark::Bar => "bar",
        Mark::Step => "step",
        Mark::Interval => "interval",
        Mark::Box => "box",
        Mark::Ribbon => "ribbon",
        Mark::Text => "text",
        Mark::Path => "path",
        Mark::Rule => "rule",
        Mark::Zone => "zone",
        Mark::Surface => "surface",
        Mark::Edge => "edge",
    }
}

pub(crate) fn transform_name(t: &Transform) -> &'static str {
    match t {
        Transform::Bin => "bin",
        Transform::Smooth => "smooth",
        Transform::SmoothBand => "smooth_band",
        Transform::Straight => "straight",
        Transform::StraightBand => "straight_band",
        Transform::Count => "count",
        Transform::Density => "density",
        Transform::Sum => "sum",
        Transform::Mean => "mean",
        Transform::Median => "median",
        Transform::Max => "max",
        Transform::Min => "min",
        Transform::Quantile => "quantile",
        Transform::Proportion => "proportion",
        Transform::Cumulative => "cumulative",
        Transform::Range => "range",
        Transform::Confidence => "confidence",
        Transform::Deviation => "deviation",
        Transform::Box => "box",
        Transform::Bounds => "bounds",
        Transform::Dodge => "dodge",
        Transform::Stack => "stack",
        Transform::Jitter => "jitter",
        Transform::Repel => "repel",
        Transform::Partition => "partition",
        Transform::Flow => "flow",
        Transform::Layout => "layout",
        Transform::Cluster => "cluster",
    }
}

/// A pair transform as the reader wrote it, arguments included, for a message
/// that hands it back in a sentence to write: `range`, `range(0.25, 0.75)`,
/// `confidence(0.9)`, `bounds(lower, upper)`. A bare name where an argument is
/// required (`bounds` with no columns) would be a sentence that does not run.
pub(super) fn written_pair(layer: &Layer, t: &Transform) -> String {
    let name = transform_name(t);
    let args = match t {
        Transform::Range => layer.range.as_ref().and_then(|r| match (r.low, r.high) {
            (Some(lo), Some(hi)) => Some(format!("{lo}, {hi}")),
            (Some(lo), None) => Some(format!("low = {lo}")),
            (None, Some(hi)) => Some(format!("high = {hi}")),
            (None, None) => None,
        }),
        Transform::Confidence => layer.confidence.as_ref().and_then(|c| c.level).map(|l| l.to_string()),
        Transform::SmoothBand => layer.smooth_band.as_ref().and_then(|c| c.level).map(|l| l.to_string()),
        Transform::StraightBand => layer.straight_band.as_ref().and_then(|c| c.level).map(|l| l.to_string()),
        Transform::Deviation => layer.deviation.as_ref().and_then(|d| d.multiplier).map(|m| m.to_string()),
        Transform::Bounds => layer.bounds.as_ref().and_then(|b| b.lower.as_deref().zip(b.upper.as_deref()))
            .map(|(lo, hi)| format!("{lo}, {hi}")),
        _ => None,
    };
    match args {
        Some(a) => format!("{name}({a})"),
        None => name.to_string(),
    }
}

pub(crate) fn channel_name(channel: &Channel) -> &'static str {
    match channel {
        Channel::X => "x",
        Channel::Y => "y",
        Channel::Z => "z",
        Channel::Color => "color",
        Channel::Size => "size",
        Channel::Shape => "shape",
        Channel::Pattern => "pattern",
        Channel::Opacity => "opacity",
        Channel::Group => "group",
        Channel::Label => "label",
        Channel::Play => "play",
    }
}

/// `a`, `a or b`, `a, b, or c` — so a suggestion list reads as a sentence.
pub(super) fn or_list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [a] => a.clone(),
        [a, b] => format!("{a} or {b}"),
        [rest @ .., last] => format!("{}, or {last}", rest.join(", ")),
    }
}

/// `a`, `a and b`, `a, b, and c` — `or_list`'s twin, for things that all hold.
pub(crate) fn and_list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [a] => a.clone(),
        [a, b] => format!("{a} and {b}"),
        [rest @ .., last] => format!("{}, and {last}", rest.join(", ")),
    }
}

/// `a`, `a and b`, `a, b and c`: plain words, for a message.
pub(super) fn code_free_list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [init @ .., last] => format!("{} and {last}", init.join(", ")),
    }
}

/// `a`, `a` and `b`, `a`, `b` and `c`: each name in code, for a message.
pub(super) fn code_list(names: &[&str]) -> String {
    match names {
        [] => String::new(),
        [one] => format!("`{one}`"),
        [init @ .., last] => format!(
            "{} and `{last}`",
            init.iter().map(|n| format!("`{n}`")).collect::<Vec<_>>().join(", ")),
    }
}

/// `a`, `a` or `b`, `a`, `b` or `c`: each name in code, as alternatives.
pub(super) fn code_list_or(names: &[&str]) -> String {
    match names {
        [] => String::new(),
        [one] => format!("`{one}`"),
        [init @ .., last] => format!(
            "{} or `{last}`",
            init.iter().map(|n| format!("`{n}`")).collect::<Vec<_>>().join(", ")),
    }
}

/// The aesthetic channels that take a column of this type on this mark and draw
/// it, the one asked for aside: the offers a type refusal can make without
/// sending the reader to a second refusal.
pub(super) fn channels_that_carry(mark: &Mark, asked: &Channel, actual: VarType) -> Vec<&'static str> {
    [Channel::Color, Channel::Size, Channel::Shape, Channel::Pattern, Channel::Opacity]
        .iter()
        .filter(|c| *c != asked)
        .filter(|c| {
            let r = rule_for(mark, c);
            r.obligation != Obligation::Cannot
                && r.accepts.accepts(actual)
                && r.renders.is_some_and(|drawn| drawn.accepts(actual))
        })
        .map(channel_name)
        .collect()
}

/// The marks that take a channel, mapped (`true`) or set (`false`), for a refusal
/// that sends the reader to another mark: read off `rule_for` as the grid is, so
/// the list names only marks that take it. "Use a mark that has one" named none. A
/// mark the graph places is left out, since only a `layout` in a network gives it
/// a position.
pub(super) fn marks_that_take(channel: &Channel, mapped: bool) -> Vec<&'static str> {
    ALL_MARKS.iter()
        .filter(|mk| is_drawable(mk) && !placed_by_the_graph(&Layer::new((*mk).clone())))
        .filter(|mk| {
            let r = rule_for(mk, channel);
            if mapped {
                r.obligation != Obligation::Cannot && r.renders.is_some()
            } else {
                r.settable
            }
        })
        .map(mark_name)
        .collect()
}

/// What a cell mark takes, read off `mark_takes_transform` as the grid is, split
/// into the transforms that make or place its cells and the reductions that fill
/// them. Returned as code lists for a message, so the list cannot drift from the
/// table: typed out, the zone's list had fallen four transforms short.
pub(super) fn cell_mark_takes(mark: &Mark) -> (String, String) {
    let taken = USER_TRANSFORMS.iter()
        .filter(|t| !is_collision_modifier(t) && mark_takes_transform(mark, t) != TransformLegality::None);
    let (reduces, tiles): (Vec<&Transform>, Vec<&Transform>) =
        taken.partition(|t| crate::transform::is_reduction(t));
    let names = |ts: Vec<&Transform>| code_list(&ts.into_iter().map(transform_name).collect::<Vec<_>>());
    (names(tiles), names(reduces))
}

/// Why a zone or a surface refuses the transforms it was given, one clause per
/// family written, so two transforms with one reason share it and a transform
/// the author did not write is not explained.
pub(super) fn cell_refusal_reasons(refused: &[&Transform]) -> String {
    // `layout`, `bounds`, `partition` and `flow` never reach here: each refuses a
    // mark in its own check (`refuses_marks_itself`), which names the marks that
    // draw it.
    let reason = |t: &Transform| -> (u8, &'static str, &'static str) {
        match t {
            Transform::Smooth | Transform::SmoothBand => (0,
                "fits a curve along a domain, which a cell has none of",
                "fit a curve along a domain, which a cell has none of"),
            Transform::Straight | Transform::StraightBand => (8,
                "fits a line along a domain, which a cell has none of",
                "fit a line along a domain, which a cell has none of"),
            Transform::Range | Transform::Confidence | Transform::Deviation => (1,
                "gives a low and a high, where a cell holds one value",
                "give a low and a high, where a cell holds one value"),
            Transform::Count | Transform::Proportion => (3,
                "tallies into the cells two categories make, and slots leave air between \
                 them, so the tiles would not join into a sheet",
                "tally into the cells two categories make, and slots leave air between them, \
                 so the tiles would not join into a sheet"),
            Transform::Cluster => (6,
                "orders the categories of an axis, and this floor has none",
                "order the categories of an axis"),
            _ => (7,
                "is not among the transforms this mark takes",
                "are not among the transforms this mark takes"),
        }
    };
    let mut clauses: Vec<(u8, Vec<&str>, &str, &str)> = Vec::new();
    for t in refused {
        let (family, one, many) = reason(t);
        match clauses.iter_mut().find(|c| c.0 == family) {
            Some(c) => c.1.push(transform_name(t)),
            None => clauses.push((family, vec![transform_name(t)], one, many)),
        }
    }
    clauses.iter()
        .map(|(_, names, one, many)| {
            format!("{} {}", code_list(names), if names.len() == 1 { one } else { many })
        })
        .collect::<Vec<_>>()
        .join("; ")
}
