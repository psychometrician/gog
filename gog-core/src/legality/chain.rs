//! How transforms compose on one layer, and what each one needs of the column it
//! reads.

use super::*;

/// The **two-dimensional group-by** (spec §5): the five reductions read over a *pair*
/// of keys, on the marks whose measurement is not either of them.
///
/// A value statistic groups by every position the mark does **not** measure with, and
/// reduces the column named on the one it does — writing the answer back into it,
/// because a reduction is *in place*. That is §5's subtraction read a second time, and
/// nothing about it is new except running it where the remainder is two:
///
/// | mark | positions | measures with | groups by | reduces |
/// |---|---|---|---|---|
/// | `bar`, flat | x, y | `y` | x | y |
/// | `bar`, `space` | x, y, z | `z` | **x, y** | z |
/// | `zone` | x, y | `color` | **x, y** | color |
///
/// So this check asks the three things the pair reading needs and the one-key reading
/// got for free from `slot_orient`: that the **measure channel names a column** (there
/// is no `y` to fall back on when the measurement rides `color`), that the column is
/// **numeric** (it is reduced, not counted), and that **both positions are
/// categorical** (a cell is a slot a category owns — a number is a point, and there is
/// nothing to cut here, since these five measure without cutting).
///
/// Data-aware, so it sits in the df-gated block beside `check_zone_extent`, which
/// defers its own "where are the sides?" refusal to this one for the reason a tally
/// does: a transform that measures into cells it did not cut can only mean slots, and
/// two refusals for one mistake is worse than the right one.
pub(super) fn check_pair_summary(out: &mut Vec<Diagnostic>, spec: &PlotSpec, df: &DataFrame, layer: &Layer) {
    let space = space_of(spec);
    // Not a two-dimensional reading, or not one of the five — the one-key form, which
    // `check_slot_shape` and `check_summary` between them already judge.
    if !cuts_both_positions(&layer.mark, space) {
        return;
    }
    let Some(t) = layer.transforms.iter().find(|t| {
        crate::transform::has_reduction(std::slice::from_ref(*t))
    }) else { return };
    // **A mark that refuses the transform has already said so**, in words about
    // itself (`check_marks_that_take_no_transform`). Asking the group-by's questions
    // after that answered a sentence the reader had been told to rewrite: `path *
    // mean` printed a second refusal asking for a `color()` to reduce, and directed
    // the reader to `path * count`, which is refused too. One mistake, one refusal.
    if mark_takes_transform(&layer.mark, t) == TransformLegality::None {
        return;
    }
    let (m, t) = (mark_name(&layer.mark), transform_name(t));
    // Which channel this mark measures with, so the message names the binding the
    // reader must write rather than a generic "the measure". `Some` because
    // `cuts_both_positions` is true above.
    let Some(ch) = measure_channel(&layer.mark, space) else { return };
    let c = channel_name(&ch);

    // 0a. A **bounded** zone is one rectangle per row, its sides named by its own
    //     four columns — not a mesh, so there are no cells to group into. That
    //     refusal moved to `check_chain_jobs` on 2026-07-31, sentence intact, because
    //     sitting here it was **mis-scoped**: this whole function is gated on
    //     `cuts_both_positions` above, so `interval * bounds(lo, hi) * mean` never
    //     reached it and drew in silence — the identical mistake this file already
    //     records at 0b, where the composition rule lived inside the two-dimensional
    //     group-by and every one-key reading walked past it.
    if layer.transforms.contains(&Transform::Bounds) {
        return;
    }

    // 0b. Two transforms cannot both be the cell's measurement — unless one of them
    //    is only describing an **extent**, which is `check_chain_jobs`'s
    //    question and is asked of every mark in every dimension rather than here.
    //    This check ran that refusal until 2026-07-26, which is why the same mistake
    //    was fatal on a `zone` and silent on a `bar`: the refusal lived inside the
    //    *two-dimensional* group-by, and the one-key reading never reached it.
    //
    //    `bin` deliberately falls through to the three questions below. Composed, it
    //    supplies the cells and this statistic supplies their measurement — so the
    //    column still has to be named (1) and still has to be a number (2), and only
    //    the *both positions categorical* question (3) is answered differently,
    //    because a cut axis owns cells too.
    if crate::transform::measures_cells(&layer.transforms)
        && !layer.transforms.contains(&Transform::Bin)
    {
        return;
    }

    // 1. The column to reduce. On the plane a value statistic never had to ask: `y`
    //    is required by every mark that takes one. Here the measurement rides a
    //    channel that is optional — `color` on a zone, `z` on a bar — and accepting
    //    the layer without it would reduce nothing and draw an empty mesh, which is
    //    the §12 drop.
    let Some(field) = measure_field(spec, layer) else {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `{m} * {t}` reduces a column within each cell, but nothing says which \
                 column — `{m}` measures by `{c}`, and no `{c}()` is bound. Name it: \
                 `{m} * {t} + x(<a>) + y(<b>) + {c}(<column>)`. To count the rows in each \
                 cell instead of reducing a column, `count` needs no such binding: \
                 `{m} * count + x(<a>) + y(<b>)`."
            ),
        });
        return;
    };

    // 2. It has to be a number, because it is reduced rather than counted. A legality
    //    question about a column's type, so it is asked here and fatally (spec §12).
    if df.float_col(field).is_none() && !df.is_empty() {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `{m} * {t}` reduces `{c}({field})` within each cell, and `{field}` is \
                 not a numeric column — there is no {t} of a category. Name the column you \
                 want summarized, or count the rows in each cell instead: \
                 `{m} * count + x(<a>) + y(<b>)`."
            ),
        });
        return;
    }

    // 3. Every position must own a cell, because these five **measure without
    //    cutting**: they answer *what is in this cell*, never *where are my cells*.
    //    A category arrives already cut — it owns its slot — and a number does not.
    //    So a continuous axis needs something to cut it, and since 2026-07-26 there
    //    is a transform that does: composed with `bin`, these five keep the
    //    measurement and `bin` keeps the cut (spec §5). Asking whether the layer
    //    cuts rather than listing the marks is what makes the summary heatmap and
    //    the confusion matrix one sentence read on two kinds of axis.
    if layer.transforms.contains(&Transform::Bin) {
        return;
    }
    let loose: Vec<String> = [Channel::X, Channel::Y].into_iter()
        .filter_map(|ch| spec.position_for(layer, &ch).map(|d| (ch, d)))
        .filter(|(_, d)| actual_type(df, &d.field) == Some(VarType::Continuous))
        .map(|(ch, d)| format!("{}({})", channel_name(&ch), d.field))
        .collect();
    if !loose.is_empty() {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `{m} * {t}` summarizes `{field}` inside the cell each pair of \
                 categories owns, and `{}` {} numbers — a number is a point, so it \
                 owns no cell to summarize into. Either put a category on both positions, \
                 or cut the numeric axis into cells first: `{m} * bin * {t} + x(<a>) + \
                 y(<b>) + {c}({field})` bins where your data lives and {t}s `{field}` \
                 inside each cell.",
                loose.join("` and `"),
                if loose.len() > 1 { "both carry" } else { "carries" },
            ),
        });
    }
}

/// **Two transforms that do the same job contradict** (spec §5) — asked of every
/// mark, in every dimension, because the answer is a fact about the transforms
/// rather than about the geometry reading them.
///
/// There are four jobs, and the book names three of them before this check does:
/// a transform says **where the cells are**, says **what is in them**, says **what
/// scale the answer is read on**, or says **where the marks sit**. The frame holds
/// one of each per cell. So a second transform doing a job the first already did is
/// a request the engine can only answer by throwing one of the two away, and
/// throwing one away in silence is the drop §12 forbids.
///
/// `transform::job_conflict` finds the pair; everything below is what to *say* about
/// it. The split is the point: a transform's job is a fact about frames, which is
/// that module's subject, while a refusal is a sentence addressed to a person.
///
/// **Why this is one rule and not four lists.** It was four lists. Three checks each
/// enumerated the family it knew — the five reductions here, `[bin, count, density]`
/// here, `bounds` against a cut over in the zone's own check — and every transform
/// outside those families composed with everything, silently. `range * confidence`
/// drew exactly `confidence`; `sum * range` drew exactly `range`; `smooth * mean`
/// drew exactly `smooth`; `dodge * stack` drew groups both side by side *and* piled.
/// A survey on 2026-07-31 found that of 582 two-transform chains that drew, only 271
/// had every transform doing something. None of those were decisions. They were the
/// cost of a rule written three times instead of once, and a fourth family was going
/// to be forgotten in exactly the same way.
///
/// **The messages stay per-pair.** A generic sentence per job is the fallback, not
/// the goal — a refusal that does not say what to do instead is a §12 failure one
/// level down. Every message names what would have been discarded, which is what
/// makes the refusal appealable: a reader who has a reading in mind can see the plot
/// the engine declined to draw and say so.
///
/// A transform can supply two different things, and §5's division of the nine value
/// statistics into *the four that invent a measurement* and *the five that reduce a
/// named one* hid the second question: which of them also says **where the cells
/// are**. Only `bin` gives its measurement up. It **cuts**, and its tally is a
/// by-product of the cut rather than the cut itself, so it can hand the measurement
/// over and still have something left to contribute — and it hands it only to a
/// transform that was handed a *column*, which is why `bar * bin * mean` composes and
/// `bar * bin * count` does not. The others cannot:
///
/// - `count` and `proportion` tally into cells the **positions** already own (the
///   fourth extent description — a category owns its slot), so taking their
///   measurement away leaves nothing at all.
/// - a `density` cell is a **sample point of an estimate**, which exists *between*
///   the data points rather than partitioning rows, so there are no rows inside one
///   to reduce. It is the same property that makes `bandwidth` a length and `bin`'s
///   width a boundary.
///
/// **Why this is a check and not a renderer branch.** Until 2026-07-26 all four
/// compositions ran, and in one dimension every one of them silently dropped the
/// statistic: `bin` overwrote the named column with its own tally, the reduction then
/// averaged a single count per cell and handed it back unchanged, and the axis was
/// relabeled to the column nobody had read. A histogram under an axis reading
/// `Life`. That is the silent drop §12 forbids, arriving through a *composition*
/// rather than through a binding, and it was invisible to every test because the plot
/// rendered and exited 0.
pub(super) fn check_chain_jobs(out: &mut Vec<Diagnostic>, spec: &PlotSpec, layer: &Layer, mark_refused: bool) {
    use crate::transform::{JobContext, job_conflict};
    let ts = &layer.transforms;
    let m = mark_name(&layer.mark);

    // **No early return merely because the mark does not take a transform.**
    // Deferring on that alone assumed a refusal that does not always exist:
    // `mark_takes_transform(interval, mean)` is `None`, but the only check that spoke
    // up was the *minimum syllable* one — and `interval * bounds * mean` satisfies
    // the syllable, so `mean` went through unrefused and unread. The grid says `none`
    // and the engine drew it anyway. An extra sentence costs a reader one confused
    // moment; a silent drop costs them a wrong plot they believe.
    //
    // So this check stands down only where the refusal **was printed**
    // (`mark_refused`, read off this layer's own diagnostics) and the conflict
    // involves a transform the mark refuses. `box * mean` is the case that forced
    // it: `check_box` says to drop `mean`, and the chain then paired `mean` with the
    // box's own summary and offered `box * mean` or `box * box`, both refused.
    let ctx = JobContext {
        measures_by_color: has_no_measure_axis(&layer.mark),
        stack_shares: layer.stack.as_ref().is_some_and(|s| s.share.unwrap_or(false)),
    };
    let Some((a, b, job)) = job_conflict(ts, ctx) else {
        // **A transform that changes nothing is still a transform nobody read.** Two
        // that do different jobs compose, but one of them can still turn out to be a
        // no-op — and saying so is the same duty as the refusals above, one step
        // softer. It is a warning rather than a refusal because the plot drawn is
        // exactly the plot asked for: nothing was discarded, one atom was simply not
        // needed. Refusing it would forbid the ugly-but-legal (Law 8).
        //
        // The list is short on purpose and grows only as a case is understood well
        // enough to say why in one sentence. A no-op nobody can explain is a refusal,
        // not a warning — a plot that quietly means something other than it says is
        // the more expensive mistake, and a refusal can be relaxed later while a
        // drawing people have built on cannot be taken back.
        if ts.contains(&Transform::Count) && ts.contains(&Transform::Proportion) {
            out.push(Diagnostic {
                kind: DiagnosticKind::Assumption,
                message: format!(
                    "gog: `{m} * count * proportion` draws the same plot as \
                     `{m} * proportion` — a share is a share *of* a tally, so \
                     `proportion` already counts the rows and `count` adds nothing. \
                     Drop it, or keep `{m} * count` on its own to read the tallies \
                     themselves rather than their shares."
                ),
            });
        }
        return;
    };
    if mark_refused
        && [&a, &b].iter().any(|t| mark_takes_transform(&layer.mark, t) == TransformLegality::None)
    {
        return;
    }
    let (an, bn) = (transform_name(&a), transform_name(&b));

    // Which binding names the column, so the way out is a sentence the reader can
    // type. A mark that reads over two positions measures with a channel the reader
    // has to bind (`color` on a zone, `z` in the cube); a one-key mark measures along
    // its own y, which `check_summary` already requires by name, so naming it twice
    // would be the restating-a-rule-from-elsewhere defect the refusal audit found.
    let bind = measure_channel(&layer.mark, space_of(spec))
        .map(|ch| format!(" + {}(<column>)", channel_name(&ch)))
        .unwrap_or_default();

    let message = chain_message(&layer.mark, m, job, (&a, an), (&b, bn), &bind, ts);
    out.push(Diagnostic { kind: DiagnosticKind::Illegal, message });
}

/// What to say about a pair of transforms that do the same job.
///
/// Ordered most specific first: the pairs that earned their own sentence keep it, and
/// the per-job fallback catches the rest. The fallback is what makes this a rule — a
/// transform added next year collides correctly on the day it arrives instead of
/// composing silently until somebody notices, which is what happened to the six that
/// were outside every check until 2026-07-31.
pub(super) fn chain_message(
    mark: &Mark,
    m: &str,
    job: Job,
    (ta, a): (&Transform, &str),
    (tb, b): (&Transform, &str),
    bind: &str,
    // The layer's whole chain, for the one message that has to offer a sentence
    // with a transform it does not name in its head (`Job::Scale`, below).
    ts: &[Transform],
) -> String {
    use crate::transform::is_reduction;
    let has = |t: &Transform| ta == t || tb == t;
    // The transform that is *not* the one a bespoke message names.
    let other = |t: &Transform| if ta == t { b } else { a };

    // **One transform written twice** is the same job asked for twice, and every
    // sentence below was written for two different transforms: `mean * mean` was
    // offered "`bar * mean` or `bar * mean`", `count`, `bin` and `density` the same
    // twice over, and `proportion * proportion` was told about
    // `stack(share = TRUE)`, which it never wrote.
    if ta == tb {
        return format!(
            "gog: `{m} * {a} * {a}` names `{a}` twice. Each transform does its job once, \
             so one of the two would be discarded. Keep one: `{m} * {a}`."
        );
    }

    // **The three whole-picture layouts are refused in their own terms.** Each
    // claims every job (`transform::jobs`), so a second transform collides with it
    // on whichever job that one fills, and the per-job sentences below then
    // describe cells and piles a layout does not have: `flow * proportion` was told
    // that `flow` divides the measurement into shares of the whole plot, `flow *
    // mean` that its cells are the slots the positions own, `layout * jitter` that
    // it decides where colliding groups go.
    if let Some(t) = [Transform::Flow, Transform::Layout, Transform::Cluster]
        .iter()
        .find(|t| has(t))
    {
        let l = transform_name(t);
        let o = other(t);
        return format!(
            "gog: `{m} * {a} * {b}` gives `{l}` a second transform, and `{l}` takes none \
             — it places every mark in the picture and measures what it places, so there \
             is nothing left for `{o}` to compute or move, and one of the two would be \
             discarded. Keep `{m} * {l}(...)` on its own. To show a value computed from \
             the same rows, compute it where your data lives and give it a layer of its \
             own."
        );
    }

    // `smooth` fits a curve of one column against another, and LOESS already averages
    // locally along that curve — so cutting the domain into cells first buys it
    // nothing it was not doing, and the sentence reads as two answers to one
    // question. (In two dimensions §5 refuses it a different way, on a floor having
    // no left to right; here there is a domain, and the redundancy is what rules.)
    // `smooth * proportion` is refused too but not here: a normalizer asks no second
    // question, so its reason is its own and `check_share_composition` gives it.
    // `smooth_band` is `smooth`'s fit with its band, and collides for the same reason.
    if let Some(sm) = [Transform::Smooth, Transform::SmoothBand].iter().find(|t| has(t)) {
        let s = transform_name(sm);
        let o = other(sm);
        let did = if is_reduction(if ta == sm { tb } else { ta }) {
            format!("reducing them with `{o}` as well")
        } else {
            "cutting them into cells first".to_string()
        };
        return format!(
            "gog: `{m} * {a} * {b}` asks one question twice — `{s}` fits a \
             curve through the rows and already averages locally as it goes, so \
             {did} changes nothing it was not doing. Keep \
             whichever you meant: `{m} * {s} + x(<a>) + y(<b>)` for the fitted \
             curve, or `{m} * {o}` for the shape `{o}` measures. For a summary per \
             cell rather than a fitted curve, name the statistic: \
             `{m} * bin * mean + x(<a>) + y(<b>)`."
        );
    }

    // A `zone` names its own sides. Named *and* cut says where they are twice, and
    // the two would disagree the moment the mesh moved.
    if job == Job::Extent && has(&Transform::Bounds) && has_no_measure_axis(mark) {
        let t = other(&Transform::Bounds);
        return format!(
            "gog: `zone * {a} * {b}` says where the sides are twice — `bounds` names \
             them from columns you have, `{t}` cuts them from the data. Keep whichever \
             you meant: `zone * bounds(...)` to shade a rectangle you chose, \
             `zone * {t}` to tile the panel with measured cells."
        );
    }

    // A **bounded** zone is one rectangle per row, its sides named by its own four
    // columns — not a mesh, so there are no cells to group into. A reduction needs
    // cells the *positions* make, which is the extent description `bounds` replaces
    // rather than supplies. Said in `bounds`' own terms, because the reader who wrote
    // it was shading a region they chose, not summarizing.
    //
    // This sentence was mis-scoped rather than missing until 2026-07-31: it lived
    // behind `cuts_both_positions` in `check_pair_summary`, so `interval * bounds *
    // mean` never reached it and drew in silence. The identical mistake, and the
    // identical fix, that this file already records for the composition rule at large.
    if has(&Transform::Bounds) && (is_reduction(ta) || is_reduction(tb)) {
        let t = other(&Transform::Bounds);
        let c = measure_channel(mark, SpaceKind::Flat).map(|ch| channel_name(&ch)).unwrap_or("y");
        return format!(
            "gog: `{m} * {a} * {b}` says what this rectangle is twice — `bounds` names \
             its sides from columns you hold, one rectangle per row, and `{t}` \
             summarizes a column within the cells your *positions* make. Keep whichever \
             you meant: `{m} * bounds(...)` to shade a region you chose, or \
             `{m} * {t} + x(<a>) + y(<b>) + {c}(<column>)` to summarize one within every \
             cell two categories cross. To shade a band the data computed, \
             `ribbon * range` is the mark that spans a statistic."
        );
    }

    match job {
        // Two members of the aggregation family each reduce *the column you named*,
        // and a cell still holds one number. Neither was handed a different column to
        // give way to, so the tie `bin` breaks has nothing to break here.
        Job::Measure if is_reduction(ta) && is_reduction(tb) => format!(
            "gog: `{m} * {a} * {b}` measures each cell twice — `{a}` and `{b}` both \
             reduce the column you named, and a cell holds one number, so there is no \
             reading that keeps both. Keep whichever you meant: `{m} * {a}` or \
             `{m} * {b}`. To show two summaries of one column, draw them as two \
             layers: `{m} * {a} + {m} * {b}`."
        ),
        // Two transforms that each invent their own measurement from the rows, with
        // neither side handed a column, so the tie cannot be broken the way the one
        // below is.
        Job::Measure if !reads_a_column(ta) && !reads_a_column(tb) => format!(
            "gog: `{m} * {a} * {b}` measures each cell twice — `{a}` and `{b}` each \
             invent their own measurement from the rows, and neither was handed a \
             column to give way to, so there is no reading that keeps both. Keep \
             whichever you meant: `{m} * {a}` or `{m} * {b}`. To cut an axis into \
             cells and measure something else inside them, the second transform has \
             to be one you hand a column: `{m} * bin * mean + x(<number>) + \
             y(<column>)`. To read either as shares of the whole rather than as \
             counts, `proportion` rescales whichever you keep: `{m} * {a} * proportion`."
        ),
        // One that supplies no extent, against one that was handed a column. Each
        // synthesizer gets its own reason, because they are not refused for the same
        // one — and a message that restates a rule from elsewhere instead of this one
        // is the defect the 2026-07-26 refusal audit went looking for.
        Job::Measure if !reads_a_column(ta) || !reads_a_column(tb) => {
            let (o, t) = if reads_a_column(ta) { (b, a) } else { (a, b) };
            let why = if has(&Transform::Density) {
                format!(
                    "a `density` cell is a point where the estimate was sampled, not a bucket \
                     holding rows — the estimate exists *between* your observations — so there \
                     is nothing inside one for `{t}` to reduce"
                )
            } else {
                format!(
                    "`{o}` supplies only a measurement: its cells are the slots the positions \
                     already own, so with `{t}` measuring them too the cell is measured twice \
                     and `{o}` has nothing left to contribute"
                )
            };
            format!(
                "gog: `{m} * {a} * {b}` measures each cell twice — {why}. Keep whichever you \
                 meant: `{m} * {o}` to measure what `{o}` computes, or `{m} * {t}{bind}` to \
                 reduce the column you name. To cut a continuous axis into cells and reduce a \
                 column inside each, `bin` is the transform that cuts without keeping the \
                 measurement: `{m} * bin * {t} + x(<number>)`."
            )
        }
        // Both were handed a column. The pair transforms land here, and until
        // 2026-07-31 nothing looked: `interval * range * confidence` drew exactly
        // `interval * confidence`, and reversed, exactly `interval * range`.
        Job::Measure => format!(
            "gog: `{m} * {a} * {b}` measures each cell twice — `{a}` and `{b}` both \
             reduce the column you named, and a cell holds one answer, so drawing \
             both would mean drawing one of them and discarding the other. Keep \
             whichever you meant: `{m} * {a}` or `{m} * {b}`. To show both, draw them \
             as two layers: `{m} * {a} + {m} * {b}`."
        ),
        Job::Extent => format!(
            "gog: `{m} * {a} * {b}` says where the cells are twice — `{a}` and `{b}` \
             each carve the panel their own way, and the marks sit in one set of cells, \
             so one of the two would be discarded. Keep whichever you meant: \
             `{m} * {a}` or `{m} * {b}`. To measure something inside cells one of them \
             cuts, name a statistic instead: `{m} * {a} * mean{bind}`."
        ),
        // Two transforms fill this job, `proportion` and `stack(share = TRUE)`, and
        // each divides by a different total, so each is named by what it divides by
        // rather than by where it sits in the sentence: read by position, `bar *
        // stack(share = TRUE) * proportion` gave the whole plot to `stack`. `stack`
        // is named with its argument in all three places, because it fills this job
        // only when filling piles; plain `stack` divides nothing, and a reader who
        // kept `stack` as told drew counts. Each way out is a sentence that draws:
        // `proportion` was also the tally when nothing else measured, so the pile's
        // share has to count in its place.
        Job::Scale => {
            let (a, b) = (a.to_string(), b.to_string());
            // What made the measurement, kept in both sentences in the order written:
            // `bin * mean` as much as `sum`.
            let before: String = ts.iter()
                .filter(|t| {
                    let j = crate::transform::jobs(t, crate::transform::JobContext::default());
                    j.measure || j.extent
                })
                .map(|t| format!(" * {}", transform_name(t)))
                .collect();
            let tally = if before.is_empty() { " * count" } else { before.as_str() };
            format!(
                "gog: `{m} * {a} * {b}`, with the `stack` asked for shares (its `share` \
                 set to true), rescales the measurement twice — `proportion` divides it \
                 into shares of the whole plot and the `stack` into shares of each pile, \
                 and dividing twice does not read as shares of anything. Keep whichever \
                 you meant: `{m}{before} * proportion * stack` for shares of the whole \
                 plot, piled, or `{m}{tally} * stack` with its `share` set to true for \
                 shares within each pile."
            )
        }
        Job::Position => format!(
            "gog: `{m} * {a} * {b}` arranges the same marks twice — `{a}` and `{b}` \
             each decide where colliding groups go, and a mark sits in one place, so \
             one of the two would be discarded. Keep whichever you meant: `{m} * {a}` \
             or `{m} * {b}`. To show both readings, draw them as two plots side by \
             side rather than as one layer."
        ),
    }
}

/// Was this transform handed a column to measure, rather than inventing its own
/// number from the rows? The distinction the measure job turns on — see
/// [`crate::transform::Jobs::reads_a_column`].
pub(super) fn reads_a_column(t: &Transform) -> bool {
    crate::transform::jobs(t, crate::transform::JobContext::default()).reads_a_column
}


/// What a **normalizer** cannot rescale (spec §5).
///
/// `proportion` divides the measurement present by its total, which means it needs
/// the measurement to *be* a total's worth of parts: one number per cell, summing to
/// something a share is a share of. Three kinds of transform fail that, each for its
/// own reason, and each says so in its own words — a message that restates a rule
/// from elsewhere is the defect the 2026-07-26 refusal audit went looking for.
///
/// The five value statistics and `bin`/`count` are all absent from this list, which
/// is the point: `bar * bin * proportion` is the relative-frequency histogram and
/// `bar * sum * proportion` is each slot's share of a summed column. A normalizer
/// composes with everything that leaves one number per cell behind it (Law 1).
pub(super) fn check_share_composition(out: &mut Vec<Diagnostic>, layer: &Layer) {
    let ts = &layer.transforms;
    if !ts.contains(&Transform::Proportion) { return }
    let m = mark_name(&layer.mark);

    // 1. A density is already normalized — it integrates to 1 by construction — and
    //    its cells are sample points rather than parts of a whole, so the sum of its
    //    heights is not a quantity anything is a share *of*. Dividing by it would
    //    hand back a number with no reading at all.
    if ts.contains(&Transform::Density) {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `{m} * density * proportion` has nothing to normalize — a density \
                 already integrates to 1, and its cells are points where the estimate was \
                 sampled rather than parts of a whole, so the sum of its heights is not a \
                 quantity anything is a share of. For the estimated shape, \
                 `{m} * density + x(<number>)`; for shares of the rows themselves, cut them \
                 into cells first: `bar * bin * proportion + x(<number>)` is the same curve's \
                 histogram, read as fractions."
            ),
        });
        return;
    }

    // 2. A fitted curve is a value *at* each x, not a part of any total. Summing the
    //    fitted values answers nothing, so there is no denominator to be found — the
    //    same absence as `density`, arrived at from the other direction (an estimate
    //    between the observations against a fit through them).
    if ts.contains(&Transform::Smooth) {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `{m} * smooth * proportion` has no total to divide by — a fitted curve \
                 gives a value *at* each x rather than a part of a whole, so adding those \
                 values up answers nothing and a share of the sum means nothing. Keep the \
                 fit: `{m} * smooth + x(<a>) + y(<b>)`. If the column you are fitting is \
                 already a share, compute it in your own table and plot it as an ordinary y."
            ),
        });
        return;
    }

    // 3. A pair transform leaves **two** numbers per cell, an extent rather than a
    //    quantity, and a share of an extent is not defined — normalizing both ends
    //    separately would move them by different factors and stop the pair being one
    //    span. (`bounds` reshapes two columns you already have, and lands here for the
    //    same reason it lands anywhere: what it supplies is a pair.)
    if let Some(p) = ts.iter().find(|t| is_pair_transform(t) || **t == Transform::Box) {
        let p = transform_name(p);
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `{m} * {p} * proportion` cannot take a share of a span — `{p}` leaves \
                 two numbers per cell, a low and a high, and an extent has no total it is a \
                 part of. For a single value per cell read as a share, name the statistic \
                 that makes one: `bar * mean * proportion + x(<a>) + y(<column>)`."
            ),
        });
    }
}

/// `bounds` supplies a *pre-computed* low/high pair (`bounds(lower, upper)`), so it
/// belongs to the marks that draw such a pair: `ribbon` fills it, `interval` draws a
/// whisker across it, `line`/`step` trace its two boundaries (the unfilled band), and
/// `zone` shades the region between them. On any other mark it names nothing to draw.
/// The refusal names all five, and a test holds it to the shared table. And because it *reshapes* rather than
/// computes, its two columns must actually exist and be numeric — a data-aware check,
/// so it sits in the df-gated block beside `check_jitter`.
pub(super) fn check_bounds(out: &mut Vec<Diagnostic>, df: &DataFrame, layer: &Layer) {
    if !layer.transforms.contains(&Transform::Bounds) {
        return;
    }
    // 1. A mark that draws a low/high pair — the four band marks and `zone`, read off
    //    the shared table (bounds is a pair transform, so `None` marks the rest).
    if mark_takes_transform(&layer.mark, &Transform::Bounds) == TransformLegality::None {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `bounds` supplies a pre-computed low/high pair, which five marks draw — \
                 `ribbon` fills it, `interval` whiskers it, `line`/`step` trace its two \
                 boundaries, `zone` shades the region between them — but `{}` is none of \
                 those. For a single value per group, a summary like `mean` is the transform \
                 you want.",
                mark_name(&layer.mark),
            ),
        });
        return;
    }
    // 2. Which pairs this mark needs. A band mark spans the *measure* axis and has
    //    no extent along the domain, so it requires `lower`/`upper` and has
    //    nothing to do with `start`/`end`. A `zone` is a rectangle and takes
    //    either pair, or both — whichever it is not given, the panel supplies,
    //    which is the whole reason the mark exists.
    let Some(spec) = layer.bounds.as_ref() else {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: if layer.mark == Mark::Zone {
                "gog: `zone` needs at least one pair of columns to bound it — \
                 `bounds(lower, upper)` for the measure axis, `bounds(start, end)` for the \
                 domain axis, or all four for a box. The axis you leave out spans the panel."
                    .to_string()
            } else {
                "gog: `bounds` needs two column names — `bounds(lower, upper)`.".to_string()
            },
        });
        return;
    };

    let is_zone = layer.mark == Mark::Zone;
    if !is_zone && (spec.start.is_some() || spec.end.is_some()) {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `bounds(start, end)` bounds a rectangle along the domain axis, and {} `{}` \
                 has no extent there — it spans the measure axis at each position. Keep \
                 `bounds(lower, upper)`, or use `zone` to shade a rectangle.",
                article(mark_name(&layer.mark)), mark_name(&layer.mark),
            ),
        });
        return;
    }
    if is_zone && spec.measure().is_none() && spec.domain().is_none() {
        // A half-named pair is the likely typo, and saying which half is missing
        // beats repeating the whole grammar at someone who nearly had it.
        let named: Vec<&str> = [("lower", &spec.lower), ("upper", &spec.upper),
                                ("start", &spec.start), ("end", &spec.end)]
            .iter().filter(|(_, v)| v.is_some()).map(|(k, _)| *k).collect();
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: if named.is_empty() {
                "gog: `zone` needs at least one complete pair — `bounds(lower, upper)` bounds it \
                 on the measure axis, `bounds(start, end)` on the domain axis, and all four make \
                 a box. The axis you leave out spans the panel.".to_string()
            } else {
                format!(
                    "gog: `zone` was given `{}` but not the other half of any pair. A rectangle \
                     needs both ends of a side: `bounds(lower, upper)`, `bounds(start, end)`, or \
                     all four.",
                    named.join("`, `"),
                )
            },
        });
        return;
    }
    if !is_zone && spec.measure().is_none() {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: "gog: `bounds` needs two column names — `bounds(lower, upper)`.".to_string(),
        });
        return;
    }

    // 3. Every named column must exist and be numeric — it reshapes, never computes.
    for col in [&spec.lower, &spec.upper, &spec.start, &spec.end].into_iter().flatten() {
        if df.float_col(col).is_none() {
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: format!(
                    "gog: `bounds` reads a pre-computed numeric column, and `{col}` is not one in \
                     the data. Check the name, or compute the bound first — gog draws it, it does \
                     not fit the model."
                ),
            });
        }
    }
}

/// The distributional transforms — `bin`, `density`, `smooth` — describe how values
/// are spread *along* an axis, so the axis they read must carry a number.
///
/// This question was answered in `transform.rs` for several sessions, one `eprintln!`
/// per transform, and answering it *there* made it unrefusable: the transform stage
/// runs after the legality gate, so it could only warn and then hand the renderer
/// something to draw. `bin` and `density` returned an empty frame, which drew an empty
/// panel with fabricated 0..1 axes; `smooth` returned its input unchanged, which drew
/// the raw scatter as if it were the fitted curve — the worse of the two, because it
/// looks finished. Either way a binding was accepted and silently dropped, which §12
/// forbids outright, and `writing.qmd` documented the `bin` case as a refusal that
/// never happened. It is a legality question, so it is asked here, once, and fatally.
///
/// Which axis is read is *relational*, the way `slot_orient` is: a vertical `bar * bin`
/// cuts `x`, a horizontal one cuts `y`. `smooth` reads **both** — it fits `y` against
/// `x` — so both must be numeric. Data-aware, so it sits in the df-gated block.
/// The columns a statistic, a `dodge` and a `stack` run *within*: empty for the
/// whole frame.
///
/// **Every channel that splits, splits** — `color`, `group`, `pattern` and `shape`,
/// in that order and without repeats — which is the rule `marks::split_series`
/// already stated for drawing a series. Stated once here because a rule about which
/// channels split a statistic has to be the same rule in the check and in the draw,
/// or a refusal describes a partition the renderer does not use.
///
/// This returned **one** column until 2026-09-23, `color` else `group`, and every
/// other split was merged away before the mark saw it. `pattern(era)` under
/// `interval * range` drew solid whiskers beneath a legend keyed by era, `shape`
/// under `point * mean` drew circles only, and `color` beside `group` under
/// `line * mean` drew an empty panel. Only a column of text splits in the end; the
/// transform stage passes over a numeric one, which is a ramp.
/// The channels that split a mark's rows into groups **and that this mark takes**:
/// [`split_fields_of`]'s four, less the ones `rule_for` refuses here. A `bar` takes
/// `color`, `pattern` and `group` and refuses `shape`, so advice or a relaxation
/// that named `shape` for a bar named a channel the same sentence would then refuse.
/// Read off the rule table, so a mark that gains a splitting channel is advised it
/// without anyone coming here.
pub(crate) fn splitting_channels(mark: &Mark) -> Vec<Channel> {
    [Channel::Color, Channel::Pattern, Channel::Group, Channel::Shape]
        .into_iter()
        .filter(|ch| rule_for(mark, ch).obligation != Obligation::Cannot)
        .collect()
}

pub(crate) fn split_fields_of(layer: &Layer) -> Vec<&str> {
    let mut fields: Vec<&str> = Vec::new();
    for ch in [Channel::Color, Channel::Group, Channel::Pattern, Channel::Shape] {
        if let Some(d) = layer.encodings.get(&ch) {
            if !fields.contains(&d.field.as_str()) {
                fields.push(d.field.as_str());
            }
        }
    }
    fields
}

/// `smooth` needs enough rows to fit a curve, and it needs them in every cell.
///
/// **This is the third road to the same failure §12 has now recorded twice.** The
/// minimum was never missing: `transform::smooth` returns the frame unchanged below
/// three rows, exactly as it does for a categorical axis. But *that* case is refused
/// upstream, so returning the input is only ever the `GOG_STRICT=0` fallback, while
/// this one had no gate at all — so the raw rows reached the page as the fitted
/// curve. A two-row group drew a two-vertex polyline and a ten-row group drew a
/// hundred-vertex one, side by side in the same picture, and nothing said which was
/// a fit. It is the worse half of the pair the transform's own comment names: the
/// scatter drawn as the curve *looks finished*.
///
/// **Counted per cell, because that is what the transform sees.** A statistic runs
/// inside each group, and faceting splits before any of it, so six rows can pass a
/// whole-frame count while all three of its panels fail. Both splits are read the
/// way the renderer reads them.
pub(super) fn check_smooth_rows(
    out: &mut Vec<Diagnostic>,
    spec: &PlotSpec,
    df: &DataFrame,
    layer: &Layer,
) {
    if !layer.transforms.iter().any(|t| matches!(t, Transform::Smooth | Transform::SmoothBand)) {
        return;
    }
    // A categorical axis is `check_distribution_axis`'s refusal, and it has just
    // run. Counting rows for a fit that was already declined would be a second
    // refusal for one mistake, which §12 forbids as plainly as silence.
    let axis_type = |c: &Channel| {
        spec.position_for(layer, c)
            .and_then(|e| actual_type(df, &e.field))
    };
    if [Channel::X, Channel::Y]
        .iter()
        .any(|c| axis_type(c) == Some(VarType::Discrete))
    {
        return;
    }

    // The partition, in the renderer's own terms: the two facet columns and the
    // splitting channel. A key that names no column contributes nothing, so an
    // unfaceted, ungrouped plot is one cell — the whole frame — which is the same
    // code, degenerate, exactly as `transform::apply` treats it.
    let keys: Vec<&str> = spec
        .facet
        .as_ref()
        .into_iter()
        .flat_map(|f| [f.col.as_deref(), f.row.as_deref()])
        .flatten()
        .chain(split_fields_of(layer))
        .filter(|k| df.str_col(k).is_some())
        .collect();

    let rows = df.len();
    let mut counts: HashMap<Vec<String>, usize> = HashMap::new();
    for r in 0..rows {
        let cell: Vec<String> = keys
            .iter()
            .map(|k| {
                df.str_col(k)
                    .and_then(|c| c.get(r).cloned())
                    .unwrap_or_default()
            })
            .collect();
        *counts.entry(cell).or_insert(0) += 1;
    }

    // The same threshold the transform uses (`transform::fit_min_rows`), so the
    // refusal and the `GOG_STRICT=0` fallback agree about which fits exist. It is
    // read from the transform the reader wrote, which also names it in the message:
    // the band needs more rows than its curve.
    let fit = layer.transforms.iter()
        .find(|t| matches!(t, Transform::Smooth | Transform::SmoothBand))
        .cloned()
        .unwrap_or(Transform::Smooth);
    let min_rows = crate::transform::fit_min_rows(&fit);
    let smallest = counts.values().copied().min().unwrap_or(rows);
    if smallest >= min_rows {
        return;
    }
    let name = transform_name(&fit);
    let reads = if fit == Transform::SmoothBand {
        "fits a curve through the rows and measures how far they fall from it, \
         which needs at least"
    } else {
        "fits a curve through the rows and needs at least"
    };

    // Two sentences rather than one clause, so the count never has to agree with a
    // verb: *needs at least 3* and *has 2* are both true of any number. And name the
    // split that is too thin, because the fix depends on it — a reader with both a
    // facet and a color would otherwise have to guess which one to give up.
    //
    // Between the two minimums the curve can still be drawn without its band, which
    // is the nearer fix, so it is the one offered there.
    let curve_min = crate::transform::fit_min_rows(&Transform::Smooth);
    let curve_alone = fit == Transform::SmoothBand && smallest >= curve_min;
    let (found, fix) = if keys.is_empty() {
        (
            format!("This data has {smallest}."),
            if curve_alone {
                format!("`line * smooth` draws the curve without its band, and needs only \
                         {curve_min}.")
            } else {
                "Plot what you have with `point` and no transform, which draws the rows \
                 as rows rather than as a curve through them."
                    .to_string()
            },
        )
    } else {
        (
            format!(
                "Split by {}, the smallest group has {smallest}.",
                or_list(&keys.iter().map(|k| format!("`{k}`")).collect::<Vec<_>>())
            ),
            if curve_alone {
                format!("Drop the split so the fit reads the rows together, or draw the \
                         curve without its band with `line * smooth`, which needs only \
                         {curve_min}.")
            } else {
                "Drop the split so the fit reads the rows together, or plot what you have \
                 with `point` and no transform, which draws the rows as rows rather than \
                 as a curve through them."
                    .to_string()
            },
        )
    };
    out.push(Diagnostic {
        kind: DiagnosticKind::Illegal,
        message: format!("gog: `{name}` {reads} {min_rows} of them. {found} {fix}"),
    });
}

pub(super) fn check_distribution_axis(
    out: &mut Vec<Diagnostic>,
    spec: &PlotSpec,
    df: &DataFrame,
    layer: &Layer,
) {
    let xd = spec.position_for(layer, &Channel::X);
    let yd = spec.position_for(layer, &Channel::Y);
    let xt = xd.and_then(|c| actual_type(df, &c.field));
    let yt = yd.and_then(|c| actual_type(df, &c.field));
    // How many axes this layer's transform is read over — the dimensionality rule,
    // which needs the space now that a `bar` cuts one axis flat and two in the cube.
    let both = cuts_both_positions(&layer.mark, space_of(spec));

    // The axis a synthesizing transform *writes* to is the measured one; it reads
    // the other. For a bar, orientation decides which — the same call `check` makes
    // to pick `synth_axis` (`reads_orientation`), so the two can never disagree
    // about the key axis. It asked `is_slot_mark` until 2026-10-11, so a counting
    // `text`, and then a counting `point`, read their key off `x` here and off `y`
    // there: `point * bin * stack + x(gdp) + y(region)` lost the refusal a bar gets,
    // that `bin` cannot cut a category.
    let (xf, yf) = (xd.map_or("", |c| c.field.as_str()), yd.map_or("", |c| c.field.as_str()));
    let key = if reads_orientation(layer) && columns_orient(df, xf, yf) == Orient::Horizontal {
        Channel::Y
    } else {
        Channel::X
    };
    let named = |ch: &Channel| -> Option<(String, VarType)> {
        let cd = match ch {
            Channel::Y => yd?,
            _ => xd?,
        };
        let t = if matches!(ch, Channel::Y) { yt } else { xt }?;
        Some((format!("{}({})", channel_name(ch), cd.field), t))
    };

    // Does a transform outside `mine` measure the cells? A tally only tallies when
    // nothing else in the chain does: beside another measurement `proportion` is a
    // normalizer of it, and `count` beside one is a contradiction `check_chain_jobs`
    // has already refused. Either way the tally's question about the axes is not
    // being asked, and answering it anyway printed a refusal of `zone * proportion`
    // under `zone * density * proportion`, once per axis, after the sentence's own.
    let measured_by_other = |mine: &[Transform]| {
        layer.transforms.iter().any(|u| {
            !mine.contains(u)
                && crate::transform::jobs(u, crate::transform::JobContext::default()).measure
        })
    };

    for t in &layer.transforms {
        // A mark that refuses the transform outright has said so in its own words
        // (`check_marks_that_take_no_transform`), and what the transform would have
        // asked of the axes is moot. Answering it anyway told a `path * count` reader
        // about `zone * count`, in a sentence with no `zone` in it.
        if mark_takes_transform(&layer.mark, t) == TransformLegality::None {
            continue;
        }
        // Which axes this transform reads, and **which type it refuses there**.
        //
        // The second half is new with the tile plot, and it is what makes the pair
        // `bin`/`count` one rule rather than two: *`bin` cuts, `count` tallies.* A
        // cutting transform needs room to cut, so it refuses a category; a tallying
        // one needs cells that already exist, so it refuses a number. Same axis, same
        // question, opposite answers — and stating the polarity here keeps both in the
        // one function that asks a position's type.
        let (required, refuses): (&[Channel], VarType) = match t {
            Transform::Smooth | Transform::SmoothBand => (&[Channel::X, Channel::Y], VarType::Discrete),
            // A **field** is a quantity that exists between the data points, so it
            // needs two axes with somewhere to spread and refuses a category on
            // either. Asked of `has_no_measure_axis` rather than of a mark name, so
            // the `zone` heatmap and the `path` contour cannot answer it differently.
            Transform::Density if both => {
                (&[Channel::X, Channel::Y], VarType::Discrete)
            }
            // A **cut** asks for less, and the difference is the mixed mesh. `bin`
            // does not need two continuous axes, it needs *something to cut*: it cuts
            // the axis with a width and leaves alone the one that arrives already
            // cut, because a category is a cell. So it refuses only where **neither**
            // axis can be cut — where there is no bin to make and the transform the
            // user wanted is `count` (spec §5).
            //
            // Why `density` is not the same case, since the two are otherwise one
            // rule: a tally over a product mesh is **joint**, so every cell holds the
            // same kind of measurement and the ramp compares them honestly. A density
            // per category would be **conditional** — each slot's estimate integrating
            // to 1 on its own — so the cells would not be comparable across slots and
            // the color scale would say they were. That is the *which margin
            // normalizes* question the 100% stacked bar asks, and it is not answered
            // by giving one transform two meanings depending on the types it was
            // handed.
            Transform::Bin
                if both
                    && (xt, yt) == (Some(VarType::Discrete), Some(VarType::Discrete)) =>
            {
                (&[Channel::X, Channel::Y], VarType::Discrete)
            }
            Transform::Bin if both => continue,
            // The **violin**: a category is one slot, and a slot is exactly what this
            // reading spreads the estimate *across* rather than along. So the type
            // that refuses `density` everywhere else is the one that selects this
            // reading, which is why the exemption is asked of `slot_density` rather
            // than written out again here — the fourth reading of one transform, on
            // the pattern `path`/`zone` set (spec §5).
            Transform::Density if slot_density(spec, layer, Some(df)).is_some() => continue,
            Transform::Bin | Transform::Density => (std::slice::from_ref(&key), VarType::Discrete),
            // A cut supplies the cells, so the tally has somewhere to land and the
            // axis's own type stops mattering — the same exemption `check_pair_summary`
            // gives the five value statistics, and for the identical reason. Without
            // it `zone * bin * proportion` was refused *as* `zone * proportion`, quoting
            // a continuous `x` and advising the `bin` the sentence already had.
            Transform::Count | Transform::Proportion
                if layer.transforms.contains(&Transform::Bin) => continue,
            Transform::Count if measured_by_other(&[Transform::Count, Transform::Proportion]) => continue,
            Transform::Proportion if measured_by_other(&[Transform::Proportion]) => continue,
            // The tally read in two dimensions — the tile plot. Its cells are the
            // categories, so *both* axes must have them, and a continuous position is
            // the mistake here rather than a categorical one. On a mark that has a
            // measure axis `count` is unconstrained (a `bar` tallies distinct numbers
            // happily), so this fires only where the tally is also the extent.
            Transform::Count | Transform::Proportion if both => {
                (&[Channel::X, Channel::Y], VarType::Continuous)
            }
            _ => continue,
        };
        // **Every axis at fault, in one refusal.** This loop used to push a message
        // per axis, so a sentence with both positions wrong printed the same paragraph
        // twice, each copy naming one binding and ending "and so is the other one". A
        // paragraph printed twice reads as two problems, and there is one (§12).
        let faults: Vec<(String, String)> = required.iter()
            .filter_map(|ch| named(ch))
            .filter(|(_, vt)| *vt == refuses)
            .map(|(binding, _)| {
                let field = binding.split_once('(')
                    .map_or("<column>", |(_, f)| f.trim_end_matches(')'))
                    .to_string();
                (binding, field)
            })
            .collect();
        let Some((_, first_field)) = faults.first() else { continue };
        let binding = faults.iter().map(|(b, _)| b.as_str()).collect::<Vec<_>>().join("` and `");
        let is = if faults.len() > 1 { "are both" } else { "is" };
        let m = mark_name(&layer.mark);
        let message = match t {
            // A zone bins to *make* its cells, and a category's cell exists already
            // — so the direction is `count`, the transform that tallies into cells
            // rather than cutting them. The one-dimensional pair says the same thing
            // (`bar * bin` against `bar * count`); this is that sentence with both
            // axes in it.
            //
            // It fires only where **both** axes are categorical, since the mixed mesh
            // is legal: one category left for `bin` to work around is a plot, and two
            // is nothing to cut at all.
            Transform::Bin if layer.mark == Mark::Zone => format!(
                "gog: `zone * bin` cuts an axis into cells, and there is no axis here it can \
                 cut — `{binding}` {is} categorical. A category is one slot, with no width to \
                 cut. To tally rows into the cells two categorical axes already make, `count` \
                 is the transform that does it: `zone * count + x(<a>) + y(<b>)` draws a cell \
                 per pair, colored by how many rows fell there. (With *one* categorical axis \
                 `zone * bin` draws the mixed mesh — the continuous axis cut into cells, one \
                 row of them per category.)"
            ),
            // The mirror, and the reason it reads as one rule: a tally needs cells that
            // exist, a cut makes them. Naming `bin` back is the whole fix. The mark is
            // the sentence's own: a 3-D `bar` reads its tally over two axes as well.
            Transform::Count | Transform::Proportion => format!(
                "gog: `{m} * {}` tallies rows into the cells that two *categorical* axes \
                 already make, and `{binding}` {is} continuous — a number is a point, and a \
                 point owns no cell. To cut a continuous axis into cells first, `bin` is the \
                 transform that does it: `{m} * bin` counts the rows in each{}.",
                transform_name(t),
                if layer.mark == Mark::Zone { ", which is the heatmap" } else { " cell" },
            ),
            // The same sentence for a field, whose plane is the two axes rather than a
            // mesh cut across them: an estimate needs somewhere to spread, and a
            // category is one slot with no room to spread in.
            Transform::Density if both => format!(
                "gog: `{m} * density` estimates a density over the *plane*, and `{binding}` \
                 {is} categorical — a category is one slot, with no interval for the estimate \
                 to spread along. Both axes must be continuous. To compare one continuous \
                 distribution across categories, `line * density + color({first_field})` \
                 draws a curve per group."
            ),
            // With a number on the other position, the sentence most likely meant a
            // histogram of that number for each category, which is a split, not a
            // key. Said for every mark that reads its orientation, as the point's
            // refusal said it before a counting point could lie on its side.
            Transform::Bin => {
                let other = if key == Channel::Y { Channel::X } else { Channel::Y };
                let split = match named(&other) {
                    Some((kept, VarType::Continuous)) => format!(
                        " To cut `{}` into bins for each `{first_field}`, keep `{kept}` and \
                         write `color({first_field})` in place of `{binding}`.",
                        kept.split_once('(').map_or("", |(_, f)| f.trim_end_matches(')'))
                    ),
                    _ => String::new(),
                };
                format!(
                    "gog: `bin` cuts a continuous axis into intervals, and `{binding}` {is} \
                     categorical — a category is one slot, with no width to cut. To tally rows \
                     per category, `count` is the transform that does it: `{m} * count`.{split}"
                )
            }
            Transform::Density => format!(
                "gog: `density` estimates a continuous distribution, and `{binding}` {is} \
                 categorical — there is no number line for the curve to spread along. For \
                 the share of rows in each category, that is `bar * proportion`."
            ),
            // The band's direction is the band per category, as the line's is the
            // value per category: `smooth_band` is to `smooth` what `confidence` is
            // to `mean`, so the pair answers the pair.
            Transform::SmoothBand => format!(
                "gog: `smooth_band` fits a curve of `y` against `x`, and `{binding}` {is} \
                 categorical — a fit needs a number line to run along. For a typical value \
                 per category and how precisely it is known, `interval * confidence` says \
                 it directly."
            ),
            _ => format!(
                "gog: `smooth` fits a curve of `y` against `x`, and `{binding}` {is} \
                 categorical — a fit needs a number line to run along. For a typical value \
                 per category, `bar * mean` (or `point * mean`) says it directly."
            ),
        };
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message,
        });
    }
}

/// A plain-number `bin(width = )` cannot cut a time axis, because the number does
/// not say its unit (spec §5).
///
/// Every binding sends a moment as seconds since 1970, so `bin(width = 7)` on a
/// column of dates cut bins seven *seconds* wide: twenty days drew 19 bars each 0
/// pixels across, an empty panel with a count axis and no message, and ten years
/// of dates held 45 million bins and 1.3 GB of memory to draw it. The reader meant
/// days, very likely, and a number cannot say so: an R `Date` counts days, a
/// JavaScript `Date` milliseconds, the engine seconds. So the width is refused,
/// with the count as the direction, since `bin(30)` divides the range and needs no
/// unit at all. The same units trap is why a time axis's `limits` are written in
/// dates and converted by the binding (§10); a width written as a duration, each
/// language's own, is the way out that is not built.
pub(super) fn check_bin_width_on_time(
    out: &mut Vec<Diagnostic>,
    spec: &PlotSpec,
    df: &DataFrame,
    layer: &Layer,
) {
    if !layer.transforms.contains(&Transform::Bin) { return; }
    // `bins` wins when both arrive, so only a width the layout would use is asked.
    let Some(BinSpec { width: Some(width), bins: None, .. }) = &layer.bin else { return };

    let xd = spec.position_for(layer, &Channel::X);
    let yd = spec.position_for(layer, &Channel::Y);
    let (xf, yf) = (xd.map_or("", |c| c.field.as_str()), yd.map_or("", |c| c.field.as_str()));
    // The axes the cut is made on, decided as `check_distribution_axis` decides
    // them: both in two dimensions, otherwise the key, which a layer on its side
    // reads off `y`.
    let cut: &[Channel] = if cuts_both_positions(&layer.mark, space_of(spec)) {
        &[Channel::X, Channel::Y]
    } else if reads_orientation(layer) && columns_orient(df, xf, yf) == Orient::Horizontal {
        &[Channel::Y]
    } else {
        &[Channel::X]
    };
    let Some((binding, unit)) = cut.iter().find_map(|ch| {
        let cd = if matches!(ch, Channel::Y) { yd? } else { xd? };
        let unit = df.time_unit(&cd.field)?;
        Some((format!("{}({})", channel_name(ch), cd.field), unit))
    }) else { return };
    let holds = match unit {
        crate::time::TimeUnit::Day => "dates",
        crate::time::TimeUnit::Second => "times",
    };
    out.push(Diagnostic {
        kind: DiagnosticKind::Illegal,
        message: format!(
            "gog: `bin(width = {width})` does not say what unit {width} is in, and `{binding}` \
             holds {holds}. gog measures time in seconds, so these bins would be {width} \
             seconds wide. Give the number of bins instead, such as `bin(30)`."
        ),
    });
}

/// The pair transforms `range`/`confidence` produce a low/high pair, which only
/// the *span* marks draw (`interval` whiskers it, `ribbon` fills it, `line`/`step`
/// trace its two boundaries). Composed onto a *locus* mark — `point`, `bar`,
/// `area` — they used to render nonsense (two points or bars per group, or a
/// region wanting to be a ribbon): a silent Law-1 gap the Mark × Transform grid
/// surfaced, and `mark_takes_transform` records as `None`. Refused here with
/// direction toward the mark whose geometry the pair actually wants. The other
/// `None` marks are refused by a check that already owns them, so this handles
/// only these three: `box` by `check_box`, `text` by the `label` requirement,
/// `bounds` on any mark by `check_bounds`; `interval`/`ribbon`/`line`/`step` take
/// the pair legally.
pub(super) fn check_pair_transform_marks(out: &mut Vec<Diagnostic>, layer: &Layer) {
    if !matches!(layer.mark, Mark::Point | Mark::Bar | Mark::Area) {
        return;
    }
    for t in &layer.transforms {
        // The **computed** pair transforms: the class, less the one that
        // reshapes. `bounds` on these marks is `check_bounds`'s, so only a pair
        // this layer computed reaches a locus mark unrefused, which is the gap
        // this closes. Written as the class rather than as its members so a
        // fourth computed pair joins without an edit here — `deviation` had to
        // be added by hand, and that is the shape this file keeps paying for.
        if !is_pair_transform(t) || *t == Transform::Bounds {
            continue;
        }
        let fix = match layer.mark {
            Mark::Area =>
                "To fill the space between a low and a high boundary, that is `ribbon`; \
                 an `area` fills to a baseline at 0 and draws one value per x",
            _ =>
                "The span marks draw a low/high pair — `interval` whiskers it, `ribbon` \
                 fills it. For a single summary value per group, a statistic like `mean` \
                 is what a point or bar draws",
        };
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `{}` produces a low value and a high one per group, but `{}` draws a \
                 single value at each x, not a span. {fix}.",
                transform_name(t),
                mark_name(&layer.mark),
            ),
        });
    }
}

/// The transforms that refuse a mark in their own check, with the marks that draw
/// them named: `bounds` (`check_bounds`), `partition`, `flow` and `layout`. A
/// general refusal beside theirs only repeats the mistake with a worse direction.
pub(super) fn refuses_marks_itself(t: &Transform) -> bool {
    matches!(t, Transform::Bounds | Transform::Partition | Transform::Flow | Transform::Layout)
}

pub(super) fn check_marks_that_take_no_transform(out: &mut Vec<Diagnostic>, layer: &Layer) {
    if !matches!(
        layer.mark,
        Mark::Path | Mark::Rule | Mark::Text | Mark::Zone | Mark::Surface
    ) {
        return;
    }
    let refused: Vec<&Transform> = layer.transforms.iter()
        .filter(|t| !is_collision_modifier(t))
        // What a mark *does* take is asked of the shared table, never typed out
        // here. This filter used to name `zone`'s one transform by hand, and a
        // hand-maintained list beside a generated one always loses: the moment
        // `zone` learned to `bin`, the list still said `bounds` and the refusal
        // fired on a plot the grid advertised as legal. Same failure as the polar
        // fallback's, which is why that one is generated now too.
        .filter(|t| mark_takes_transform(&layer.mark, t) == TransformLegality::None)
        // `proportion` beside a `partition` divides the partition's measure axis by
        // its total and replaces no rows, so the label a node carries is still
        // there to draw. Refused here, `text * partition(…) * proportion` could not
        // name the nodes its own `zone` layer drew as shares.
        .filter(|t| !(**t == Transform::Proportion
            && layer.transforms.contains(&Transform::Partition)))
        .collect();
    // **The four transforms that refuse a mark in their own check are left to it**
    // (`check_bounds`, `check_partition`, `check_flow`, `check_layout`), each of
    // which names the marks that draw it. Refused here as well, every one of these
    // marks printed two refusals for one mistake, and some arms gave the wrong
    // reason: a path was told "Use `line * flow`", which a line refuses too, and a
    // text that `bounds` "replaces those rows with one summary per key". What is
    // left here is the statistics, and for a path, `line` is their mark.
    let refused: Vec<&Transform> =
        refused.into_iter().filter(|t| !refuses_marks_itself(t)).collect();
    let names: Vec<&str> = refused.iter().map(|t| transform_name(t)).collect();
    if names.is_empty() {
        return;
    }
    let listed = names.join("`, `");
    let message = match layer.mark {
        Mark::Path => format!(
            "gog: `path` strokes the rows in the order the table gives them, and `{listed}` \
             replaces those rows with one summary per key — after which the order is \
             the keys' and the path is a `line`. Use `line * {}`, which sorts by `x` \
             and is the mark a statistic is drawn on.",
            names[0],
        ),
        Mark::Rule => format!(
            "gog: `rule` is placed by one column and spans the other axis, so it has no \
             measure for `{listed}` to compute and nowhere to put the answer. Compute the \
             value where your data lives and give the rule a table of the results — one row \
             per line — which is what a rule's position always is: a column."
        ),
        // **A zone and a surface say why, per transform, and name what they take
        // from the table the grid is generated from.** Both arms were one fixed
        // sentence whatever was refused: the zone's called `bounds` refused though
        // `zone * bounds` draws, its list of what a zone takes had drifted four
        // transforms short, and `layout` was never named; the surface's explained
        // five of the thirteen it refuses. `cell_refusal_reason` gives one clause
        // per family written, and `cell_mark_takes` reads `mark_takes_transform`.
        Mark::Zone => {
            let (tiles, reduces) = cell_mark_takes(&layer.mark);
            format!(
                "gog: a `zone` fills each cell with one value, read as its color. {}. \
                 A zone takes {tiles}, and {reduces} reduce the column `color` names in \
                 every cell: `zone * mean + x(<a>) + y(<b>) + color(<column>)`. To shade a \
                 band the data computed, `ribbon * range` is the mark that spans a statistic.",
                cell_refusal_reasons(&refused),
            )
        }
        Mark::Surface => {
            let (tiles, reduces) = cell_mark_takes(&layer.mark);
            format!(
                "gog: a `surface` is a sheet over a floor whose cells tile without gaps. \
                 {}. {tiles} tile a floor, and {reduces} reduce a column inside the cells \
                 `bin` cuts: `surface * bin * mean + x(<a>) + y(<b>) + z(<column>)`, while \
                 `surface * density + x(<a>) + y(<b>) + space()` estimates the field. Over \
                 categories, `bar` is the mark, where the column under each tile says which \
                 cell it belongs to.",
                cell_refusal_reasons(&refused),
            )
        }
        // **A text prints a summary now** (spec §8, the rows rule), so "one summary
        // per key, which has no label to draw" stopped being the reason: it is
        // exactly what `text * mean + label(life)` prints. What is left here is a
        // fitted curve, whose points are its own, and a pair, which is two numbers
        // where a label prints one; each is said as itself.
        _ if refused.iter().any(|t| matches!(t, Transform::Smooth | Transform::SmoothBand)) => format!(
            "gog: `text` prints one label at each place, and `{listed}` fits a curve at \
             points of its own, which hold no value of your table to print. To print a \
             summary of each group instead, write `text * mean + label(<column>)`, with \
             the column the summary reads."
        ),
        _ => {
            let range = refused.iter().any(|t| **t == Transform::Range);
            let instead = if range {
                "To print the high, write `text * max`, and for the low, `text * min`, each \
                 with the column it reads."
            } else {
                "To print one of them, compute it where your data lives and give `text` \
                 that table."
            };
            format!(
                "gog: `text` prints one label at each place, and `{listed}` gives each group a \
                 low and a high value, two numbers where a label prints one. {instead}"
            )
        }
    };
    out.push(Diagnostic { kind: DiagnosticKind::Illegal, message });
}

/// The statistics that still need a position axis even when the bar does not.
/// `count` and the aggregations answer "one value for these rows", which is
/// meaningful with nothing to group by; `bin`, `density` and `smooth` describe how
/// values are *distributed along* an axis, so with no axis there is nothing for
/// them to say. Refused with direction rather than quietly returning the input.
pub(super) fn check_keyless_statistic(
    out: &mut Vec<Diagnostic>, spec: &PlotSpec, df: Option<&DataFrame>, layer: &Layer,
) {
    if !bar_is_one_pile(spec, layer, df) {
        return;
    }
    for t in &layer.transforms {
        let needs_axis = matches!(t, Transform::Bin | Transform::Density | Transform::Smooth
            | Transform::SmoothBand);
        if !needs_axis {
            continue;
        }
        let n = transform_name(t);
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `{n}` describes how values are spread along an axis, and this `bar` has \
                 no `x()` to spread them along. Add `x(<column>)` to draw the distribution, \
                 or use `count`/`sum` for one value per group."
            ),
        });
    }
    // A pair transform has the same problem one level up: it produces a low and a
    // high *per position*, and there is no position here.
    if layer.transforms.iter().any(is_pair_transform) {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: "gog: a range transform gives a low and a high at each position, and this \
                      `bar` has no `x()` to place them at. Add `x(<column>)`, or drop the range."
                .to_string(),
        });
    }
}
