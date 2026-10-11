//! Facets: panels cut by a column, laid out in space by `facet` or in time by
//! `play`.

use super::*;

// ---------------------------------------------------------------------------
// Facets
//
// A facet variable names the panels, so it must be a category column — a
// number is a position along an axis, not a name for a frame. This is the same
// judgment `shape` and `group` make, applied to the plot's outer frame
// instead of a mark. The checks live here rather than in `rule_for` because a
// facet is not a channel: it maps a column to a *frame*, not to a visual
// feature of a mark, so the mark × channel table has nothing to say about it.
// ---------------------------------------------------------------------------

pub(super) fn check_facet(out: &mut Vec<Diagnostic>, spec: &PlotSpec, data: &HashMap<String, DataFrame>) {
    let Some(facet) = &spec.facet else { return };

    if let (Some(col), Some(row)) = (&facet.col, &facet.row) {
        if col == row {
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: format!(
                    "gog: `{col}` cannot form both the panel rows and the panel columns — \
                     the grid would repeat every panel along its diagonal. \
                     Facet by two different columns, or drop one operator."
                ),
            });
            return;
        }
    }

    // `wrap` folds a *line* of panels into a rectangle, so it needs a line to
    // fold. A crossing has already fixed the shape with two columns, and a count
    // beside it could only be read as a second, contradicting statement of it.
    if let Some(n) = facet.wrap {
        match (&facet.col, &facet.row) {
            (Some(col), Some(row)) => {
                out.push(Diagnostic {
                    kind: DiagnosticKind::Illegal,
                    message: format!(
                        "gog: `wrap` folds one line of panels into a rectangle, but \
                         `{col}` and `{row}` already cross into one. Drop `wrap`, or \
                         facet by one column and let `wrap` shape it."
                    ),
                });
                return;
            }
            (None, None) => {
                out.push(Diagnostic {
                    kind: DiagnosticKind::Illegal,
                    message: "gog: `wrap` shapes a facet's panels, and this plot has no \
                              facet to shape. Write `plot | facet(g, wrap = 4)`."
                        .to_string(),
                });
                return;
            }
            _ => {}
        }
        if n == 0 {
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: "gog: `wrap = 0` asks for a line that turns after no panels. \
                          Give the number of panels to draw before the line turns, \
                          e.g. `wrap = 4`."
                    .to_string(),
            });
            return;
        }
    }

    for (field, op) in [(&facet.col, "|"), (&facet.row, "/")] {
        let Some(field) = field else { continue };

        // Judge the column against every table it appears in; remember which
        // layers lack it so the every-panel behavior can be said out loud.
        let mut found = false;
        let mut absent_from: Vec<&str> = Vec::new();
        for layer in &spec.layers {
            let name = layer.data.as_ref().or(spec.data.as_ref());
            let Some(df) = name.and_then(|n| data.get(n)) else { continue };

            if df.time_unit(field).is_some() {
                out.push(Diagnostic {
                    kind: DiagnosticKind::Illegal,
                    message: format!(
                        "gog: `facet({field})` splits on a date column, which has a value \
                         per moment — that is one panel per row, not small multiples. \
                         Facet by a text column naming the period, e.g. the year formatted \
                         as text."
                    ),
                });
                return;
            }
            match actual_type(df, field) {
                // **A line's own `x` is refused as a facet whatever its type**
                // (`check_domain_split`), and that refusal stands alone: "make it
                // text" led straight back to it, since a line cut by its own `x`
                // holds one position per panel either way.
                Some(VarType::Continuous)
                    if spec.layers.iter().any(|l| domain_column(spec, l) == Some(field.as_str())) =>
                {
                    return;
                }
                Some(VarType::Continuous) => {
                    out.push(Diagnostic {
                        kind: DiagnosticKind::Illegal,
                        message: format!(
                            "gog: `facet({field})` splits on a number column, but a facet \
                             variable names the panels, so it must be a category column. \
                             Store `{field}` as text or categories before plotting, or cut \
                             it into named groups first."
                        ),
                    });
                    return;
                }
                Some(VarType::Discrete | VarType::Either) => found = true,
                None => absent_from.push(name.map(String::as_str).unwrap_or("?")),
            }
        }

        if !found {
            // A name a transform writes is a column, but a facet splits the table's
            // rows before any transform runs, so it is not there yet: say that, not
            // a misspelling.
            let message = match written_after_split(spec, data, None, field) {
                Some(chain) => format!(
                    "gog: `{op} facet({field})` names the `{field}` that `{chain}` writes, \
                     but a facet splits the table's rows into panels before any transform \
                     runs, so there is no `{field}` yet to split by. Facet by a category \
                     column of the table."
                ),
                None => format!(
                    "gog: `{op} facet({field})` refers to a column that is not in the data. \
                     Check the spelling of `{field}`."
                ),
            };
            out.push(Diagnostic { kind: DiagnosticKind::Illegal, message });
            continue;
        }

        // Not an error: a layer without the facet column is drawn in every
        // panel, which is how a shared reference layer belongs behind small
        // multiples — but it is a chosen default, so it is said.
        for table in absent_from {
            out.push(Diagnostic {
                kind: DiagnosticKind::Assumption,
                message: format!(
                    "gog: table `{table}` has no column `{field}`, so its layer is drawn \
                     in every panel. If it should be split too, add `{field}` to that table."
                ),
            });
        }
    }
}

// ---------------------------------------------------------------------------
// Play — the facet read in time
//
// `play` *is* a channel, so its mark row lives in `rule_for` like any other and
// what is left here is only what the table cannot say: the questions that are
// about the plot rather than about one mark × channel pair. That is the same
// division `check_facet` observes one section up, and the two checks are
// deliberately neighbors — they are one partition asked twice, once of the page
// and once of the clock.
//
// The column-type question is *not* here, and that is on purpose: `play` accepts
// either type, `rule_for` says so, and the reason a number is welcome here where
// `facet` refuses one is recorded at `data::frames_across`.
// ---------------------------------------------------------------------------

/// Above this many frames, say how long the loop will run before drawing it —
/// counted in default holds, which is the frame count for an even sequence and
/// more or fewer for an uneven one (`data::frame_holds`).
///
/// Under it the default is unambiguous and §12 says use it silently: a dozen
/// frames at the default pace is ten seconds, which is what anyone writing
/// `play(year)` over a decade already expects. Above it the number stops being
/// obvious, and the caller is choosing a two-minute loop without being told.
pub(super) const FRAMES_WORTH_MENTIONING: usize = 30;

pub(super) fn check_play(out: &mut Vec<Diagnostic>, spec: &PlotSpec, data: &HashMap<String, DataFrame>) {
    // Scopes are already resolved, so a plot-scoped `play` is on every layer that
    // accepts it and a layer-scoped one is on its own layer. Either way the
    // bindings are here.
    let mut played: Option<(&str, &ChannelDef)> = None;
    for layer in &spec.layers {
        if let Some(def) = layer.encodings.get(&Channel::Play) {
            played = Some((def.field.as_str(), def));
            break;
        }
    }
    let Some((field, def)) = played else { return };

    // A column splits the page or the clock, not both. Drawn, this would repeat
    // one frame's rows into one panel and leave every other panel empty in every
    // other frame — a grid of blanks with a single cell alive at a time, which is
    // not what either operator promised. `check_facet` refuses the same shape one
    // dimension over (a column as both panel rows and panel columns).
    if let Some(facet) = &spec.facet {
        for (f, op) in [(&facet.col, "|"), (&facet.row, "/")] {
            if f.as_deref() == Some(field) {
                out.push(Diagnostic {
                    kind: DiagnosticKind::Illegal,
                    message: format!(
                        "gog: `{field}` cannot both name the frames and name the panels — \
                         every panel but one would be empty in every frame. \
                         Split the page by one column and the time by another, or drop \
                         `{op} facet({field})` and let `play({field})` carry it."
                    ),
                });
                return;
            }
        }
    }

    // How many frames this actually cuts. Counted from the source tables, before
    // any transform, exactly as `check_facet` judges the facet column — the frames
    // are decided by the data the user handed in, not by what a statistic left.
    let source: Vec<&DataFrame> = spec
        .layers
        .iter()
        .filter_map(|l| l.data.as_ref().or(spec.data.as_ref()))
        .filter_map(|n| data.get(n))
        .collect();
    if source.is_empty() {
        return; // a missing table is already reported by the binding loop
    }

    let levels = crate::data::frames_across(&source, field);
    let n = levels.len();

    // **A sequence claims an order, so say when nobody stated one.**
    //
    // Not a refusal, and the reason is not Law 8 alone. The book *teaches* with
    // this plot: it draws `play(continent)` live so a reader can watch the points
    // jump and read why the jumps mean nothing. Refused, the chapter could only
    // assert that failure instead of showing it, and the demonstration is what
    // makes the point land.
    //
    // The test is not the column's type, which is the trap here. An **ordered**
    // category is a perfectly good thing to play — `factor(size, levels = c("Low",
    // "Medium", "High"))` is a sequence anyone would want — so refusing text would
    // refuse the case that works. What matters is whether *anyone stated* an
    // order: `levels` on the wire means the author chose one, and its absence
    // means `categories_across` fell back to the order values happen to appear
    // in, which is a fact about how the file was sorted rather than about the
    // world. So this fires on a bare text column and stays silent on a factor,
    // ordered or not, exactly as `categories_across` reads the two.
    let text = source.iter().any(|df| df.str_col(field).is_some());
    let stated = source.iter().any(|df| df.levels(field).is_some());
    if text && !stated {
        // Enough of the running order to recognize it, not all of it. The point
        // is that the reader sees *whose* order it is, which the first few names
        // carry as well as fifty would.
        //
        // **Never "and 1 more".** Cutting the list to hide a single name costs a
        // word to save a word, and it read badly where it mattered most: five
        // continents printed four and "and 1 more" directly above a paragraph
        // that named all five. So the tail has to be worth summarizing before it
        // is summarized.
        const SHOWN: usize = 4;
        let listed = if n > SHOWN + 1 {
            let names: Vec<&str> = levels.iter().take(SHOWN).map(|l| l.label.as_str()).collect();
            format!("{}, and {} more", names.join(", "), n - SHOWN)
        } else {
            levels.iter().map(|l| l.label.as_str()).collect::<Vec<_>>().join(", ")
        };
        out.push(Diagnostic {
            kind: DiagnosticKind::Assumption,
            message: format!(
                "gog: `play({field})` has no stated order, so each frame is a snapshot of one \
                 `{field}` and the frames run in the order the rows arrive: {listed}. A sequence \
                 claims that one frame comes after another. Declare the column's levels in the \
                 table ({DECLARE_LEVELS}), or use `facet({field})`, which claims no order."
            ),
        });
    }

    // Not a refusal. A hundred frames is a legal plot and Law 8 does not forbid
    // the ugly-but-legal — but the loop length is a default the caller did not
    // choose and cannot see, which is exactly §12's Assumption: it renders, and
    // the chosen default is said out loud so it can be confirmed.
    //
    // The loop is counted in default holds, not frames: a frame holds in
    // proportion to the gap it stands for (`data::frame_holds`), so an uneven
    // sequence can loop for longer than its frame count says. Even, the two are
    // the same number and nothing below changes.
    let beats = crate::data::frame_holds(&levels).iter().fold(0.0, |a, h| a + h);
    if beats > FRAMES_WORTH_MENTIONING as f64 {
        let secs = beats * def.frame_seconds();

        // The pace that brings this loop back under the length that would not
        // have been worth mentioning at all, which is `FRAMES_WORTH_MENTIONING`
        // frames at the default one. `FRAME_SECONDS` is on both sides and
        // cancels, so the answer is the frame count over that threshold, and
        // the number the message offers is the number that fixes it.
        //
        // It is offered only when it is faster than the speed already written.
        // This used to be the constant 4, which meant a sentence that had
        // already been sped up was told to slow down: `play(second, speed = 6)`
        // printed "Run it faster with `play(second, speed = 4)`" underneath a
        // plot whose author had taken the advice once. A direction that points
        // backwards is worse than no direction, and §12 asks for a direction.
        let enough = (beats / FRAMES_WORTH_MENTIONING as f64).ceil();
        let direction = if enough > def.speed.unwrap_or(1.0) {
            format!(
                " Run it faster with `play({field}, speed = {enough:.0})`, or bind a \
                 coarser column."
            )
        } else {
            " Bind a coarser column to cut fewer frames.".to_string()
        };

        // Said only when the holds differ, since only then is the loop not the
        // frame count times one hold.
        let held = if beats == n as f64 {
            ","
        } else {
            ", each held in proportion to the gap to the next,"
        };
        out.push(Diagnostic {
            kind: DiagnosticKind::Assumption,
            message: format!(
                "gog: `play({field})` cuts {n} frames{held} so the animation loops every \
                 {secs:.0} seconds.{direction}"
            ),
        });
    }
}

/// The column a layer reads a **domain** along: `x`, for the four marks that are a
/// function read in `x` order. `None` for every other mark, and for a violin, which
/// reads along its slot. `check_domain_split` refuses that column cutting the plot,
/// and `check_facet` leaves the column's type to that refusal.
pub(super) fn domain_column<'a>(spec: &'a PlotSpec, layer: &'a Layer) -> Option<&'a str> {
    if !matches!(layer.mark, Mark::Line | Mark::Step | Mark::Area | Mark::Ribbon) {
        return None;
    }
    // A violin reads along its slot, not along the plot's domain.
    if slot_density(spec, layer, None).is_some() {
        return None;
    }
    spec.position_for(layer, &Channel::X).map(|d| d.field.as_str())
}

/// A mark that reads a **domain** cannot have that domain's column also cutting
/// the plot into subsets — whether the subsets are frames or panels.
///
/// `line`, `step`, `area` and `ribbon` are functions read along `x`: one `y` for
/// each `x`, in `x` order. Cut the plot by the very column that supplies `x` and
/// every subset holds a single `x`, so there is no domain left to read and the
/// mark draws a vertical stroke, or nothing at all. Which of the two you get
/// depends on how many rows happen to share the subset — with a `color` split it
/// is nothing, without one it is a degenerate upright line — and neither is a
/// picture anyone meant.
///
/// **Found by a reader, in the book, in a chapter written the same day `play`
/// shipped.** `line + x(year) + play(year)` rendered axes, gridlines, a legend
/// and a frame strip, with an empty panel behind them, exit 0 — the `area`
/// empty-panel defect of §12 arriving through a new door. That it was *my*
/// sentence in the manual and not a user's is the part worth recording: a live
/// chunk proves a call is accepted, never that it draws.
///
/// **It covers `facet` as well as `play`, and that is the point.** The bug was
/// found through `play`, but `line + x(g) | facet(g)` had drawn the same empty
/// panel since faceting shipped. `play` and `facet` are one partition asked
/// twice (§11), so a check that caught the newer door and not the older one
/// would be precisely the per-feature exception Law 1 exists to catch.
///
/// A `path` is deliberately not here: it connects rows in the *table's* order and
/// never promised a domain, so a path whose points share an `x` is degenerate but
/// honest. Nor is a violin, whose reading runs along its slot rather than along
/// `x`.
pub(super) fn check_domain_split(out: &mut Vec<Diagnostic>, spec: &PlotSpec, layer: &Layer) {
    let Some(x) = domain_column(spec, layer) else { return };

    // The two doors, and what each calls its subsets.
    let play = layer.encodings.get(&Channel::Play).map(|d| d.field.as_str());
    let facet = spec.facet.as_ref();
    let (split, noun, direction) = if play == Some(x) {
        (x, "frames", format!("Animate a different column — `play` is what the frames \
            advance through, and `x({x})` is what the {} draws along. Or use `point`, \
            which places each row on its own and so needs no domain.", mark_name(&layer.mark)))
    } else if facet.and_then(|f| f.col.as_deref()) == Some(x) {
        (x, "panels", format!("Facet by a different column, or drop `| facet({x})` — \
            with `x({x})` the whole point of the panels is already on the axis."))
    } else if facet.and_then(|f| f.row.as_deref()) == Some(x) {
        (x, "panels", format!("Facet by a different column, or drop `/ facet({x})` — \
            with `x({x})` the whole point of the panels is already on the axis."))
    } else {
        return;
    };

    out.push(Diagnostic {
        kind: DiagnosticKind::Illegal,
        message: format!(
            "gog: `{m}` reads a function along `x`, but `{split}` both cuts the plot into \
             {noun} and supplies `x` — so every {one} holds a single `{split}`, and {an} \
             {m} needs at least two positions to read between. {direction}",
            m = mark_name(&layer.mark),
            an = article(mark_name(&layer.mark)),
            one = noun.trim_end_matches('s'),
        ),
    });
}

/// Whether [`check_domain_split`] refuses this layer: the column its domain is read
/// along also cuts the plot into frames or panels.
///
/// Asked by the renderer, which counts the series a path draws and refuses a plot
/// whose series all hold one position (`svg::one_position`). This sentence reaches
/// that state too, and already has its refusal, so the count leaves it to this one
/// rather than refusing one mistake twice under `GOG_STRICT=0`.
pub(crate) fn splits_its_domain(spec: &PlotSpec, layer: &Layer) -> bool {
    let Some(x) = domain_column(spec, layer) else { return false };
    layer.encodings.get(&Channel::Play).is_some_and(|d| d.field == x)
        || spec.facet.as_ref().is_some_and(|f| {
            f.col.as_deref() == Some(x) || f.row.as_deref() == Some(x)
        })
}

/// Check a stated pace against the channel it is stated on (§15).
///
/// The narrowest of the four binding parameters, and the line is the same one
/// `tick_count` draws: `limits` needs a **domain**, `tick_count` needs an **axis**,
/// and this needs a **duration**. Only `play` has one, because only `play` spends
/// the plot's time rather than its ink — so `color(continent, speed = 2)` is not a
/// slower legend, it is a word with nothing to modify, and is refused rather than
/// accepted and dropped.
/// `free` — fit this axis from each panel's own rows (spec §11).
///
/// Three things have to be true before it means anything: the channel draws an
/// **axis**, the plot has **panels** to fit one per, and the caller has not also
/// stated the domain. Each failure is a different sentence, because each has a
/// different fix.
pub(super) fn check_free(
    out: &mut Vec<Diagnostic>,
    def: &ChannelDef,
    c: &str,
    field: &str,
    channel: &Channel,
    spec: &PlotSpec,
) {
    if !def.free {
        return;
    }

    // Only the three positions draw an axis. `limits` reaches all six magnitude
    // channels because every one of them has a domain, but a legend is one key
    // for the whole plot: a per-panel color scale would make it decode nothing.
    // Every binding passes `free` from every channel so that this sentence is
    // the one a reader meets, rather than their language's "unused argument"
    // (ruled 2026-09-28). So each of these messages names `free` as a word and
    // never as one binding's spelling of it: `free = TRUE` is R's, and the same
    // engine answers Python's `free=True`, Julia's `free = true` and
    // JavaScript's `{ free: true }`.
    if !matches!(channel, Channel::X | Channel::Y | Channel::Z) {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `{c}({field})` cannot be freed. `free` fits an axis to each \
                 panel's own rows, and only a position, `x`, `y` or `z`, has an axis. \
                 Write `free` on a position instead, or drop it."
            ),
        });
        return;
    }

    // A stated domain *is* a fixed scale. Asking for both is asking the axis to
    // be two things, so it is refused rather than silently ranked.
    if def.limits.is_some() {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `{c}({field})` states `limits` and asks for `free`. `limits` fixes \
                 one domain for every panel, and `free` asks each panel to choose its \
                 own. Keep `limits` for one scale across every panel, or `free` for one \
                 scale per panel."
            ),
        });
        return;
    }

    // A packing has no axes at all, only regions — the same reason `x_label()`
    // is refused there, and the same wording.
    if matches!(spec.coord, CoordSpace::Nest) {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `free` on `{c}({field})` frees an axis, and a `nest()` packing \
                 has none — it has regions, whose shares are read inside each panel \
                 already. Drop `free`."
            ),
        });
        return;
    }

    let faceted = spec.facet.as_ref()
        .is_some_and(|f| f.col.is_some() || f.row.is_some());
    if faceted {
        return;
    }

    // `play` partitions rows exactly as `facet` does, so this is the one case
    // where the refusal has to argue rather than just point: the panels the
    // caller is imagining are moments, and a scale fitted per moment moves the
    // axis under the data (§16). A frame replaces the one before it where a
    // panel sits beside its neighbors, which is why the same freedom is a fair
    // trade in space and a lie in time.
    if spec.layers.iter().any(|l| l.encodings.contains_key(&Channel::Play))
        || spec.channels.contains_key(&Channel::Play)
    {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `free` on `{c}({field})` fits one scale per *panel*, and this \
                 plot has frames rather than panels. A frame replaces the one before \
                 it, so an axis refitted per frame would move under the data and the \
                 motion would be the scale's rather than the data's. Facet the plot to \
                 free a scale across panels, or leave the sequence on one scale."
            ),
        });
        return;
    }

    out.push(Diagnostic {
        kind: DiagnosticKind::Illegal,
        message: format!(
            "gog: `free` on `{c}({field})` fits one scale per panel, and this plot \
             has one panel. Facet it — `plot | facet(<column>)` — or drop `free`."
        ),
    });
}

pub(super) fn check_speed(
    out: &mut Vec<Diagnostic>,
    def: &ChannelDef,
    c: &str,
    field: &str,
    channel: &Channel,
) {
    let Some(s) = def.speed else { return };

    if *channel != Channel::Play {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `speed` on `{c}({field})`: `{c}` is drawn all at once, so it has no \
                 pace to set. Only `play` spends time. Put `speed` on the frames, \
                 `play(<column>)`, or drop it."
            ),
        });
        return;
    }

    // Zero is a frame that never ends and a negative one is a frame that ends
    // before it starts; neither names a pace. Reported rather than clamped, for
    // `tick_count`'s reason — silently drawing the default would leave the caller
    // believing a number they wrote had been honored.
    if !s.is_finite() || s <= 0.0 {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `play({field}, speed = {s})` — a speed is how many times faster \
                 than normal the frames run, so it has to be above zero. \
                 `speed = 2` is twice as fast, `speed = 0.5` half."
            ),
        });
    }
}
