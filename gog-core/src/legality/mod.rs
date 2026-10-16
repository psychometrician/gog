//! Legality — which `(mark, channel)` bindings are grammatical, and which the
//! engine can actually render today.
//!
//! This is the "syllable formation rule" of the grammar. In Hangeul a syllable
//! block requires an initial consonant and a vowel; you cannot write `ㅏㅏ`, and
//! the writing system rejects it structurally rather than rendering a blank.
//! A gog *layer* is the same unit: a mark plus the channels that mark requires.
//!
//! Two orthogonal questions per binding:
//!
//! 1. **Obligation** — must / can / cannot be bound to this mark
//! 2. **Variable type** — continuous / discrete / either
//!
//! Diagnostics separate three cases, because the reader needs a different
//! action from each:
//!
//! | Kind | Meaning | What the reader should do |
//! |---|---|---|
//! | `Illegal` | the grammar forbids it | rewrite the expression |
//! | `Unsupported` | grammar allows it, engine can't do it yet | use a different atom, or wait |
//! | `Assumption` | it renders, but a choice was made for you | confirm the default is what you meant |
//!
//! Keeping `Illegal` and `Unsupported` distinct matters: telling someone a
//! valid combination is "illegal" teaches them a rule that isn't real.
//!
//! The rules live one family to a file. `rules` is the Mark × Channel table,
//! `matrix` the grids generated from it, and every other module the checks of one
//! family: a coordinate space, a group of transforms, the style settings, the
//! scales. This file holds the gate, [`check`], which runs them in order, and the
//! [`Diagnostic`] it returns.

use crate::color::{css_rgb, edit_distance, is_valid_color, nearest_color, numbered_shade};
use crate::data::DataFrame;
use crate::ir::{
    BinSpec, Channel, ChannelDef, CoordSpace, Figure, Layer, LevelColors, Mark, PageSpec, PaletteDef,
    PlotSpec, RangeSpec, ScaleType, StyleSpec, ThemeSpec, Transform,
};
use crate::transform::Job;
use std::collections::HashMap;

// `is_value_statistic` moved down to `transform.rs` on 2026-08-05, beside
// `is_reduction`. Which transforms leave one value per x is a fact about a
// transform rather than about legality, and the render modules need the same
// answer: `line` was carrying its own copy of the list and had fallen two
// members behind it.
use crate::transform::is_value_statistic;

mod rules;
mod words;
mod matrix;
mod style;
mod measure;
mod field;
mod orient;
mod marks;
mod chain;
mod graph;
mod collision;
mod cumulative;
mod scope;
mod columns;
mod page;
mod theme;
mod spaces;
mod brush;
mod positions;
mod params;
mod order;
mod facets;
mod scales;
mod palette;

pub use self::rules::*;
pub(crate) use self::words::*;
pub use self::matrix::*;
pub use self::style::*;
pub use self::measure::*;
pub use self::field::*;
pub use self::orient::*;
use self::marks::*;
pub(crate) use self::chain::*;
pub use self::graph::*;
pub use self::collision::*;
use self::cumulative::*;
pub use self::scope::*;
use self::columns::*;
pub use self::columns::names_a_written_column;
pub use self::page::*;
use self::theme::*;
use self::spaces::*;
pub use self::brush::*;
pub use self::positions::*;
use self::params::*;
use self::order::*;
use self::facets::*;
pub(crate) use self::facets::splits_its_domain;
pub use self::scales::*;
pub(crate) use self::palette::*;

// ---------------------------------------------------------------------------
// Diagnostics
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticKind {
    /// The grammar forbids this. It will never render.
    Illegal,
    /// The grammar allows it; this engine cannot draw it yet.
    Unsupported,
    /// It renders, but a default was chosen on the caller's behalf.
    Assumption,
}

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub kind: DiagnosticKind,
    /// Directional message: says what to do, not only what went wrong.
    pub message: String,
}

impl Diagnostic {
    /// `true` when the plot must not be rendered.
    pub fn is_fatal(&self) -> bool {
        matches!(self.kind, DiagnosticKind::Illegal | DiagnosticKind::Unsupported)
    }
}

/// Actual type of a bound column, or `None` when the column is absent.
fn actual_type(df: &DataFrame, field: &str) -> Option<VarType> {
    if df.float_col(field).is_some() {
        Some(VarType::Continuous)
    } else if df.str_col(field).is_some() {
        Some(VarType::Discrete)
    } else {
        None
    }
}

/// Check every layer of `spec` against the table.
///
/// Returns diagnostics in spec order. An empty vector means the plot is
/// grammatical and fully renderable.
pub fn check(spec: &PlotSpec, data: &HashMap<String, DataFrame>) -> Vec<Diagnostic> {
    check_inner(spec, data)
}

thread_local! {
    /// How many trial checks are running inside one another.
    static TRIAL_DEPTH: std::cell::Cell<u8> = const { std::cell::Cell::new(0) };
}

/// **Would this suggested sentence draw?** The one way a refusal offers a sentence
/// it has not written (the count hint, a ribbon's edge line, an interval's center,
/// a pile's summary, a surface's cells): it is checked first, alone in the plot,
/// and offered only when nothing fatal comes back, since a suggestion the engine
/// refuses sends the reader from one refusal to the next.
///
/// **Two deep at most** (2026-10-14). A suggestion is checked by the same rules that
/// made it, so a rule whose suggestion meets the same rule recursed without end:
/// `bar * count + x(continent) + space()` offered `bar * count * count`, whose check
/// offered three, until the engine's stack overflowed. That rule is fixed where it
/// stood; this keeps the next one from crashing. A suggestion three trials deep is
/// not offered, which costs a hint and never a plot.
/// The channel splits that leave every series of a layer one position along
/// `domain`, as the sentence writes them (`` `color(continent)` ``), or `None`.
///
/// Read from the table's rows: two rows of one series at the same value of the
/// domain are one position once a summary has reduced them. A measured color
/// splits nothing (it is a ramp along the stroke), so it is not counted.
fn one_position_per_series(
    spec: &PlotSpec,
    layer: &Layer,
    df: Option<&DataFrame>,
    domain: &Channel,
) -> Option<String> {
    let df = df?;
    let x = &spec.position_for(layer, domain)?.field;
    let xs: Vec<String> = match df.str_col(x) {
        Some(c) => c.clone(),
        None => df.float_col(x)?.iter().map(|v| v.to_bits().to_string()).collect(),
    };
    let mut written = Vec::new();
    let mut cols = Vec::new();
    for ch in [Channel::Color, Channel::Group, Channel::Pattern] {
        let Some(d) = layer.encodings.get(&ch).or(spec.channels.get(&ch)) else { continue };
        let Some(col) = df.str_col(&d.field) else { continue };
        written.push(format!("`{}({})`", channel_name(&ch), d.field));
        cols.push(col);
    }
    if cols.is_empty() || xs.is_empty() {
        return None;
    }
    let mut first: HashMap<String, &str> = HashMap::new();
    for (i, xv) in xs.iter().enumerate() {
        let key = cols.iter().map(|c| c.get(i).map_or("", String::as_str))
            .collect::<Vec<_>>().join("\u{1f}");
        match first.get(&key) {
            Some(seen) if *seen != xv.as_str() => return None,
            Some(_) => {}
            None => { first.insert(key, xv.as_str()); }
        }
    }
    Some(and_list(&written))
}

pub(super) fn trial_draws(trial: &PlotSpec, data: &HashMap<String, DataFrame>) -> bool {
    let depth = TRIAL_DEPTH.with(|d| d.get());
    if depth >= 2 {
        return false;
    }
    TRIAL_DEPTH.with(|d| d.set(depth + 1));
    let draws = !check_inner(trial, data).iter().any(Diagnostic::is_fatal);
    TRIAL_DEPTH.with(|d| d.set(depth));
    draws
}

fn check_inner(spec: &PlotSpec, data: &HashMap<String, DataFrame>) -> Vec<Diagnostic> {
    let mut out = Vec::new();

    // A plot with no mark has no layer for any check below to ask, so the one
    // question that applies is asked here: Law 7's second half.
    check_has_a_mark(&mut out, spec, data);
    check_plot_scope(&mut out, spec);
    // Before scopes resolve, for the reason `check_plot_scope` is: one binding
    // written for the whole plot is one statement, however many layers it reaches.
    check_legend(&mut out, spec);
    // The same reading for the guide a position draws.
    check_axis(&mut out, spec);
    // Which layers' labels the pass below fills rather than the sentence writing
    // them, read before it runs: a name nobody wrote cannot clash with anything.
    let label_written: Vec<bool> = spec.layers.iter()
        .map(|l| l.encodings.contains_key(&Channel::Label) || spec.channels.contains_key(&Channel::Label))
        .collect();
    let spec = &resolve_scopes(spec);
    // Across layers, before any one layer is asked: an axis read as text by one
    // table and as numbers by another has no single reading to check against.
    check_axis_kinds(&mut out, spec, data);
    // A zone's numbered sides on a categorical axis are slots, and one past every
    // slot is drawn off the panel; read across layers for the same reason.
    check_zone_slots(&mut out, spec, data);
    // The same question for each channel that draws a legend, which names one column.
    check_legend_columns(&mut out, spec, data);
    // And for an axis that two layers' transforms write to: one kind of number.
    check_one_measure_per_axis(&mut out, spec, data);

    for (li, layer) in spec.layers.iter().enumerate() {
        let mark = &layer.mark;
        let m = mark_name(mark);
        // Where this layer's diagnostics start, so a check that should not speak on
        // a refused layer can ask whether it was refused.
        let layer_from = out.len();

        // A mark this engine cannot draw makes every later question about the
        // layer moot — and answering them anyway produces advice that does not
        // apply. Say the one true thing and move on.
        if check_mark(&mut out, mark) {
            continue;
        }

        // A layer may name its own *column* for a shared axis; a layer naming its
        // own *scale* is a second axis, which §18 refuses.
        check_layer_position(&mut out, spec, layer);

        // The layer's own table, resolved once. Read this early because the very
        // first refusal below is data-aware: which reading of `density` a layer is in
        // is a question only the column types answer.
        let df = layer
            .data
            .as_ref()
            .or(spec.data.as_ref())
            .and_then(|name| data.get(name));

        // An interval or a ribbon floats between two extents, and only a range
        // transform — or the violin's reflection — supplies them; without either
        // there is nothing to span.
        check_span_needs_range(&mut out, spec, df, layer);

        // Where this layer's refusals about its *mark* begin: the checks from here
        // to `check_keyless_statistic` each refuse a transform the mark does not
        // take, in words about the mark. `check_chain_jobs` reads it to stay quiet
        // about a pair of transforms that one of them has already refused.
        let mark_refusals_from = out.len();

        // A box carries its own summary, so it takes no transform — the inverse
        // refusal to `check_interval`'s.
        check_box(&mut out, layer);
        // A `text` whose label names the column its transform writes has a label
        // on every summary row, so "no label to draw" would be false. The column
        // rules judge that label instead (spec §8): the right kind, not drawn yet.
        let labels_a_written_column = layer.mark == Mark::Text
            && binding_of(spec, layer, &Channel::Label)
                .is_some_and(|d| written_column(spec, layer, &d.field).is_some());
        if !labels_a_written_column {
            check_marks_that_take_no_transform(&mut out, layer);
        }
        // A text that counts draws one label per summary row, so its label names
        // a column those rows hold, or it names nothing (spec §8, the fourth rule).
        check_text_label_rows(&mut out, spec, df, layer);

        // `dodge` is legal on the slot marks, with a group split to separate, and on
        // `point`, whose own rows collide — refused with direction otherwise (spec §5).
        check_dodge(&mut out, layer);

        // `stack` is `dodge`'s sibling: legal only on the accumulating marks
        // (`bar`, `area`), and only with a group split to pile — the same two
        // conditions, refused with the geometry-aware direction (spec §5).
        check_stack(&mut out, layer);
        // Where the pile hangs: a baseline that names one of the three, and a space
        // with an origin to spend. Structural, so it sits with `check_stack` rather
        // than in the df-gated block below.
        check_baseline(&mut out, spec, layer);
        // A running total is drawn on the marks that draw a measure, and made of a
        // measurement that adds up (spec §5).
        check_cumulative(&mut out, spec, layer);

        // `group` on a bar or a point splits what a statistic, `dodge` or `stack`
        // gathers or arranges; with none of them it changes nothing (spec §6).
        check_group_gathers(&mut out, spec, df, layer);

        // `repel` is the fourth offset, and the one that needs no table: what it
        // moves is ink, and which mark has ink is a fact about the sentence. So it
        // sits here rather than beside `check_jitter` below (spec §5).
        check_repel(&mut out, layer);

        // The pair transforms (`range`/`confidence`) draw a span, so a locus mark
        // (`point`/`bar`/`area`) that draws one value per x is refused toward the
        // span mark whose geometry the pair wants — the Law-1 gap the grid found.
        check_pair_transform_marks(&mut out, layer);

        // A bar with no position axis takes only the statistics that mean something
        // without one (spec §15) — `count`/`sum`, not `bin`/`density`/`smooth`.
        check_keyless_statistic(&mut out, spec, df, layer);
        // One pile bent by `polar()` is a pie only from `y`.
        check_sideways_pile_in_polar(&mut out, spec, layer, df);

        // Two transforms that do the same job contradict (spec §5). Mark-agnostic and
        // dimension-agnostic, so it sits out here rather than in `check_pair_summary`,
        // where the same refusal reached only the marks that read over two positions
        // and left every one-key composition drawing a silent lie.
        let mark_refused = out[mark_refusals_from..].iter().any(Diagnostic::is_fatal);
        check_chain_jobs(&mut out, spec, layer, mark_refused);
        // What a normalizer cannot rescale (spec §5). Its own check because
        // `proportion` composes with far more than it refuses — the question is not
        // *which transform owns the measurement* but *is there one number per cell
        // for a share to be a share of*.
        check_share_composition(&mut out, layer);
        // A mark that reads a domain, cut by the column that supplies it — the
        // subset holds one position and there is nothing left to read between.
        // Asked per layer because the mark decides it, and it covers both
        // partitions: `play`'s frames and `facet`'s panels (§11).
        check_domain_split(&mut out, spec, layer);
        // A two-dimensional reading — `zone * bin`, `zone * density`, `path *
        // density` — needs both axes and colors itself by what it measured. None
        // of that is true of a bounded zone, so it is checked apart from
        // `check_bounds`.
        check_field(&mut out, spec, df, layer);
        // Every way a hierarchy can be malformed, from the mark that cannot read one
        // down to an interior node carrying its own value. Takes the frame as an
        // `Option` rather than sitting in the block below, because its first two
        // branches — the wrong mark, and no levels named — are facts about the
        // sentence and must refuse even when the table is missing.
        check_partition(&mut out, spec, df, layer);
        // The flow family's own malformations, `check_partition`'s shape one
        // family over — the wrong mark, too few stages, the wrong space, a bound
        // `x`, an aesthetic no reading holds, and the counted dropped rows.
        check_flow(&mut out, spec, df, layer);
        // The network family's malformations, the same shape a third time.
        check_layout(&mut out, spec, df, layer);
        // The cluster family's malformations, the same shape a fourth time —
        // the wrong mark, the atom with too little named, a bound position the
        // tree invents, and the profile with a hole in it.
        check_cluster(&mut out, spec, df, layer);

        // Which mesh, and whether this mark has a plane to tile at all. Takes the
        // frame because the third answer is data-aware: a mixed mesh has two axes and
        // still no plane, one of them being categorical.
        check_tiling(&mut out, spec, df, layer);
        // Whether a `surface` has the cube and a grid to span. Takes the frame the same
        // way and for the same reason: the cube is a fact about the sentence, the grid
        // one only the rows can state.
        check_surface(&mut out, spec, df, layer, data);
        // `levels` needs a plane to cut and `bandwidth` needs a single axis, so each
        // is refused in the reading it cannot mean.
        check_density_params(&mut out, spec, df, layer, space_of(spec));
        // `range`'s two ends are probabilities, and one that is not a probability
        // describes no band at all.
        check_range_params(&mut out, layer);
        // `deviation`'s multiplier is a count of standard deviations, and
        // `quantile`'s probability is required rather than defaulted.
        check_deviation_params(&mut out, layer);
        check_quantile_params(&mut out, layer);

        // The trio's third offset, `jitter`, is checked in the df-gated block
        // below (`check_jitter`) — unlike these two it must read the axis types.

        if let Some(df) = df {
            check_slot_shape(&mut out, spec, df, layer);
            check_area_overlap(&mut out, spec, df, layer);
            // Solids that share a cell of the cube's floor hide one another.
            check_cube_cells(&mut out, spec, df, layer);
            note_group_on_its_slot(&mut out, spec, df, layer);
            // A `shape` or `pattern` column with more categories than the
            // vocabulary has kinds draws two groups alike — legal, and said.
            check_which_one_cycles(&mut out, df, mark, layer);
            check_border_on_crosses(&mut out, df, mark, layer);
            // `jitter` is point-only and legal only when a position axis is
            // categorical — a data-aware check, so it sits with the others (§5).
            check_jitter(&mut out, spec, df, layer);
            // `point * dodge` swarms across a category's slot and keeps the other
            // axis true, so exactly one position must be a category (§5).
            check_swarm(&mut out, spec, df, layer);
            // `bounds` is legal only on the span marks, and its two columns must
            // exist — a data-aware check, so it sits here too.
            check_bounds(&mut out, df, layer);
            // Where a `zone`'s sides come from. Data-aware because one of the four
            // answers is the axis itself: a category owns a slot, a number is a point.
            check_zone_extent(&mut out, spec, df, layer);
            // A cell of a plain zone holds one row, or the others are hidden.
            check_zone_cells(&mut out, spec, df, layer);
            // The two-dimensional group-by: a reduction read over a pair of keys
            // needs its measure channel bound to a numeric column, and both
            // positions categorical. Data-aware for the same reason `check_zone_extent`
            // is, and placed after it because it takes over that refusal for the five.
            check_pair_summary(&mut out, spec, df, layer);
            // `bin`/`density`/`smooth` describe a spread along an axis, so the axis
            // they read must carry a number. Answered here rather than in
            // `transform.rs`, which could only warn and then draw anyway (§12).
            check_distribution_axis(&mut out, spec, df, layer);
            // A width cuts in the axis's own units, and a time axis's are seconds
            // that a plain number cannot name.
            check_bin_width_on_time(&mut out, spec, df, layer);
            // And having a number to read is not the same as having enough of it.
            // Same reason it lives here: a transform can only warn.
            check_smooth_rows(&mut out, spec, df, layer);
            // Which axis a `rule` sits on — Law 7's second relaxation, and a
            // data-aware question for the same reason `check_bounds` is: it is
            // answered by which of the plot's position columns this layer's own
            // table holds.
            check_rule(&mut out, spec, df, layer);
            // `stack`'s conditions that have to read the rows: a pile holds one
            // value for each group at a position, and its members agree in sign, or
            // the band going the other way is drawn inside the ones below it.
            // Data-aware, and it runs the layer's other transforms first because the
            // pile is made of *their* output.
            check_stack_piles(&mut out, spec, df, layer, data);
            // What a stated domain actually cuts — counted aloud, and fatal when
            // it cuts everything. Data-aware by nature, and last because it is
            // the only check that reports on rows rather than on the sentence.
            check_limit_rows(&mut out, spec, df, layer);
        }

        let synth_axis = synth_axis(spec, layer, df);

        // Effective bindings: the positions this layer actually reads, plus its
        // own channels. A layer naming its own column for a shared axis takes
        // precedence over the plot's — nearest wins, as with `data()` (spec §8) —
        // and it must, or the plot's column would be looked for in the layer's
        // table and reported missing, which is how the designed annotation
        // sentence used to fail.
        let mut bound: Vec<(Channel, &str)> = Vec::new();
        for ch in [Channel::X, Channel::Y, Channel::Z] {
            if let Some(cd) = spec.position_for(layer, &ch) {
                bound.push((ch, &cd.field));
            }
        }
        for (ch, cd) in &layer.encodings {
            if matches!(ch, Channel::X | Channel::Y | Channel::Z) {
                continue; // already resolved above
            }
            bound.push((ch.clone(), &cd.field));
        }

        // An axis a mark *spans* is not one it reads, so the plot's column for that
        // axis is expected to be absent from the layer's own table — a thresholds
        // table holds the threshold and nothing else. Checking it as an ordinary
        // binding would report that normal case as a missing column, which is how
        // the designed sentence would have failed.
        //
        // The two relaxed marks answer it differently because they read position
        // differently. A `rule` reads *one* of the plot's position columns, so
        // only the other is skipped. A `zone` whose sides are named by `bounds`
        // reads **neither** — its four sides are its own columns — so both are
        // skipped and `check_bounds` owns its whole position story, and a plain
        // zone over a slot is left to `check_zone_extent` the same way.
        //
        // **Any other zone reads its positions like any other mark.** A density,
        // a bin, a tally, a reduction or a partition reads the columns its `x` and
        // `y` name, and skipping them let `zone * density + y(petal_width)` over a
        // table with no such column draw an empty panel in silence, where
        // `point` and `path * density` refuse the same misspelling. A written
        // axis (a partition's `depth`) is still spared, by the exemption below.
        let spans_axis = |channel: &Channel| {
            if !matches!(channel, Channel::X | Channel::Y) {
                return false;
            }
            match mark {
                Mark::Zone => layer.transforms.contains(&Transform::Bounds)
                    || (layer.transforms.is_empty() && !layer.encodings.contains_key(&Channel::Group)),
                Mark::Rule => df.map_or(true, |d| rule_axis(spec, d, layer).as_ref() != Some(channel)),
                _ => false,
            }
        };

        // --- bindings that are present ------------------------------------
        for (channel, field) in &bound {
            // Asked first, before any exemption can take an empty name for one it
            // spares: `no_column_message`.
            if field.is_empty() {
                out.push(Diagnostic {
                    kind: DiagnosticKind::Illegal,
                    message: no_column_message(layer, channel, &synth_axis),
                });
                continue;
            }
            if spans_axis(channel) {
                // **A plain zone still reads its positions, as slots**, so what is
                // stated about their scale is still checked. `check_zone_extent`
                // owns where the sides come from, but skipping the scale checks
                // with the rest let `zone + x(year, scale = "category")` over a
                // number draw bands across the panel in silence, where `point`
                // and `bar` refuse the same binding (§12). A `bounds` zone reads
                // neither position and stays skipped.
                if matches!(mark, Mark::Zone) && !layer.transforms.contains(&Transform::Bounds) {
                    if let (Some(d), Some(def)) = (df, binding_of(spec, layer, channel)) {
                        if let Some(actual) = actual_type(d, field) {
                            let c = channel_name(channel);
                            check_scale(&mut out, def, c, field, d, actual, channel, true);
                            check_limits(&mut out, def, c, field, actual, channel);
                            check_tick_count(&mut out, def, c, field, actual, channel);
                            check_free(&mut out, def, c, field, channel, spec);
                        }
                    }
                }
                continue;
            }
            let c = channel_name(channel);
            let r = rule_for(mark, channel);

            // A layer the graph places takes its positions from the layout, and its
            // own refusals say so: `check_layout` for a bound position under `layout`
            // and for an `edge` with none, `check_network` for a network nothing
            // feeds. Refused here as well, an `edge` bound to `x` was sent to every
            // flat mark that maps `x`.
            if r.obligation == Obligation::Cannot
                && matches!(channel, Channel::X | Channel::Y | Channel::Z)
                && placed_by_the_graph(layer)
            {
                continue;
            }

            if r.obligation == Obligation::Cannot {
                out.push(Diagnostic {
                    kind: DiagnosticKind::Illegal,
                    // **Two refusals, because `Cannot` covers two different facts.**
                    // `SET_ONLY` is `Cannot` *with* `settable`: the mark has the feature
                    // and cannot vary it row by row. Telling that reader the mark "has no
                    // {c} feature" is false, and it sends them looking for another mark
                    // when the value they wanted is one `style()` away.
                    //
                    // **The direction is safe by construction, which is rare here.** §12
                    // records that "is the direction itself legal?" is the property
                    // nothing checks — a `surface` refusal once recommended a sentence the
                    // engine also refused. This one is gated on `settable`, the same flag
                    // `mark_takes_setting` reads and the Mark × Setting grid is generated
                    // from, so it can only name a setting the mark actually takes.
                    message: if r.settable {
                        format!(
                            "gog: `{c}` cannot be bound to `{m}` — {} {m} takes one {c} for \
                             the whole layer, not one per row. Drop the `{field}` mapping and \
                             set it with `style({c} = )`, or use a mark that maps `{c}`: {}.",
                            article(m), code_list_or(&marks_that_take(channel, true))
                        )
                    } else {
                        format!(
                            // **The direction names the column, never a call.** The engine
                            // is not told which binding built the request — the wire spec
                            // carries no field naming the caller — so a spelled call here
                            // reaches a Python reader in R's idiom, telling them to remove
                            // `opacity(life)` when they wrote `opacity(col.life)`. Naming the
                            // channel and the column separately is idiom-free and reads
                            // correctly in all four. A refusal a *binding* raises may still
                            // spell a call, because a binding knows its own reader.
                            //
                            // `style({c} = )` in the branch above is not a counter-example:
                            // it is a setting rather than a call the four bindings spell
                            // differently, and `check_style` already writes it that way.
                            "gog: `{c}` cannot be bound to `{m}` — {} {m} has no {c} feature. \
                             Remove the `{field}` mapping from `{c}`, or use a mark that maps \
                             it: {}.",
                            article(m), code_list_or(&marks_that_take(channel, true))
                        )
                    },
                });
                continue;
            }

            // `decimals` is judged here, before the exemptions below pass over a
            // written column: `label(name, decimals = 1)` under `partition` names
            // text as surely as a table's column of names does.
            if let Some(def) = binding_of(spec, layer, channel) {
                let kind = df.and_then(|d| actual_type(d, field))
                    .or_else(|| written_column(spec, layer, field));
                check_decimals(&mut out, def, c, field, kind, channel);
                check_short(&mut out, def, c, field, kind, channel);
                check_percent(&mut out, def, c, field, kind, channel);
                let is_time = df.is_some_and(|d| d.time_unit(field).is_some());
                check_number_place(&mut out, spec, layer, def, c, field, channel, is_time);
            }

            // `group` and `play` split the table's rows before any transform runs,
            // so a name a transform writes is not there yet to split by. Before the
            // exemptions below, which pass over a node's `name` and `degree`.
            if matches!(channel, Channel::Group | Channel::Play) {
                if let Some(chain) = written_after_split(spec, data, Some(layer), field) {
                    let (into, verb) = match channel {
                        Channel::Play => ("cuts the table's rows into frames", "cut"),
                        _ => ("splits the table's rows into groups", "split"),
                    };
                    out.push(Diagnostic {
                        kind: DiagnosticKind::Illegal,
                        message: format!(
                            "gog: `{c}({field})` names the `{field}` that `{chain}` writes, \
                             but `{c}` {into} before any transform runs, so there is no \
                             `{field}` yet to {verb} by. Name a column of the table: \
                             `{c}(<column>)`."
                        ),
                    });
                    continue;
                }
            }

            // A name the transform writes, when the table holds one by that name too:
            // the written column is meant, said in a note, and a column another
            // channel reads under that name is the one refusal.
            let filled = *channel == Channel::Label && !label_written.get(li).copied().unwrap_or(true);
            let clash = !filled
                && check_written_name_clash(&mut out, spec, layer, df, &bound, channel, field);

            // A synthesizing transform *writes* y: `y(count)` names the output
            // column, not an input. There is nothing in the data to check it
            // against, so it is checked against the measure's own name instead, and
            // a column the table holds is refused too: `synth_axis_names_a_column`,
            // unless that column has the measure's own name, which is the clash
            // answered just above.
            // The **violin** is the one layer carrying a synthesizing transform that
            // writes to no axis at all: both positions name real columns (the slot and
            // the measure) and the estimate rides a width instead. So the exemption is
            // withheld here and `y(life)` is type-checked like any other binding — a
            // misspelled measure column on a violin would otherwise pass unremarked.
            if *channel == synth_axis && synthesizes_measure(&layer.mark, &layer.transforms)
                && slot_density(spec, layer, df).is_none() {
                if clash {
                    continue;
                }
                if let Some(message) =
                    df.and_then(|d| synth_axis_names_a_column(spec, layer, d, channel, field))
                {
                    out.push(Diagnostic { kind: DiagnosticKind::Illegal, message });
                }
                continue;
            }

            // The same exemption one channel over. A two-dimensional reading writes
            // its measurement to `color` rather than to an axis — both positions are
            // spoken for — so `color(count)`/`color(density)`/`color(level)` names an
            // output column too. `check_field` has already refused every *other*
            // field here, so reaching this line means the name is the synthesized one.
            if *channel == Channel::Color && measures_cells(&layer.mark, &layer.transforms) {
                continue;
            }

            // The same exemption a second time, for a reading that measures with `z`
            // and is asked to ramp by that measurement too: `surface * density +
            // color(density)` names the estimate the sheet is already raised by. It
            // is an output column exactly as a `zone`'s is, so refusing it as a
            // misspelling was `density` behaving differently under one mark than
            // under another, and the message sent the reader after a typo that was
            // not there. The redundancy is the volcano's, which the manual argues
            // for: a ramp says the heights out loud where slope shading leaves them
            // to be inferred. Taste is the reader's (Law 8), and grammar does not
            // refuse a legal sentence for being emphatic.
            //
            // **Named rather than blanket**, so `color(<anything else>)` keeps the
            // ordinary missing-column check — the exemption is for this layer's own
            // measurement, not for every color binding on a mark that has one.
            if *channel == Channel::Color && measurement_column(spec, layer) == Some(*field) {
                // Only where the mark's color can actually carry a number. A 3-D
                // `bar` measures with `z` too and takes a **categorical** color only
                // (a column is a region, `area`'s ruling), so exempting it here would
                // hand the renderer a binding it drops in silence — §12's silent
                // letter, and the one failure the manual promises never happens. It
                // gets the refusal it always deserved instead of a spelling hint.
                if r.renders.is_some_and(|v| v.accepts(VarType::Continuous)) {
                    continue;
                }
                out.push(Diagnostic {
                    kind: DiagnosticKind::Illegal,
                    message: format!(
                        "gog: `{c}({field})` names the {field} this {m} measures, but \
                         `{c}` on `{m}` needs a categorical (text) column. That \
                         measurement is already the height here, so drop \
                         `{c}({field})` and read it off the {field} axis."
                    ),
                });
                continue;
            }

            // The same exemption a third time, and this one is on `label`. A
            // partition writes a node's own name into a column of its own, so
            // `text * partition(…) + label(name)` names an *output* — the counterpart
            // of `color(count)` one channel over, for a mark that measures with
            // neither an axis nor a ramp but with a string. The other columns it
            // publishes (`depth`, and the levels carried down) are real inputs or are
            // checked on their own axes, so only this one needs saying.
            if *channel == Channel::Label
                && (layer.transforms.contains(&Transform::Partition)
                    || layer.transforms.contains(&Transform::Flow)
                    || layer.transforms.contains(&Transform::Layout))
                && *field == crate::transform::NODE_NAME
            {
                continue;
            }

            // The same exemption for the two columns a layout publishes about
            // its nodes: `size(degree)` and `color(degree)` name the neighbor
            // count the layout counted, and `color(name)` names the node — all
            // outputs, `color(count)`'s counterpart on the node readers.
            if layer.transforms.contains(&Transform::Layout)
                && matches!(layer.mark, Mark::Point | Mark::Text)
                && (*field == crate::transform::NODE_NAME
                    || *field == crate::transform::NODE_DEGREE)
            {
                continue;
            }

            let Some(df) = df else { continue };
            // **A column the layer's transforms write is a column** (spec §8, the
            // column rules' first). Named on a channel the renderer does not draw it
            // on yet, it was refused as "not in the data. Check the spelling", which
            // sent the reader after a typo that was not there. It is judged by the
            // channel's own rule instead, by the kind of value the transform writes:
            // the wrong kind gets the type refusal below, and the right kind is
            // refused as not built, which is true and keeps the pair open.
            let written = match actual_type(df, field) {
                Some(_) => None,
                None => written_column(spec, layer, field),
            };
            let Some(actual) = actual_type(df, field).or(written) else {
                out.push(Diagnostic {
                    kind: DiagnosticKind::Illegal,
                    message: written_for_other_rows(layer, c, field)
                        .or_else(|| replaced_by_a_share(spec, layer, df, &channel, field))
                        .or_else(|| share_not_written(spec, layer, df, &channel, field))
                        .unwrap_or_else(|| missing_column_message(c, m, field, &channel)),
                });
                continue;
            };
            if written.is_some() && names_a_written_column(spec, layer, Some(df), channel, field) {
                continue;
            }
            let chain = std::iter::once(m)
                .chain(layer.transforms.iter().map(transform_name))
                .collect::<Vec<_>>().join(" * ");

            // **A violin reads its orientation off the bindings**, exactly as
            // `bar`/`box`/`interval` do (§6), so the sideways form — the category on
            // `y`, with room for long names — is a plot rather than a mistake. The
            // static rule table cannot say that, because it is keyed on (mark,
            // channel) and this is a fact about the *transform*: an `area`'s `y` is
            // its measure in every other reading, and the refusal below is right
            // there. `slot_density` has already established that the two positions
            // are one category and one number, which is the whole of what this check
            // would ask, so skipping it drops nothing.
            let violin_position = matches!(channel, Channel::X | Channel::Y)
                && slot_density(spec, layer, Some(df)).is_some();

            if !r.accepts.accepts(actual) && !violin_position {
                // The direction, where the type alone does not imply it. Keyed on the
                // mark as well as the channel since `surface` joined: which column
                // type a *position* wants is a fact about the geometry, so the fix is
                // a different mark rather than a different channel — the one shape
                // this hint could not express while it matched on the channel alone.
                //
                // **An aesthetic's hint names the channels that take this column on
                // this mark**, read off `rule_for`, the same table that refused it. It
                // was keyed on the channel alone, so `size(<category>)` offered
                // `pattern` to a `point`, a numeric `pattern` offered `size` and
                // `color` to a `bar`, which refuses both, and `bar + color(<number>)`,
                // with no arm at all, was given no direction. Where no channel on the
                // mark takes the type, the message says what `{c}` needs and no more.
                // A written column's direction names only a channel that draws it,
                // or the hint would send the reader to a refusal of its own.
                let mut carriers = channels_that_carry(mark, channel, actual);
                if written.is_some() {
                    carriers.retain(|name| ALL_CHANNELS.iter().any(|ch| channel_name(ch) == *name
                        && names_a_written_column(spec, layer, Some(df), ch, field)));
                }
                let hint: std::borrow::Cow<'static, str> = match (mark, channel, actual) {
                    (_, Channel::Color | Channel::Size | Channel::Shape | Channel::Pattern
                        | Channel::Opacity, _) if !carriers.is_empty() => {
                        let job = match actual {
                            VarType::Continuous => "show a numeric column",
                            _ => "distinguish categories",
                        };
                        format!(" Use {} to {job}.", code_list_or(&carriers)).into()
                    }
                    // A face spans the gap between two samples and so asserts every
                    // value in it; between two categories there is no value to assert.
                    // A column *stands in* its category's cell and claims nothing
                    // about the space between cells, which is why the same floor a
                    // surface refuses is exactly what the 3-D histogram is drawn on.
                    // *The direction said `bar * bin` until 2026-07-28, and that
                    // sentence is itself refused* — `bin` cuts a **continuous** axis,
                    // so a reader who followed it hit a second wall naming the same
                    // category. Over two categorical positions the transform that
                    // makes cells is `count`, which tallies into the slots the
                    // categories already are. The book's copy of this sentence had
                    // been corrected earlier the same day and the engine's had not,
                    // which is the drift a message duplicated in prose always risks.
                    // *It said "for a mesh over categories" until 2026-09-24*, and a
                    // mesh is what a surface's faces make, the one thing a column plot
                    // is not; the direction is the columns, named as columns.
                    (Mark::Surface, Channel::X | Channel::Y, VarType::Discrete) => {
                        " A face spans the gap between two samples, and between two \
                         categories there is nothing to span. Over two categories, stand \
                         a column in each cell instead: `bar * count + x(<a>) + y(<b>) + \
                         space()` — a column stands in its own cell and claims nothing in \
                         between.".into()
                    }
                    // **The path family's measure axis**, and this arm exists because
                    // these four were documented as "refused with direction" while
                    // the message gave none — a claim the engine did not meet, found
                    // by auditing the messages the book actually
                    // prints. The direction is worth giving because the fix is not
                    // "supply a number": it is *which axis the category belongs on*.
                    // A path's two axes have fixed roles (spec §6) — `x` is the domain
                    // and `y` the measure — so a category on `y` is nearly always a
                    // category that wanted to be the domain, and saying so turns a
                    // dead end into the profile plot.
                    (
                        Mark::Line | Mark::Step | Mark::Area | Mark::Ribbon,
                        Channel::Y,
                        VarType::Discrete,
                    ) => {
                        " On these marks `x` is the domain and `y` the measure, and a \
                         category is not a quantity to measure: a mean of category names \
                         is not a number, and a region has no categorical baseline to \
                         close on. Put the category on `x` instead — \
                         `line * mean + x(<category>) + y(<number>)` is the profile plot, \
                         and `area * mean` fills it. Unlike `bar`/`box`/`interval`, these \
                         marks do not read their orientation off the bindings, because \
                         their two axes do not have the same role.".into()
                    }
                    _ => "".into(),
                };
                let what = if written.is_some() {
                    format!("names the `{field}` that `{chain}` writes, a {} column,", actual.describe())
                } else {
                    format!("maps a {} column,", actual.describe())
                };
                out.push(Diagnostic {
                    kind: DiagnosticKind::Illegal,
                    message: format!(
                        "gog: `{c}({field})` {what} but `{c}` on `{m}` needs a {} \
                         column.{hint}",
                        r.accepts.describe(),
                    ),
                });
                continue;
            }

            // The right kind on a channel that takes it, and not drawn there yet:
            // §12's Unsupported, the pair kept open rather than called a typo.
            if written.is_some() {
                // A tally, a share or a mean is arithmetic a reader can do on the
                // table; an estimate, a ring or a merge height is not, and leaving the
                // binding out is the sentence that draws.
                let computable = matches!(*field, crate::transform::CELL_COUNT
                    | crate::transform::CELL_SHARE | crate::transform::SPAN_CENTER);
                let instead = if computable {
                    format!(" To draw it now, compute `{field}` where your data lives and \
                             map that table's column.")
                } else {
                    format!(" Until then, leave `{c}({field})` out.")
                };
                out.push(Diagnostic {
                    kind: DiagnosticKind::Unsupported,
                    message: format!(
                        "gog: `{c}({field})` names the `{field}` that `{chain}` writes. It is \
                         a column like any other, and `{c}` on `{m}` takes it, but gog does \
                         not draw it on `{c}` yet.{instead}"
                    ),
                });
                continue;
            }

            // The right kind on a channel that takes it, read from rows the
            // transform replaced: a number of your table the drawn rows do not hold.
            if !clash {
                if let Some(message) = rows_lack_column(spec, layer, df, channel, field) {
                    out.push(Diagnostic { kind: DiagnosticKind::Illegal, message });
                    continue;
                }
            }

            // A scale override, when there is one. The raw column is the right
            // thing to judge it against *except* on the axis a transform writes,
            // where the values that will be scaled do not exist yet.
            if let Some(def) = binding_of(spec, layer, channel) {
                check_scale(
                    &mut out, def, c, field, df, actual,
                    channel,
                    layer.transforms.is_empty() || *channel != synth_axis,
                );
                // A stated domain is a scale property, so it is judged against
                // the same two facts and on the same axis-role distinction.
                check_limits(&mut out, def, c, field, actual, channel);
                // A stated tick count is a scale property too, but on a narrower
                // set — `limits` needs a domain and this needs an axis.
                check_tick_count(&mut out, def, c, field, actual, channel);
                // Narrower still: only `play` has a duration to scale.
                check_speed(&mut out, def, c, field, channel);
                // A free scale needs an axis to free *and* panels to free it
                // across, so it is judged against the plot as well as the binding.
                check_free(&mut out, def, c, field, channel, spec);
            }

            match r.renders {
                // `z` is the one channel whose blanks are not all one thing, so it
                // does not take the generic wording. Three different sentences are
                // true of the marks that refuse it, and a message that says "not
                // drawn yet" for a **decided** refusal promises a feature the design
                // has already declined — the same defect as a book chunk claiming a
                // refusal that stopped happening, one layer down. See `z_refusal`.
                // A rule is refused as the plane its one position makes, named by
                // that position; a layer naming two is `check_rule`'s to refuse,
                // since there is no one plane to name.
                None if *channel == Channel::Z && *mark == Mark::Rule => {
                    let own: Vec<(&str, &str)> = [Channel::X, Channel::Y, Channel::Z].iter()
                        .filter_map(|ch| layer.encodings.get(ch)
                            .map(|d| (channel_name(ch), d.field.as_str())))
                        .collect();
                    let placed = match own.as_slice() {
                        [one] => Some(*one),
                        [] => rule_axis(spec, df, layer)
                            .and_then(|ch| spec.axis_def(&ch)
                                .map(|d| (channel_name(&ch), d.field.as_str())))
                            .or(Some(("z", *field))),
                        _ => None,
                    };
                    if let Some((axis, placed_by)) = placed {
                        out.push(Diagnostic {
                            kind: z_refusal_kind(mark),
                            message: rule_plane(axis, placed_by),
                        });
                    }
                }
                None if *channel == Channel::Z => out.push(Diagnostic {
                    kind: z_refusal_kind(mark),
                    message: z_refusal(mark, field),
                }),
                // A layout's `play` is refused by `graph.rs` toward `| facet()`, the
                // one way to the frames; the general words would only repeat it with
                // "remove it" as the fix.
                None if *channel == Channel::Play && placed_by_the_graph(layer) => {}
                None => out.push(Diagnostic {
                    kind: DiagnosticKind::Unsupported,
                    message: format!(
                        "gog: `{c}` is valid grammar for `{m}`, but this engine does not draw \
                         it — `{c}({field})` would have no visual effect. \
                         Remove it, or use a channel that renders."
                    ),
                }),
                // The violin's slot again, and skipped for the same reason one check
                // up: the rule table describes the mark, this is the transform's
                // reading, and the pair has already been established well-formed.
                Some(_) if violin_position => {}
                Some(supported) if !supported.accepts(actual) => out.push(Diagnostic {
                    kind: DiagnosticKind::Unsupported,
                    message: format!(
                        "gog: `{c}({field})` on `{m}` is valid grammar, but this engine does \
                         not draw it: `{field}` is a {} column, and `{c}` on `{m}` draws {} \
                         only.",
                        actual.describe(),
                        supported.describe(),
                    ),
                }),
                Some(_) => {}
            }
        }

        // --- required bindings that are missing ---------------------------
        for channel in [Channel::X, Channel::Y, Channel::Label] {
            let r = rule_for(mark, &channel);
            if r.obligation != Obligation::Must {
                continue;
            }
            if bound.iter().any(|(c, _)| *c == channel) {
                continue;
            }
            // A synthesizing transform supplies y itself.
            if channel == synth_axis && synthesizes_measure(&layer.mark, &layer.transforms) {
                continue;
            }
            // Law 7's three relaxations of a missing `x`, stated once in
            // `x_needs_no_binding`. It was extracted because a warning in the
            // renderer asked the same question from its own shorter list and so
            // libeled plots this check had blessed; that warning is gone now, and
            // the extraction is what makes the question answerable in one place.
            // The third of them is `nest`'s, added 2026-07-27 with `text`: `bar`
            // reads `Can` on `x` in every space and so never asked, while `text`
            // reads `Must` because flat a glyph has to be put *somewhere*. In a
            // packing nothing is put anywhere — a row gets a **region**, and `x` is
            // what subdivides the panel when it is bound at all.
            if channel == Channel::X && x_needs_no_binding(spec, layer, df) {
                continue;
            }
            // The horizontal pile needs no `y`, as the upright one needs no `x`.
            if channel == Channel::Y && bar_is_one_pile_sideways(spec, layer, df) {
                continue;
            }
            // `y` relaxes for a partition alone, and never for the packing: there
            // `y` is the **measure**, the one thing a space with no coordinates
            // cannot do without. A partition is the other way about — it supplies
            // both positions, so naming either is optional and still means
            // something when you do (`x(amount)` weighs the arcs, `y(depth,
            // limits = c(0, 4))` is the hole).
            if channel == Channel::Y && layer.transforms.contains(&Transform::Partition) {
                continue;
            }
            // A flow's `y` is the weight, and the tally is its default — the
            // same optionality `partition` has, for the same reason: the
            // transform supplies both positions and a binding refines one.
            if channel == Channel::Y && layer.transforms.contains(&Transform::Flow) {
                continue;
            }
            // And its `x`, which carries the count when the flow runs top to bottom
            // and is refused by `check_flow` in every other use.
            if channel == Channel::X && layer.transforms.contains(&Transform::Flow) {
                continue;
            }
            // In the network every position is the layout's, so `y` stands
            // down for the same reason `x` does one arm up.
            if channel == Channel::Y && space_of(spec) == SpaceKind::Network {
                continue;
            }
            // A layer the graph places takes both positions from its `layout`,
            // and outside `network()` its own refusal says so.
            if matches!(channel, Channel::X | Channel::Y) && placed_by_the_graph(layer) {
                continue;
            }
            // A transform the mark does not take is refused on its own, with the
            // mark that does take it. A missing `y` beside that refusal sent the
            // reader to add a column the refused sentence has no use for:
            // `line * cluster(amount, over = nutrient) + x(food)` said "Add
            // `y(<column>)`", and adding one reached only another refusal.
            if channel == Channel::Y && layer.transforms.iter().any(|t| {
                !is_collision_modifier(t)
                    && mark_takes_transform(&layer.mark, t) == TransformLegality::None
            }) {
                continue;
            }
            let c = channel_name(&channel);
            // **A statistic that reads a column says which one it is short of.**
            // The general message, "`bar` needs `y()`", sent `bar * bin * mean +
            // x(life)` looking for a reason the bar needed a column, when the
            // reason is that `mean` averages one. A column a transform *writes*
            // may be left out (`bar * bin`); a column it *reads* may not, and the
            // message now says so in the statistic's own words, with the counting
            // reading beside it where the mark takes one.
            // A running total reads the column too, when nothing before it measures:
            // it adds up what the layer measures and invents nothing (spec §5).
            let reader = layer.transforms.iter().find(|t| {
                reads_a_column(t)
                    || matches!(t, Transform::Smooth | Transform::SmoothBand | Transform::Cumulative)
            });
            let message = match reader {
                Some(t) if channel == synth_axis => {
                    let tn = transform_name(t);
                    // The chain as written: a `box` carries its summary without
                    // the reader typing it, and "`box * box` needs `y()`" named a
                    // sentence nobody wrote.
                    let chain = std::iter::once(m)
                        .chain(layer.transforms.iter()
                            .filter(|t| !(layer.mark == Mark::Box && **t == Transform::Box))
                            .map(transform_name))
                        .collect::<Vec<_>>().join(" * ");
                    let does = match t {
                        Transform::Smooth | Transform::SmoothBand => "fits a curve through",
                        Transform::Sum => "adds up",
                        Transform::Mean => "averages",
                        Transform::Median => "takes the median of",
                        Transform::Max => "takes the largest value of",
                        Transform::Min => "takes the smallest value of",
                        Transform::Quantile => "takes a quantile of",
                        Transform::Range => "spans the values of",
                        Transform::Confidence | Transform::Deviation =>
                            "puts an interval around the mean of",
                        Transform::Cumulative => "adds up, position by position,",
                        _ => "reads",
                    };
                    // Counting is the other reading of a summary, not of a fitted
                    // curve or an interval, and it is offered only where the mark
                    // takes it: by bin over numbers, by slot over categories.
                    let summary = crate::transform::has_reduction(std::slice::from_ref(t));
                    let numeric_x = spec.position_for(layer, &Channel::X).zip(df)
                        .is_some_and(|(d, df)| actual_type(df, &d.field) == Some(VarType::Continuous));
                    let takes = |t: &Transform| mark_takes_transform(&layer.mark, t)
                        != TransformLegality::None;
                    let offer: Option<(String, Vec<Transform>)> = if *t == Transform::Cumulative {
                        // The rows themselves can be added up, once they are counted.
                        if numeric_x && takes(&Transform::Bin) {
                            Some(("To add up the rows in bins of `x` instead, write ".into(),
                                  vec![Transform::Bin, Transform::Cumulative]))
                        } else if !numeric_x && takes(&Transform::Count) {
                            Some(("To add up the rows in each slot instead, write ".into(),
                                  vec![Transform::Count, Transform::Cumulative]))
                        } else {
                            None
                        }
                    } else if !summary {
                        None
                    } else if layer.transforms.contains(&Transform::Bin) {
                        Some((format!("To count the rows in each bin instead, drop `{tn}`: "),
                              vec![Transform::Bin]))
                    } else if numeric_x && takes(&Transform::Bin) {
                        Some(("To count the rows in bins of `x` instead, write ".into(),
                              vec![Transform::Bin]))
                    } else if !numeric_x && takes(&Transform::Count) {
                        Some(("To count the rows in each slot instead, write ".into(),
                              vec![Transform::Count]))
                    } else {
                        None
                    };
                    // **The counting sentence is offered only when it draws** (2026-10-14),
                    // checked alone in the plot as the missing-position hint below is:
                    // `line * mean + x(continent) + color(continent)` was sent to
                    // `line * count`, whose one-row series draw nothing, and `bin * mean`
                    // over a category to a `bin` that cuts no category. A text is given
                    // the label that prints its count, and says so.
                    let print = if layer.mark == Mark::Text {
                        ", and print each count with `label(count)`"
                    } else {
                        ""
                    };
                    // A `line`, `step` or `area` joins the counts into series, and
                    // whether each series keeps two slots is counted by the renderer,
                    // after this check, so the trial cannot see it. Counting by slot
                    // is offered to them only when nothing splits the layer, where
                    // the one series runs through every slot.
                    let joins = matches!(layer.mark, Mark::Line | Mark::Step | Mark::Area);
                    let split = [Channel::Color, Channel::Group, Channel::Pattern].iter().any(|ch| {
                        layer.encodings.get(ch).or(spec.channels.get(ch)).is_some_and(|d| {
                            *ch != Channel::Color || df.is_some_and(|df| df.str_col(&d.field).is_some())
                        })
                    }) || spec.facet.is_some()
                        || layer.encodings.contains_key(&Channel::Play)
                        || spec.channels.contains_key(&Channel::Play);
                    let count = offer
                        .filter(|(_, ts)| !(joins && split && ts.contains(&Transform::Count)))
                        .filter(|(_, ts)| {
                            let mut l = layer.clone();
                            l.transforms = ts.clone();
                            if l.mark == Mark::Text {
                                l.encodings.insert(Channel::Label,
                                    ChannelDef::field(crate::transform::CELL_COUNT));
                            }
                            let mut trial = spec.clone();
                            trial.layers = vec![l];
                            trial_draws(&trial, data)
                        })
                        .map_or(String::new(), |(lead, ts)| {
                            let chain = std::iter::once(m).chain(ts.iter().map(transform_name))
                                .collect::<Vec<_>>().join(" * ");
                            format!(" {lead}`{chain}`{print}.")
                        });
                    // **A split that leaves each series one position** (2026-10-14, the
                    // 0.6.1 round's A32): with `y()` added as asked, `line * mean +
                    // x(continent) + color(continent)` was refused in turn, since each
                    // line then held one continent. A summary makes one position per
                    // value of the other axis in each series, so the table's rows
                    // already say how many each series will hold. The renderer still
                    // counts them for the refusal; this only words the direction.
                    let other = if channel == Channel::Y { Channel::X } else { Channel::Y };
                    let lone = (summary && matches!(layer.mark, Mark::Line | Mark::Step | Mark::Area))
                        .then(|| one_position_per_series(spec, layer, df, &other))
                        .flatten()
                        .map(|split| {
                            let alone = std::iter::once("point")
                                .chain(layer.transforms.iter()
                                    .filter(|t| **t != Transform::Stack)
                                    .map(transform_name))
                                .collect::<Vec<_>>().join(" * ");
                            format!(
                                " Split by {split}, each `{m}` would then hold one position and \
                                 draw nothing, so use `{alone}`, which draws each position on its own."
                            )
                        })
                        .unwrap_or_default();
                    format!(
                        "gog: `{chain}` needs `{c}()`, because `{tn}` {does} the column named \
                         there, and none is set. Add `{c}(<column>)` with that column.{lone}{count}"
                    )
                }
                // A text that counts or summarizes prints its number, and the label
                // is how it says which: the column the transform writes, or the one
                // a summary reduces in place (spec §8).
                _ if channel == Channel::Label && layer.mark == Mark::Text
                    && summary_label(spec, layer, &synth_axis).is_some() =>
                {
                    let (measure, what) = summary_label(spec, layer, &synth_axis)
                        .unwrap_or(("", ""));
                    let chain = std::iter::once(m).chain(layer.transforms.iter().map(transform_name))
                        .collect::<Vec<_>>().join(" * ");
                    let does = if measure == what { "writes" } else { "computes" };
                    format!(
                        "gog: `{chain}` needs `label()` but none is set. A text draws the \
                         string its label names: `label({measure})` prints the {what} \
                         that `{chain}` {does} at each place."
                    )
                }
                _ => {
                    // A text column on the other position is a slot, and the
                    // measure a slot most often wants is its count of rows:
                    // `bar * stack + x(continent) + color(continent)` with no `y`.
                    let other = if channel == Channel::X { Channel::Y } else { Channel::X };
                    let slotted = matches!(channel, Channel::X | Channel::Y)
                        && spec.position_for(layer, &other).zip(df).is_some_and(|(p, df)| {
                            actual_type(df, &p.field) == Some(VarType::Discrete)
                        });
                    // The counted sentence is offered only when it draws, so it is
                    // checked first, alone in the plot: a line split into one-row
                    // series, a line on its side, or a stack with nothing to pile
                    // has a refusal of its own that the count would not cure. A
                    // transform the mark refuses is left out of it, since that
                    // refusal is printed beside this one. Only the marks that draw
                    // one shape per slot are offered it: a `line`, `step` or `area`
                    // joins the counts into series, and whether each series keeps
                    // two of them is a question the renderer answers after this
                    // check, so the sentence could still draw nothing.
                    // **A layer that already counts is not offered the count.** It
                    // cannot cure the missing position, and the trial would recurse:
                    // `bar * count + x(continent) + space()` still needs `y` in the
                    // cube, so each trial wrote one more `count` in front and checked
                    // again, until the engine's stack overflowed (2026-10-14, found
                    // walking transforms against spaces).
                    let counted = (slotted
                        && !layer.transforms.contains(&Transform::Count)
                        && matches!(layer.mark, Mark::Bar | Mark::Point | Mark::Text)
                        && mark_takes_transform(&layer.mark, &Transform::Count)
                            != TransformLegality::None)
                        .then(|| {
                            let mut l = layer.clone();
                            l.transforms = std::iter::once(Transform::Count)
                                .chain(layer.transforms.iter()
                                    .filter(|t| mark_takes_transform(&layer.mark, t)
                                        != TransformLegality::None)
                                    .cloned())
                                .collect();
                            if l.mark == Mark::Text {
                                l.encodings.insert(Channel::Label, ChannelDef::field(crate::transform::CELL_COUNT));
                            }
                            let mut trial = spec.clone();
                            trial.layers = vec![l];
                            trial
                        })
                        .filter(|trial| trial_draws(trial, data));
                    let count = match counted {
                        Some(trial) => {
                            let chain = std::iter::once(m)
                                .chain(trial.layers[0].transforms.iter().map(transform_name))
                                .collect::<Vec<_>>().join(" * ");
                            let print = if layer.mark == Mark::Text {
                                ", and print each count with `label(count)`"
                            } else {
                                ""
                            };
                            format!(" To count the rows in each slot instead, write `{chain}`{print}.")
                        }
                        None => String::new(),
                    };
                    format!(
                        "gog: `{m}` needs `{c}()` but none is set. \
                         Add `{c}(<column>)` — {} {m} cannot be drawn without it.{count}",
                        article(m)
                    )
                }
            };
            out.push(Diagnostic { kind: DiagnosticKind::Illegal, message });
        }

        // --- constant settings from `style()` -----------------------------
        check_style(&mut out, mark, &layer.style, &bound);
        // A color the reading fills by itself is a mapping as much as a written one
        // (spec §8, the column rules' third), so setting it too is the same
        // contradiction `check_style` refuses. Asked here because `bound` holds only
        // what the sentence wrote, and a heatmap's count is in no binding: drawn, the
        // set color painted every cell under a legend that still decoded the count,
        // and on a contour the setting was dropped without a word.
        if let (Some(measure), Some((_, written))) = (
            field_measure(layer),
            layer.style.set_features().into_iter().find(|(ch, _)| *ch == Channel::Color),
        ) {
            if !bound.iter().any(|(ch, _)| *ch == Channel::Color) {
                let chain = std::iter::once(mark_name(mark))
                    .chain(layer.transforms.iter().map(transform_name))
                    .collect::<Vec<_>>().join(" * ");
                out.push(Diagnostic {
                    kind: DiagnosticKind::Illegal,
                    message: format!(
                        "gog: `{chain}` maps color to its `{measure}` by itself, and \
                         `style(color = {written})` sets it — one layer cannot do both. \
                         Drop `style(color = {written})` to keep the `{measure}` colors, \
                         and give `palette()` the colors you want them drawn in."
                    ),
                });
            }
        }
        check_nothing_to_fade(&mut out, mark, layer);
        let edge = ribbon_edge_line(spec, layer, data);
        check_border(&mut out, mark, &layer.style, &layer.transforms, edge.as_deref());
        check_halo_ground(&mut out, mark, &layer.style, spec);
        check_caps(&mut out, mark, &layer.style);
        check_arrow(&mut out, mark, &layer.style);
        check_center(&mut out, mark, &layer.style);
        check_center_exists(&mut out, spec, layer, data);
        check_nudge(&mut out, mark, &layer.style);
        check_angle(&mut out, mark, &layer.style);
        check_reach(&mut out, mark, &layer.style);
        check_gap(&mut out, spec, layer);
        // A mapped `pattern()` and a `style(pattern = )` setting are contradictory
        // — honoring one silently drops the other, exactly the conflict `check_style`
        // catches for `color`/`shape`. Refuse, and skip the value check.
        if let (Some(_), Some((_, field))) =
            (&layer.style.pattern, bound.iter().find(|(ch, _)| *ch == Channel::Pattern))
        {
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: format!(
                    "gog: `pattern({field})` maps the texture and `style(pattern = )` sets it — \
                     one layer cannot do both. Keep `pattern({field})` to vary it by category, \
                     or `style(pattern = )` to fix one texture for every {}.",
                    mark_name(mark)
                ),
            });
        } else {
            check_pattern(&mut out, mark, &layer.style);
        }

        // A summary whose every group is one row summarizes nothing. Legal on the
        // sentence, so only the rows can say it; and last, on a layer nothing
        // refused, since a refused layer already has the one message it needs.
        if let Some(df) = df {
            if !out[layer_from..].iter().any(Diagnostic::is_fatal) {
                check_one_row_groups(&mut out, spec, df, layer,
                    plot_orient(spec, data) == Orient::Horizontal);
            }
        }
    }

    check_palette(&mut out, spec, data);
    check_facet(&mut out, spec, data);

    check_play(&mut out, spec, data);
    // Ahead of the three gates below, because it answers the wider question: they
    // ask which marks stand in a space, and this asks whether the space draws.
    check_coord(&mut out, spec);
    check_brush(&mut out, spec, data);
    check_space(&mut out, spec);
    check_polar(&mut out, spec);
    check_order(&mut out, spec, data);
    check_zone_group(&mut out, spec, data);
    check_globe(&mut out, spec, data);
    check_map(&mut out, spec, data);
    check_nest(&mut out, spec, data);
    check_network(&mut out, spec);
    check_theme(&mut out, spec);

    // A stroke or an outline through several rows at one `x`, joined in x order:
    // a note about how a layer draws, so only when the plot draws at all. Asked
    // after every gate, because the ones that refuse a whole space come last, and
    // `line` on a `globe()` printed this note above its own refusal.
    if !out.iter().any(Diagnostic::is_fatal) {
        for layer in &spec.layers {
            let table = layer.data.as_ref().or(spec.data.as_ref()).and_then(|name| data.get(name));
            if let Some(df) = table {
                check_joined_rows(&mut out, spec, df, layer);
            }
        }
        // "So it is drawn flat" is a note about how the plot draws too, and it stood
        // beside refusals that draw nothing: `bar * mean + x(<a>) + y(<b>) + space()`,
        // whose refusal already asks for `z`, and a `surface` refused for having no
        // cube to stand in.
        note_space_drawn_flat(&mut out, spec);
    }

    // **One sentence is said once.** A check that runs per layer repeats itself for
    // every layer it holds for: a network drawn as edges, nodes and names printed
    // its self-loop refusal three times. The renderer's notes were always kept to
    // one of each by their text, and the check's now are too, first kept first.
    let mut seen = std::collections::HashSet::new();
    out.retain(|d| seen.insert(d.message.clone()));
    out
}

#[cfg(test)]
mod tests;
