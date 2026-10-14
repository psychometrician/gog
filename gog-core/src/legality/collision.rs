//! The collision modifiers (`dodge`, `stack`, `jitter`, the swarm, `repel`) and
//! what each needs of a layer.

use super::*;

/// Does this engine draw this mark at all?
///
/// The per-channel `renders: None` mechanism cannot answer this. It reports a
/// *binding* that would have no effect, so it only ever fires for a channel the
/// layer actually carries — and `x`/`y` are plot-scoped, so a layer with no
/// encodings of its own produced no diagnostics whatsoever. `area` therefore
/// rendered an empty panel and exited 0, which is precisely the silent drop
/// this crate exists to refuse. The question "can this mark be drawn?" belongs
/// to the mark, so it is asked once, here.
///
/// Split areas overlap, and the engine chose that on the caller's behalf.
///
/// A split `line` draws polylines, which cross without hiding one another; a
/// split `area` draws opaque regions, and the last one drawn can bury the rest
/// completely. That is a *default* — overlay rather than stack — and §12 says a
/// default only stays silent when one sensible value exists. Two do: `stack`
/// (now built — `area * stack` piles the regions into abutting bands) and a set
/// opacity you can see through.
///
/// Silent when the caller has already answered the question — a `* stack` that
/// resolves the overlap outright, or a `style(opacity = )` they can see through.
/// An Assumption, never fatal: the picture is
/// legitimate, and Law 8 forbids blocking the ugly-but-legal.
pub(super) fn check_area_overlap(
    out: &mut Vec<Diagnostic>,
    spec: &PlotSpec,
    df: &DataFrame,
    layer: &Layer,
) {
    if layer.mark != Mark::Area
        || layer.style.opacity.is_some()
        || layer.transforms.contains(&Transform::Stack)
    {
        return;
    }
    // **A split violin does not overlap**, so the warning that a split `area` hides
    // itself is false there and was firing on every colored ridgeline. The regions
    // an ordinary split area draws share one domain and stack up on the page; a
    // violin's each stand in their own category's slot, which is what a slot is for.
    // (Two violins genuinely *can* share a slot — when `color` splits *within* a
    // category — and the renderer already answers that the way `box` does, by
    // drawing them translucent, so there is nothing for this warning to add.)
    if slot_density(spec, layer, Some(df)).is_some() {
        return;
    }
    let Some(def) = [Channel::Color, Channel::Group]
        .iter()
        .find_map(|c| binding_of(spec, layer, c))
    else {
        return;
    };
    // **Regions that are drawn**, which a group of one row is not: an area runs
    // through two positions at least. Counted over the table's own rows, so a
    // transform that makes more of them can only add regions this leaves out,
    // never claim one that is not there. One row per group used to be told it
    // "draws 3 regions on top of one another" over an empty panel.
    let n = match df.str_col(&def.field) {
        Some(col) => {
            let mut rows: HashMap<&str, usize> = HashMap::new();
            for v in col {
                *rows.entry(v.as_str()).or_insert(0) += 1;
            }
            rows.values().filter(|&&k| k >= 2).count()
        }
        None => crate::data::categories_across(&[df], &def.field).len(),
    };
    if n < 2 {
        return;
    }
    out.push(Diagnostic {
        kind: DiagnosticKind::Assumption,
        message: format!(
            "gog: `area` split by `{}` draws {n} regions on top of one another, and \
             the last one drawn can hide the rest. Add `style(opacity = 0.5)` to see \
             through them, or use `line` to compare shapes without filling.",
            def.field,
        ),
    });
}

/// `dodge` is a **collision modifier** (spec §5): it sets side by side what would
/// otherwise land on one shared position. Two things make it well-formed, each
/// refused with direction otherwise:
///
/// 1. **Something to set apart across a slot.** The three offsets divide the mark
///    set by geometry — `dodge` tiles a slot (bar, box, interval, point), `stack`
///    accumulates along the measure axis (bar, area, and `point` as a pile of
///    dots), `jitter` spreads a point along a category by a seeded amount. So
///    `dodge` is legal on the slot marks, on `point`, and on `text`, whose label
///    moves to the tile its group's bar stands in (2026-10-10, so dodged bars can
///    be labeled); elsewhere the refusal names the offset that fits (line/area →
///    stack). This is Law 1 read correctly — a modifier combines with the marks
///    whose geometry it was defined for.
/// 2. **A split to separate — for the width marks and `text`.** With nothing that
///    splits a bar, box, whisker or label (any channel in `split_fields_of`:
///    `color`, `group`, `pattern`, `shape`) there is one mark per slot and nothing
///    to set beside anything, so `dodge` is refused toward adding the split rather
///    than accepted as a silent no-op (spec §12). A `point` needs none: its own
///    rows collide in the slot, and the swarm is what sets them apart. A split on a
///    point tiles the slot first, exactly as it tiles a bar's, and each tile swarms
///    on its own.
///
/// Whether the points have a slot to swarm in at all is data-aware, so it is asked
/// by [`check_swarm`] with the axis types in hand.
pub(super) fn check_dodge(out: &mut Vec<Diagnostic>, layer: &Layer) {
    if !layer.transforms.contains(&Transform::Dodge) {
        return;
    }
    let name = mark_name(&layer.mark);
    // 1. Something for dodge to set apart — read off the shared table so the grid
    //    and this refusal name the same mark set.
    if mark_takes_transform(&layer.mark, &Transform::Dodge) == TransformLegality::None {
        // Each direction draws. A `line` shared `area`'s, "offset by accumulating
        // (`stack`)", and `line * stack` is refused in turn, toward `area`.
        let fix = match layer.mark {
            Mark::Area => "A filled area is offset by *accumulating* (`stack`), not by subdividing a width",
            Mark::Line => "A line is a thin stroke with no width to subdivide, so lines that meet \
                           cross rather than cover each other. To pile the groups instead, \
                           `area * stack` draws each as a filled band",
            Mark::Ribbon => "A filled band has no width to subdivide — overlapping bands are told apart by transparency (`style(opacity = )`)",
            _ => "dodge subdivides a mark's width across groups, which this mark does not have",
        };
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `dodge` sets side by side the bars, boxes, whiskers or points that would \
                 land on one position — but `{name}` is not one of those. {fix}."
            ),
        });
        return;
    }
    // Under a whole-picture layout every second transform is refused in that
    // layout's own terms (`chain_message`); asking here for what this one needs
    // would send the reader to add it only to meet that refusal (2026-10-13).
    if layer.transforms.iter().any(is_whole_picture) {
        return;
    }
    // A point's own rows are what collide, so it swarms with no split at all, and
    // whether it has a slot to swarm in is `check_swarm`'s to say.
    if layer.mark == Mark::Point {
        return;
    }
    if offset_on_estimate(out, layer, &Transform::Dodge) {
        return;
    }
    // 2. A split to separate: any channel that splits, the same list the statistics
    //    split by (`split_fields_of`). `pattern` alone counts, since 2026-09-23: it
    //    splits a mark, so its groups are groups to set side by side.
    if split_fields_of(layer).is_empty() {
        // The channels this mark takes that split it, and no other: advising
        // `group` on a `bar`, which refuses it, sent the reader to a second refusal.
        let add = or_list(&splitting_channels(&layer.mark).iter()
            .map(|ch| format!("`{}(<field>)`", channel_name(ch)))
            .collect::<Vec<_>>());
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `{name} * dodge` has no groups to set side by side — nothing splits the \
                 mark. Add {add} so there are groups to dodge."
            ),
        });
    }
}

/// `group` on a mark whose rows are already separate shapes (spec §6).
///
/// On `line`, `area` and the other path marks, `group` says which rows form one
/// shape. A `bar` or a `point` is one shape per row already, so there `group` can
/// only split what something else gathers: a value statistic, which then runs once
/// per group (`bar * mean * dodge + group(era)` is one mean per era), or `dodge`
/// and `stack`, which then arrange the groups. That is what `color` does to the
/// same layer, without the color. With none of them, splitting the rows changes
/// nothing at all, so the binding is refused toward a statistic or toward `color`
/// rather than accepted and dropped (spec §12), the ruling `dodge` with nothing to
/// split already follows. `jitter` and `layout` move a point without reading a
/// split, so they do not count. A `text` is one row's label, as a point is one
/// row's dot, and follows the same rule with `dodge` setting each group's label
/// over its own bar (2026-10-10).
pub(super) fn check_group_gathers(
    out: &mut Vec<Diagnostic>, spec: &PlotSpec, df: Option<&DataFrame>, layer: &Layer,
) {
    if !matches!(layer.mark, Mark::Bar | Mark::Point | Mark::Text) {
        return;
    }
    let Some(def) = layer.encodings.get(&Channel::Group) else { return };
    let arranges: &[Transform] = if layer.mark == Mark::Bar {
        &[Transform::Dodge, Transform::Stack]
    } else {
        &[Transform::Dodge]
    };
    if layer.transforms.iter()
        .any(|t| crate::transform::is_value_statistic(t) || arranges.contains(t))
    {
        return;
    }
    let (name, field) = (mark_name(&layer.mark), def.field.as_str());
    // **`dodge` first** (ruled 2026-10-09): a table with one row for each bar of
    // each group is the commonest way to arrive here, and for it the fix is
    // `dodge`; a statistic is the fix only when a slot holds several rows of one
    // group. A point swarms only across a category's slot, so on a scatterplot
    // neither `dodge` nor a statistic helps (`point * mean` there averages
    // one-row groups), and only `color` and `shape` are offered.
    let slotted = |ch: Channel| df.map_or(true, |d| spec.position_for(layer, &ch)
        .and_then(|c| actual_type(d, &c.field)) == Some(VarType::Discrete));
    let message = match layer.mark {
        Mark::Bar => format!(
            "gog: `group` on a `bar` needs `dodge`, `stack` or a statistic to act on. Each \
             row is already its own bar, so splitting the rows by `{field}` alone would \
             change nothing. Add `dodge` to set the bars of each `{field}` side by side, as \
             in `bar * dodge`, or add a statistic such as `mean` when a slot holds several \
             rows of one `{field}`, as in `bar * mean * dodge`. Or map `{field}` to `color`."
        ),
        Mark::Text if slotted(Channel::X) != slotted(Channel::Y) => format!(
            "gog: `group` on a `text` needs `dodge` or a statistic to act on. Each row is \
             already its own label, so splitting the rows by `{field}` alone would change \
             nothing. Add `dodge` to set the label of each `{field}` over its own bar, as \
             in `text * dodge`, or add a statistic such as `mean` when a slot holds several \
             rows of one `{field}`, as in `text * mean * dodge`. Or map `{field}` to `color`."
        ),
        // A labeled scatter has no bars to stand over and no slot to summarize in.
        Mark::Text => format!(
            "gog: `group` on a `text` needs `dodge` or a statistic to act on, and on this \
             plot neither helps: there is no category axis whose bars the labels could \
             stand over. Each row is already its own label, so splitting the rows by \
             `{field}` would change nothing. To show the groups, map `{field}` to `color`."
        ),
        _ if slotted(Channel::X) != slotted(Channel::Y) => format!(
            "gog: `group` on a `point` needs `dodge` or a statistic to act on. Each row is \
             already its own point, so splitting the rows by `{field}` alone would change \
             nothing. Add `dodge` to set the points of each `{field}` side by side, as in \
             `point * dodge`, or a statistic such as `mean` to draw one point for each \
             `{field}`, as in `point * mean`. Or map `{field}` to `color`."
        ),
        _ => {
            let key = spec.position_for(layer, &Channel::X)
                .map_or("x".to_string(), |c| format!("`{}`", c.field));
            format!(
                "gog: `group` on a `{name}` needs `dodge` or a statistic to act on, and on \
                 this plot neither helps: `dodge` needs a category axis, and a statistic \
                 over {key} would summarize one row at a time. Each row is already its own \
                 point, so splitting the rows by `{field}` would change nothing. To show the \
                 groups, map `{field}` to `color` or `shape`."
            )
        }
    };
    out.push(Diagnostic { kind: DiagnosticKind::Illegal, message });
}

/// `stack` is `dodge`'s sibling (spec §5) — the collision modifier for the marks
/// that *accumulate* rather than sit side by side. It hands every element the span
/// `[base, top]` along the measure axis, and each mark draws that span as its own
/// geometry. Well-formed on two conditions, each refused with direction otherwise:
///
/// 1. **A measure to accumulate along.** The three offsets divide the mark set by
///    geometry — `stack` accumulates (`bar`, `area`, `point`), `dodge` subdivides a
///    width (`bar`, `box`, `interval`), `jitter` spreads a widthless mark along a
///    category (`point`). Elsewhere the refusal names the offset that fits.
/// 2. **Something at one position to pile** — and that is a different sentence for
///    a mark whose element *is* a quantity than for one whose element is a row:
///    - `bar`/`area` measure a height, so what piles is a split (`split_fields_of`).
///      With no split there is one mark per position and nothing to accumulate, so
///      it is refused toward adding the split rather than accepted as a silent
///      no-op (spec §12).
///    - `point` has no height. Its contribution to a measure is that it is *there*,
///      so a pile of points is a **count of rows** and the layer needs the transform
///      that counts them — `bin` on a continuous axis, `count` on a categorical
///      one. It needs no split: the dot plot piles a single series.
pub(super) fn check_stack(out: &mut Vec<Diagnostic>, layer: &Layer) {
    if !layer.transforms.contains(&Transform::Stack) {
        return;
    }
    let name = mark_name(&layer.mark);
    // 1. A measure for stack to accumulate along — read off the shared table.
    if mark_takes_transform(&layer.mark, &Transform::Stack) == TransformLegality::None {
        let fix = match layer.mark {
            Mark::Line | Mark::Step => "An unfilled path has nothing to fill and pile — use `area`, which stacks as filled bands",
            Mark::Box | Mark::Interval => "A box or whisker is set beside its neighbors by subdividing the slot — that is `dodge`, not `stack`",
            Mark::Ribbon => "A ribbon already spans a low to a high, so it measures no height from a baseline to pile — overlapping bands are told apart by transparency (`style(opacity = )`)",
            Mark::Text => "A label has no measured height to accumulate — to move labels off one another, `repel` is the tool",
            _ => "stack piles a mark's measured height across groups, which this mark does not have",
        };
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `stack` piles on top of one another the bars or areas that a `color` split \
                 draws at one position — but `{name}` is not one of those. {fix}."
            ),
        });
        return;
    }
    // Under a whole-picture layout every second transform is refused in that
    // layout's own terms (`chain_message`); asking here for what this one needs
    // would send the reader to add it only to meet that refusal (2026-10-13).
    if layer.transforms.iter().any(is_whole_picture) {
        return;
    }
    // 2a. A point's pile is a count of rows, so it asks for the transform that
    // counts them — not for a split, which it does not need.
    if layer.mark == Mark::Point {
        if !layer.transforms.iter().any(|t| matches!(t, Transform::Bin | Transform::Count)) {
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message:
                    "gog: `point * stack` piles one dot per observation along the measure axis — \
                     the dot plot — so the measure has to be a count of rows, and nothing here \
                     counts them. Add the transform that does: `point * bin * stack` for a \
                     continuous axis (dots per interval), `point * count * stack` for a \
                     categorical one (dots per category)."
                    .to_string(),
            });
        // **A share cannot be piled, because a dot is a whole observation.** The
        // pile's height is `round(top - base)` dots, and every share is below one, so
        // it rounds to none: `point * bin * proportion * stack` drew an **empty
        // panel** with a fabricated 0..1 axis and exited 0. That is the same failure
        // the book's binned-category chunk had — a plot with nothing in it standing
        // where a refusal belonged — and it is not the job rule's to catch, because
        // `proportion` (scale) and `stack` (position) really are different jobs. What
        // collides is narrower: this mark's pile counts *units*, and a fraction of an
        // observation is not one.
        // **A reduction after the cut is not a count either** (2026-10-13, found
        // walking the chains of three and four). The check above asked only whether
        // `bin` was written, and `bin` hands its tally to the reduction that follows
        // it: `point * bin * mean * stack` piled one bin's mean GDP as 31307 dots,
        // the very picture `point * mean * stack` is refused for.
        //
        // **Nor is a running count**: it counts every earlier row again in each later
        // bin, so a pile of it would draw each row many times. `point * bin *
        // cumulative * stack` drew one group's 142 dots and lost the other's.
        } else if let Some(r) = layer.transforms.iter()
            .find(|t| crate::transform::is_reduction(t))
            .filter(|_| layer.transforms.contains(&Transform::Bin))
        {
            let r = transform_name(r);
            // The sentence as written, and as written without the pile.
            let chain = |keep: &dyn Fn(&Transform) -> bool| std::iter::once("point")
                .chain(layer.transforms.iter().filter(|t| keep(t)).map(transform_name))
                .collect::<Vec<_>>().join(" * ");
            let written = chain(&|_| true);
            let unpiled = chain(&|t| *t != Transform::Stack);
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: format!(
                    "gog: `{written}` piles one dot per observation, the dot plot, so the \
                     measure has to be a count of rows, and here `{r}` measures each bin \
                     instead of counting its rows. A pile of that number is not a count of \
                     anything. Keep whichever you meant: `point * bin * stack` to pile the \
                     rows of each bin, or `{unpiled}` for one point at each bin's value."
                ),
            });
        } else if layer.transforms.contains(&Transform::Cumulative) {
            let tally = if layer.transforms.contains(&Transform::Bin) { "bin" } else { "count" };
            let cell = if tally == "bin" { "bin" } else { "category" };
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: format!(
                    "gog: `point * {tally} * cumulative * stack` piles one dot per observation, \
                     the dot plot, and a running count counts every earlier row again in each \
                     later {cell}, so the piles would draw each row many times. Keep whichever \
                     you meant: `point * {tally} * stack` to pile the rows of each {cell} once, \
                     or `point * {tally} * cumulative` for one point at each running count."
                ),
            });
        } else if layer.transforms.contains(&Transform::Proportion) {
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message:
                    "gog: `point * proportion * stack` has no dots to pile — a dot plot stacks \
                     one dot per observation, and `proportion` turns the count into a fraction, \
                     which is less than one whole dot. Keep whichever you meant: \
                     `point * count * stack` to pile the observations themselves, or \
                     `bar * count * proportion` to read the same shape as shares."
                    .to_string(),
            });
        }
        return;
    }
    // An estimate gives a pile no shared position. A point is not asked: its pile
    // is a count of rows, and its own refusal above says what counts them.
    if offset_on_estimate(out, layer, &Transform::Stack) {
        return;
    }
    // 2b. A split to separate: any channel that splits (`split_fields_of`).
    if split_fields_of(layer).is_empty() {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `{name} * stack` has no groups to pile up — nothing splits the mark. Add \
                 `color(<field>)` (or `group(<field>)`) so there are groups to stack."
            ),
        });
    }
}

/// Where a pile hangs — `stack(baseline = )`, checked for a name that exists and for a
/// space with an origin to spend (spec §5).
///
/// **Displacing a pile costs the measure axis's origin**, and that is only ever free in
/// the plane. In `polar` the measure axis is an angle or a radius, and neither origin
/// is a choice: a radius of zero is the center of the circle, and moving a pile off it
/// asks for a negative radius, which is not a place. In `space` the pile stands on the
/// floor, and in `nest` the space has already fixed what each position means before any
/// layer is read. So a displaced baseline is refused outside `flat` — the same shape of
/// ruling as the donut (§18), where a want that is meaningless in five spaces out of
/// six turned out to belong to the scale rather than to the space.
///
/// `"zero"` is legal everywhere, being what every pile has always done.
pub(super) fn check_baseline(out: &mut Vec<Diagnostic>, spec: &PlotSpec, layer: &Layer) {
    let Some(b) = layer.stack.as_ref().and_then(|s| s.baseline.as_deref()) else { return };
    if !BASELINES.contains(&b) {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `{b}` is not a baseline. `stack(baseline = )` takes {}. \
                 `\"zero\"` stands every pile on the axis, which is the plain stacked \
                 bar; `\"center\"` hangs each pile so its middle is at zero; \
                 `\"wiggle\"` chooses the foot that makes the bands as flat as it can, \
                 which is the streamgraph.",
                BASELINES.iter().map(|t| format!("`\"{t}\"`")).collect::<Vec<_>>().join(", "),
            ),
        });
        return;
    }
    if b == "zero" {
        return;
    }
    let space = space_of(spec);
    if space != SpaceKind::Flat {
        let s = space_name(space);
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `stack(baseline = \"{b}\")` moves every pile off the origin of the \
                 measure axis, and `{s}()` has no origin to spare. A displaced pile is a \
                 statement about a plane: in `{s}()` the measure is an angle or a radius \
                 or a height the space has already fixed, so moving the foot asks for a \
                 place that is not on the plot. Keep the space and drop the baseline \
                 (`stack` alone, or with its `share` set to true for composition), or keep the \
                 baseline and draw it flat."
            ),
        });
    }
}

/// **A summary over groups of one row summarizes nothing**, and the rows are the only
/// place that can say so: the data-dependent half of §12's standing rule, a proven
/// redundancy warns and draws.
///
/// A statistic groups the rows by the exact value of its key column, within each split,
/// facet panel and frame, which is the partition the renderer makes. A key of numbers
/// whose values never repeat gives one group per row, and the summary of one number is
/// that number: `bar * mean + x(gdp) + y(life)` on the book's 142 countries drew 142
/// bars, each a country's own `life`, under an axis titled `Life`, and nothing on the
/// page said the means were the rows. Legal on the sentence, so no refusal can see it,
/// and drawn, because the same sentence over rows whose values repeat does summarize.
/// Silent as soon as any group holds two rows, since then the statistic does work.
///
/// **Numbers only, and not dates.** A category with one row each is a table that was
/// summarized already, and it draws exactly; a daily series with one row a day is
/// grouped by the day it means. What misleads is a measured column read as groups.
pub(super) fn check_one_row_groups(
    out: &mut Vec<Diagnostic>, spec: &PlotSpec, df: &DataFrame, layer: &Layer, horizontal: bool,
) {
    use crate::transform::is_reduction;
    // The statistics that group by the key's exact values. The rest cut or fit along
    // the axis (`bin`, `density`, `smooth`), reshape (`bounds`), or place their own
    // marks (the four layouts), and none of those has a group to be one row.
    let groups = |t: &Transform| is_reduction(t) || matches!(t,
        Transform::Count | Transform::Proportion | Transform::Range
            | Transform::Confidence | Transform::Deviation | Transform::Box);
    // A running total reads along the axis too: each group adds its rows to the
    // ones before it, so a group of one row is a step of one row, and over a number
    // that is the exact empirical CDF (`step * count * cumulative(share = true)`).
    let along = |t: &Transform| matches!(t,
        Transform::Bin | Transform::Density | Transform::Smooth | Transform::SmoothBand
            | Transform::Bounds | Transform::Partition | Transform::Flow | Transform::Layout
            | Transform::Cluster | Transform::Cumulative);
    let Some(stat) = layer.transforms.iter().find(|t| groups(t)) else { return };
    if layer.transforms.iter().any(along)
        || reads_two_dimensions(&layer.mark, &layer.transforms, space_of(spec))
    {
        return;
    }

    // The key, read off the bindings as the renderer reads it (`key_is_x`): a layer
    // that reads its orientation (a slot mark, a summarizing point or text) groups
    // by `y` on its side, and every other layer by `x`. Asked of the slot marks
    // alone, a sideways `text * mean` beside its bars was told it grouped by the
    // numeric `x` while the renderer grouped it by `y`.
    let key_ch = if horizontal && reads_orientation(layer) { Channel::Y } else { Channel::X };
    let Some(key) = spec.position_for(layer, &key_ch).map(|e| e.field.clone()) else { return };
    if actual_type(df, &key) != Some(VarType::Continuous) || df.time_unit(&key).is_some() {
        return;
    }
    let Some(keys) = df.float_col(&key) else { return };

    let splits: Vec<&str> = split_fields_of(layer).into_iter()
        .filter(|f| df.str_col(f).is_some())
        .collect();
    let frame = layer.encodings.get(&Channel::Play).or(spec.channels.get(&Channel::Play))
        .map(|e| e.field.as_str());
    let panels: Vec<&str> = spec.facet.as_ref().into_iter()
        .flat_map(|f| [f.col.as_deref(), f.row.as_deref()])
        .flatten()
        .chain(frame)
        .filter(|k| df.str_col(k).is_some() || df.float_col(k).is_some())
        .collect();
    let at = |f: &str, r: usize| -> String {
        match (df.str_col(f), df.float_col(f)) {
            (Some(c), _) => c.get(r).cloned().unwrap_or_default(),
            (None, Some(c)) => c.get(r).map(|v| v.to_bits().to_string()).unwrap_or_default(),
            _ => String::new(),
        }
    };
    let mut seen: std::collections::HashSet<Vec<String>> = std::collections::HashSet::new();
    for (r, v) in keys.iter().enumerate() {
        if !v.is_finite() {
            continue;
        }
        let mut cell = vec![v.to_bits().to_string()];
        cell.extend(splits.iter().chain(panels.iter()).map(|f| at(f, r)));
        if !seen.insert(cell) {
            return; // two rows in one group: the statistic summarizes something
        }
    }
    if seen.len() < 2 {
        return;
    }

    let m = mark_name(&layer.mark);
    let t = transform_name(stat);
    let c = channel_name(&key_ch);
    let head = match stat {
        Transform::Box => "`box`".to_string(),
        _ => format!("`{m} * {t}`"),
    };
    let within = match splits.is_empty() {
        true => String::new(),
        false => format!(" within each {}",
            splits.iter().map(|s| format!("`{s}`")).collect::<Vec<_>>().join(" and ")),
    };
    let (what, fix) = match stat {
        Transform::Count => ("every count is 1".to_string(),
            format!("`{m} * bin + {c}({key})` counts the rows in ranges of `{key}` instead")),
        Transform::Proportion => ("every share is one row's".to_string(),
            format!("`{m} * bin * proportion + {c}({key})` reads ranges of `{key}` as \
                     shares instead")),
        Transform::Box => ("every box is a single value".to_string(),
            "group by a column whose values repeat".to_string()),
        Transform::Range | Transform::Confidence | Transform::Deviation => (
            format!("each `{t}` is one row's own value"),
            "group by a column whose values repeat".to_string()),
        _ => (format!("each `{t}` is one row's own value"),
            format!("cut `{key}` into ranges first, `{m} * bin * {t}`, or group by a \
                     column whose values repeat")),
    };
    // **A ribbon of one-row bands draws nothing.** Every band runs from a value to
    // itself, so it has no height, and the panel comes out empty under its legend:
    // the failure refused above all others, which an Assumption ending "Drawn as
    // asked" let through (found 2026-10-13 walking the mark × channel grid, with
    // `color(country)` splitting `gapminder_asia`'s five rows a year into one each;
    // the author: "yes do what you recommended"). An `interval` still draws a
    // one-row span, its caps meeting at the value, so it keeps the note.
    if layer.mark == Mark::Ribbon
        && matches!(stat, Transform::Range | Transform::Confidence | Transform::Deviation)
    {
        // The bindings that split the rows, and whether leaving them out gives some
        // band two rows to span: offered only when it does.
        let mut split_by: Vec<String> = layer.encodings.iter()
            .filter(|(ch, d)| !matches!(ch, Channel::X | Channel::Y | Channel::Play)
                && splits.contains(&d.field.as_str()))
            .map(|(ch, d)| format!("`{}({})`", channel_name(ch), d.field))
            .collect();
        split_by.sort();
        let mut unsplit: std::collections::HashSet<Vec<String>> = std::collections::HashSet::new();
        let widens = keys.iter().enumerate().filter(|(_, v)| v.is_finite()).any(|(r, v)| {
            let mut cell = vec![v.to_bits().to_string()];
            cell.extend(panels.iter().map(|f| at(f, r)));
            !unsplit.insert(cell)
        });
        let leave = match (split_by.is_empty(), widens) {
            (false, true) => format!("Leave out {} to draw the spread across them, or \
                                      group by a column whose values repeat.",
                                     split_by.join(" and ")),
            _ => "Group by a column whose values repeat.".to_string(),
        };
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: {head} groups the rows by their value of `{key}`{within}, and here \
                 every group holds a single row, so each band runs from a value to itself \
                 and has no height: the ribbon would draw nothing. {leave}"
            ),
        });
        return;
    }
    out.push(Diagnostic {
        kind: DiagnosticKind::Assumption,
        message: format!(
            "gog: {head} groups the rows by their value of `{key}`{within}, and here \
             every group holds a single row, so {what}. Drawn as asked. To summarize, \
             {fix}."
        ),
    });
}

/// **A pile has one direction** — the third condition on `stack`, and the only one
/// of the three that has to read the numbers (spec §5, §12).
///
/// `stack_frame` gives each element the span `[base, base + value]`, where `base` is
/// everything piled below it at the same position. That is the whole of stacking, and
/// it is correct for a pile whose members agree in sign: all-positive grows up from
/// zero, all-negative grows *down* from zero, and in both the bands sit end to end
/// with the axis reading the total. Mix the signs at one position and the arithmetic
/// turns on itself — `top < base`, so the band is drawn **inside** the ones beneath
/// it, and a reader sees ink of length `|value|` sitting where the data said the pile
/// shrank. Measured before this refusal existed: `a = 5`, `b = -3` drew `b` from 2 up
/// to 5, entirely overlapping `a`, with nothing said. Not a poor picture but an
/// **inverted** one, which is why it is fatal rather than an Assumption; a
/// warn-then-draw here is the silent drop §12 exists to forbid.
///
/// `stack(share = true)` fails in the same place and more obviously: a share is a
/// fraction of the pile's total, and with `5` and `-3` the total is `2`, so the two
/// "shares" come out `2.5` and `-1.5`. There is no normalization that rescues it,
/// which is the confirming sign that the defect is in the *pile* and not in how it
/// is drawn.
///
/// **Per position, not per column**, because the pile is what has to agree: piles
/// pointing up at one x and down at another are two coherent piles and stay legal
/// (the diverging stacked bar). Refusing the whole column would forbid that to catch
/// this, which is Law 8 — enforce well-formedness hard, never forbid the ugly-but-legal.
///
/// **Asked of the values `stack` will actually pile**, which is why this runs the
/// layer's other transforms first rather than reading the bound column: `bar * sum *
/// stack` piles sums, and a column of mixed signs whose *cells* come out all-positive
/// is a plot with nothing wrong with it. Running `transform::apply` minus `Stack` is
/// the one way to be sure the check and the renderer are looking at the same numbers,
/// on the same reasoning as `limit_cut` — a count the user is given must not disagree
/// with the picture. It costs a second pass over the frame, and only on layers that
/// carry `stack`.
///
/// **A pile holds one value for each group at a position**, and that is asked first
/// (2026-10-13, at the author's word, found walking the marks × transforms grid).
/// `stack_frame` lifts a group by *every* row below it at the position, but piles
/// the rows of one group on nothing: they all start at the group's foot. So
/// `data(gm_eras) + bar * stack + x(continent) + y(life) + color(era)` lifted
/// Asia's 2007 bars by the total of its 33 rows of 1957 and drew both eras' 33 bars
/// over one another from one foot each, leaving the middle of the column empty and
/// the axis measuring a total no bar showed. §5 has always said `stack` expects one
/// value per (position, group) and a raw table is summarized first; nothing checked
/// it. Refused rather than piled row by row, because the cure that piles every row
/// is already a sentence (a split that names each row), and the refusal says which
/// column would do it. A `point` is exempt: its pile is of counted rows, written by
/// `bin`/`count`, and `transform::pile` draws one dot for each.
pub(super) fn check_stack_piles(
    out: &mut Vec<Diagnostic>,
    spec: &PlotSpec,
    df: &DataFrame,
    layer: &Layer,
    data: &HashMap<String, DataFrame>,
) {
    if !layer.transforms.contains(&Transform::Stack) {
        return;
    }
    // The conditions `check_stack` already refuses. Asking again here would be a
    // second refusal for one mistake, and `apply` on a mark that cannot stack would
    // be answering a question nobody may ask.
    if mark_takes_transform(&layer.mark, &Transform::Stack) == TransformLegality::None {
        return;
    }
    // Nor are the piles of a layout or an estimate checked: each is refused before
    // there is a pile (`check_stack`).
    if layer.transforms.iter().any(|t| is_whole_picture(t)
        || matches!(t, Transform::Density | Transform::Smooth | Transform::SmoothBand))
    {
        return;
    }
    let group_fields: Vec<&str> = split_fields_of(layer).into_iter()
        .filter(|f| df.str_col(f).is_some())
        .collect();
    if group_fields.is_empty() { return } // refused by `check_stack` 2b

    // Which axis the pile stands on, read off the bindings exactly as the renderer
    // reads it (`key_is_x`): the plot's orientation, for a layer that reads one. The
    // key groups, the measure accumulates. It read `columns_orient` of the two
    // columns until 2026-10-13, which takes an unbound `x` for the axis a transform
    // fills: a pile with no position axis (`bar * stack + y(population) +
    // color(continent)`) found its measure on the key, no column to pile, and went
    // unchecked, where `plot_orient` stands it upright as the renderer does.
    let field = |c: Channel| spec.position_for(layer, &c).map(|e| e.field.as_str()).unwrap_or("");
    let (x_field, y_field) = (field(Channel::X), field(Channel::Y));
    let horizontal = plot_orient(spec, data) == Orient::Horizontal;
    let (key_field, out_field) = match horizontal && reads_orientation(layer) {
        false => (x_field, y_field),
        true => (y_field, x_field),
    };

    let piled = crate::transform::apply(
        df,
        &layer.transforms.iter().filter(|t| **t != Transform::Stack).cloned().collect::<Vec<_>>(),
        key_field, out_field,
        layer.bin.as_ref(), None, layer.density.as_ref(), layer.range.as_ref(),
        layer.level_spec(), layer.deviation.as_ref(), layer.quantile.as_ref(),
        layer.r#box.as_ref(), layer.bounds.as_ref(), None, layer.cumulative.as_ref(),
        &group_fields,
    );
    // Over running totals the pile is of the totals, which is what is drawn.
    let piled_field = match crate::transform::accumulates(&layer.transforms) {
        true => crate::transform::CELL_CUMULATIVE,
        false => out_field,
    };
    let Some(vals) = piled.float_col(piled_field) else { return };
    // A group is named by its whole combination, so a pile split by two columns
    // names both halves of the band at fault.
    let cols: Vec<&Vec<String>> = group_fields.iter().filter_map(|f| piled.str_col(f)).collect();
    if cols.len() != group_fields.len() { return }
    let groups: Vec<String> = (0..vals.len())
        .map(|i| cols.iter().filter_map(|c| c.get(i).map(String::as_str)).collect::<Vec<_>>().join(", "))
        .collect();

    // Two elements share a position on the same terms `stack_frame` uses, so the piles
    // this check inspects are the piles that will be drawn: string equality for a
    // categorical key, a tolerance compare for a numeric one, and *everything* in one
    // pile when no key is named (the share-of-total bar, and the pie).
    let key_str = piled.str_col(key_field);
    let key_num = piled.float_col(key_field);
    let same_pos = |a: usize, b: usize| -> bool {
        if key_field.is_empty() { return true; }
        if let Some(k) = key_str { return k[a] == k[b]; }
        if let Some(k) = key_num { return (k[a] - k[b]).abs() < 1e-9; }
        a == b
    };

    if matches!(layer.mark, Mark::Bar | Mark::Area)
        && stack_rows_repeat(out, spec, df, data, layer, &piled, key_field, &group_fields)
    {
        return; // the signs of a pile that cannot be drawn are not the first thing wrong
    }

    for i in 0..vals.len() {
        if !(vals[i] > 0.0) { continue }
        // The first negative sharing this position, if there is one. Reported with
        // both offending groups and both numbers, because "some values are negative"
        // sends a reader to scan the whole table for a pile they cannot see.
        let Some(j) = (0..vals.len()).find(|&j| vals[j] < 0.0 && same_pos(i, j)) else { continue };
        let name = mark_name(&layer.mark);
        let at = match (key_str, key_num) {
            _ if key_field.is_empty() => "this position".to_string(),
            (Some(k), _) => format!("{key_field} = {}", k[i]),
            (_, Some(k)) => format!("{key_field} = {}", k[i]),
            _ => "this position".to_string(),
        };
        // The direction divides by geometry, the way every `stack` refusal does: a
        // bar can be set beside its neighbors and keep its own baseline, where an
        // area has no baseline of its own to keep and simply stops piling.
        let fix = match layer.mark {
            Mark::Bar => "`bar * dodge` sets the groups side by side instead, and a dodged bar \
                          keeps its own baseline, so a negative one hangs below the axis where \
                          a reader can see that it is negative",
            Mark::Area => "drop `stack`: overlaid areas each run from their own baseline, \
                           translucent so the ones behind show through",
            _ => "drop `stack`: without it each group is drawn from its own baseline",
        };
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `{name} * stack` piles the groups at one position on top of one another, \
                 and at {at} they disagree in sign — `{}` is {} and `{}` is {}. A pile has one \
                 direction: the band going the other way is drawn *inside* the ones below it, so \
                 the plot would show a block of {} where the number is negative and read its sign \
                 backwards. Either keep the sign and drop the pile — {fix} — or keep the pile and \
                 drop the sign, stacking magnitudes you took the absolute value of in the host. \
                 (A pile whose members agree stays legal both ways: all-negative grows downward \
                 from zero exactly as all-positive grows up.)",
                groups[i], vals[i], groups[j], vals[j], vals[j].abs(),
            ),
        });
        return; // one refusal per layer — the first pile names the defect
    }
}

/// Whether some position holds two rows of one group, refusing if so. The first
/// question `check_stack_piles` asks; see there for why it is a refusal.
///
/// Asked of `piled`, the frame after the layer's other transforms, so `bar * sum *
/// stack` is never troubled: a statistic leaves one row per group at a position.
/// A facet panel or a `play` frame draws its own piles, so its columns are part of
/// the key, as in `check_zone_cells`. The message names the first repeated pile
/// and counts the rest, and offers each way out only when a trial `check()` of it
/// passes (the count hint's rule): a statistic for one segment per group, and a
/// split by a column that names each row for one segment per row.
#[allow(clippy::too_many_arguments)]
fn stack_rows_repeat(
    out: &mut Vec<Diagnostic>,
    spec: &PlotSpec,
    df: &DataFrame,
    data: &HashMap<String, DataFrame>,
    layer: &Layer,
    piled: &DataFrame,
    key_field: &str,
    group_fields: &[&str],
) -> bool {
    let text = |field: &str, r: usize| -> String {
        piled.str_col(field).and_then(|c| c.get(r).cloned())
            .or_else(|| piled.float_col(field).and_then(|c| c.get(r)).map(|v| v.to_string()))
            .unwrap_or_default()
    };
    let mut panel: Vec<&str> = Vec::new();
    if let Some(f) = &spec.facet {
        panel.extend(f.col.as_deref());
        panel.extend(f.row.as_deref());
    }
    if let Some(p) = layer.encodings.get(&Channel::Play) {
        panel.push(p.field.as_str());
    }
    panel.retain(|f| piled.str_col(f).is_some() || piled.float_col(f).is_some());
    // No key named is one pile holding everything, as in `stack_frame`.
    let cell_of = |r: usize, extra: Option<&str>| -> Vec<String> {
        let mut k = vec![if key_field.is_empty() { String::new() } else { text(key_field, r) }];
        k.extend(group_fields.iter().map(|f| text(f, r)));
        k.extend(panel.iter().map(|f| text(f, r)));
        k.extend(extra.map(|f| text(f, r)));
        k
    };

    let mut cells: HashMap<Vec<String>, usize> = HashMap::new();
    let mut first: Option<Vec<String>> = None;
    for r in 0..piled.len() {
        let k = cell_of(r, None);
        let n = cells.entry(k.clone()).or_insert(0);
        *n += 1;
        if *n == 2 && first.is_none() {
            first = Some(k);
        }
    }
    let Some(first) = first else { return false };
    let crowded = cells.values().filter(|&&n| n > 1).count();

    let name = mark_name(&layer.mark);
    let piece = if layer.mark == Mark::Area { "band" } else { "segment" };
    let trial_passes = |l: Layer| {
        let mut trial = spec.clone();
        trial.layers = vec![l];
        !check(&trial, data).iter().any(Diagnostic::is_fatal)
    };
    let mut ways: Vec<String> = Vec::new();
    if !crate::transform::has_reduction(&layer.transforms) {
        let mut summed = layer.clone();
        let at = summed.transforms.iter().position(|t| *t == Transform::Stack)
            .unwrap_or(summed.transforms.len());
        summed.transforms.insert(at, Transform::Sum);
        let written = summed.transforms.iter().map(transform_name).collect::<Vec<_>>().join(" * ");
        if trial_passes(summed) {
            ways.push(format!(
                "To draw one {piece} for each group, summarize the rows first: `{name} * \
                 {written}` adds them, and `mean`, `median`, `max` and `min` work the same way."
            ));
        }
    }
    // A column that tells apart every row sharing a pile, offered as a `group` split.
    // One `group` to a layer, so a layer that already splits by it is not offered one.
    if !layer.encodings.contains_key(&Channel::Group) {
        let separator = piled.column_names()
            .filter(|c| *c != key_field && !group_fields.contains(c) && !panel.contains(c))
            .filter(|c| piled.str_col(c).is_some() && df.str_col(c).is_some())
            .find(|c| {
                let mut seen: HashMap<Vec<String>, ()> = HashMap::new();
                (0..piled.len()).all(|r| seen.insert(cell_of(r, Some(c)), ()).is_none())
            })
            .map(str::to_string);
        if let Some(c) = separator {
            let mut split = layer.clone();
            split.encodings.insert(Channel::Group, ChannelDef::field(c.clone()));
            if trial_passes(split) {
                ways.push(format!(
                    "To draw one {piece} for each row, split by a column that names each \
                     row as well: `+ group({c})`."
                ));
            }
        }
    }
    if ways.is_empty() {
        ways.push("Keep one row for each position and group before plotting.".to_string());
    }

    let at = if key_field.is_empty() {
        "in its one pile".to_string()
    } else {
        format!("at `{key_field} = {}`", first[0])
    };
    let group = group_fields.iter().zip(&first[1..])
        .map(|(f, v)| format!("`{f} = {v}`"))
        .collect::<Vec<_>>()
        .join(" and ");
    let n = cells[&first];
    let pairs = if key_field.is_empty() { "groups" } else { "position and group pairs" };
    out.push(Diagnostic {
        kind: DiagnosticKind::Illegal,
        message: format!(
            "gog: `{name} * stack` piles one value for each group at a position, and {at} \
             {n} rows have {group} ({crowded} of the {} {pairs} hold more than one row). \
             Those rows would all start from the same foot and cover one another, while the \
             groups above would be lifted by all of them, so the pile would show none of \
             its rows. {}",
            cells.len(),
            ways.join(" "),
        ),
    });
    true
}

/// `jitter` is the third collision modifier (spec §5): the offset for a mark with
/// no width to subdivide, along an axis with no magnitude to spend — `point` on a
/// *category*. It spreads a strip plot's coincident points apart. Two things make it
/// well-formed, and — unlike `dodge`/`stack` — the second is *data-aware*, which
/// is why this check takes `df` and lives beside `check_slot_shape` rather than in
/// the structural block:
///
/// 1. **A point cloud to spread.** The three offsets divide by geometry, and
///    jitter's mark is `point` alone; every other mark is refused toward the offset
///    its geometry wants (the width marks → `dodge`). It is `point`'s answer to
///    overlap along the **categorical** axis; `stack` is its answer along the
///    **measure** axis, where the offset can be a count instead of a nudge — one
///    mark, two overlap problems, one modifier each (spec §5).
/// 2. **A categorical band to spread within.** jitter offsets *only* along a
///    categorical position axis — never along one carrying a measured value, which
///    it would falsify (the ggplot2 divergence, spec §5). With both axes
///    continuous there is neither a band nor an axis it may honestly move, so it is
///    refused toward `style(opacity = )`, the honest tool for continuous
///    overplotting. It does **not** require a `color`/`group` split: it resolves
///    same-position overlap of individual points, which needs no grouping.
pub(super) fn check_jitter(out: &mut Vec<Diagnostic>, spec: &PlotSpec, df: &DataFrame, layer: &Layer) {
    if !layer.transforms.contains(&Transform::Jitter) {
        return;
    }
    // 1. A scatter of individual points to spread — read off the shared table.
    if mark_takes_transform(&layer.mark, &Transform::Jitter) == TransformLegality::None {
        let name = mark_name(&layer.mark);
        let fix = match layer.mark {
            Mark::Bar | Mark::Box | Mark::Interval =>
                "Those marks have a width; to set their `color` groups apart, `dodge` sets them side by side",
            Mark::Line | Mark::Area | Mark::Step | Mark::Ribbon =>
                "A connected path or filled region is one shape, not a cloud of separate points to spread",
            Mark::Text =>
                "A label is spread by the width of its own word, so a slot-sized nudge is the wrong \
                 measure — `repel` moves labels by what actually overlaps, their ink",
            _ => "jitter spreads a scatter of individual points, which this mark does not draw",
        };
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `jitter` spreads overlapping points sideways so a strip plot's density \
                 shows — but `{name}` is not a point mark. {fix}."
            ),
        });
        return;
    }
    // Under a whole-picture layout every second transform is refused in that
    // layout's own terms (`chain_message`); asking here for what this one needs
    // would send the reader to add it only to meet that refusal (2026-10-13).
    if layer.transforms.iter().any(is_whole_picture) {
        return;
    }
    // 2. A categorical band to spread within: at least one position axis must be a
    //    category. Both continuous → there is nothing jitter may honestly move.
    //    An axis a transform writes is a number too (`bin`'s count, `density`'s
    //    height): until 2026-10-13 only two *bound* numbers were asked about, so
    //    `point * bin * jitter + x(life)` drew its points unmoved and said nothing.
    let xt = spec.position_for(layer, &Channel::X).and_then(|c| actual_type(df, &c.field));
    let yt = spec.position_for(layer, &Channel::Y).and_then(|c| actual_type(df, &c.field));
    let category = |t: Option<VarType>| t == Some(VarType::Discrete);
    if !category(xt) && !category(yt) && (xt.is_none() || yt.is_none()) {
        let made = layer.transforms.iter()
            .find(|t| !matches!(t, Transform::Jitter))
            .map(transform_name).unwrap_or("a transform");
        let bound = [Channel::X, Channel::Y].iter()
            .filter_map(|ch| spec.position_for(layer, ch).map(|c| format!("`{}({})`", channel_name(ch), c.field)))
            .next().unwrap_or_else(|| "the bound axis".to_string());
        let kept: Vec<&str> = layer.transforms.iter()
            .filter(|t| !matches!(t, Transform::Jitter)).map(transform_name).collect();
        let chain = std::iter::once("point").chain(kept.iter().copied()).collect::<Vec<_>>().join(" * ");
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `point * jitter` spreads points sideways within a categorical band, but \
                 here neither axis is a category: {bound} is a number, and the other axis is \
                 the measurement `{made}` writes, so there is no band to spread within, and \
                 nudging a value would misplace it. Drop `jitter`: `{chain}` draws each point \
                 at its value."
            ),
        });
        return;
    }
    if xt == Some(VarType::Continuous) && yt == Some(VarType::Continuous) {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message:
                "gog: `point * jitter` spreads points sideways within a categorical band, but \
                 here both `x` and `y` are continuous — there is no band to spread within, and \
                 nudging a measured value would misplace it. For overplotting on two continuous \
                 axes, `style(opacity = )` reveals density without moving any point off its value."
                    .to_string(),
        });
        return;
    }
    // 3. A spread that stays inside its own category. Past `JITTER_MAX` a point can
    //    land in the neighboring slot, and there it reads as one of that category's
    //    rows: a wrong value drawn rather than a looser picture. Refused rather than
    //    clamped, because a clamp would draw a spread nobody asked for in silence.
    if let Some(a) = layer.jitter.as_ref().and_then(|j| j.amount).filter(|a| *a > JITTER_MAX) {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `jitter({a})` spreads points past the edge of their own category, so some \
                 would land in the neighboring one and be read as its rows. A point moves up \
                 to 0.4 of a slot for each unit of the amount, so `jitter({JITTER_MAX})` reaches \
                 the slot's edge and is the widest spread that keeps every point in its own \
                 category. Use an amount of {JITTER_MAX} or less; to show where many points \
                 overlap, `style(opacity = )` or a smaller `style(size = )` does it without \
                 moving any point."
            ),
        });
    }
    // 4. A spread of nothing. `jitter(0)` moves no point, so the plot is the one drawn
    //    without it: a proven redundancy, which draws and says so (§12's standing
    //    rule, `quantile(0.5)`'s precedent). Legal, because it harms nothing (Law 8);
    //    said, because a transform that changes nothing is a transform nobody read.
    //    The bindings refuse a negative amount, so zero is the only one that is a
    //    no-op.
    if layer.jitter.as_ref().and_then(|j| j.amount) == Some(0.0) {
        let m = mark_name(&layer.mark);
        out.push(Diagnostic {
            kind: DiagnosticKind::Assumption,
            message: format!(
                "gog: `jitter(0)` moves no point, so this draws the same plot as `{m}` \
                 without it. Drop `jitter`, or give it a spread: `jitter` alone spreads by \
                 the default, `jitter(0.5)` by half of it. Drawn as asked."
            ),
        });
    }
}

/// The widest `jitter` whose spread stays inside its own slot.
///
/// A jittered point moves at most half its band, and the band is `amount` times the
/// width a categorical `bar` takes, which is 0.80 of a slot
/// (`render::marks::bar_thickness_svg`). So the largest move is `0.4 × amount` of a
/// slot, and it reaches the slot's edge at exactly 1.25. The spec's `jitter` entry
/// promised the spread was "bounded to the slot" long before anything checked it:
/// `jitter(2)` moved a third of a strip plot's points into the neighboring category
/// and `jitter(3)` pushed some past the panel, all without a word. A test in the
/// renderer draws a strip plot at this amount and asserts no point leaves its slot,
/// so the number here cannot drift away from the band it is derived from.
pub const JITTER_MAX: f64 = 1.25;

/// `point * dodge` — the beeswarm (spec §5) — and the one question about it the
/// sentence alone cannot answer: **is there a slot to swarm in, and an axis that
/// stays true?** Data-aware for `check_jitter`'s reason, so it sits beside it.
///
/// A swarm sets colliding points apart across a category's slot and moves each
/// one along the category axis only, never along the other, which carries the
/// measurement. So exactly one position must be a category:
///
/// - **Both continuous** (or a position with no column behind it): there is no
///   slot to set points apart in, and moving either value would misplace it.
///   Refused toward `style(opacity = )`, `check_jitter`'s direction for the same
///   geometry.
/// - **Both categories**: no axis holds a measurement, so nothing says which way a
///   swarm may move and which way it must not. Refused toward the two sentences
///   that already answer a crowded cell: `point * jitter` spreads within it,
///   `zone * count` colors it by how many rows it holds.
///
/// Two spaces are named here because the swarm is a flat placement in page units
/// and neither draws it yet. A **polar** slot is a wedge whose width grows with the
/// radius, so "the least amount that clears" is a different sum at every ring; in
/// the **cube** a slot is a strip of floor seen at an angle. Both are Unsupported
/// rather than Illegal: the sentence is well formed and the reading is clear, and
/// only the drawing is missing. `nest` refuses every collision modifier in its own
/// check, so this one stays quiet there rather than refusing twice.
pub(super) fn check_swarm(out: &mut Vec<Diagnostic>, spec: &PlotSpec, df: &DataFrame, layer: &Layer) {
    if layer.mark != Mark::Point || !layer.transforms.contains(&Transform::Dodge) {
        return;
    }
    // A whole-picture layout refuses the `dodge` itself (`check_dodge`'s guard).
    if layer.transforms.iter().any(is_whole_picture) {
        return;
    }
    match space_of(spec) {
        SpaceKind::Nest => return,
        SpaceKind::Polar => {
            out.push(Diagnostic {
                kind: DiagnosticKind::Unsupported,
                message: "gog: `point * dodge` in `polar()` is valid grammar, but this engine \
                          does not draw it: the swarm sets points apart by the least distance \
                          that clears them across a straight slot, and a `polar()` slot is a \
                          wedge that widens with the radius. Drawn flat, drop `polar()`; inside \
                          the wedges, `point * jitter` spreads the points by a seeded amount \
                          instead."
                    .to_string(),
            });
            return;
        }
        SpaceKind::Space => {
            out.push(Diagnostic {
                kind: DiagnosticKind::Unsupported,
                message: "gog: `point * dodge` in a cube is valid grammar, but this engine does \
                          not draw it: the swarm sets points apart across a category's slot on \
                          the page, and in a cube that slot is a strip of floor seen at an \
                          angle. Drop `z()` to draw the swarm flat."
                    .to_string(),
            });
            return;
        }
        _ => {}
    }
    let xt = spec.position_for(layer, &Channel::X).and_then(|c| actual_type(df, &c.field));
    let yt = spec.position_for(layer, &Channel::Y).and_then(|c| actual_type(df, &c.field));
    let x_cat = xt == Some(VarType::Discrete);
    let y_cat = yt == Some(VarType::Discrete);
    if x_cat && y_cat {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: "gog: `point * dodge` keeps one axis true and sets points apart along the \
                      other, and here both `x` and `y` are categories, so neither holds a \
                      measurement to keep. To show a crowded cell, `point * jitter` spreads its \
                      points inside it, and `zone * count` colors it by how many rows it holds."
                .to_string(),
        });
    } else if !x_cat && !y_cat {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: "gog: `point * dodge` sets points apart inside a category's slot and moves \
                      them along the category axis only, but here neither `x` nor `y` is a \
                      category — there is no slot to swarm in, and moving a measured value would \
                      misplace it. For overplotting on two continuous axes, `style(opacity = )` \
                      reveals density without moving any point off its value."
                .to_string(),
        });
    }
}

/// `repel` is the fourth collision modifier (spec §5), and the only one whose
/// collision is made of **ink**. The other three answer *two marks landed on one
/// position*; a label is as wide as the word it draws, so two labels overlap at
/// positions their points never shared, and no offset computed from a position can
/// see it.
///
/// One thing makes it well-formed, which is why this check is shorter than its
/// three siblings: **a word to move.** `text` is the only mark whose glyph is data
/// rather than geometry, so it is the only mark with ink of its own to collide, and
/// every other mark is refused toward the offset its geometry does want. There is
/// no second condition of `check_jitter`'s kind — jitter must find a categorical
/// band to spread inside, and repel needs nothing from the axes at all, since the
/// page is where the overlap happens and every plot has one.
///
/// A single-row layer is **not** refused, and that is the deliberate difference
/// from `check_dodge`, which turns down a layer with no split. A dodge with no
/// groups can never do anything, whatever the table holds; a repel with one label
/// today moves four of them the moment the table has four rows, so refusing it
/// would be refusing a sentence for the shape of this week's data (Law 4).
pub(super) fn check_repel(out: &mut Vec<Diagnostic>, layer: &Layer) {
    if !layer.transforms.contains(&Transform::Repel) {
        return;
    }
    if mark_takes_transform(&layer.mark, &Transform::Repel) == TransformLegality::None {
        let name = mark_name(&layer.mark);
        let fix = match layer.mark {
            Mark::Point =>
                "A dot is as wide as its `size` and no wider, so what overlaps is its position — \
                 `jitter` spreads points sharing a category, and `style(opacity = )` shows density \
                 where both axes are continuous",
            Mark::Bar | Mark::Box | Mark::Interval =>
                "Those marks have a width, and the groups crowding one slot are set beside each \
                 other by subdividing it — that is `dodge`",
            Mark::Line | Mark::Step | Mark::Area | Mark::Ribbon =>
                "A path or a filled band is one shape rather than many small ones, so it has no \
                 crowd to thin — `stack` lays filled bands end to end, and transparency \
                 (`style(opacity = )`) tells overlapping ones apart",
            _ => "repel moves labels off one another, and this mark draws no label",
        };
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `repel` moves overlapping *labels* apart, so it belongs to `text` — the one \
                 mark whose glyph is a word — and `{name}` draws no words. {fix}."
            ),
        });
    }
}

/// `stack` or `dodge` on an estimated curve (`matrix::offset_conflict`), refused with
/// the reason: an offset moves marks that land on one position, and an estimate
/// samples each group's curve along that group's own range, so the groups' points do
/// not line up. Returns whether it refused.
fn offset_on_estimate(out: &mut Vec<Diagnostic>, layer: &Layer, offset: &Transform) -> bool {
    let Some(e) = layer.transforms.iter()
        .find(|t| matches!(t, Transform::Density | Transform::Smooth | Transform::SmoothBand))
    else { return false };
    let (m, en) = (mark_name(&layer.mark), transform_name(e));
    let chain = std::iter::once(m).chain(layer.transforms.iter().map(transform_name))
        .collect::<Vec<_>>().join(" * ");
    let (does, none) = match offset {
        Transform::Stack => ("piles the groups that land on one position", "nothing would pile"),
        _ => ("sets side by side the groups that land on one position",
              "nothing would be set apart"),
    };
    out.push(Diagnostic {
        kind: DiagnosticKind::Illegal,
        message: format!(
            "gog: `{chain}` {does}, and `{en}` computes each group's curve at points along \
             that group's own range, so the groups' points do not line up and {none}. To \
             compare the groups' \
             curves, overlay them: `{m} * {en} + color(<group>) + style(opacity = 0.5)`, or \
             give each group its own panel: `{m} * {en} | facet(<group>)`."
        ),
    });
    true
}
