//! The transforms that place marks by a structure in the rows: partitions,
//! flows, layouts and clusters.

use super::*;

/// A `box` carries its own five-number summary (spec §6), so — unlike `interval`,
/// which *requires* a range transform — it takes **none**. `resolve_scopes`
/// injects `Transform::Box`; any further *summary* the user composed onto a box
/// (`box * mean`) is refused with direction rather than silently run before the
/// summary. **All three collision modifiers are exempt here** and judged by their
/// own checks, which give geometry-aware direction: `box * dodge` is legal (the
/// grouped boxes are set side by side), while `box * stack` and `box * jitter` are
/// refused by `check_stack`/`check_jitter` toward `dodge` (a box subdivides its
/// slot, it does not pile or scatter) rather than by the blunter "box takes no
/// transform" here (spec §5). This owns only "a `box` takes no *statistic*."
/// `path` takes no transform, and the refusal explains why rather than only
/// saying no.
///
/// This is `check_box`'s mirror image, and the pair is worth reading together:
/// `box` refuses every transform because it *already carries* one, while `path`
/// refuses every transform because there is nothing left of it afterwards. A
/// value statistic reduces each key to a single row, so what would remain is one
/// point per key in key order — which is a `line`, drawn the long way round. A
/// pair transform emits two rows per key, a span rather than a route. And the
/// collision modifiers each need geometry a path lacks: a width, a baseline, a
/// cloud. So every arm of the grid's `None` for `path` has the same cause, and
/// the direction is always the same mark.
/// The marks with an **empty** transform row, refused here so the grid's `—` is
/// a promise the engine keeps.
///
/// Three marks are in this set, and their reasons rhyme: a transform replaces the
/// rows, and each of these three needs something about the *original* rows that
/// does not survive that. `path` needs their order. `rule` has no measure to
/// compute, having handed one whole axis to the panel. `text` needs a string per
/// row, and a summary per key has none.
///
/// Checked here rather than left to the transform stage for the reason spec §12
/// gives: a transform can only warn and then hand the renderer whatever it made,
/// and warn-then-draw is the silent drop the grammar refuses. Both non-`path`
/// arms were found by `every_none_cell_of_the_transform_grid_actually_refuses`
/// and both were live defects — `rule * mean` warned twice about a missing `x`
/// and drew an empty panel, and `text * mean` drew nine glyphs with no complaint
/// at all, for combinations the book already prints as impossible.
///
/// **The collision modifiers are exempt**, exactly as they are in `check_box`,
/// and for the same reason: their own checks give geometry-aware direction
/// (`dodge` wants a width, `stack` a baseline, `jitter` a cloud), where this
/// function can only say "no transform". Not exempting them was a live wording
/// bug: `path * dodge` used to answer with *both* messages, the first of them
/// claiming `dodge` "replaces those rows with one summary per key", which a
/// collision modifier does not do.
/// Everything a hierarchy can be wrong about, answered in one place.
///
/// **Why it is a check and not a warning inside the transform** (§12, and the
/// `bin`-on-a-category lesson): a column-type question is legality, and a
/// transform that discovers one can only warn and then hand the renderer an empty
/// frame — the warn-then-draw silent drop the grammar refuses. Every branch here
/// is fatal and names what to do.
///
/// The five failures, in the order a caller meets them: the mark cannot read a
/// partition; no levels were named; a level is missing or is not a category; a
/// branch has a **hole** in it; and an interior node carries a value of its own,
/// which is the one genuine ambiguity in the set and the reason Law 5 makes it a
/// refusal rather than a default.
pub(super) fn check_partition(
    out: &mut Vec<Diagnostic>, spec: &PlotSpec, df: Option<&DataFrame>, layer: &Layer,
) {
    if !layer.transforms.contains(&Transform::Partition) {
        return;
    }

    // 1. A mark with no reading for a region description. Refused toward the two
    //    that have one, and toward what each of them does with it, so the reader
    //    learns the sentence rather than only its verdict.
    if mark_takes_transform(&layer.mark, &Transform::Partition) == TransformLegality::None {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `partition` divides a whole among nested parts and hands back one \
                 **region** per node — four edges and a center — and `{}` has no reading \
                 for a region. Two marks do: `zone` takes the edges and draws the \
                 rectangle, which in `polar()` is the sector, so `zone * partition(<a>, \
                 <b>) + x(<measure>)` is the icicle and the same sentence `+ polar()` is \
                 the sunburst; and `text` takes the center, so `text * partition(<a>, \
                 <b>) + label(name)` names each node where it sits. A length from a \
                 baseline, a route through its rows, or a low-and-high pair are the \
                 other geometries, and a node's ring is none of them.",
                mark_name(&layer.mark),
            ),
        });
        return;
    }

    // 2. The atom with nothing named. Its own message rather than a missing-column
    //    one, because the caller wrote a word and gave it no arguments.
    let levels: &[String] = match layer.partition.as_ref() {
        Some(p) if !p.levels.is_empty() => &p.levels,
        _ => {
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: "gog: `partition` needs the hierarchy's columns, outermost \
                          first — `partition(group, item, detail)` puts `group` on the \
                          innermost ring and `detail` on the rim. One row of the table \
                          is one leaf, and those columns spell the path down to it."
                    .to_string(),
            });
            return;
        }
    };

    let Some(df) = df else { return };

    // 3. A level that is not there, or is not a category. A number cannot name a
    //    level: a node is a *name* the rows share, and no two rows share a
    //    measurement the way they share a word.
    for l in levels {
        match actual_type(df, l) {
            None => {
                out.push(Diagnostic {
                    kind: DiagnosticKind::Illegal,
                    message: format!(
                        "gog: `partition({l}, …)` names a level the table does not have. \
                         Every level is a column of the same table, outermost first."
                    ),
                });
                return;
            }
            Some(VarType::Continuous) => {
                out.push(Diagnostic {
                    kind: DiagnosticKind::Illegal,
                    message: format!(
                        "gog: `partition` reads `{l}` as a level of the hierarchy, and \
                         `{l}` holds numbers. A level is a **name** the rows of a branch \
                         share; a measurement is what the branch is *weighed* by, which \
                         is `x({l})`. If `{l}` is a code rather than a quantity, make it \
                         text where your data lives."
                    ),
                });
                return;
            }
            // `Either` is a column the data cannot decide about (an all-empty one,
            // in practice), and it is let through rather than refused: a level with
            // no values reads as a branch that ends, which is the ragged rim.
            Some(VarType::Discrete) | Some(VarType::Either) => {}
        }
    }

    // 4 and 5 are facts about the rows, read by the same function the layout uses,
    //   so the refusal and the picture cannot disagree about what the tree is.
    let paths = crate::transform::paths(df, levels);

    if let Some(p) = paths.gap {
        let shown = p.iter().filter(|s| !s.is_empty()).cloned().collect::<Vec<_>>().join(" / ");
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: this table has a branch with a hole in it — a row reaching \
                 `{shown}` with a level above it left empty. A blank level *ends* a \
                 branch, which is what gives a hierarchy its ragged rim, and nothing \
                 below an ending can be reached. Fill the missing level, or move the \
                 row up to the depth it really sits at."
            ),
        });
        return;
    }

    if let Some(p) = paths.interior {
        let shown = p.join(" / ");
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `{shown}` has a value of its own *and* children with values, and \
                 those two readings draw different pictures: either its arc is the \
                 children's total (its own number already counted among them), or its \
                 own number sits beside them and widens it. gog will not pick — the \
                 arithmetic is the accounts', not the grammar's. Put every number on a \
                 **leaf** and let the parents be the sums, which is what a partition \
                 computes."
            ),
        });
        return;
    }

    // **A `y` other than `depth` has no reading here.** A partition places its
    // levels itself, one depth per level, and weighs its branches by `x`. The one
    // column `y` can name is the `depth` the transform computes, which is how a
    // sunburst states its radial domain (`y(depth, limits = c(0, 4))` hollows the
    // middle). Any other `y` was accepted, ignored as data and printed as the axis
    // title: `y(people)` drew an unweighted mosaic whose share axis read "People".
    // So it is refused, and pointed at `x`, where the weight is read.
    if let Some(y) = spec.position_for(layer, &Channel::Y)
        .filter(|y| y.field != crate::transform::NODE_DEPTH)
    {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `y({0})` has no reading under `partition`: a partition places its \
                 levels itself and reads its weight from `x`. Write `x({0})` to weigh the \
                 branches by it, or drop `y({0})`. The one column `y` can name here is \
                 `depth`, which `partition` computes, to state its domain with `limits`.",
                y.field,
            ),
        });
        return;
    }

    // The measure is the bound `x`, so it has to be a number. Refused here rather
    // than by `check_distribution_axis`, whose message is about cutting an axis and
    // would send the caller looking for a bin.
    if let Some(x) = spec.position_for(layer, &Channel::X) {
        if actual_type(df, &x.field) == Some(VarType::Discrete) {
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: format!(
                    "gog: `x({0})` is what each branch is weighed by, and `{0}` holds \
                     categories rather than numbers. Name the column that carries the \
                     amount — `x(<amount>)` — or bind nothing at all, in which case \
                     every leaf weighs 1 and the partition tallies them.",
                    x.field,
                ),
            });
        } else if has_negative(df, &x.field) {
            // `nest()`'s refusal, for the same reason one family over: a branch's
            // share of the whole cannot be negative, and the transform clamped a
            // negative weight to 0, which drew that leaf with no width and shrank
            // the whole it was a share of, in silence.
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: format!(
                    "gog: `x({0})` weighs each branch, and `{0}` has negative values. A \
                     partition turns a weight into a share of the whole, and a share cannot \
                     be negative: those rows would be drawn with no width, and the whole \
                     would shrink. Filter or offset the column, or draw it flat, where a \
                     bar below the baseline reads as a loss.",
                    x.field,
                ),
            });
        }
    }
}

/// Does this column hold a negative number? A weight read as a share, a region
/// or a band's thickness has no reading below zero.
pub(super) fn has_negative(df: &DataFrame, field: &str) -> bool {
    df.float_col(field).is_some_and(|c| c.iter().any(|v| v.is_finite() && *v < 0.0))
}

/// Does this flow run its stages top to bottom? It does when its count is bound
/// to `x` and nothing to `y`: orientation read off the bindings, as `bar`'s is
/// (spec §6), so there is no word for it. A categorical `x` is refused by
/// `check_flow` and never reaches the renderer asking this.
pub(crate) fn flow_runs_down(spec: &PlotSpec, layer: &Layer) -> bool {
    layer.transforms.contains(&Transform::Flow)
        && spec.position_for(layer, &Channel::X).is_some()
        && spec.position_for(layer, &Channel::Y).is_none()
}

/// Every way a flow can be malformed, `check_partition`'s shape one family over.
/// The first branches are facts about the sentence and refuse without a table;
/// the column checks run when the frame is there to ask.
pub(super) fn check_flow(
    out: &mut Vec<Diagnostic>, spec: &PlotSpec, df: Option<&DataFrame>, layer: &Layer,
) {
    if !layer.transforms.contains(&Transform::Flow) {
        return;
    }

    // 1. A mark with no reading for a pair of stage intervals. Refused toward the
    //    three that have one, with the sentence spelled out.
    if mark_takes_transform(&layer.mark, &Transform::Flow) == TransformLegality::None {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `flow` lays a path's magnitude through its stages — an interval \
                 at each stage, and a band between adjacent ones — and `{}` has no \
                 reading for that. Three marks do: `ribbon * flow(<a>, <b>)` draws the \
                 bands, `zone * flow(<a>, <b>)` draws each stage's slots, and `text * \
                 flow(<a>, <b>) + label(name)` names them. Bind `y(<amount>)` to weigh \
                 the paths, or bind nothing and each row counts 1.",
                mark_name(&layer.mark),
            ),
        });
        return;
    }

    // 2. The atom with too little named. One column has no between, so a flow
    //    needs at least two stages — its own message, because the caller wrote a
    //    word and the complaint is about the sentence rather than the table.
    let stages: &[String] = match layer.flow.as_ref() {
        Some(f) if f.stages.len() >= 2 => &f.stages,
        _ => {
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: "gog: `flow` needs at least two stage columns, in reading \
                          order — `flow(class, sex, survived)` runs each row from its \
                          `class` to its `survived`. One row of the table is one path \
                          through every named stage, and one column has no between."
                    .to_string(),
            });
            return;
        }
    };

    // 2b. A stage named twice. The layout places each stage by its column's name,
    //     so a second `class` fell on the first: `zone * flow(class, class)` drew
    //     every slot at the first stage, a ribbon one strip and nothing after it,
    //     and `flow(class, survived, class)` stacked its third stage on its first,
    //     all with no message.
    let mut named = std::collections::HashSet::new();
    if let Some(twice) = stages.iter().find(|s| !named.insert(s.as_str())) {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `{twice}` is named twice in `flow()`. Each stage of a flow is a \
                 column of its own, placed by its name, so the two would fall in one \
                 place. Name each column once, in reading order."
            ),
        });
        return;
    }

    // 2c. A shared flow is a relation between two columns: the place each amount
    //     leaves and the place it arrives at. A third column would make each row a
    //     route through three places on one axis, which this engine does not draw.
    let shared = layer.flow_is_shared();
    let layered = layer.flow_is_layered();
    // 2d. Two answers to where the places stand: one axis, or layers worked out
    //     from the links. A contradiction, so refused rather than ranked.
    if shared && layered {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `flow({}, {})` with both `shared` and `layered` set to true says two \
                 things about where the places stand. `shared` puts every place on one \
                 axis, the arc diagram, and `layered` stands them in layers worked out \
                 from the links, the Sankey diagram. Keep one.",
                stages[0], stages[1]
            ),
        });
        return;
    }
    if layered && stages.len() != 2 {
        out.push(Diagnostic {
            kind: DiagnosticKind::Unsupported,
            message: format!(
                "gog: a `flow` with `layered` set to true and {} columns is valid grammar, \
                 but this engine draws a layered flow between two columns only: the place \
                 each amount leaves, then the place it arrives at, as in `flow({}, {})` \
                 with `layered` set to true.",
                stages.len(), stages[0], stages[1]
            ),
        });
        return;
    }
    if shared && stages.len() != 2 {
        out.push(Diagnostic {
            kind: DiagnosticKind::Unsupported,
            message: format!(
                "gog: a `flow` with `shared` set to true and {} columns is valid grammar, \
                 but this engine draws a shared flow between two columns only: the place \
                 each amount leaves, then the place it arrives at, as in `flow({}, {})` \
                 with `shared` set to true.",
                stages.len(), stages[0], stages[1]
            ),
        });
        return;
    }
    // 2e. A shared or layered flow's names stay where they are. Each is written
    //     beyond the place it names (a shared flow's under the axis or round the
    //     ring, turned when the places are close and never thinned, the author's
    //     ruling of 2026-10-03) or beside it (a layered flow's), so its position is
    //     the only thing saying which place it names. `repel` was accepted on both
    //     and drew the same picture, the silent drop §12 forbids; a stage flow's
    //     names sit inside their slots, where it does move them.
    if (shared || layered) && layer.mark == Mark::Text
        && layer.transforms.contains(&Transform::Repel)
    {
        let (placed, room) = if shared {
            ("a shared flow's names are not moved: each is written beyond the place it \
              names, turned when the places are close",
             "a wider plot gives them room, `theme(width = )`, or `polar()` draws the \
              same flow as a chord diagram, where each name points outward from its place")
        } else {
            ("a layered flow's names are not moved: each stands beside the place it names",
             "a taller plot gives them room, `theme(height = )`")
        };
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `repel` moves labels that overlap, and {placed}, so its position is \
                 what says which place it names. Where narrow places crowd their names, \
                 {room}."
            ),
        });
    }

    // 3. The space. A flow reads its stages along one axis and its magnitude up
    //    the other, which only the flat plane offers. A **shared** flow has one
    //    axis, its count, so `polar()` puts that on the angle, as a pie's, and the
    //    slots make one ring: the chord diagram, drawn. A stage flow bent round a
    //    rim keeps a ring or an angle per stage, a circular alluvial and not the
    //    chord diagram (which this cell once claimed it was, 2026-10-03); it
    //    refuses as unbuilt rather than as wrong, and points at the chord diagram
    //    for two columns that name one set of places.
    match space_of(spec) {
        SpaceKind::Flat => {}
        SpaceKind::Polar if shared => {}
        // Layers bent into rings would be a circular Sankey diagram, which no
        // part of this engine draws; the chord diagram is one word away.
        SpaceKind::Polar if layered => {
            out.push(Diagnostic {
                kind: DiagnosticKind::Unsupported,
                message: format!(
                    "gog: a layered flow in `polar()` is valid grammar, its layers bent \
                     into rings, but this engine does not draw it. Draw it flat, or set \
                     `shared` to true in place of `layered`: `flow({}, {})` in `polar()` \
                     is then the chord diagram, which puts every place on one ring.",
                    stages[0], stages[1]
                ),
            });
            return;
        }
        // Said of the whole flow, not of "the bands": a `zone` draws the stages' slots
        // and a `text` their names, and each layer printed the ribbon's sentence. One
        // sentence for every layer is also said once (`check` keeps one of each).
        SpaceKind::Polar => {
            out.push(Diagnostic {
                kind: DiagnosticKind::Unsupported,
                message: format!(
                    "gog: `flow` in `polar()` is valid grammar, a flow bent round a rim \
                     with a ring for each stage, but this engine does not draw it. Draw \
                     the flow flat. If two columns name one set of places, `flow({}, {})` \
                     with `shared` set to true draws them in `polar()` as the chord \
                     diagram.",
                    stages[0], stages[1]
                ),
            });
            return;
        }
        other => {
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: format!(
                    "gog: `flow` reads its stages left to right and its magnitude up, \
                     and `{}` has no such pair of axes to give it. Draw the flow flat.",
                    space_name(other),
                ),
            });
            return;
        }
    }

    // 4. Which axis the stages take is read off the bindings, as a bar's
    //    orientation is (spec §6): the count on `y` runs the stages left to right,
    //    and the count on `x` runs them top to bottom (2026-10-02, at the author's
    //    word). A *category* on `x` is a second answer to where the stages sit,
    //    Law 5's ambiguity, and a count on both axes is two weights for one path.
    if let Some(x) = spec.position_for(layer, &Channel::X) {
        let categorical = df.and_then(|d| actual_type(d, &x.field)) == Some(VarType::Discrete);
        if let Some(y) = spec.position_for(layer, &Channel::Y) {
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: format!(
                    "gog: under `flow` one axis carries the count and the other the \
                     stages, and both `x({})` and `y({})` are bound. Keep `y(<amount>)` \
                     to run the stages left to right, or `x(<amount>)` to run them top \
                     to bottom.",
                    x.field, y.field,
                ),
            });
        } else if categorical {
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: format!(
                    "gog: under `flow` the stage axis is drawn from the atom's own \
                     columns, so `x({})` has nothing left to say. Remove it: to reorder \
                     the stages, reorder `flow(...)`'s arguments. To run the stages top \
                     to bottom, bind the count to `x` instead: `x(<amount>)`.",
                    x.field,
                ),
            });
        }
    }

    // 5. Mapped aesthetics. A band is a whole path, so any *stage* names
    //    something every band holds, and any other column of categories splits
    //    a band into parts that each hold one value (2026-10-03, as a color of
    //    categories splits a line). A node aggregates paths whose other
    //    categories differ, so nothing names a node but its own name.
    for (ch, def) in &layer.encodings {
        match ch {
            Channel::Y | Channel::X => {}
            Channel::Label if layer.mark == Mark::Text => {
                if def.field != crate::transform::NODE_NAME {
                    out.push(Diagnostic {
                        kind: DiagnosticKind::Illegal,
                        message: format!(
                            "gog: under `flow` a `text` names the slots, and the flow \
                             publishes each slot's name under `name` — write \
                             `label(name)`, not `label({})`.",
                            def.field,
                        ),
                    });
                }
            }
            // A stage, or any other column of categories, which splits each band.
            // A number is refused by the ribbon's own rule, which asks every
            // ribbon for a category on `color`, so it is not said twice here.
            Channel::Color | Channel::Pattern if layer.mark == Mark::Ribbon => {}
            // A slot's own `name` is what names a node, so it is the column rules'
            // to judge (spec §8): the right kind, and not drawn there yet.
            _ if def.field == crate::transform::NODE_NAME => {}
            other => {
                out.push(Diagnostic {
                    kind: DiagnosticKind::Illegal,
                    message: format!(
                        "gog: `{}()` does not combine with `{}` under `flow`. A band \
                         takes `color(<categorical column>)` on the `ribbon` layer; a \
                         slot takes its paint from `style()`; and `y(<amount>)` weighs \
                         the paths.",
                        channel_name(other), mark_name(&layer.mark),
                    ),
                });
            }
        }
    }

    let Some(df) = df else { return };

    // 6. A stage that is not there, or is not a category — a number cannot name
    //    a slot, for `partition`'s reason: no two rows share a measurement the
    //    way they share a word.
    for s in stages {
        match actual_type(df, s) {
            None => {
                out.push(Diagnostic {
                    kind: DiagnosticKind::Illegal,
                    message: format!(
                        "gog: `flow({s})` names a column this table does not have. \
                         Each stage is a categorical column, and one row is one path \
                         through all of them."
                    ),
                });
                return;
            }
            Some(VarType::Continuous) => {
                out.push(Diagnostic {
                    kind: DiagnosticKind::Illegal,
                    message: format!(
                        "gog: `flow({s})` names a numeric column, and a stage is a \
                         set of named slots — no two rows share a measurement the way \
                         they share a word. Cut it into bands first, or name a \
                         categorical column."
                    ),
                });
                return;
            }
            _ => {}
        }
    }

    // 7. The weight is the bound `y`, or the bound `x` of a flow that runs top to
    //    bottom, so it has to be a number. (A category on `x` was refused above.)
    let weight = spec.position_for(layer, &Channel::Y).map(|d| ("y", d))
        .or_else(|| flow_runs_down(spec, layer)
            .then(|| spec.position_for(layer, &Channel::X).map(|d| ("x", d))).flatten());
    if let Some((c, y)) = weight {
        if actual_type(df, &y.field) == Some(VarType::Discrete) {
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: format!(
                    "gog: `{c}({0})` is what each path is weighed by, and `{0}` holds \
                     categories rather than numbers. Name the column that carries the \
                     amount — `{c}(<amount>)` — or bind nothing at all, in which case \
                     every path weighs 1 and the flow tallies them.",
                    y.field,
                ),
            });
            return;
        }
        // A band's thickness cannot be negative; the layout clamped a negative
        // weight to 0 and dropped the path, so the stages no longer summed to
        // the table, in silence. `nest()`'s refusal, one family over.
        if has_negative(df, &y.field) {
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: format!(
                    "gog: `{c}({0})` weighs each path, and `{0}` has negative values. A flow \
                     draws a weight as the thickness of a band, and a thickness cannot be \
                     negative: those paths would be dropped, and every stage would sum to \
                     less than the table. Filter or offset the column.",
                    y.field,
                ),
            });
            return;
        }
    }

    // 7b. A layered flow stands each place after every place that sends to it,
    //     so links that form a cycle have no order to stand in. Named,
    //     with the way that draws them.
    if layered {
        let measure = weight.map(|(_, d)| d.field.as_str());
        if let Some(cycle) = crate::transform::flow_cycle(df, &stages[0], &stages[1], measure) {
            out.push(Diagnostic {
                kind: DiagnosticKind::Unsupported,
                message: format!(
                    "gog: these links form a cycle, {}, and a layered flow stands \
                     each place after every place that sends to it, so a cycle has no \
                     order. A cycle drawn as a loop is valid grammar this engine does not \
                     draw. Set `shared` to true in place of `layered`: `flow({}, {})` then \
                     puts every place on one axis and draws a cycle as arcs. Or remove one \
                     of the links.",
                    cycle.join(" → "), stages[0], stages[1]
                ),
            });
            return;
        }
    }

    // 8. A row missing a stage value cannot be a path and is left out — said
    //    once, with the count, so the drop is never silent (§12).
    if let Some(cols) = stages.iter()
        .map(|s| df.str_col(s)).collect::<Option<Vec<_>>>()
    {
        let dropped = (0..df.len())
            .filter(|&r| cols.iter().any(|c| c[r].is_empty()))
            .count();
        if dropped > 0 {
            out.push(Diagnostic {
                kind: DiagnosticKind::Assumption,
                message: format!(
                    "gog: {dropped} of {} rows are missing a value on at least one \
                     stage and were left out — a path has to pass through every \
                     stage `flow(...)` names.",
                    df.len(),
                ),
            });
        }
    }
}

/// Every way a layout can be malformed — `check_flow`'s shape one family over,
/// which is `check_partition`'s two families over. The first branches are facts
/// about the sentence; the column checks run when the frame is there to ask.
pub(super) fn check_layout(
    out: &mut Vec<Diagnostic>, spec: &PlotSpec, df: Option<&DataFrame>, layer: &Layer,
) {
    if !layer.transforms.contains(&Transform::Layout) {
        // 0. An `edge` with no `layout`. Its whole geometry is the two nodes a
        //    layout places, so without one it has nothing to draw, and it drew
        //    nothing, in silence: an empty panel with made-up 0 to 1 axes on the
        //    plane and in the cube, and a legend keying nothing beside a `color`.
        //    The rule table has always called `layout` required on `edge`; nothing
        //    asked, because the rest of this function starts from the layout.
        //    In a `network()` that no layer feeds, `check_network` already says
        //    this for the whole plot, so it is said once, there. A network another
        //    layer does feed leaves this `edge` to this check alone.
        let network_says_it = matches!(spec.coord, CoordSpace::Network(_))
            && !spec.layers.iter().any(|l| l.transforms.contains(&Transform::Layout));
        if layer.mark == Mark::Edge && !network_says_it {
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: "gog: an `edge` connects two nodes, and only `layout` places \
                          them, so an `edge` with no `layout` has nothing to draw. Name \
                          the two endpoint columns and draw it in the network: `edge * \
                          layout(<from>, <to>) + network()`."
                    .to_string(),
            });
        }
        return;
    }

    // 1. A mark with no reading for a placed graph. Refused toward the three
    //    that have one, with the sentence spelled out.
    if mark_takes_transform(&layer.mark, &Transform::Layout) == TransformLegality::None {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `layout` places a graph — a position per node, computed from \
                 an edge table — and `{}` has no reading for that. Three marks do: \
                 `edge * layout(<from>, <to>)` draws the connections, `point * \
                 layout(...)` the nodes, and `text * layout(...) + label(name)` \
                 names them, all inside `network()`.",
                mark_name(&layer.mark),
            ),
        });
        return;
    }

    // 2. The atom with too little named.
    let Some(lay) = layer.layout.as_ref()
        .filter(|sp| !sp.from.is_empty() && !sp.to.is_empty())
    else {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: "gog: `layout` takes the two endpoint columns, in order — \
                      `layout(from, to)`. One row of the table is one edge, and \
                      those columns name the two nodes it connects."
                .to_string(),
        });
        return;
    };

    // 3. The space. A layout's positions mean nothing on data axes, so they
    //    draw only in the space that abolishes them.
    if space_of(spec) != SpaceKind::Network {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: "gog: a `layout`'s positions are the graph's, not the data's — \
                      an axis under them would number something no reader can mean. \
                      Draw it in the network: `+ network()` flat, or `+ network(turn \
                      = 30, tilt = 25)` for the cube."
                .to_string(),
        });
        return;
    }

    // 4. A bound position under a transform that places its own nodes.
    for ch in [Channel::X, Channel::Y, Channel::Z] {
        if spec.position_for(layer, &ch).is_some() {
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: format!(
                    "gog: under `layout` the nodes place themselves, so `{0}(...)` \
                     has nothing left to say. Remove it — the third dimension, when \
                     you want it, is the space's: `network(turn = 30, tilt = 25)`.",
                    channel_name(&ch),
                ),
            });
        }
    }

    // 5. A played layout is not built. Its old reason is gone: the placement is
    //    made once per layer across every panel and frame, so a node would stand
    //    still. What stays is that nobody has drawn and tested the frames yet, so
    //    the cell waits, and the direction is the facet, where the same shared
    //    placement already holds.
    if layer.encodings.contains_key(&Channel::Play) {
        out.push(Diagnostic {
            kind: DiagnosticKind::Unsupported,
            message: "gog: `play` under `layout` is valid grammar, but this engine \
                      does not draw it. Draw the frames as panels beside each other \
                      with `| facet()` on the column you would have played: every \
                      panel shares one layout, so each node keeps its place."
                .to_string(),
        });
    }

    // 6. On the node readers, a mapped aesthetic must name what a node has: the
    //    two columns the layout publishes. An edge is 1:1 with its input row,
    //    so its columns are checked like anyone's.
    //
    //    `label` is asked too, and it once was not: a node's text is the
    //    commonest thing to write, and a column of the edge table there drew a
    //    `text` layer with nothing in it, silently (§12). The layout's output has
    //    no such column to read the strings from.
    if matches!(layer.mark, Mark::Point | Mark::Text) {
        for (ch, def) in &layer.encodings {
            if matches!(ch, Channel::Play) {
                continue;
            }
            if *ch == Channel::Label
                && def.field != crate::transform::NODE_NAME
                && def.field != crate::transform::NODE_DEGREE
            {
                out.push(Diagnostic {
                    kind: DiagnosticKind::Illegal,
                    message: format!(
                        "gog: `label({})` under `layout` must name something a node \
                         has, and a node has two things: its `name`, and its \
                         `degree`, the count of its relations. A column of the edge \
                         table belongs to the edges, not to the nodes this `{}` \
                         layer draws. Name each node with `label(name)`.",
                        def.field, mark_name(&layer.mark),
                    ),
                });
                continue;
            }
            if def.field != crate::transform::NODE_NAME
                && def.field != crate::transform::NODE_DEGREE
            {
                out.push(Diagnostic {
                    kind: DiagnosticKind::Illegal,
                    message: format!(
                        "gog: `{}({})` under `layout` must name something a node \
                         has, and a node has two things: its `name`, and its \
                         `degree`, the count of its relations. A column of the \
                         edge table belongs to the edges; map it on the `edge` \
                         layer.",
                        channel_name(ch), def.field,
                    ),
                });
            }
        }
    }

    let Some(df) = df else { return };

    // 7. An endpoint column that is not there, or is not a category.
    for col in [lay.from.as_str(), lay.to.as_str()] {
        match actual_type(df, col) {
            None => {
                out.push(Diagnostic {
                    kind: DiagnosticKind::Illegal,
                    message: format!(
                        "gog: `layout({col})` names a column this table does not \
                         have. Each endpoint is a categorical column, and one row \
                         is one edge between its two values."
                    ),
                });
                return;
            }
            Some(VarType::Continuous) => {
                out.push(Diagnostic {
                    kind: DiagnosticKind::Illegal,
                    message: format!(
                        "gog: `layout({col})` names a numeric column, and a node is \
                         a *name* the rows share — no two rows share a measurement \
                         the way they share a word."
                    ),
                });
                return;
            }
            _ => {}
        }
    }

    // 8. A row from a node to itself is not an edge — refused with the count, not
    //    dropped in silence. Both notes name the table, so the layers that draw one
    //    graph say each once (`check` drops a repeated sentence) and two tables with
    //    the same count are told apart.
    //
    //    The refusal once offered to "carry the fact as a node's own column", and
    //    a node has no column to carry it in: it has its `name` and its `degree`,
    //    and `color(<column>)` on a node layer is refused.
    let table = layer.data.as_ref().or(spec.data.as_ref())
        .map(|t| format!(" of `{t}`")).unwrap_or_default();
    if let (Some(a), Some(b)) = (df.str_col(&lay.from), df.str_col(&lay.to)) {
        let loops = (0..df.len())
            .filter(|&r| !a[r].is_empty() && a[r] == b[r])
            .count();
        if loops > 0 {
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: format!(
                    "gog: {loops} row(s){table} connect a node to itself, and a stroke \
                     from a point to the same point has no extent. Remove those rows \
                     in the host language: an edge joins two different nodes."
                ),
            });
            return;
        }
        let missing = (0..df.len())
            .filter(|&r| a[r].is_empty() || b[r].is_empty())
            .count();
        if missing > 0 {
            out.push(Diagnostic {
                kind: DiagnosticKind::Assumption,
                message: format!(
                    "gog: {missing} of {} row(s){table} are missing an endpoint and were \
                     left out — an edge needs both of its nodes named.",
                    df.len(),
                ),
            });
        }
    }
}

/// The cluster family's malformations, `check_layout`'s shape one statistic
/// over: the wrong mark, the atom with too little named, a bound position the
/// tree invents, the aesthetics no reading holds, and the data facts — a
/// profile with a hole in it, a cell said twice, a leaf axis that is not
/// categories. Sentence facts refuse before data facts, so a wrong sentence is
/// refused even when the table is missing.
pub(super) fn check_cluster(
    out: &mut Vec<Diagnostic>, spec: &PlotSpec, df: Option<&DataFrame>, layer: &Layer,
) {
    if !layer.transforms.contains(&Transform::Cluster) {
        return;
    }

    // 1. A mark with no reading for a merge tree. Two have one, and the
    //    refusal names both sentences.
    if mark_takes_transform(&layer.mark, &Transform::Cluster) == TransformLegality::None {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `cluster` joins the closest leaves under one elbow, and `{}` \
                 has no reading for that. Two marks do: `path * cluster(<value>, \
                 over = <profile>) + x(<leaves>)` draws the tree, and `zone * \
                 cluster(over = <profile>)` reorders a tile plot's slots to the \
                 tree's leaf order.",
                mark_name(&layer.mark),
            ),
        });
        return;
    }

    let on_zone = layer.mark == Mark::Zone;
    let cl = layer.cluster.clone().unwrap_or_default();
    let x_def = spec.position_for(layer, &Channel::X);
    let y_def = spec.position_for(layer, &Channel::Y);

    // 2. The tree's positions: one axis for the leaves, and the other is the
    //    tree's own — it carries the merge distance the tree writes, under the
    //    name `distance`. Naming it says so out loud and gives that axis its
    //    settings (spec §8, a written column is a column); any other column
    //    there names something the tree never reads.
    if !on_zone {
        let names_distance = |d: &Option<&ChannelDef>| {
            d.is_some_and(|d| d.field == crate::transform::CLUSTER_DISTANCE)
        };
        match (&x_def, &y_def) {
            (Some(_), Some(_)) if names_distance(&x_def) || names_distance(&y_def) => {}
            (Some(_), Some(_)) => {
                out.push(Diagnostic {
                    kind: DiagnosticKind::Illegal,
                    message: "gog: under `cluster` one position holds the leaves and \
                              the other carries the merge distance, which the tree \
                              writes as `distance`. Bind the leaf axis alone: \
                              `x(<leaves>)` grows the tree up, `y(<leaves>)` grows \
                              it sideways. To give the distance axis its settings, \
                              name it: `x(<leaves>) + y(distance, limits = …)`."
                        .to_string(),
                });
                return;
            }
            // A position naming `distance` holds the tree's distances, not its
            // leaves, so it alone binds no leaf axis.
            _ if !(x_def.is_some() && !names_distance(&x_def))
                && !(y_def.is_some() && !names_distance(&y_def)) => {
                out.push(Diagnostic {
                    kind: DiagnosticKind::Illegal,
                    message: "gog: a cluster tree's leaves are the levels of a bound \
                              categorical position, and nothing here binds one. Add \
                              `x(<leaves>)`, or `y(<leaves>)` for the sideways tree."
                        .to_string(),
                });
                return;
            }
            _ => {}
        }
        // The value column is the tree's one required name: distance is
        // measured on it, and nothing else in the sentence says which column
        // that is.
        if cl.value.as_deref().unwrap_or("").is_empty() {
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: "gog: `cluster` measures distance on a value column, and \
                          nothing names one. Name it first: `cluster(<value>)`, or \
                          `cluster(<value>, over = <profile>)` when each leaf has a \
                          row per profile entry."
                    .to_string(),
            });
            return;
        }
    }

    let leaf_field: String = if on_zone {
        // On a tile plot both positions are bound; the profile axis is named
        // and the other categorical position holds the leaves.
        let over = cl.over.as_deref().unwrap_or("");
        let (xf, yf) = (
            x_def.as_ref().map(|d| d.field.clone()).unwrap_or_default(),
            y_def.as_ref().map(|d| d.field.clone()).unwrap_or_default(),
        );
        let needs_both = "A clustered tile plot needs a category on each position: \
            `zone * cluster(over = <profile>) + x(<leaves>) + y(<profile>) + color(<value>)`.";
        // One position bound leaves no axis for the leaves or none for the
        // profile. Checked first, because every message below assumes two: the
        // leaf axis used to arrive empty and be refused as "`` ", a name this
        // table does not have.
        if xf.is_empty() || yf.is_empty() {
            let (bound, missing) = if xf.is_empty() { ("y", "x") } else { ("x", "y") };
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: format!(
                    "gog: `cluster` reorders a tile plot's slots, and this plot binds `{bound}` \
                     but not `{missing}`. {needs_both}"
                ),
            });
            return;
        }
        if over.is_empty() {
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: "gog: on a tile plot both axes hold categories, so \
                          `cluster` cannot tell which one profiles the other. Name \
                          the profile axis: `cluster(over = <profile>)` — the other \
                          axis's slots take the tree's leaf order."
                    .to_string(),
            });
            return;
        }
        if over != xf && over != yf {
            // Offer only what would work. A numeric position was offered too, and
            // `cluster(over = <it>)` is then refused as numeric; with one numeric
            // position nothing works until it holds a category, so say that.
            let numeric: Vec<&str> = [xf.as_str(), yf.as_str()].into_iter()
                .filter(|f| df.and_then(|d| actual_type(d, f)) == Some(VarType::Continuous))
                .collect();
            let fix = match numeric.as_slice() {
                [] => format!("Name `{xf}` or `{yf}`."),
                [one] => format!("`{one}` is numeric. {needs_both}"),
                _ => format!("`{xf}` and `{yf}` are numeric. {needs_both}"),
            };
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: format!(
                    "gog: `cluster(over = {over})` names a column neither axis \
                     holds. On a tile plot the profile axis is one of the two \
                     positions. {fix}"
                ),
            });
            return;
        }
        if over == xf { yf } else { xf }
    } else {
        [x_def.as_ref(), y_def.as_ref()].into_iter().flatten()
            .find(|d| d.field != crate::transform::CLUSTER_DISTANCE)
            .map(|d| d.field.clone()).unwrap_or_default()
    };

    // 3. The profile axis must not be the leaf axis: a level cannot profile
    //    itself.
    if cl.over.as_deref() == Some(leaf_field.as_str()) && !on_zone {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `cluster(over = {leaf_field})` names the leaf axis itself, \
                 and a level cannot be its own profile. Name the column that \
                 indexes each profile's entries."
            ),
        });
        return;
    }

    // 4. On a zone, the value is `color`'s column; a second name must be the
    //    same one, and no name at all needs `color` bound.
    let color_field = layer.encodings.get(&Channel::Color).map(|d| d.field.clone());
    if on_zone {
        match (&cl.value, &color_field) {
            (Some(v), Some(c)) if v != c => {
                out.push(Diagnostic {
                    kind: DiagnosticKind::Illegal,
                    message: format!(
                        "gog: `cluster({v}, ...)` and `color({c})` name different \
                         columns, and a tile plot measures by `color` — the \
                         profiles are its values. Say the same column, or drop the \
                         first name and let `color` supply it."
                    ),
                });
                return;
            }
            (None, None) => {
                out.push(Diagnostic {
                    kind: DiagnosticKind::Illegal,
                    message: "gog: a clustered tile plot reads its profile values \
                              from `color`, and nothing binds one. Add \
                              `color(<value>)`."
                        .to_string(),
                });
                return;
            }
            _ => {}
        }
    }

    // 5. The aesthetics the tree reading cannot hold: its strokes are computed
    //    from every row, so there is no column left to split or measure them
    //    by. `style(color = )` paints the whole tree; coloring subtrees needs
    //    a cut height the grammar has not designed.
    if !on_zone {
        for ch in [Channel::Color, Channel::Group, Channel::Pattern, Channel::Size,
                   Channel::Opacity, Channel::Shape, Channel::Label] {
            // The tree's own `distance` is something to read, so it is the column
            // rules' to judge (each join has one; drawing it is not built yet).
            if layer.encodings.get(&ch)
                .is_some_and(|d| d.field != crate::transform::CLUSTER_DISTANCE)
            {
                out.push(Diagnostic {
                    kind: DiagnosticKind::Illegal,
                    message: format!(
                        "gog: `{}(...)` under `cluster` has nothing to read — the \
                         tree's strokes are computed from every row, so no column \
                         is left to split them by. `style(color = )` paints the \
                         tree; coloring its branches by a cut is not designed.",
                        channel_name(&ch),
                    ),
                });
                return;
            }
        }
        if layer.encodings.contains_key(&Channel::Z) {
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: "gog: the cluster tree is drawn in the plane — a merge \
                          has no depth for `z` to carry. Drop `z()`."
                    .to_string(),
            });
            return;
        }
        if space_of(spec) == SpaceKind::Polar {
            out.push(Diagnostic {
                kind: DiagnosticKind::Unsupported,
                message: "gog: a circular cluster tree is valid grammar, but this \
                          engine does not draw it: the tree's horizontal bars \
                          would bend into arcs, and a path's segments draw \
                          straight. Drop `polar()`."
                    .to_string(),
            });
            return;
        }
    }

    // 6. A played or faceted cluster would re-derive the leaf order per frame
    //    or per panel, and an axis that re-sorts itself mid-read is not an
    //    axis. Both wait rather than lie.
    if layer.encodings.contains_key(&Channel::Play) {
        out.push(Diagnostic {
            kind: DiagnosticKind::Unsupported,
            message: "gog: `play` under `cluster` is valid grammar, but this engine \
                      does not draw it: each frame would re-cluster and re-sort \
                      the leaf axis mid-animation. Filter to one moment where \
                      your data lives."
                .to_string(),
        });
        return;
    }
    if spec.facet.is_some() {
        out.push(Diagnostic {
            kind: DiagnosticKind::Unsupported,
            message: "gog: `facet` under `cluster` is valid grammar, but this engine \
                      does not draw it: each panel would derive its own leaf \
                      order for one shared axis. Compose separate plots with `|` \
                      and `/` instead; each keeps its own axis."
                .to_string(),
        });
        return;
    }

    // 7. `order()` and `cluster` on one axis are two orders for one axis.
    //    Sorting the *profile* axis by its own declared order stays legal —
    //    that axis is not the tree's.
    if let Some(o) = &spec.order {
        let sorts_only_the_profile_axis =
            on_zone && cl.over.as_deref() == Some(o.field.as_str());
        if !sorts_only_the_profile_axis {
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: format!(
                    "gog: `order({})` and `cluster` both order the `{leaf_field}` \
                     axis — one sorts by a column, the other derives the order \
                     from the tree, and an axis holds one order. Drop one.",
                    o.field,
                ),
            });
            return;
        }
    }

    // The data facts. Everything below needs the table.
    let Some(df) = df else { return };

    // 8. The named columns must be there and be the right kind of thing.
    let value_field = if on_zone {
        cl.value.clone().or(color_field).unwrap_or_default()
    } else {
        cl.value.clone().unwrap_or_default()
    };
    match actual_type(df, &value_field) {
        None => {
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: format!(
                    "gog: `cluster` measures distance on `{value_field}`, which \
                     this table does not have."
                ),
            });
            return;
        }
        Some(VarType::Discrete) => {
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: format!(
                    "gog: `cluster` measures distance on `{value_field}`, and \
                     distance is measured on numbers — that column holds \
                     categories. Name a numeric column."
                ),
            });
            return;
        }
        _ => {}
    }
    if let Some(over) = cl.over.as_deref() {
        match actual_type(df, over) {
            None => {
                out.push(Diagnostic {
                    kind: DiagnosticKind::Illegal,
                    message: format!(
                        "gog: `cluster(over = {over})` names a column this table \
                         does not have."
                    ),
                });
                return;
            }
            Some(VarType::Continuous) => {
                out.push(Diagnostic {
                    kind: DiagnosticKind::Illegal,
                    message: format!(
                        "gog: `cluster(over = {over})` names a numeric column, and \
                         a profile's entries are indexed by names — every leaf \
                         holds one value per level of it. Make it text where your \
                         data lives, or bin it there first."
                    ),
                });
                return;
            }
            _ => {}
        }
    }

    // 9. The leaves are categories. A continuous axis has no levels to join,
    //    and gog will not cut it uninvited.
    match actual_type(df, &leaf_field) {
        Some(VarType::Continuous) => {
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: format!(
                    "gog: a cluster tree's leaves are the levels of `{leaf_field}`, \
                     and that column is numeric — a measurement has no levels to \
                     join. Make it text where your data lives, or bin it there."
                ),
            });
            return;
        }
        None => {
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: format!(
                    "gog: the leaf axis names `{leaf_field}`, which this table \
                     does not have."
                ),
            });
            return;
        }
        _ => {}
    }

    let leaves = crate::data::categories_across(&[df], &leaf_field);
    // 10. One leaf joins nothing.
    if leaves.len() < 2 {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `{leaf_field}` holds {} level(s), and a tree needs at least \
                 two leaves to join.",
                leaves.len(),
            ),
        });
        return;
    }

    // 11. The profiles must be complete and said once: a missing cell has no
    //     distance and a duplicated one has two, and inventing either is a
    //     measurement nobody made. Both name the first offender.
    let keys = df.str_col(&leaf_field);
    let vals = df.float_col(&value_field);
    let (Some(keys), Some(vals)) = (keys, vals) else { return };
    let coords: Vec<String> = match cl.over.as_deref() {
        Some(o) => crate::data::categories_across(&[df], o),
        None => vec![String::new()],
    };
    let coord_col = cl.over.as_deref().and_then(|o| df.str_col(o));
    let mut counts = vec![vec![0usize; coords.len()]; leaves.len()];
    for r in 0..df.len().min(keys.len()).min(vals.len()) {
        let Some(li) = leaves.iter().position(|l| *l == keys[r]) else { continue };
        let ci = match coord_col {
            Some(cs) => match coords.iter().position(|c| *c == cs[r]) {
                Some(ci) => ci,
                None => continue,
            },
            None => 0,
        };
        if vals[r].is_finite() {
            counts[li][ci] += 1;
        }
    }
    for (li, per_leaf) in counts.iter().enumerate() {
        for (ci, &n) in per_leaf.iter().enumerate() {
            if n == 0 {
                let place = match cl.over.as_deref() {
                    Some(o) => format!("no value at `{o}` = {}", coords[ci]),
                    None => "no value".to_string(),
                };
                out.push(Diagnostic {
                    kind: DiagnosticKind::Illegal,
                    message: format!(
                        "gog: `cluster` compares complete profiles, and \
                         `{leaf_field}` = {} has {place} — every leaf needs one \
                         value at every level. Fill or drop where your data lives.",
                        leaves[li],
                    ),
                });
                return;
            }
            if n > 1 {
                let place = match cl.over.as_deref() {
                    Some(o) => format!(" at `{o}` = {}", coords[ci]),
                    None => String::new(),
                };
                out.push(Diagnostic {
                    kind: DiagnosticKind::Illegal,
                    message: format!(
                        "gog: `{leaf_field}` = {}{place} holds {n} rows, and a \
                         profile's entry is one value — the mean, the sum and the \
                         median are all defensible, so gog will not choose. \
                         Aggregate to one row per pair where your data lives.",
                        leaves[li],
                    ),
                });
                return;
            }
        }
    }
    // 12. With several rows per leaf and no profile axis named, the vectors
    //     have no alignment — refused above as a duplicate of the one implicit
    //     coordinate, so the direction here rides that message. (A bare
    //     `cluster(<value>)` is the one-row-per-leaf case.)
}

/// Does this spec compute a leaf order for `field` on `channel` — is it an
/// *authority* for that axis's category order when plots compose? True when a
/// layer clusters and `field` is its leaf axis on that channel. The page reads
/// this to give a shared categorical axis the order a clustered panel derived
/// (§9's composition rule).
pub fn cluster_orders(spec: &PlotSpec, channel: &Channel, field: &str) -> bool {
    let spec = resolve_scopes(spec);
    spec.layers.iter().any(|l| {
        if !l.transforms.contains(&Transform::Cluster) {
            return false;
        }
        let over = l.cluster.as_ref().and_then(|c| c.over.clone()).unwrap_or_default();
        spec.position_for(l, channel)
            .map(|d| d.field == field && d.field != over)
            .unwrap_or(false)
    })
}

/// **A layer the graph places**: an `edge`, or any mark derived by `layout`. Its
/// positions exist only in `network()`, and outside it its own refusals already say
/// so and name the space (`edge` with no `layout`, and a `layout` anywhere else).
/// So the checks written for other spaces stand down for it rather than add a second
/// remedy that fails: "Drop `polar()` to draw it flat" reached a flat `edge`, which
/// is refused too, and a `point * layout` in `map()` was told to add `x` and `y`.
pub(super) fn placed_by_the_graph(layer: &Layer) -> bool {
    layer.mark == Mark::Edge || layer.transforms.contains(&Transform::Layout)
}
