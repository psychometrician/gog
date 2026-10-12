//! The measure a transform invents: which axis it takes, and what that axis may
//! be named.

use super::*;

/// Transforms that invent the measured column, so the user need not bind it.
///
/// Named for the *role*, not for `y`: on a horizontal slot mark the axis being
/// invented is `x` (`interval * bounds(lo, hi) + y(term)` is the forest plot),
/// and the caller picks the channel via `synth_axis`.
///
/// `bounds` belongs here: its two named columns *are* the extents, so — like
/// `count` tallying rows — the user binds no measure. It differs from
/// `range`/`confidence`, which reduce an existing measure the user must name and
/// so are absent.
///
/// `pub` because the renderer asks the same question when deciding whether an
/// unbound axis is a mistake worth warning about. Two copies of this list drifted
/// once already: the renderer's omitted `bounds` and told a correctly-drawn forest
/// plot it was "rendering empty chart".
///
/// **It takes the mark**, because a transform only invents a measure where the mark
/// left an axis free for one. Read on a `path` or a `zone` — the marks with no
/// measure axis — the same `bin`/`density` cuts *both* positions and invents
/// neither, so both must be bound and both must be checked against the data. Ask
/// this without the mark and `path * density + x(a) + y(typo)` skips its column
/// check and draws nothing, which is the silent drop §12 forbids.
/// **A composed `proportion` synthesizes nothing**, and getting that wrong is how
/// `bar * sum * proportion + x(continent) + y(pop)` — with no `pop` column in the
/// table — drew an **empty panel on fabricated 0..1 axes** instead of refusing, the
/// exact §12 silent drop this function's own note describes. A normalizer invents an
/// output column only when it had to make the tally itself; standing after a
/// statistic it rescales the column the *user* named, so that name is an **input**
/// and has to be in the data. `bar * sum + y(pop)` refused correctly throughout,
/// which is what made the pair diagnostic: adding a word that measures nothing
/// cannot turn a misspelling into a legal name.
/// **A composed `bin` synthesizes nothing either**, for the same reason, and
/// `transform::cell_measure` already read it that way. Cut and handed to a
/// statistic, a `bin` supplies only the intervals, and the measure is the column the
/// statistic reads. Counted here as a tally, `bar * bin * mean + x(life) +
/// y(lfe)` skipped its column check and drew an empty panel on 0..1 axes, and
/// `bar * bin * mean + x(life)`, with no `y` at all, drew the same panel titled
/// "Count", both in silence.
pub fn synthesizes_measure(mark: &Mark, transforms: &[Transform]) -> bool {
    if measures_cells(mark, transforms) {
        return false;
    }
    transforms.iter().any(|t| {
        match t {
            Transform::Proportion => !crate::transform::measures_beside_share(transforms),
            Transform::Bin => !crate::transform::measures_a_column(transforms),
            Transform::Count | Transform::Density | Transform::Bounds => true,
            // A partition writes the **ring** to the measure axis, which is a
            // synthesized column exactly as a tally is: `y(depth, …)` names an
            // output. It reaches this list rather than being special-cased because
            // every consumer wants the same answer — the axis names itself, the
            // binding is well-formed with nothing in the input to check it against,
            // and Law 7's requirement stands down.
            Transform::Partition => true,
            // A flow writes the stacked interval to the measure axis the same
            // way: bound, `y` names the weight; unbound, every path weighs 1.
            Transform::Flow => true,
            // A cluster tree writes the merge distance to the position its
            // leaves do not hold — always synthesized, never bindable, so the
            // axis names itself. On `zone` the measurement is `color`'s and
            // the cluster supplies only the order, so no axis is invented.
            Transform::Cluster => matches!(mark, Mark::Path),
            _ => false,
        }
    })
}

/// **A tally or an estimate draws its own number, so a column named where it draws
/// is read by nothing** (spec §12).
///
/// `bin`, `count`, `density` and a `proportion` that tallies make the number they
/// draw and write it to the axis the mark measures along. A name there is the
/// *output's* name, which is why `y(count)` is well-formed: no column is read, and
/// the axis takes the title. Any other name the table does not hold is refused by
/// [`synth_axis_takes_its_measure_name`] (ruled 2026-10-09; until then it titled
/// the axis, so `y(frequency)` was well-formed too). A name the table **holds** is
/// another thing. It asks for a column the transform never reads, and until
/// 2026-09-23 it was taken as a title and nothing more: `bar * count +
/// x(continent) + y(life)` drew the tallies on an axis titled "Life", and
/// `point * bin * stack + x(life) + y(continent)` a dot plot of counts on an axis
/// titled "Continent", without a word.
///
/// Refused toward the reading that uses the column (a statistic that reads a
/// number, a split for a category) and toward `<axis>_label()` for a title. Asked
/// only when no transform in the chain reads a column: beside one
/// (`bar * bin * mean + y(life)`) the name on the measure axis is that statistic's
/// input. `bounds`, `partition`, `flow` and `cluster` also write to the measure
/// axis, and a name there can be a real input, a flow's weight, so they are not
/// asked either.
pub(super) fn synth_axis_names_a_column(
    spec: &PlotSpec, layer: &Layer, df: &DataFrame, channel: &Channel, field: &str,
) -> Option<String> {
    let ts = &layer.transforms;
    // **`bounds` draws the axis from its own two columns**, so a category named on
    // it is never read: `ribbon * bounds(zero, height) + x(at) + y(group)` drew one
    // comb through every group, under a `y` axis titled with the category over the
    // band's numbers. Asked first, because `bounds` reads its two columns and the
    // arm below would take it for a statistic reading this one.
    if ts.contains(&Transform::Bounds) && actual_type(df, field) == Some(VarType::Discrete) {
        let m = mark_name(&layer.mark);
        let c = channel_name(channel);
        return Some(format!(
            "gog: `{m} * bounds` draws its band between the two columns `bounds` names, on \
             `{c}`, so `{c}({field})` is never read, and `{field}` holds categories: every \
             group would be drawn as one band. For a band for each value of `{field}`, \
             split them with `group({field})` or `color({field})`, or give each its own \
             panel with `/ facet({field})`. For one ridge per category, \
             `area * density + x(<measure>) + y({field})` estimates each."
        ));
    }
    // A statistic that reads a column makes the name on the measure axis its input:
    // the five reductions and the pairs by their jobs, and `smooth`, which fits that
    // column against the other position.
    if ts.iter().any(|t| reads_a_column(t) || matches!(t, Transform::Smooth | Transform::SmoothBand)) {
        return None;
    }
    // A chain that contradicts itself is refused by `check_chain_jobs`, and which
    // transform the reader keeps decides whether this question arises at all.
    let ctx = crate::transform::JobContext {
        measures_by_color: has_no_measure_axis(&layer.mark),
        stack_shares: layer.stack.as_ref().is_some_and(|s| s.share.unwrap_or(false)),
    };
    if crate::transform::job_conflict(ts, ctx).is_some() {
        return None;
    }
    let maker = ts.iter().find(|t| matches!(
        t, Transform::Bin | Transform::Count | Transform::Density | Transform::Proportion))?;
    // **A `bin` with a category on its key axis is refused by the chain check**,
    // which says `bin` cannot cut the category and to keep this column and color
    // by the category. Said here too, the column was "never read" and sent to
    // `* bin * mean`, which `bin` refuses for the same category: two refusals
    // pulling opposite ways on one sentence.
    let key = match channel {
        Channel::X => spec.position_for(layer, &Channel::Y),
        Channel::Y => spec.position_for(layer, &Channel::X),
        _ => None,
    };
    if *maker == Transform::Bin
        && key.is_some_and(|k| actual_type(df, &k.field) == Some(VarType::Discrete))
    {
        return None;
    }
    let m = mark_name(&layer.mark);
    let c = channel_name(channel);
    let chain = std::iter::once(m).chain(ts.iter().map(transform_name))
        .collect::<Vec<_>>().join(" * ");
    if actual_type(df, field).is_none() {
        return synth_axis_takes_its_measure_name(spec, layer, df, channel, field, maker,
            ctx.stack_shares, &chain);
    }
    let kind = actual_type(df, field)?;
    let piles = layer.mark == Mark::Point && ts.contains(&Transform::Stack);
    // What the chain draws on the axis, in words about the transform that makes it.
    let does = match maker {
        _ if piles => "piles one dot for each row it counts".to_string(),
        Transform::Bin => "cuts its other axis into bins and counts the rows in each".to_string(),
        Transform::Density => "estimates how the rows are spread".to_string(),
        Transform::Proportion => "counts the rows in each slot as a share of all of them".to_string(),
        _ => "counts the rows in each slot".to_string(),
    };
    // The other flat position, for a direction that names both columns.
    let key = match channel {
        Channel::X => spec.position_for(layer, &Channel::Y),
        Channel::Y => spec.position_for(layer, &Channel::X),
        _ => None,
    }.map(|d| d.field.as_str());
    // **Two categories are a floor**, and a `bar` counted over one stands up in the
    // cube, which keeps both columns where they were written. The one slot mark a
    // tally gives a length to: a `box` takes no transform and an `interval` needs a
    // range, so for them `space()` reaches another refusal, and a `point` does not
    // stand a count on `z`.
    let floor = layer.mark == Mark::Bar
        && matches!(maker, Transform::Count | Transform::Proportion)
        && key.is_some_and(|k| actual_type(df, k) == Some(VarType::Discrete));
    let direction = match (kind, maker) {
        (VarType::Discrete, _) if floor => format!(
            "To split the rows by `{field}`, `color({field})` colors each group, \
             `/ facet({field})` draws one panel per category, and `+ space()` stands one \
             bar on each pair of categories in the cube."
        ),
        (VarType::Discrete, _) => format!(
            "To split the rows by `{field}`, `color({field})` colors each group, and \
             `/ facet({field})` draws one panel per category."
        ),
        (_, Transform::Bin) => format!(
            "To summarize `{field}` inside each bin instead of counting rows, \
             `{m} * bin * mean` averages it."
        ),
        (_, Transform::Proportion) => format!(
            "For each slot's share of the total `{field}`, `{m} * sum * proportion` adds \
             it up first."
        ),
        (_, Transform::Density) => match key {
            Some(k) => format!(
                "For how `{k}` and `{field}` spread together, the contour draws both: \
                 `path * density + x({k}) + y({field})`."
            ),
            // With no other position, dropping this one leaves the layer with none,
            // which is the next refusal; the column wants the axis density reads.
            None => {
                let other = if *channel == Channel::X { "y" } else { "x" };
                format!(
                    "To estimate how `{field}` is spread, write it on `{other}` instead: \
                     `{m} * density + {other}({field})`."
                )
            }
        },
        _ => format!(
            "To summarize `{field}` in each slot instead of counting rows, name a \
             statistic that reads it, such as `{m} * mean`."
        ),
    };
    Some(format!(
        "gog: `{chain}` {does}, and draws that number on `{c}`, so `{c}({field})` names a \
         column it never reads: the axis would be titled `{field}` over numbers that are \
         not `{field}`. {direction} To title the axis, write `{c}_label()`."
    ))
}

/// **A transform that makes its own measure axis names it too**, so on that axis a
/// position takes the measure's own name or none (spec §5, ruled 2026-10-09).
///
/// `bin`, `count`, `density` and a tallying `proportion` make the number they draw
/// and call it `count`, `density` or `proportion`. Until 2026-10-09 any other name
/// the table did not hold was taken as a title: `y(countries)` drew the counts under
/// "Countries". That made a bare name inside a position, a column everywhere else,
/// a title here, and it hid the mistakes a column check exists to catch:
/// `y(cuont)` drew an axis titled "Cuont", `bar * bin + x(life) + y(density)` drew
/// counts under "Density", and `line * density + x(life) + y(count)` a density
/// under "Count", all in silence. It was also a second way to say what
/// `y_label()` says. The two-dimensional readings already worked this way
/// (`zone * bin` takes `color(count)` and nothing else), so the rule is now one rule
/// for every reading that makes its measure.
///
/// Refused, never rescaled: `y(density)` on a histogram does not turn counts into a
/// density, because rescaling is a transform's job (`proportion`'s), not a channel
/// argument's. The measure is read off `transform::cell_measure`, the name a tile
/// plot's legend takes; a stack that fills each slot to one keeps the tally's name,
/// since it rescales that column in place, and its axis reads "Share" whatever the
/// name. Each refusal says what the name would have done, names
/// the reading the reader most likely meant, and gives the same two ways out:
/// the measure's name, which still carries the axis's settings, or no `y()`, with
/// `y_label()` for the label. A `y()` with settings and no name is refused before
/// this is asked (`no_column_message`), so no message here offers it.
#[allow(clippy::too_many_arguments)]
pub(super) fn synth_axis_takes_its_measure_name(
    spec: &PlotSpec, layer: &Layer, df: &DataFrame, channel: &Channel, field: &str,
    maker: &Transform, stack_shares: bool, chain: &str,
) -> Option<String> {
    // A stack that fills each pile to one rescales the tally's own column in place,
    // so the column keeps the tally's name; the renderer labels its axis "Share"
    // whatever `y()` names, which is why no message below promises a label there.
    let tally = crate::transform::cell_measure(&layer.transforms)?;
    // A running total over the tally is drawn from the column `cumulative` writes,
    // so that is the name the axis takes (spec §5).
    let accumulates = crate::transform::accumulates(&layer.transforms);
    let measure = if accumulates { crate::transform::CELL_CUMULATIVE } else { tally };
    if field == measure {
        return None;
    }
    let what = match measure {
        crate::transform::CELL_CUMULATIVE => "running totals",
        _ if stack_shares => "shares",
        crate::transform::CELL_SHARE => "shares",
        crate::transform::FIELD_DENSITY => "density estimates",
        _ => "counts",
    };
    let m = mark_name(&layer.mark);
    let c = channel_name(channel);
    // The other flat position, which a direction toward another reading keeps.
    let other = match channel {
        Channel::X => spec.position_for(layer, &Channel::Y).map(|d| ("y", d)),
        Channel::Y => spec.position_for(layer, &Channel::X).map(|d| ("x", d)),
        _ => None,
    };
    let numeric_key = other.filter(|(_, d)| actual_type(df, &d.field) == Some(VarType::Continuous));
    let tallies = matches!(maker, Transform::Bin | Transform::Count);
    let transform_word = USER_TRANSFORMS.iter().any(|t| transform_name(t) == field);
    let misspelled = !transform_word && field.chars().count() >= 3
        && crate::color::edit_distance(&field.to_lowercase(), measure) <= 2;
    // What the name would have done, and the reading it most likely asked for.
    let why = if transform_word {
        let instead = match field {
            // A density is estimated along a number, and drawn standing up from it.
            "density" => match numeric_key {
                Some(("x", d)) if *channel == Channel::Y => format!(
                    " For a density curve, `line * density + x({})` estimates one.", d.field),
                _ => String::new(),
            },
            "proportion" if tallies && !stack_shares && !accumulates => format!(
                " For each {}'s share of the rows, write `{chain} * proportion`.",
                if *maker == Transform::Bin { "bin" } else { "slot" }),
            "count" if *maker == Transform::Density => match numeric_key {
                Some((k, d)) => format!(
                    " To count the rows instead, `bar * bin + {k}({})` draws a histogram.",
                    d.field),
                None => String::new(),
            },
            "count" if layer.transforms.contains(&Transform::Proportion) => {
                " To draw the counts rather than shares, drop `proportion`.".to_string()
            }
            "count" if accumulates =>
                " To draw the counts rather than their running total, drop `cumulative`.".to_string(),
            "proportion" if accumulates =>
                " For running shares that end at 1, set `cumulative`'s `share` to true.".to_string(),
            // A statistic's word is usually a reader asking for that statistic. It
            // reads a column, which goes on the measure axis in place of the word.
            "sum" | "mean" | "median" | "max" | "min" if tallies => format!(
                " To draw the {field} of a column in each {}, name the column there: \
                 `{m} * {}{field} + {c}(<column>)`.",
                if *maker == Transform::Bin { "bin" } else { "slot" },
                if *maker == Transform::Bin { "bin * " } else { "" }),
            _ => String::new(),
        };
        if stack_shares {
            format!("`{field}` is the name of a transform, not of that column.{instead}")
        } else {
            format!(
                "`{field}` is the name of a transform, so `{c}({field})` would label the \
                 axis `{field}` over {what}.{instead}"
            )
        }
    } else if misspelled {
        format!("`{field}` is not a column of the table, and it looks like a misspelling \
                 of `{measure}`.")
    } else if stack_shares {
        format!("`{field}` is not a column of the table, and a stack that fills each pile \
                 to one labels its axis \"Share\" whatever `{c}()` names.")
    } else {
        format!("`{field}` is not a column of the table, so `{c}({field})` would change \
                 only the axis label.")
    };
    // A title the reader can keep: the name as words, the first one capitalized. A
    // transform's word or a misspelling would make a wrong title, so neither is
    // offered as one.
    let title = if transform_word || misspelled || stack_shares {
        format!("To choose a different label, write `{c}_label()`.")
    } else {
        let words = field.replace('_', " ");
        let mut chars = words.chars();
        let titled: String = chars.next()
            .map(|f| f.to_uppercase().chain(chars).collect()).unwrap_or_default();
        format!("To choose the label, write `{c}_label(\"{titled}\")`.")
    };
    Some(format!(
        "gog: `{chain}` draws {what} on `{c}`, in a column it names `{measure}`, so \
         `{c}()` takes that name or none. {why} Write `{c}({measure})`, which still takes the \
         axis's settings such as `limits`, or leave `{c}()` out. {title}"
    ))
}

/// **One axis holds one kind of number** (spec §8, the secondary axis). A tally, a
/// share and a density estimate each invent the number they draw, each in its own
/// units: rows, a fraction of the rows, and rows per unit of the axis the estimate
/// spreads along. Two layers that invent different ones on one axis have no scale in
/// common, so one is drawn in the other's units: `bar * bin + line * density +
/// x(life)` drew its curve less than a pixel tall along the foot of the histogram,
/// on an axis titled "Count", and said nothing.
///
/// Refused toward two plots, one below the other, each measuring one thing: the
/// direction §8 gives for a second layer in different units. Two layers that invent
/// the *same* number (two tallies, a line over an area of one estimate) share the
/// axis as before. A layer that reads a named column beside a tally is
/// [`synth_axis_names_a_column`]'s to refuse, and a violin's estimate is drawn as a
/// width, on no axis.
pub(super) fn check_one_measure_per_axis(
    out: &mut Vec<Diagnostic>, spec: &PlotSpec, data: &HashMap<String, DataFrame>,
) {
    // (axis, what the layer draws there, the layer's chain), one per inventing layer.
    let mut readings: Vec<(Channel, &'static str, String)> = Vec::new();
    for layer in &spec.layers {
        if !synthesizes_measure(&layer.mark, &layer.transforms) {
            continue;
        }
        let df = layer.data.as_ref().or(spec.data.as_ref()).and_then(|n| data.get(n));
        if slot_density(spec, layer, df).is_some() {
            continue;
        }
        let Some(measure) = crate::transform::cell_measure(&layer.transforms) else { continue };
        // A stack filled to one turns the tally into shares in place, under its name.
        let stack_shares = layer.stack.as_ref().is_some_and(|s| s.share.unwrap_or(false));
        // A running total of counts is still counted rows, so it shares an axis with
        // the counts it adds up; ended at 1 in each group, it is shares.
        let total_shares = crate::transform::accumulates(&layer.transforms)
            && layer.cumulative.as_ref().is_some_and(|c| c.share.unwrap_or(false));
        let what = match measure {
            _ if stack_shares || total_shares => "shares",
            crate::transform::CELL_SHARE => "shares",
            crate::transform::FIELD_DENSITY => "density estimates",
            _ => "counts",
        };
        let chain = std::iter::once(mark_name(&layer.mark))
            .chain(layer.transforms.iter().map(transform_name))
            .collect::<Vec<_>>().join(" * ");
        readings.push((synth_axis(spec, layer, df), what, chain));
    }
    let mut said: Vec<Channel> = Vec::new();
    for (i, (axis, what, chain)) in readings.iter().enumerate() {
        if said.contains(axis) {
            continue;
        }
        let Some((_, other_what, other_chain)) =
            readings[i + 1..].iter().find(|(a, w, _)| a == axis && w != what)
        else {
            continue;
        };
        said.push(axis.clone());
        let c = channel_name(axis);
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `{chain}` draws {what} on `{c}`, and `{other_chain}` draws \
                 {other_what} there. An axis holds one kind of number, so one layer would \
                 be drawn in the other's units. Give each its own plot, the second below \
                 the first with `/`, so that each `{c}` axis measures one thing."
            ),
        });
    }
}

/// Does this mark have **no measure axis**, so that what a transform measures has
/// no position to be written to and goes to `color` instead?
///
/// **This is the *destination* question, not the dimensionality one**, and the two
/// were one predicate until `bar` stood up in the cube (spec §5/§15). They agreed
/// on every mark that existed: a `zone` and a `path` cut both positions *and* have
/// nowhere to put the answer, a flat `bar` cuts one *and* has `y`. A 3-D `bar` is
/// the case that separates them — it cuts **two** positions and still measures
/// along one, because `space` gave it a third. So the conflated predicate split in
/// two: this one names where the measurement goes, [`cuts_both_positions`] how many
/// axes it was read over, and each caller now asks the one it meant.
///
/// A mark's answer here is a property of the mark alone, in every space: a bar
/// measures by length wherever it stands, and a zone measures by color.
pub fn has_no_measure_axis(mark: &Mark) -> bool {
    matches!(mark, Mark::Zone | Mark::Path)
}

/// Does a distributional transform on this mark, in this space, cut **both** of the
/// axes it is read over rather than one?
///
/// Spec §5's dimensionality rule, as a predicate: *how many dimensions a transform
/// cuts is read off the mark, never asked for*. The rule is a subtraction — the
/// positions the space offers, less the one the mark measures along:
///
/// | | positions | measures along | cuts |
/// |---|---|---|---|
/// | `zone`/`path`, flat | 2 | none | **2** |
/// | `bar`, flat | 2 | `y` | 1 |
/// | `bar`, `space` | 3 | `z` | **2** |
/// | `surface`, `space` | 3 | `z` | **2** |
///
/// So the 3-D histogram is this rule read with the third axis present, not a new
/// one: `bar * bin + x(a) + y(b) + space()` cuts the floor into cells for the same
/// reason `zone * bin` cuts the plane, and the count rises along `z` for the same
/// reason it rises along `y` when the plot is flat.
///
/// Asked in one place so the transform stage, the axis fitting, the legality checks
/// and the mark writers cannot disagree about how many dimensions a layer is in.
///
/// Restricted to the marks that **take an extent on the floor and measure up**,
/// rather than stated for every mark with a measure axis. `point` also leaves an axis
/// free (the dot plot's pile is `point * bin * stack`), so the subtraction would give
/// it two dimensions in space as well — a reading nobody has designed, and claiming it
/// here would be the enumeration §5 refuses. `box`/`interval` inherit the widening for
/// free on the day they learn to stand in the cube, because that is exactly what
/// `mark_draws_in_space` will then say.
///
/// The set is the slot marks **plus `surface`**, and the two halves differ in what
/// they take off the floor rather than in the subtraction: a slot mark stands *in* a
/// cell and reads its edges, a surface spans the whole floor and reads its rows as
/// nodes (spec §15). Both spend the third axis on the measurement, which is the only
/// thing this predicate asks.
pub fn cuts_both_positions(mark: &Mark, space: SpaceKind) -> bool {
    has_no_measure_axis(mark)
        || (space == SpaceKind::Space
            && (is_slot_mark(mark) || *mark == Mark::Surface)
            && mark_draws_in_space(mark, space))
}

/// Is this layer a two-dimensional reading — a measurement made per **cell** rather
/// than along an axis?
///
/// The conjunction of [`has_no_measure_axis`] and a transform that invents its own
/// measurement. [`field_measure`] names what such a layer measures; this says
/// whether there is one.
///
/// **Asked of the destination, not of the dimensionality**, which is what keeps a
/// 3-D bar out of it: `bar * bin + x + y + space()` reads its bin over two axes
/// exactly as the heatmap does, and is still not a per-cell *measurement by color*
/// — it has `z` to stand its tally up along. So it answers `false` here and `true`
/// to [`cuts_both_positions`], and every consumer of this predicate (the `color`
/// exemption, the ramp, the legend, the field checks) correctly leaves it alone.
///
/// Four transforms qualify and they divide by where the cells come from, not by what
/// they measure: `bin` and `density` **cut** a continuous plane into cells, `count`
/// and `proportion` tally into the cells two categorical axes already are. Cutting is
/// the *field* half and slotting is the *tile* half, which is why this is no longer
/// called `reads_a_field` — a confusion matrix is cells all the way down and a
/// density estimate nowhere in it.
pub fn measures_cells(mark: &Mark, transforms: &[Transform]) -> bool {
    has_no_measure_axis(mark) && crate::transform::measures_cells(transforms)
}

/// Is this layer read over **two** positions — the dimensionality question, where
/// [`measures_cells`] just above is the destination one?
///
/// The two are the same predicate on every mark but one, and the exception is what
/// forced them apart: a 3-D `bar` reads its `bin` over two axes (`true` here) and
/// still stands its tally up along `z` rather than painting it (`false` there). So
/// the transform stage asks *this* — it decides whether to run `bin2d` or `bin` —
/// while the `color` exemption, the ramp and the legend ask the other.
///
/// Every mark answers both the same way in the plane, which is why one predicate
/// served until the cube had a mark with a measure axis in it.
///
/// **Either class of statistic gets here**, and that is the two-dimensional group-by
/// (spec §5). The four that invent their own measurement read over two axes because
/// nobody handed them a column; the five that reduce a named one read over two
/// because the mark measures with a channel that is not either of them. Both are the
/// same subtraction — *the positions, less the one the mark measures with* — so both
/// belong to the same predicate, and a layer's dimension stays a fact about the mark
/// rather than about which transform was typed.
pub fn reads_two_dimensions(mark: &Mark, transforms: &[Transform], space: SpaceKind) -> bool {
    cuts_both_positions(mark, space)
        && (crate::transform::measures_cells(transforms)
            || crate::transform::has_reduction(transforms)
            // **A pair is a reduction too** — it reduces to two numbers instead of
            // one, which is a fact about the statistic and not about how many axes
            // it is read over. Left out until 2026-07-26, when `interval` and `box`
            // stood in the cube and needed the floor grouped: without it a whisker
            // fell through to the one-key branch, grouped by `x` alone, and drew one
            // per *row* — 75 whiskers on a 6-cell floor. Same subtraction, same
            // cells, same absent-pair rule as `agg2d`; only the arity of the answer
            // differs (`transform::pairs2d`).
            || crate::transform::pairs_a_column(transforms))
}

/// Which channel a mark makes its **measurement** with, when it is read over both
/// positions — the other half of §5's subtraction, and the whole of the
/// two-dimensional group-by.
///
/// The dimensionality rule is *the positions the space offers, less the one the mark
/// measures along*. Read left to right it says how many axes a distributional
/// transform cuts; read right to left it names the leftover — the channel that
/// carries the measurement, which is exactly the channel a **value statistic** takes
/// its column from. A value statistic has always named its column with the channel it
/// writes back to, because it reduces *in place*: on a flat `bar * mean + x(continent)
/// + y(life)`, `y` both names `life` and receives the mean.
///
/// | mark | positions | measures with | groups by | reduces |
/// |---|---|---|---|---|
/// | `bar`, flat | x, y | `y` | x | y |
/// | `bar`, `space` | x, y, z | `z` | **x, y** | z |
/// | `zone` | x, y | `color` | **x, y** | color |
///
/// So the claim these five were refused under — *both positions are spoken for, so
/// there is no channel left to say which column to summarize* — was false on its own
/// terms. A zone measures by color; color is the channel. Nothing was missing but
/// reading the subtraction's remainder as a name.
///
/// `None` for a mark that is not read over both positions, where the answer is
/// `slot_orient`'s and depends on the bindings rather than on the mark.
pub fn measure_channel(mark: &Mark, space: SpaceKind) -> Option<Channel> {
    if !cuts_both_positions(mark, space) {
        return None;
    }
    // A mark with no measure *axis* measures by color, in every space — which is
    // what `has_no_measure_axis` has always said, now read as a name rather than as
    // an absence. Anything else that cuts both positions did so by gaining a third
    // axis, and in the cube `z` measures, always (spec §15).
    Some(if has_no_measure_axis(mark) { Channel::Color } else { Channel::Z })
}

/// The **column** a two-dimensional value statistic reduces — [`measure_channel`]'s
/// binding, resolved on this layer.
///
/// Positions go through `position_for` (a layer may name its own column for a shared
/// axis, spec §8) and everything else through the layer's own encodings, which is
/// `binding_of`'s split and is asked here so the transform stage and the checks read
/// one answer.
pub fn measure_field<'a>(spec: &'a PlotSpec, layer: &'a Layer) -> Option<&'a str> {
    let ch = measure_channel(&layer.mark, space_of(spec))?;
    binding_of(spec, layer, &ch).map(|d| d.field.as_str())
}

/// Is this layer's `color` carrying the layer's **own measurement**, rather than a
/// column the reader bound for its own sake?
///
/// The question anything reading a color *domain* has to ask, because the answer
/// decides which frame holds the numbers: a measurement exists only downstream of the
/// transform that made it, and the raw table either lacks the column or — the case
/// that hid this — holds the *unreduced* values under the same name.
///
/// Both halves of §5's division say yes. The four that invent a measurement publish it
/// under a synthesized name (`count`, `density`, `proportion`, `level`), so the raw
/// table has no such column at all; the five that reduce a named one rewrite the
/// reader's own column, so the raw table has it with every original row still in it.
/// Stating the rule as "is the column synthesized" caught only the first, and the
/// second then drew a heatmap of cell means beside a key that spanned the raw
/// column's range — self-consistent fills under a legend that decoded them wrongly,
/// which is §12's silent wrongness rather than a cosmetic slip.
/// **Asked of the column, not of the channel.** Stating it as *does this mark measure
/// by color* is true for a `zone` and false for a `surface`, which measures up along
/// `z` — so `zone * density + color(density)` drew and `surface * density +
/// color(density)` was refused as a misspelled column, one transform behaving two
/// ways depending on the mark it fed. That is the per-mark special case Law 2 forbids
/// and the silent letter Law 6 forbids, and the same wrong question was answering for
/// the legend: past the refusal the sheet ramped correctly with no key beside it,
/// because the domain was looked for in the raw table where the estimate does not
/// exist. One predicate, so both are right together.
pub fn color_is_the_measurement(spec: &PlotSpec, layer: &Layer) -> bool {
    if !reads_two_dimensions(&layer.mark, &layer.transforms, space_of(spec)) {
        return false;
    }
    // The mark measures by color, so whatever `color` names here is that measurement
    // — the flat readings, where `check_field` has already refused every other field.
    if measure_channel(&layer.mark, space_of(spec)) == Some(Channel::Color) {
        return true;
    }
    // Or the mark measures up along `z` and the reader named that same measurement on
    // `color` as well — the height said twice, which is a `surface`'s to accept
    // because a face is small enough to hold one value (§15). Named, so a reader who
    // binds some *other* column keeps the ordinary check.
    binding_of(spec, layer, &Channel::Color).map(|d| d.field.as_str())
        == measurement_column(spec, layer)
}

/// The column this layer's own measurement rides in, whatever channel carries it.
///
/// [`field_measure`] answers this for the flat readings, where a mark with no measure
/// axis measures by color and the *geometry* decides the name — rings by the level
/// they were cut at, cells by what was tallied or estimated in each. In the cube the
/// same measurement stands up along `z` instead, under the name the transform
/// synthesized, which is the column `render::svg` fits the third axis to. Both are the
/// layer's own measurement; only the channel carrying it differs.
///
/// `None` for the five that **reduce a column the reader named**: they rewrite it in
/// place, so the measurement is in the user's own column and needs no exemption
/// anywhere.
pub fn measurement_column(spec: &PlotSpec, layer: &Layer) -> Option<&'static str> {
    field_measure(layer).or_else(|| {
        reads_two_dimensions(&layer.mark, &layer.transforms, space_of(spec))
            .then(|| crate::transform::cell_measure(&layer.transforms))
            .flatten()
    })
}
