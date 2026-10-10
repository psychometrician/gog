//! What a named column must be: present in its table, and one kind of value on
//! every layer that shares its axis or its legend.

use super::*;

/// Refuse a binding whose column name is empty (ruled 2026-10-09).
///
/// Only R could ever send one: a missing argument deparses to `""`, so
/// `y(limits = c(0, 60))` reached the wire with no name. Under `bar * count` it
/// drew the counts, because the measure-name rule took an empty name as naming
/// nothing, and on a `point` it was refused as a misspelling of a column called
/// nothing. Python, Julia and JavaScript cannot write it at all, so the ruling
/// was to refuse it in R where it is written, and here for a specification
/// written by hand. A setting describes the axis of the column it sits beside,
/// so the direction names that column: the measure's own name where a transform
/// makes the measure (`synth_axis_takes_its_measure_name`'s rule), and a column
/// of the table everywhere else.
pub(super) fn no_column_message(layer: &Layer, channel: &Channel, synth_axis: &Channel) -> String {
    let c = channel_name(channel);
    let measure = if channel == synth_axis {
        crate::transform::cell_measure(&layer.transforms)
    } else {
        None
    };
    if let Some(measure) = measure {
        let chain = std::iter::once(mark_name(&layer.mark))
            .chain(layer.transforms.iter().map(transform_name))
            .collect::<Vec<_>>().join(" * ");
        return format!(
            "gog: `{c}()` names no column. On `{chain}` the {c} axis shows the `{measure}` \
             the transform makes, so to give that axis its settings, name it: \
             `{c}({measure}, limits = …)`. With nothing to set, leave `{c}()` out."
        );
    }
    if matches!(channel, Channel::X | Channel::Y | Channel::Z) {
        return format!(
            "gog: `{c}()` names no column. A position maps a column of the table, and its \
             settings describe that column's axis, so name the column first: \
             `{c}(<column>, limits = …)`."
        );
    }
    format!("gog: `{c}()` names no column. A channel maps a column of the table: `{c}(<column>)`.")
}

/// Explain a binding that names a column the table does not hold.
///
/// The R front end deparses the argument, so a quoted string arrives with its
/// quotes intact: `color(species)` gives `species`, `color("red")` gives
/// `"red"`. That surviving quote is a reliable signal that the caller passed a
/// *value* where the grammar expects a *column name* — which is almost always
/// someone reaching for a constant. Saying "check the spelling" to that reader
/// sends them looking for a typo that isn't there.
pub(super) fn missing_column_message(c: &str, m: &str, field: &str, channel: &Channel) -> String {
    let literal = field.len() >= 2 && field.starts_with('"') && field.ends_with('"');
    if !literal {
        return format!(
            "gog: `{c}({field})` refers to a column that is not in the data. \
             Check the spelling of `{field}`."
        );
    }

    let inner = &field[1..field.len() - 1];

    // A color where a column belongs: they meant to set, not map.
    if *channel == Channel::Color && is_valid_color(inner) {
        return format!(
            "gog: `{c}({field})` names a column, and there is no column called `{inner}`. \
             To paint every {m} {inner}, that is a setting rather than a mapping: \
             use `style(color = {field})`."
        );
    }

    format!(
        "gog: `{c}({field})` is quoted, so it names a column called `{inner}` — and there \
         is no such column. Column names are written bare: `{c}({inner})`. If you meant a \
         fixed value rather than a column, that is `style()`."
    )
}

/// An axis holds one kind of value, whichever table each layer reads it from.
///
/// Two layers can draw from two tables, and a column of one name can be text in
/// one and numbers in the other. The axis then took the categorical reading and
/// read every number as a category's place, counting from 0: a label meant for
/// `g = 10` over three categories was drawn far past the right edge, and points at
/// 1, 2 and 3 under one text label left the canvas. Nothing was said. A date read
/// against a plain number is the same contradiction, since the number is taken as
/// seconds from 1970. So a disagreement is refused, and the message names each
/// table and what it holds. Reading a number as a place between categories is a
/// possible later feature; refusing now keeps that door open.
///
/// Only a column the layer's own table holds is asked. One it does not hold is
/// the missing-column refusal's business, or an axis the mark spans rather than
/// reads (a `rule`'s other axis, a `zone`'s sides from `bounds`).
pub(super) fn check_axis_kinds(out: &mut Vec<Diagnostic>, spec: &PlotSpec, data: &HashMap<String, DataFrame>) {
    for ch in [Channel::X, Channel::Y, Channel::Z] {
        // (kind, table, column, marks) for each distinct reading of this axis.
        let mut readings: Vec<(&'static str, String, String, Vec<&'static str>)> = Vec::new();
        for layer in &spec.layers {
            let Some(cd) = spec.position_for(layer, &ch) else { continue };
            let Some(name) = layer.data.as_ref().or(spec.data.as_ref()) else { continue };
            let Some(df) = data.get(name) else { continue };
            let kind = if df.time_unit(&cd.field).is_some() {
                "dates"
            } else {
                match actual_type(df, &cd.field) {
                    Some(VarType::Continuous) => "numbers",
                    Some(VarType::Discrete) => "text",
                    _ => continue,
                }
            };
            let m = mark_name(&layer.mark);
            match readings.iter_mut().find(|r| r.0 == kind && r.1 == *name && r.2 == cd.field) {
                Some(r) => {
                    if !r.3.contains(&m) {
                        r.3.push(m);
                    }
                }
                None => readings.push((kind, name.clone(), cd.field.clone(), vec![m])),
            }
        }
        let mut kinds: Vec<&str> = readings.iter().map(|r| r.0).collect();
        kinds.sort_unstable();
        kinds.dedup();
        if kinds.len() < 2 {
            continue;
        }
        let ways = if kinds.len() == 2 { "two" } else { "three" };
        let axis = match ch {
            Channel::X => "x",
            Channel::Y => "y",
            _ => "z",
        };
        let parts: Vec<String> = readings
            .iter()
            .map(|(kind, table, field, marks)| {
                let marks = marks.iter().map(|m| format!("`{m}`")).collect::<Vec<_>>().join(", ");
                format!("`{field}` holds {kind} in `{table}` (read by {marks})")
            })
            .collect();
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: the `{axis}` axis is read {ways} ways: {}. An axis holds one kind of \
                 value, and a number on an axis of categories would be taken as a \
                 category's place, counting from 0, so it lands on the wrong category \
                 or off the plot. Give the column one kind in every table the plot \
                 reads: the category names as text, or numbers on a numeric axis.",
                parts.join(", and ")
            ),
        });
    }
}

/// A plot draws one legend for each channel that draws one, so its layers map
/// such a channel from one column.
///
/// A second layer's `color` column was drawn through the first layer's scale when
/// it held categories, and through a ramp of its own when it held numbers. Either
/// way the one legend named the first column only, so the second column's colors
/// reached the reader with no legend: §12's silent assumption, in the commonest real
/// form a heatmap's ramp under points colored by a category. `size`, `opacity`,
/// `shape` and `pattern` did the same, each with a legend for its first column alone.
/// Refused (ruled 2026-09-27, R2, for `color`; the four siblings by Law 2, since
/// the reason is word for word the same), and pointed at another channel for the
/// second column, one that draws a legend of its own, or at a facet. A plot that
/// writes `legend = FALSE` on a channel has chosen to go without its legend, and is
/// let through. One legend per column is the later feature this leaves room for;
/// merging both columns into one legend was the third reading, set aside.
pub(super) fn check_legend_columns(out: &mut Vec<Diagnostic>, spec: &PlotSpec, data: &HashMap<String, DataFrame>) {
    const WITH_LEGEND: [Channel; 5] =
        [Channel::Color, Channel::Size, Channel::Opacity, Channel::Shape, Channel::Pattern];
    'channels: for ch in &WITH_LEGEND {
        // (column, kind, table, marks) for each distinct reading of this channel.
        let mut readings: Vec<(String, &'static str, String, Vec<Mark>)> = Vec::new();
        for layer in &spec.layers {
            let Some(cd) = layer.encodings.get(ch) else { continue };
            if cd.legend == Some(false) {
                continue 'channels;
            }
            let Some(name) = layer.data.as_ref().or(spec.data.as_ref()) else { continue };
            let Some(df) = data.get(name) else { continue };
            let kind = if df.time_unit(&cd.field).is_some() {
                "dates"
            } else {
                match actual_type(df, &cd.field) {
                    Some(VarType::Continuous) => "numbers",
                    Some(VarType::Discrete) => "text",
                    _ => continue,
                }
            };
            match readings.iter_mut().find(|r| r.0 == cd.field && r.1 == kind) {
                Some(r) => {
                    if !r.3.contains(&layer.mark) {
                        r.3.push(layer.mark.clone());
                    }
                }
                None => readings.push((cd.field.clone(), kind, name.clone(), vec![layer.mark.clone()])),
            }
        }
        if readings.len() < 2 {
            continue;
        }
        let c = channel_name(ch);
        // A column name read twice, as two kinds, needs its table to tell them apart.
        let named_twice = |f: &str| readings.iter().filter(|r| r.0 == f).count() > 1;
        let parts: Vec<String> = readings
            .iter()
            .map(|(field, _, table, marks)| {
                let marks = marks.iter().map(|m| format!("`{}`", mark_name(m))).collect::<Vec<_>>().join(", ");
                if named_twice(field) {
                    format!("`{field}` in `{table}` (on {marks})")
                } else {
                    format!("`{field}` (on {marks})")
                }
            })
            .collect();
        let (first, _, _, _) = &readings[0];
        let (second, kind, _, marks) = &readings[1];
        // Ways to show the second column that draw a legend of their own: another
        // channel with a legend that its marks take for its kind, one the plot does not map
        // already, and a facet for a column of categories.
        let wanted = if *kind == "text" { VarType::Discrete } else { VarType::Continuous };
        let others: &[Channel] = if *kind == "text" {
            &[Channel::Color, Channel::Shape, Channel::Pattern]
        } else {
            &[Channel::Color, Channel::Size, Channel::Opacity]
        };
        let mapped = |o: &Channel| spec.layers.iter().any(|l| l.encodings.contains_key(o));
        let mut ways: Vec<String> = Vec::new();
        for m in marks {
            for o in others.iter().filter(|o| *o != ch && !mapped(o)) {
                let rule = rule_for(m, o);
                if rule.obligation != Obligation::Cannot && rule.renders.is_some_and(|t| t.accepts(wanted)) {
                    ways.push(format!("`{}({second})` on the `{}`", channel_name(o), mark_name(m)));
                }
            }
        }
        ways.truncate(2);
        if *kind == "text" {
            ways.push(format!("`facet({second})` for a panel each"));
        }
        if ways.is_empty() {
            ways.push("a second plot beside this one, with `|`".to_string());
        }
        let ways = match ways.len() {
            1 => ways[0].clone(),
            n => format!("{}, or {}", ways[..n - 1].join(", "), ways[n - 1]),
        };
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `{c}` maps {} columns in one plot: {}. A plot draws one legend for \
                 `{c}`, and it names `{first}`, so `{second}` would be drawn with no legend \
                 to read it by. Keep one column on `{c}` and show `{second}` another way: {ways}.",
                readings.len(),
                parts.join(", and ")
            ),
        });
    }
}
