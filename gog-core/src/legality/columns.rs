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

/// Whether `channel(field)` names a column this layer's own transforms write, rather
/// than a column of its table.
///
/// The one answer to "which columns does a transform write that a sentence may
/// name", read by the name clash below and by the legend (spec §8, the column
/// rules). Each arm is a column a reading makes and a channel it lands on:
///
/// - the measure a counting transform writes on the axis it fills (`count`,
///   `proportion`, `density`), unless a statistic in the chain reads the name;
/// - the measurement a two-dimensional reading or a raised surface paints with
///   (`count`, `density`, `level`);
/// - a node's `name`, under the three transforms that make nodes, and the
///   `depth` of its ring under `partition`;
/// - what a layout publishes about each node, `name` and `degree`, on the marks
///   that draw nodes.
pub fn names_a_written_column(
    spec: &PlotSpec, layer: &Layer, df: Option<&DataFrame>, channel: &Channel, field: &str,
) -> bool {
    use crate::transform::{cell_measure, NODE_DEGREE, NODE_NAME};
    let ts = &layer.transforms;
    if *channel == synth_axis(spec, layer, df)
        && synthesizes_measure(&layer.mark, ts)
        && slot_density(spec, layer, df).is_none()
        && !ts.iter().any(|t| reads_a_column(t) || matches!(t, Transform::Smooth | Transform::SmoothBand))
        && cell_measure(ts) == Some(field)
    {
        return true;
    }
    if *channel == Channel::Color
        && (field_measure(layer) == Some(field) || measurement_column(spec, layer) == Some(field))
    {
        return true;
    }
    let makes_nodes = ts.iter().any(|t| matches!(t, Transform::Partition | Transform::Flow | Transform::Layout));
    if *channel == Channel::Label && makes_nodes && field == NODE_NAME {
        return true;
    }
    // The ring a partition's node sits on, named on the axis it is drawn along.
    if ts.contains(&Transform::Partition)
        && matches!(channel, Channel::X | Channel::Y)
        && field == crate::transform::NODE_DEPTH
    {
        return true;
    }
    // The same measure on an aesthetic that draws a number on this mark: a point
    // sized by its tally, a bar faded by it, a curve shaded by its height, the
    // count written as text. The renderer publishes the measure under its own name
    // beside the axis's column, so the channel reads it off the rows it draws.
    if !matches!(channel, Channel::X | Channel::Y | Channel::Z)
        && synthesizes_measure(&layer.mark, ts)
        && slot_density(spec, layer, df).is_none()
        && !ts.iter().any(|t| reads_a_column(t) || matches!(t, Transform::Smooth | Transform::SmoothBand))
        && cell_measure(ts) == Some(field)
        && rule_for(&layer.mark, channel).renders.is_some_and(|v| v.accepts(VarType::Continuous))
    {
        return true;
    }
    // The merge distance a cluster tree writes, named on the axis it is drawn along,
    // or on `color`: every vertex of the tree's strokes holds the distance it is
    // drawn at, so a path ramps each elbow from its branches' height to the join's,
    // as it ramps any measured color along a route.
    if ts.contains(&Transform::Cluster)
        && layer.mark == Mark::Path
        && (*channel == synth_axis(spec, layer, df) || *channel == Channel::Color)
        && field == crate::transform::CLUSTER_DISTANCE
    {
        return true;
    }
    ts.contains(&Transform::Layout)
        && matches!(layer.mark, Mark::Point | Mark::Text)
        && (field == NODE_NAME || field == NODE_DEGREE)
}

/// The kind of value in `field`, when this layer's transforms write a column of that
/// name onto the rows its mark draws (spec §8, the column rules' first: a written
/// column is a column).
///
/// Where [`names_a_written_column`] answers which channels draw a written column
/// today, this answers which written columns exist at all, whatever channel names
/// them. A name in this list is never "not in the data": the channel's own rule
/// decides it, by the kind of value given here, and a pair the renderer does not
/// draw yet is refused as not built rather than as a misspelling.
///
/// The rows decide, not the transform alone. A layout writes `degree` for each node,
/// so a `point` or a `text` has it and an `edge`, which draws the links, does not;
/// a flow's slots carry a `name` and its bands, which cross every stage, do not.
pub fn written_column(spec: &PlotSpec, layer: &Layer, field: &str) -> Option<VarType> {
    use crate::transform::{cell_measure, CLUSTER_DISTANCE, NODE_DEGREE, NODE_DEPTH, NODE_NAME};
    let ts = &layer.transforms;
    let reads = ts.iter().any(|t| reads_a_column(t) || matches!(t, Transform::Smooth | Transform::SmoothBand));
    if (!reads && cell_measure(ts) == Some(field))
        || field_measure(layer) == Some(field)
        || measurement_column(spec, layer) == Some(field)
    {
        return Some(VarType::Continuous);
    }
    let draws_nodes = matches!(layer.mark, Mark::Zone | Mark::Text);
    if ts.contains(&Transform::Partition) && draws_nodes {
        if field == NODE_NAME {
            return Some(VarType::Discrete);
        }
        if field == NODE_DEPTH {
            return Some(VarType::Continuous);
        }
    }
    if ts.contains(&Transform::Flow) && draws_nodes && field == NODE_NAME {
        return Some(VarType::Discrete);
    }
    if ts.contains(&Transform::Layout) && matches!(layer.mark, Mark::Point | Mark::Text) {
        if field == NODE_NAME {
            return Some(VarType::Discrete);
        }
        if field == NODE_DEGREE {
            return Some(VarType::Continuous);
        }
    }
    if ts.contains(&Transform::Cluster) && layer.mark == Mark::Path && field == CLUSTER_DISTANCE {
        return Some(VarType::Continuous);
    }
    if field == crate::transform::SPAN_CENTER
        && ts.iter().any(|t| matches!(t, Transform::Confidence | Transform::Deviation | Transform::SmoothBand))
    {
        return Some(VarType::Continuous);
    }
    None
}

/// A `text` that counts writes its number at each place, and its label must name a
/// column the rows it draws hold (spec §8, the column rules' fourth: a channel reads
/// the rows the mark draws). `count`, `bin`, `proportion` and `density` replace the
/// table's rows with one per place, which keep the place itself (the key position
/// and any split) and the number the transform writes. Any other column of the
/// table is not on them, so `text * count + label(country)` has no string to draw.
/// A name the table does not hold is the missing-column refusal's.
pub(super) fn check_text_label_rows(
    out: &mut Vec<Diagnostic>, spec: &PlotSpec, df: Option<&DataFrame>, layer: &Layer,
) {
    if layer.mark != Mark::Text {
        return;
    }
    let synth = synth_axis(spec, layer, df);
    // What a label can print, and what to call it: the column a count writes, or
    // the column a summary reduces in place (`life` under `text * mean`).
    let Some((measure, what)) = summary_label(spec, layer, &synth) else { return };
    let Some(def) = binding_of(spec, layer, &Channel::Label) else { return };
    let field = def.field.as_str();
    if field.is_empty() || field == measure || df.and_then(|d| actual_type(d, field)).is_none() {
        return;
    }
    let keys: Vec<&str> = [Channel::X, Channel::Y].iter()
        .filter(|ch| **ch != synth)
        .filter_map(|ch| spec.position_for(layer, ch).map(|d| d.field.as_str()))
        .collect();
    if keys.contains(&field) || split_fields_of(layer).contains(&field) {
        return;
    }
    let chain = std::iter::once(mark_name(&layer.mark))
        .chain(layer.transforms.iter().map(transform_name))
        .collect::<Vec<_>>().join(" * ");
    let (per, name_each) = match keys.first() {
        Some(k) if !layer.transforms.contains(&Transform::Bin) =>
            (format!("one for each `{k}`"), format!(", and `label({k})` names each one")),
        _ => ("one for each bin".to_string(), String::new()),
    };
    out.push(Diagnostic {
        kind: DiagnosticKind::Illegal,
        message: format!(
            "gog: `label({field})` names a column of your table, and `{chain}` replaces your \
             rows with its own, {per}, which have no `{field}`. `label({measure})` prints \
             each {what}{name_each}."
        ),
    });
}

/// The column a `text` that counts or summarizes can print, and the word for
/// what it holds: `("count", "count")` under `text * count`, and `("life",
/// "mean")` under `text * mean + y(life)`, where the mean is reduced into `life`
/// itself. `None` for a text that does neither, or a summary with no column.
pub(super) fn summary_label<'a>(
    spec: &'a PlotSpec, layer: &'a Layer, synth: &Channel,
) -> Option<(&'a str, &'static str)> {
    let ts = &layer.transforms;
    if let Some(measure) = crate::transform::cell_measure(ts) {
        return Some((measure, measure));
    }
    let reduction = ts.iter().find(|t| crate::transform::is_reduction(t))?;
    let read = spec.position_for(layer, synth)?.field.as_str();
    (!read.is_empty()).then_some((read, transform_name(reduction)))
}

/// A number from your table, named on an aesthetic of a layer whose transform
/// replaced its rows (spec §8, the column rules' fourth: a channel reads the rows
/// the mark draws).
///
/// After `point * count + x(continent)` there are five rows, one per continent,
/// and a column the count did not group by is not on them, just as `country` is
/// gone. `size(gdp)` there was dropped in silence while its legend was still drawn,
/// so the key decoded sizes no point had, and `line * mean + color(year)` drew an
/// empty panel under a ramp. Choosing a mean or a sum for the reader would be a
/// reduction nobody wrote, the choice `cluster` refuses to make as well, so the
/// binding is refused and the direction is to summarize where the data lives.
///
/// **What still reaches those rows is left alone.** A category is a group key, so
/// the transform carries it (`color(continent)`); a position is a key or the column
/// the statistic reads (`size(life)` under `mean + y(life)` draws each mean); a
/// reading that measures with `color` or `z` reads its column there; and a name the
/// transform writes is `check_written_name_clash`'s, even when the table has one.
pub(super) fn rows_lack_column(
    spec: &PlotSpec, layer: &Layer, df: &DataFrame, channel: &Channel, field: &str,
) -> Option<String> {
    // The aesthetics that carry a number. `group` and `play` split the rows before
    // the transform runs, so their column is carried, and `label` on a counting
    // `text` is `check_text_label_rows`'s.
    if !matches!(channel, Channel::Color | Channel::Size | Channel::Opacity) {
        return None;
    }
    if actual_type(df, field) != Some(VarType::Continuous) {
        return None;
    }
    let ts = &layer.transforms;
    if !ts.iter().any(replaces_rows) {
        return None;
    }
    // A transform the mark does not take is refused on its own, and that refusal
    // is the one to read: said here as well, `bar * range` was told to draw its
    // summary table with `bar`, the mark the other message had just refused.
    if ts.iter().any(|t| mark_takes_transform(&layer.mark, t) == TransformLegality::None) {
        return None;
    }
    let on_a_position = [Channel::X, Channel::Y, Channel::Z].iter()
        .any(|ch| spec.position_for(layer, ch).is_some_and(|d| d.field == field));
    if on_a_position || measure_field(spec, layer) == Some(field) {
        return None;
    }
    // A two-dimensional reading's color is `check_field`'s, which says what that
    // reading measures by and offers the summary heatmap.
    let cells = measures_cells(&layer.mark, ts);
    if cells && *channel == Channel::Color {
        return None;
    }
    let c = channel_name(channel);
    let m = mark_name(&layer.mark);
    let chain = std::iter::once(m)
        .chain(ts.iter().map(transform_name))
        .collect::<Vec<_>>().join(" * ");
    let leave = format!("leave `{c}({field})` out");
    // What each new row stands for: the bins or cells it cut, or the keys it
    // grouped by, which are the positions it did not measure and the categories
    // that split it.
    let each = if ts.contains(&Transform::Bin) {
        Some(if cells { "cell".to_string() } else { "bin".to_string() })
    } else {
        let measured = synth_axis(spec, layer, Some(df));
        let mut keys: Vec<&str> = [Channel::X, Channel::Y, Channel::Z].iter()
            .filter(|ch| cells || reads_two_dimensions(&layer.mark, ts, space_of(spec)) || **ch != measured)
            .filter_map(|ch| spec.position_for(layer, ch).map(|d| d.field.as_str()))
            .collect();
        for f in split_fields_of(layer) {
            if f != field && !keys.contains(&f) && actual_type(df, f) == Some(VarType::Discrete) {
                keys.push(f);
            }
        }
        (!keys.is_empty()).then(|| keys.iter().map(|k| format!("`{k}`"))
            .collect::<Vec<_>>().join(" and "))
    };
    // A pair or a fitted curve is not a table a reader can make and draw again
    // with the mark alone: `line` draws no low-and-high, and no mark fits a
    // curve without the transform. Its direction is the reader's own rows,
    // drawn as a second layer beside it.
    let own_rows = match ts {
        _ if cells && field_geometry(layer) == Some(FieldGeometry::Rings) =>
            Some("the contours it traces".to_string()),
        _ if cells => None,
        ts if ts.contains(&Transform::Density) => Some("the points of the curve it estimates".to_string()),
        ts if ts.iter().any(|t| matches!(t, Transform::Smooth | Transform::SmoothBand)) =>
            Some("the points of the curve it fits".to_string()),
        ts if ts.iter().any(|t| matches!(t, Transform::Range | Transform::Confidence
            | Transform::Deviation)) => Some(match &each {
                Some(each) => format!("a low and a high value for each {each}"),
                None => "one low and one high value".to_string(),
            }),
        _ => None,
    };
    if let Some(rows) = own_rows {
        return Some(format!(
            "gog: `{c}({field})` names a column of your table, and `{chain}` replaces your \
             rows with {rows}, which have no `{field}`. To show `{field}` on your own rows, \
             draw them in a layer of their own beside this one. Otherwise, {leave}."
        ));
    }
    // **The direction has to survive being followed.** "Summarize it and map that
    // table's column" did not: drawn again with `point * count`, the summary table
    // was counted one row per continent and its column refused a second time. The
    // new table must hold what the transform measured as well, and be drawn
    // without the transforms that replace rows.
    let replaced: Vec<&str> = ts.iter().filter(|t| replaces_rows(t)).map(transform_name).collect();
    let kept = std::iter::once(m)
        .chain(ts.iter().filter(|t| !replaces_rows(t)).map(transform_name))
        .collect::<Vec<_>>().join(" * ");
    let read = spec.position_for(layer, &synth_axis(spec, layer, Some(df))).map(|d| d.field.as_str());
    let reduced = ts.iter().find(|t| crate::transform::is_reduction(t));
    let measured = match (ts.contains(&Transform::Proportion), reduced, read) {
        (true, _, _) => "its share".to_string(),
        (_, Some(t), Some(y)) => format!("the {} of `{y}`", transform_name(t)),
        _ => "its count".to_string(),
    };
    let rows = match &each {
        Some(each) => format!("its own, one for each {each}, which have"),
        None => "a single row, which has".to_string(),
    };
    let per = match &each {
        Some(each) => format!("one row for each {each}"),
        None => "one row".to_string(),
    };
    // Drawn without the transform, nothing writes the measure axis any more, so
    // the direction says where the measured value goes, or the reader who follows
    // it is told the mark needs that axis.
    let axis = channel_name(&synth_axis(spec, layer, Some(df)));
    let put = measured.replacen("its ", "the ", 1);
    Some(format!(
        "gog: `{c}({field})` names a column of your table, and `{chain}` replaces your rows \
         with {rows} no `{field}`. To show `{field}`, make a table with {per}, holding \
         {measured} and a summary of `{field}`. Draw that table with `{kept}`, without \
         `{}`, and put {put} on `{axis}`. Otherwise, {leave}.",
        replaced.join(" * "),
    ))
}

/// A transform that replaces the rows its mark draws with rows of its own: a
/// value statistic, a pair, or a fitted band. The collision modifiers move rows
/// and keep them.
fn replaces_rows(t: &Transform) -> bool {
    crate::transform::is_value_statistic(t)
        || matches!(t, Transform::Range | Transform::Confidence | Transform::Deviation
            | Transform::SmoothBand)
}

/// The refusal for a name a transform writes on rows another mark draws: a node's
/// `name` or `degree` asked of an `edge`, which draws the links, or a slot's `name`
/// asked of a flow's `ribbon`, which draws the bands. Spelled correctly and present
/// in the plot, so "not in the data" would send the reader after a typo.
pub(super) fn written_for_other_rows(layer: &Layer, c: &str, field: &str) -> Option<String> {
    use crate::transform::{NODE_DEGREE, NODE_NAME};
    let ts = &layer.transforms;
    if ts.contains(&Transform::Layout) && layer.mark == Mark::Edge
        && (field == NODE_NAME || field == NODE_DEGREE)
    {
        return Some(format!(
            "gog: `{c}({field})` names the `{field}` that `layout` writes for each node, and \
             an `edge` draws the links between the nodes, which have no `{field}`. Map it on \
             a layer that draws the nodes: `point * layout(…)` or `text * layout(…)`."
        ));
    }
    if ts.contains(&Transform::Flow) && layer.mark == Mark::Ribbon && field == NODE_NAME {
        return Some(format!(
            "gog: `{c}({field})` names the `{field}` that `flow` writes for each slot, and a \
             `ribbon` draws the bands, which cross every stage and so have no one name. To \
             split the bands, map a stage instead: `{c}(<stage>)`."
        ));
    }
    None
}

/// A binding that names a column the transform writes, when the table holds a
/// column of that name too (spec §8, the column rules' fourth: a channel reads the
/// rows the mark draws). Returns whether it was one, so the caller skips the
/// refusal the measure axis would otherwise give a name the table holds.
///
/// **The name means the transform's column**, because the transform replaced the
/// rows the mark draws: after `bar * count` there is one row per slot, and the
/// table's `count`, like any column the transform did not group by, is not on them.
/// `count` and `name` are ordinary names in a table, so refusing every clash would
/// refuse ordinary tables. But a table's column of that name was most likely meant,
/// so the sentence draws with a note that says which column it read and how to
/// reach the other one: §12's Assumption, a choice said out loud.
///
/// **The one refusal is a real collision.** When another channel reads the table's
/// column of the same name (`x(count)` under `bar * count + y(count)`), that column
/// is carried into the rows the mark draws, beside the written one, and one name
/// would mean two columns there.
pub(super) fn check_written_name_clash(
    out: &mut Vec<Diagnostic>, spec: &PlotSpec, layer: &Layer, df: Option<&DataFrame>,
    bound: &[(Channel, &str)], channel: &Channel, field: &str,
) -> bool {
    let Some(d) = df else { return false };
    if actual_type(d, field).is_none() || !names_a_written_column(spec, layer, df, channel, field) {
        return false;
    }
    let c = channel_name(channel);
    let m = mark_name(&layer.mark);
    let chain = std::iter::once(m)
        .chain(layer.transforms.iter().map(transform_name))
        .collect::<Vec<_>>().join(" * ");
    // Channels the reading fills by itself, where leaving the name out loses nothing.
    let fills_itself = *channel == synth_axis(spec, layer, df)
        || (*channel == Channel::Color && field_measure(layer) == Some(field))
        || (*channel == Channel::Label && layer.mark == Mark::Text);
    let reader = bound.iter().find(|(ch, f)| {
        ch != channel && *f == field && !names_a_written_column(spec, layer, df, ch, f)
    });
    if let Some((other, _)) = reader {
        let o = channel_name(other);
        let leave_out = if fills_itself {
            format!(", or leave `{c}()` out: `{chain}` fills `{c}` by itself")
        } else {
            String::new()
        };
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `{o}({field})` reads your table's `{field}` column, and `{c}({field})` \
                 names the `{field}` that `{chain}` writes, so one name would mean two \
                 columns. Rename your table's column{leave_out}."
            ),
        });
        return true;
    }
    // **One note for the name, however many channels say it.** `y(count) +
    // size(count)` is one choice made once, and two notes nearly word for word
    // alike read as two problems. Every channel's note names them all, so the
    // notes are the same words and `check` keeps one.
    let namers: Vec<&Channel> = bound.iter()
        .filter(|(ch, f)| *f == field && names_a_written_column(spec, layer, df, ch, f))
        .map(|(ch, _)| ch)
        .collect();
    let named = namers.iter().map(|ch| format!("`{}({field})`", channel_name(ch)))
        .collect::<Vec<_>>();
    let (named, verb) = match named.as_slice() {
        [one] => (one.clone(), "names"),
        [rest @ .., last] => (format!("{} and {last}", rest.join(", ")), "name"),
        [] => (format!("`{c}({field})`"), "names"),
    };
    // A tally named over a table of tallies is the likeliest mistake of all: counts
    // already computed, drawn by counting rows. The direction adds them up instead.
    let synth = synth_axis(spec, layer, df);
    let tallies = namers.contains(&&synth)
        && layer.transforms.iter().any(|t| matches!(t, Transform::Bin | Transform::Count));
    // Not "rename it": a renamed column is still not on the rows the transform
    // made, and `rows_lack_column` refuses it there.
    let instead = if tallies {
        let bin = if layer.transforms.contains(&Transform::Bin) { "bin * " } else { "" };
        let s = channel_name(&synth);
        format!(" To add up your table's `{field}` instead, write `{m} * {bin}sum + {s}({field})`.")
    } else {
        format!(" To show your table's `{field}`, summarize it into a table of its own, \
                 one row for each place `{chain}` draws, and draw that table.")
    };
    out.push(Diagnostic {
        kind: DiagnosticKind::Assumption,
        message: format!(
            "gog: {named} {verb} the `{field}` that `{chain}` writes, not your table's \
             `{field}` column, which the rows it draws do not have.{instead}"
        ),
    });
    true
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
