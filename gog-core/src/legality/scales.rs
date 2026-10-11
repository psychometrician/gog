//! Scales: what a channel's stated domain, ticks, legend and axis may say.

use super::*;

// ---------------------------------------------------------------------------
// Scales
//
// A scale is a property of a binding, not an atom, so it is checked here beside
// the binding rather than in a table of its own. What can go wrong is different
// in kind from what goes wrong with a channel: a mapping is judged against the
// column's *type*, a scale against its *values*. `log` is the first thing in
// gog that a perfectly well-typed column can still fail.
// ---------------------------------------------------------------------------

/// Does this channel run along a scale at all?
///
/// Every channel that can carry a *continuous* column does, and for the same
/// reason: a scale answers "how far along?", which is a question only a
/// magnitude has. `shape` and `group` take a categorical column, where the
/// answer is "which one?" — there is no distance between circle and square for
/// a logarithm to compress.
pub(super) fn reads_a_scale(channel: &Channel) -> bool {
    matches!(
        channel,
        Channel::X | Channel::Y | Channel::Z | Channel::Color | Channel::Size | Channel::Opacity
    )
}

/// Which axis a synthesizing transform writes to.
///
/// Orientation decides it: `bar * count + x(cat)` invents y, `bar * count +
/// y(cat)` invents x. True of every slot mark, not just `bar` — `interval *
/// bounds(lo, hi) + y(cat)` is the horizontal error bar whose extents are the
/// two named columns, so the axis it invents is x.
///
/// **In the cube the answer is `z`, and orientation never comes up.** A mark
/// that cuts both positions there has spent `x` and `y` on the two edges of its
/// footprint, so the axis left to write a tally to is the third one — which is
/// why there is no 3-D counterpart of the horizontal bar. The flat reading is
/// unchanged: which of `x`/`y` measures is a property of the pair, and
/// `slot_orient` reads it off the bindings.
///
/// `pub` since scale limits arrived (spec §10): a stated domain filters rows
/// *before* the statistics on the axis they group by and judges the computed
/// values on the axis they write, which is the same distinction `check_scale`
/// draws with `values_are_the_input`. Two copies of this `match` would be two
/// answers to one question, and this file records what that costs.
pub fn synth_axis(spec: &PlotSpec, layer: &Layer, df: Option<&DataFrame>) -> Channel {
    match (&layer.mark, df) {
        // A cluster tree writes the merge distance to the position its leaf
        // axis does not hold — read off the bindings, the `slot_orient` rule:
        // leaves on `x` grow the tree up `y`, leaves on `y` grow it along `x`.
        // A position naming `distance` is the tree's own and holds no leaves
        // (spec §8, a written column is a column): `x(distance) + y(<leaves>)`
        // lies the tree down, and `y(distance)` alone binds no leaf axis.
        (Mark::Path, _) if layer.transforms.contains(&Transform::Cluster) => {
            let names = |ch: &Channel, distance: bool| spec.position_for(layer, ch)
                .is_some_and(|d| (d.field == crate::transform::CLUSTER_DISTANCE) == distance);
            if names(&Channel::X, true)
                || (names(&Channel::Y, false) && !names(&Channel::X, false))
            {
                Channel::X
            } else {
                Channel::Y
            }
        }
        (m, _) if cuts_both_positions(m, space_of(spec)) && !has_no_measure_axis(m) => Channel::Z,
        // A bar that is one pile has no key axis, so its measure is on `y`: the rule
        // the plot's orientation states first (`bar_is_one_pile`), asked here too so
        // the two cannot disagree about which axis the tally writes. They did, for a
        // `y` naming a column under a tally: the orientation drew one pile while this
        // sent the column to `slot_orient` as a key, so it was never checked and only
        // titled the pile's axis. That `y` is a key now in both, and the bar lies on
        // its side.
        (Mark::Bar, _) if bar_is_one_pile(spec, layer, df) => Channel::Y,
        (_, Some(df)) if reads_orientation(layer) => {
            let xt = spec.position_for(layer, &Channel::X).and_then(|c| actual_type(df, &c.field));
            let yt = spec.position_for(layer, &Channel::Y).and_then(|c| actual_type(df, &c.field));
            match slot_orient(xt, yt) {
                Orient::Horizontal => Channel::X,
                Orient::Vertical => Channel::Y,
            }
        }
        _ => Channel::Y,
    }
}

/// What a layer's stated domains keep, and what they cut (spec §10).
///
/// One authority for the question, because the renderer filters on it and
/// `check_limits` reports on it: a count that disagreed with the rows actually
/// dropped would be the silent-drop failure wearing a diagnostic.
pub struct LimitCut {
    /// One entry per row of the frame this was computed against.
    pub keep: Vec<bool>,
    /// `(channel, column, excluded)` for every stated domain that cut something.
    pub cuts: Vec<(Channel, String, usize)>,
    pub total: usize,
}

impl LimitCut {
    /// Nothing stated, or nothing outside it — the overwhelmingly common case.
    pub fn is_empty(&self) -> bool {
        self.cuts.is_empty()
    }
    /// Every row cut: an empty panel with fabricated axes unless someone refuses.
    pub fn takes_everything(&self) -> bool {
        !self.cuts.is_empty() && !self.keep.iter().any(|&k| k)
    }
}

/// Apply this layer's stated domains to a frame.
///
/// Only the bindings whose **values are the input** take part: on the axis a
/// transform writes there is no column yet to filter, and the limit judges the
/// computed values instead (§10's ordering rule, which a limit inherits from
/// being a scale property rather than restating).
pub fn limit_cut(spec: &PlotSpec, layer: &Layer, df: &DataFrame) -> LimitCut {
    let total = df.len();
    let mut keep = vec![true; total];
    let mut cuts = Vec::new();
    let synth = synth_axis(spec, layer, Some(df));

    for channel in ALL_CHANNELS {
        if !reads_a_scale(&channel) {
            continue;
        }
        // The axis a statistic writes: its values do not exist yet.
        if !layer.transforms.is_empty() && channel == synth {
            continue;
        }
        let Some(def) = binding_of(spec, layer, &channel) else { continue };
        if !crate::scale::has_limits(Some(def)) {
            continue;
        }
        let Some(col) = df.float_col(&def.field) else { continue };
        // Counted per binding rather than per row, so two domains cutting the
        // same row each say so — the reader needs to know which limit did it,
        // and the totals are reported separately from the rows kept.
        let mut cut = 0usize;
        for (i, &v) in col.iter().enumerate().take(keep.len()) {
            if !crate::scale::within_limits(Some(def), v) {
                keep[i] = false;
                cut += 1;
            }
        }
        if cut > 0 {
            cuts.push((channel, def.field.clone(), cut));
        }
    }
    LimitCut { keep, cuts, total }
}

/// The binding for a channel, wherever it lives — on the layer or the plot.
///
/// Positions resolve through `position_for` (the layer's own column, else the
/// axis's), so the two kinds of channel answer the same question the same way.
pub(super) fn binding_of<'a>(spec: &'a PlotSpec, layer: &'a Layer, channel: &Channel) -> Option<&'a ChannelDef> {
    match channel {
        Channel::X | Channel::Y | Channel::Z => spec.position_for(layer, channel),
        other => layer.encodings.get(other),
    }
}

/// Check a scale override against the column it is applied to.
///
/// `values_are_the_input` is false on the axis a transform *writes*. The raw
/// column is not what gets scaled there, so its values say nothing about whether
/// the result can be placed: `bar * sum + y(profit, scale = "log")` is well
/// formed over negative profits that sum to a positive total, and refusing it on
/// the raw column would be a false alarm. What actually failed to place is
/// reported by the renderer, which is the only stage that can see it.
#[allow(clippy::too_many_arguments)]
pub(super) fn check_scale(
    out: &mut Vec<Diagnostic>,
    def: &ChannelDef,
    c: &str,
    field: &str,
    df: &DataFrame,
    actual: VarType,
    scaled_channel: bool,
    values_are_the_input: bool,
) {
    // Nothing said about scaling — the overwhelmingly common case.
    if def.scale.is_none() && def.base.is_none() {
        return;
    }

    // A categorical channel has no "how far along?" for a scale to answer.
    if !scaled_channel {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `{what}` on `{c}({field})`: `{c}` distinguishes categories rather than \
                 measuring them, so there is no scale for it to run along. Drop `{what}`.",
                what = match (def.scale.is_some(), def.base.is_some()) {
                    (true, true) => "scale` and `base",
                    (false, true) => "base",
                    _ => "scale",
                }
            ),
        });
        return;
    }

    // A base belongs to a logarithm and to nothing else. There is no base of a
    // linear scale, so `base = 2` without one is a request the engine cannot
    // honor in any way — silently ignoring it is the outcome forbidden here.
    if def.base.is_some() && def.scale != Some(ScaleType::Log) {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `{c}({field}, base = …)` has no scale to be the base of. A base belongs to \
                 a logarithm — add `scale = \"log\"`, or remove the base."
            ),
        });
        return;
    }

    match def.scale.as_ref().unwrap_or(&ScaleType::Linear) {
        // The default. Saying it out loud is allowed and means nothing extra —
        // except on a date column, where "linear" would mean *un*-dating the
        // axis into raw epoch seconds. Honoring that silently would draw an
        // axis labeled 1.7B; ignoring it silently is forbidden outright.
        ScaleType::Linear => {
            if df.time_unit(field).is_some() {
                out.push(Diagnostic {
                    kind: DiagnosticKind::Illegal,
                    message: format!(
                        "gog: `{c}({field}, scale = \"linear\")` — `{field}` is a date column, \
                         and a date always reads as a calendar. If you truly want raw epoch \
                         numbers, store the column as numbers before plotting."
                    ),
                });
            }
        }

        // The time scale comes from the column's type, like `category` does: a
        // date column is already temporal, so saying so is allowed and means
        // nothing extra. On anything else it is unanswerable — the engine
        // cannot know whether 20656 is a day, a second, or a year.
        ScaleType::Time => {
            if df.time_unit(field).is_none() {
                out.push(Diagnostic {
                    kind: DiagnosticKind::Illegal,
                    message: format!(
                        "gog: `{c}({field}, scale = \"time\")` — `{field}` is not a date column, \
                         and a number alone does not say what moment it is. Store it as dates, \
                         or as date-times, before plotting; gog reads the calendar from the \
                         column's type."
                    ),
                });
            }
        }

        // The one scale whose refusal cannot be written once, because what
        // *removing it* leaves behind depends on the column underneath. On text
        // it leaves the categorical axis you already had, so "remove it" is the
        // whole answer. On a number it leaves the **continuous** axis the caller
        // was trying to escape, so the same sentence sends them in a circle —
        // which is what it did until 2026-07-28, having been written for the text
        // column and then handed to everyone. §12 asks a diagnostic to say what to
        // do, and a direction that does not lead anywhere fails that test as
        // squarely as no direction at all.
        //
        // **On a text column it is now allowed and means nothing extra** (ruled
        // 2026-07-28), which is the allowance `linear` has on a number and `time`
        // has on a date. It was a refusal until then, and that was a Law-2
        // exception hiding in plain sight: three scales chosen from the column's
        // type, two of them sayable out loud, one refused. The comment on the
        // `Time` arm above even names the parallel — *"the time scale comes from
        // the column's type, like `category` does"* — and the code then broke it
        // two arms later.
        //
        // **On anything else it is Illegal rather than Unsupported**, and that is
        // a ruling and not an inability (spec §18). A scale refines *how* a
        // measured column is placed; **which kind of axis you get is the column's
        // type**, and every other cell of this match already enforces exactly
        // that: `log` on text, `linear` on a date, `time` on a number. Category on
        // a number is the fourth cell of that table and was the only one filed as
        // "not built yet". Two further reasons, both in §18: a category position
        // is a *rank over the whole column*, which is transform-shaped rather than
        // scale-shaped (§10 opens by drawing that very line); and nothing is lost,
        // since one slot per distinct value is a text column and cutting a number
        // into ranges is `bin`.
        //
        // **Each direction names one edit that renders**, which is the lesson of
        // the 2026-07-28 surface refusal. Converting the column is only half the
        // edit — a reader who does exactly that and keeps `scale = "category"`
        // used to land in the text branch and be refused a second time, for a new
        // reason, having done what they were told. That trap is gone now that text
        // is allowed, but the directions still say it, because they should read
        // the same whichever way that ruling later goes.
        ScaleType::Category => {
            // The axis a transform writes does not carry this column's values, so
            // the column's type says nothing about what lands there — and advice
            // about converting it would be advice about the wrong numbers.
            if !values_are_the_input {
                out.push(Diagnostic {
                    kind: DiagnosticKind::Illegal,
                    message: format!(
                        "gog: `{c}({field}, scale = \"category\")` — `{c}` carries what the \
                         transform computed, not `{field}`, and a computed number has no \
                         distinct values to give slots to. Remove the scale."
                    ),
                });
            } else if df.time_unit(field).is_some() {
                // A date carries a distinct moment per row, so one slot per
                // distinct value is one slot per row. The column has to be
                // coarsened to the period actually meant before it is a category
                // at all — the sentence `facet` gives a date column.
                out.push(Diagnostic {
                    kind: DiagnosticKind::Illegal,
                    message: format!(
                        "gog: `{c}({field}, scale = \"category\")` — `{field}` is a date column, \
                         and one slot per distinct moment is one slot per row. Format it as the \
                         period you mean, the year or the month, and drop the scale: a text \
                         column gets a categorical axis from its type."
                    ),
                });
            } else if actual == VarType::Continuous {
                out.push(Diagnostic {
                    kind: DiagnosticKind::Illegal,
                    message: format!(
                        "gog: `{c}({field}, scale = \"category\")` — a scale says how a measured \
                         column is placed; whether an axis measures at all is the column's type. \
                         Removing the scale alone would leave the continuous axis you were trying \
                         to escape. Store `{field}` as text or categories before plotting, and \
                         drop the scale. To cut the numbers into ranges instead, use `bin`."
                    ),
                });
            }
        }

        ScaleType::Log => {
            // log base 1 is a division by zero and a base at or below zero has
            // no logarithm at all. `scale::log_base` falls back to 10 so the
            // renderer cannot emit infinities, but falling back silently would
            // draw a picture nobody asked for.
            if let Some(b) = def.base {
                if !b.is_finite() || b <= 1.0 {
                    out.push(Diagnostic {
                        kind: DiagnosticKind::Illegal,
                        message: format!(
                            "gog: `{c}({field}, base = {b})` is not a base a logarithm can have — \
                             it must be greater than 1. Use 10 (the default), 2 for doublings, \
                             or `exp(1)` for e-foldings."
                        ),
                    });
                    return;
                }
            }
            if actual == VarType::Discrete {
                out.push(Diagnostic {
                    kind: DiagnosticKind::Illegal,
                    message: format!(
                        "gog: `{c}({field}, scale = \"log\")` needs a number to take the \
                         logarithm of, but `{field}` is text. A text column already gets a \
                         categorical axis — remove `scale = \"log\"`."
                    ),
                });
                return;
            }
            // A moment in time is a point on an interval scale: its zero —
            // the epoch — is an arbitrary convention, and a logarithm measured
            // from an arbitrary zero measures nothing.
            if df.time_unit(field).is_some() {
                out.push(Diagnostic {
                    kind: DiagnosticKind::Illegal,
                    message: format!(
                        "gog: `{c}({field}, scale = \"log\")` — `{field}` is a date column, and \
                         a moment in time has no logarithm: the calendar's zero is an arbitrary \
                         origin. Log the measured axis instead, or remove the scale."
                    ),
                });
                return;
            }
            if !values_are_the_input {
                return;
            }
            if let Some(u) = crate::scale::unplaceable(df, field) {
                out.push(Diagnostic {
                    kind: DiagnosticKind::Illegal,
                    message: format!(
                        "gog: `{c}({field}, scale = \"log\")` has no place for {} of {} rows — a \
                         logarithm is undefined at zero and below, and `{field}` reaches {}. \
                         Filter those rows before plotting, or use a linear scale.",
                        u.count, u.total, u.smallest,
                    ),
                });
            }
        }
    }
}

/// Check a stated domain against the channel and column it is stated on (§10).
///
/// The row-counting half is `check_limit_rows`, which needs the frame and the
/// whole layer; this half needs only the binding, and answers the three
/// questions that make a domain meaningless before any row is read.
/// **`tick_count` reaches a narrower set than `limits`, and the line between them
/// is the point** (spec §10). Both describe the scale, so both ride the binding —
/// but `limits` states a **domain**, which every magnitude channel has, while this
/// states how many ticks an **axis** gets, and only the three positions draw one.
///
/// A legend is not a short axis. It names three rows structurally, both ends and
/// the middle, and on a log scale that middle is the geometric mean √(min·max)
/// rather than the midpoint — so its rows are derived from the scale's shape, not
/// chosen. A denser color bar is a real and drawable thing; it is a property of
/// the *legend*, which is furniture, so it would arrive through `theme()` and not
/// here. Refused with that direction rather than accepted and ignored.
/// `decimals` rounds the numbers a channel prints: `label(life, decimals = 1)`.
///
/// Only `label` prints a column's numbers as text of its own today, so the
/// setting is said not to be drawn anywhere else (an axis and a key choose their
/// own clean values), a column of text has no number to round, and more places
/// than a measurement carries is a mistake about the number rather than a style.
pub(super) fn check_decimals(
    out: &mut Vec<Diagnostic>,
    def: &ChannelDef,
    c: &str,
    field: &str,
    actual: Option<VarType>,
    channel: &Channel,
) {
    let Some(d) = def.decimals else { return };
    if *channel != Channel::Label {
        out.push(Diagnostic {
            kind: DiagnosticKind::Unsupported,
            message: format!(
                "gog: `{c}({field}, decimals = {d})`: gog rounds the numbers it prints only \
                 on `label()` today, and `{c}` prints its numbers on an axis or a key, which \
                 choose their own. Leave `decimals` out."
            ),
        });
        return;
    }
    if actual == Some(VarType::Discrete) {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `label({field}, decimals = {d})`: `{field}` is a categorical column, \
                 read as names rather than numbers, so there is no number to round. Leave \
                 `decimals` out. If its values are numbers, make the column numeric in the \
                 host language first."
            ),
        });
        return;
    }
    if d > 10 {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `label({field}, decimals = {d})`: a measurement does not carry {d} \
                 decimal places. Write a number from 0 to 10."
            ),
        });
    }
}

/// `short` writes a large number short, with K, M, B or T:
/// `label(population, short = TRUE)` prints 3.81B.
///
/// Judged as `decimals` is, and for its reasons: only `label` prints a column's
/// numbers as text of its own today, and a column of text has no number to write
/// short. Only a `short` asked for is judged. Set to false it is the default, and
/// there is nothing to drop.
pub(super) fn check_short(
    out: &mut Vec<Diagnostic>,
    def: &ChannelDef,
    c: &str,
    field: &str,
    actual: Option<VarType>,
    channel: &Channel,
) {
    if def.short != Some(true) {
        return;
    }
    if *channel != Channel::Label {
        out.push(Diagnostic {
            kind: DiagnosticKind::Unsupported,
            message: format!(
                "gog: `{c}({field})` with `short` set to true: gog writes numbers short only \
                 on `label()` today, and `{c}` prints its numbers on an axis or a key, which \
                 choose their own. Leave `short` out."
            ),
        });
        return;
    }
    if actual == Some(VarType::Discrete) {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `label({field})` with `short` set to true: `{field}` is a categorical \
                 column, read as names rather than numbers, so there is no number to write \
                 short. Leave `short` out. If its values are numbers, make the column \
                 numeric in the host language first."
            ),
        });
    }
}

pub(super) fn check_tick_count(
    out: &mut Vec<Diagnostic>,
    def: &ChannelDef,
    c: &str,
    field: &str,
    actual: VarType,
    channel: &Channel,
) {
    let Some(n) = def.tick_count else { return };

    if !matches!(channel, Channel::X | Channel::Y | Channel::Z) {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `tick_count` on `{c}({field})`: `{c}` has no axis, so there are no \
                 ticks to count. Put `tick_count` on a position, `x`, `y` or `z`, or drop it."
            ),
        });
        return;
    }

    // A category axis has one slot per level: the levels *are* the ticks, so a
    // count would have to invent or hide one. `order()` is the atom for choosing
    // which appear in what sequence, and filtering is the atom for choosing which
    // appear at all — the same two directions `limits` gives on a category.
    if actual == VarType::Discrete {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `{c}({field}, tick_count = {n})` — `{field}` is text, and a categorical \
                 axis has one tick per category, so the count is the data's rather than \
                 yours. To change which categories appear, filter the table before plotting; \
                 to change their order, use `order({field})`."
            ),
        });
        return;
    }

    // Two is the fewest that describes an axis: one tick shows a position and no
    // direction, and zero is an axis with no scale on it. Reported rather than
    // clamped, because a `tick_count = 1` is a mistake about what a tick is and
    // silently drawing five would leave the caller believing otherwise.
    if n < 2 {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `{c}({field}, tick_count = {n})` — an axis needs at least two ticks to \
                 show a direction as well as a place. Ask for 2 or more, or leave \
                 `tick_count` off for the default of 5."
            ),
        });
    }
}

pub(super) fn check_limits(
    out: &mut Vec<Diagnostic>,
    def: &ChannelDef,
    c: &str,
    field: &str,
    actual: VarType,
    scaled_channel: bool,
) {
    let Some([lo, hi]) = def.limits else { return };
    if lo.is_none() && hi.is_none() {
        // `limits = c(NA, NA)` says nothing, which is what saying nothing says.
        return;
    }

    // Same reason a scale is refused there: `shape` and `group` answer *which
    // one?*, and there is no distance between a circle and a square for a
    // domain to be an interval of.
    if !scaled_channel {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `limits` on `{c}({field})`: `{c}` distinguishes categories rather than \
                 measuring them, so there is no range along it for limits to cut. Drop \
                 `limits`."
            ),
        });
        return;
    }

    // ggplot2 gives discrete limits a second meaning — select and reorder the
    // categories — which is one word doing two jobs, told apart only by the
    // column's type. gog has an atom for that job already, so this points at it
    // rather than growing the word (spec §10, §13).
    if actual == VarType::Discrete {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `{c}({field}, limits = …)` — `{field}` is text, and a category has no \
                 range to lie inside. To choose which categories appear, filter the table \
                 before plotting; to change the order they appear in, use `order({field})`."
            ),
        });
        return;
    }

    if let (Some(l), Some(h)) = (lo, hi) {
        if !(l < h) {
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: format!(
                    "gog: the `limits` of `{c}({field})`, {l} to {h}, run backwards or have no \
                     width — the first number is the low end. Write {} to {}, or leave one end \
                     missing to let the data decide it.",
                    l.min(h), l.max(h),
                ),
            });
            return;
        }
    }

    // A logarithm is undefined at zero and below, so an end there is an end the
    // axis cannot reach. Refused for the same reason a base ≤ 1 is: falling back
    // silently would draw a range nobody asked for.
    if matches!(def.scale.as_ref(), Some(ScaleType::Log)) {
        for (which, end) in [("lower", lo), ("upper", hi)] {
            if let Some(v) = end {
                if v <= 0.0 {
                    out.push(Diagnostic {
                        kind: DiagnosticKind::Illegal,
                        message: format!(
                            "gog: `{c}({field}, scale = \"log\", limits = …)` puts its {which} end \
                             at {v}, and a logarithm is undefined at zero and below. Use a \
                             positive end — on a log axis the bottom is a small number, never 0."
                        ),
                    });
                    return;
                }
            }
        }
    }
}

/// What a layer's stated domains actually cut, said out loud (§10, §12).
///
/// The rule is not *never drop a row* but *never drop one in silence*, and a
/// limit is the one place where dropping is the instruction rather than a
/// surprise — which is why this is an Assumption where the same condition under
/// `scale = "log"` is a refusal. Taking **every** row is the exception: that is
/// an empty panel with fabricated axes, the failure §12 has been burned by three
/// times, so it is fatal and names the range the column actually has.
pub(super) fn check_limit_rows(out: &mut Vec<Diagnostic>, spec: &PlotSpec, df: &DataFrame, layer: &Layer) {
    let cut = limit_cut(spec, layer, df);
    if cut.is_empty() {
        return;
    }
    let fatal = cut.takes_everything();
    for (channel, field, excluded) in &cut.cuts {
        let c = channel_name(channel);
        let def = binding_of(spec, layer, channel);
        // The guarded read: a malformed pair cut nothing, so it is not reported
        // on here — `check_limits` has already named it as the typo it is.
        let (lo, hi) = crate::scale::domain_of(def);
        // A temporal domain is quoted back as dates. The caller wrote
        // `as.Date("2024-03-01")`; answering with 1709251200 tells them nothing
        // they can act on, which is the whole of what §12 asks a message for.
        // `fmt_moment` is the same one the legends use, and says so.
        let unit = df.time_unit(field);
        let end = |v: Option<f64>| match (v, unit) {
            (Some(v), Some(u)) => crate::time::fmt_moment(v, u),
            (Some(v), None) => format!("{v}"),
            (None, _) => "…".to_string(),
        };
        let shown = format!("[{}, {}]", end(lo), end(hi));
        if fatal {
            let span = df.float_col(field).map(|col| {
                let mn = col.iter().copied().filter(|v| v.is_finite()).fold(f64::INFINITY, f64::min);
                let mx = col.iter().copied().filter(|v| v.is_finite()).fold(f64::NEG_INFINITY, f64::max);
                format!("{} to {}", end(Some(mn)), end(Some(mx)))
            });
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: format!(
                    "gog: `{c}({field}, limits = …)` leaves no rows at all — {} does not meet \
                     {shown}, so the panel would be empty with axes drawn over nothing. Widen \
                     the limits, or remove them.",
                    span.map(|s| format!("`{field}` runs {s}, which"))
                        .unwrap_or_else(|| format!("`{field}`")),
                ),
            });
        } else {
            out.push(Diagnostic {
                kind: DiagnosticKind::Assumption,
                message: format!(
                    "gog: `{c}({field}, limits = …)` excludes {excluded} of {} rows, which fall \
                     outside {shown}. Stating a domain is what removes them — widen the limits \
                     if they should be drawn.",
                    cut.total,
                ),
            });
        }
    }
}

/// Report where a plot-scoped channel lands, when it does not land everywhere.
///
/// Plot scope reaches only the marks that have the feature, which is what makes
/// `size(population) + line + point` usable at all. But a binding that quietly
/// applies to some layers and not others is exactly the kind of silence the
/// grammar refuses, so the skip is said out loud: an Assumption when it still
/// reaches something, and Illegal when it reaches nothing.
/// Does this channel earn a key? The five that `render::legend` draws one for,
/// and only those: the line §2 draws between an aesthetic channel and `group`.
pub(super) fn earns_a_key(channel: &Channel) -> bool {
    matches!(
        channel,
        Channel::Color | Channel::Size | Channel::Shape | Channel::Pattern | Channel::Opacity
    )
}

/// `legend = ` — whether a channel's key is drawn (spec §10, "A legend turned off").
///
/// Two refusals, and neither needs a table, so this reads the sentence as written,
/// before scopes are resolved: a plot-scoped binding copied into three layers is
/// one statement and is answered once.
///
/// **On a channel with no key there is nothing to turn off**, and accepting the
/// word there would be accepting it and dropping it. A position is read from its
/// axis; `group` splits without encoding, which is exactly what makes it earn no
/// key; `label` is read where it is written; and a `play` sequence names each
/// frame in its strip. Each gets its own reason, and all four the same direction.
///
/// **Two bindings of one channel that disagree are refused rather than ranked.**
/// The key is the channel's, one for the plot, so `legend = FALSE` on one binding
/// already leaves it out and saying nothing elsewhere does not bring it back. An
/// explicit `legend = TRUE` beside it is the one case where the sentence says two
/// things about one key, and picking either would silently overrule the other.
pub(super) fn check_legend(out: &mut Vec<Diagnostic>, spec: &PlotSpec) {
    let mut written: Vec<(&Channel, &ChannelDef)> = Vec::new();
    for (channel, def) in [(&Channel::X, &spec.x), (&Channel::Y, &spec.y), (&Channel::Z, &spec.z)] {
        if let Some(def) = def {
            written.push((channel, def));
        }
    }
    written.extend(spec.channels.iter());
    for layer in &spec.layers {
        written.extend(layer.encodings.iter());
    }

    // Collected and sorted before they are pushed: the bindings sit in hash maps,
    // and two runs of one sentence must print one message in one order.
    let mut messages: Vec<String> = Vec::new();
    for (channel, def) in &written {
        if def.legend.is_none() { continue }
        if earns_a_key(channel) {
            continue;
        }
        let c = channel_name(channel);
        let field = &def.field;
        let why = match channel {
            Channel::X | Channel::Y | Channel::Z => format!(
                "`{c}` is read off its axis, so there is no legend to show or leave out."
            ),
            Channel::Group => "`group` splits the rows without encoding anything, which is \
                 why it never draws a legend."
                .to_string(),
            Channel::Label => "`label` is the text a `text` mark writes, read where it is \
                 written, so it draws no legend."
                .to_string(),
            Channel::Play => "`play` names each frame in the strip above the panel as it \
                 shows it, so it draws no legend."
                .to_string(),
            _ => format!("`{c}` draws no legend."),
        };
        messages.push(format!(
            "gog: `{c}({field})` is given `legend` — {why} `legend` belongs on the five \
             channels that draw one: `color`, `size`, `shape`, `pattern` and `opacity`."
        ));
    }

    for channel in [Channel::Color, Channel::Size, Channel::Shape, Channel::Pattern, Channel::Opacity] {
        let hidden = written.iter().find(|(c, d)| **c == channel && d.legend == Some(false));
        let shown = written.iter().find(|(c, d)| **c == channel && d.legend == Some(true));
        if let (Some((_, off)), Some((_, on))) = (hidden, shown) {
            let c = channel_name(&channel);
            messages.push(format!(
                "gog: `{c}({})` leaves its legend out and `{c}({})` shows it, which are \
                 opposite things, and a plot draws one `{c}` legend. Keep one of them.",
                off.field, on.field,
            ));
        }
    }

    messages.sort();
    messages.dedup();
    out.extend(messages.into_iter().map(|message| Diagnostic { kind: DiagnosticKind::Illegal, message }));
}

/// `axis = ` — whether a position's axis is drawn (spec §10, "An axis turned off").
///
/// [`check_legend`] one guide over, read the same way: as written, before scopes
/// are resolved, so a plot-scoped binding is one statement answered once. Four
/// refusals, and the first and third are the legend's own.
///
/// **On a channel with no axis there is nothing to turn off.** Only the three
/// positions draw one. The five refining channels are read from a legend, which
/// has its own word, and `group`, `label` and `play` draw no guide that is an
/// axis.
///
/// **A space that draws no axes refuses it as well**, on the rule the globe
/// already keeps for `limits` and `tick_count`: nothing may shape or name an
/// axis that is not there. A globe, a packing and a network have none to leave
/// out, and accepting the word there would be accepting it and doing nothing.
///
/// **Two bindings of one axis that disagree are refused rather than ranked.** The
/// axis is shared by every layer, so `axis = FALSE` on one binding already leaves
/// it out; an explicit `axis = TRUE` beside it says two things about one axis.
///
/// **And an axis left out cannot be named.** The name is one of the three things
/// `axis = FALSE` removes, so `x_label()` beside it asks for ink the same sentence
/// takes away. Refused, as `nest()` refuses a name for axes it does not have,
/// rather than drawn against the request or dropped in silence.
pub(super) fn check_axis(out: &mut Vec<Diagnostic>, spec: &PlotSpec) {
    let mut written: Vec<(&Channel, &ChannelDef)> = Vec::new();
    for (channel, def) in [(&Channel::X, &spec.x), (&Channel::Y, &spec.y), (&Channel::Z, &spec.z)] {
        if let Some(def) = def {
            written.push((channel, def));
        }
    }
    written.extend(spec.channels.iter());
    for layer in &spec.layers {
        written.extend(layer.encodings.iter());
    }

    let axisless = match spec.coord {
        CoordSpace::Globe(_) => Some(
            "a `globe()` plot draws none: a position there is a place on the sphere, and \
             the graticule is the reference",
        ),
        CoordSpace::Nest => Some(
            "a `nest()` packing has none: its two directions carry no variable",
        ),
        CoordSpace::Network(_) => Some(
            "a network has none: its positions are the layout's, not measurements",
        ),
        _ => None,
    };

    // Sorted before they are pushed, as `check_legend`'s are: the bindings sit in
    // hash maps, and one sentence must print one message in one order.
    let mut messages: Vec<String> = Vec::new();
    for (channel, def) in &written {
        if def.axis.is_none() { continue }
        let c = channel_name(channel);
        let field = &def.field;
        if !matches!(channel, Channel::X | Channel::Y | Channel::Z) {
            let why = match channel {
                Channel::Group => "`group` splits the rows without encoding anything, so it \
                     draws no guide at all."
                    .to_string(),
                Channel::Label => "`label` is the text a `text` mark writes, read where it is \
                     written."
                    .to_string(),
                Channel::Play => "`play` names each frame in the strip above the panel."
                    .to_string(),
                _ => format!(
                    "`{c}` is read from its legend, and `{c}({field})` with its `legend` set to \
                     false is how a legend is left out."
                ),
            };
            messages.push(format!(
                "gog: `{c}({field})` is given `axis`, and `{c}` draws no axis: {why} `axis` \
                 belongs on the three positions, `x`, `y` and `z`."
            ));
            continue;
        }
        if let Some(why) = axisless {
            messages.push(format!(
                "gog: `{c}({field}, axis = )` turns an axis on or off, and {why}. Drop `axis`."
            ));
        }
    }

    for channel in [Channel::X, Channel::Y, Channel::Z] {
        let hidden = written.iter().find(|(c, d)| **c == channel && d.axis == Some(false));
        let shown = written.iter().find(|(c, d)| **c == channel && d.axis == Some(true));
        if let (Some((_, off)), Some((_, on))) = (hidden, shown) {
            let c = channel_name(&channel);
            messages.push(format!(
                "gog: `{c}({})` leaves its axis out and `{c}({})` draws it, which are \
                 opposite things, and a plot draws one `{c}` axis. Keep one of them.",
                off.field, on.field,
            ));
        }
    }

    // A space with no axes already refuses every name, in its own words.
    if axisless.is_none() {
        for (channel, label, atom) in [
            (Channel::X, &spec.x_axis.label, "x_label"),
            (Channel::Y, &spec.y_axis.label, "y_label"),
            (Channel::Z, &spec.z_axis.label, "z_label"),
        ] {
            if label.is_none() { continue }
            let Some((_, off)) = written.iter().find(|(c, d)| **c == channel && d.hides_axis())
            else { continue };
            let c = channel_name(&channel);
            messages.push(format!(
                "gog: `{atom}()` names the {c} axis, and `{c}({})` with its `axis` set to \
                 false leaves that axis out, its name included. Keep one of them: drop \
                 `{atom}()`, or set `axis` to true to draw the axis with its name.",
                off.field,
            ));
        }
    }

    messages.sort();
    messages.dedup();
    out.extend(messages.into_iter().map(|message| Diagnostic { kind: DiagnosticKind::Illegal, message }));
}
