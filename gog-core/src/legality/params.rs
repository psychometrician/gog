//! A transform's own arguments: the values `range`, `deviation`, `quantile`,
//! `density` and a mesh accept, and the `places` every computed line takes.

use super::*;

/// `bin(tiling = )` — how the plane is partitioned, and the two ways to ask for
/// it wrongly.
///
/// **An unknown mesh** is a typo, answered with the list. **A mesh on a
/// one-dimensional bin** is the interesting refusal: `bar * bin(tiling = "hex")`
/// is not a typo, it is a reasonable guess about what the parameter means, and
/// the honest answer is that a 1-D bin's cells are *intervals* and an interval
/// has no shape. Only a mark that bins in two dimensions has a plane to tile,
/// and today that is `zone`. Saying so points at the plot the user wanted rather
/// than at their spelling.
pub(super) fn check_tiling(
    out: &mut Vec<Diagnostic>, spec: &PlotSpec, df: Option<&DataFrame>, layer: &Layer,
) {
    let Some(tiling) = layer.bin.as_ref().and_then(|b| b.tiling.as_deref()) else { return };

    if !TILINGS.contains(&tiling) {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `{tiling}` is not a tiling. `bin(tiling = )` takes {}. \
                 `\"rect\"` cuts equal-interval cells on each axis; `\"hex\"` staggers \
                 alternate rows, which stops the eye reading the mesh's own alignment \
                 as structure in the data.",
                TILINGS.iter().map(|t| format!("`\"{t}\"`")).collect::<Vec<_>>().join(" or "),
            ),
        });
        return;
    }

    if layer.mark != Mark::Zone {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `bin(tiling = )` says how to divide a *plane*, and {} `{}` bins one \
                 axis — its cells are intervals, and an interval has no shape. Drop the \
                 tiling for a histogram, or use `zone * bin(tiling = \"{tiling}\")` to cut \
                 both axes into cells and color each by its count.",
                article(mark_name(&layer.mark)), mark_name(&layer.mark),
            ),
        });
        return;
    }

    // **The mixed mesh has two axes and still no plane**, which is the same refusal
    // arrived at from a third side. A tiling partitions a *plane*, and a plane is
    // two axes you can measure distance along: `hex` interleaves two lattices and
    // weights the vertical difference against the horizontal, so it has to compare
    // a step in x with a step in y. A category has no distance to another category
    // — the slots are an *order*, not a metric — so there is nothing for a hexagon
    // to be regular with respect to. The cells of a mixed mesh are rectangles by
    // construction, which is what `"rect"` already says, so only a non-rectangular
    // tiling is refused here.
    let Some(df) = df else { return };
    let slotted = |ch: &Channel| {
        spec.position_for(layer, ch).and_then(|c| actual_type(df, &c.field)) == Some(VarType::Discrete)
    };
    let cat = match (slotted(&Channel::X), slotted(&Channel::Y)) {
        (true, false) => "x",
        (false, true) => "y",
        // Both categorical is `check_distribution_axis`'s refusal — there is no bin
        // at all there, so saying anything about its mesh would be the second
        // message for one mistake.
        _ => return,
    };
    if tiling != "rect" {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `bin(tiling = \"{tiling}\")` partitions a *plane*, and `{cat}` here is \
                 categorical — its slots are an order, not a distance, so there is nothing \
                 for a hexagon to be regular against. This plot is the mixed mesh: one axis \
                 cut into cells, one row of them per category, and its cells are rectangles \
                 by construction. Drop the tiling, or bind continuous columns to both axes for \
                 `bin(tiling = \"{tiling}\")` to have a plane to cut."
            ),
        });
    }
}

/// `density`'s two knobs, each refused in the reading it cannot mean — the mirror
/// pair of `bin(tiling = )`'s refusal, and for the same kind of reason.
///
/// **`levels` needs contours.** It says how many iso-lines to trace, so it belongs
/// to the reading that *traces* — `path * density` — and to no other. A
/// one-dimensional `density` is one curve, with nothing to cut it into; a `zone *
/// density` has the whole plane and still traces nothing, painting the field as
/// cells whose color is the estimate itself. Both are refused and pointed at the
/// path, exactly as `bin(tiling = )` is refused on a 1-D bin because an interval has
/// no shape.
///
/// The zone half was accepted and **silently dropped** until 2026-07-25, because the
/// test asked `reads_a_field` — true of a painted field as much as a traced one —
/// when the question was whether anything gets traced. "Needs a plane" was the wrong
/// half of the reason, and the wrong half still passed every plot the contour had.
///

/// **`bandwidth` needs one axis.** It is a length in the data's own units, and a
/// field has *two* columns carrying different quantities, so one number cannot mean
/// both — the lesson `hex` learned when its circumradius had to come out as a
/// half-width and a half-height. `adjust` is dimensionless and means the same thing
/// on either axis, so the refusal points there rather than inventing a second knob.
/// Not a gap: a per-axis bandwidth is a *pair*, and a pair is a different parameter
/// from a scalar, to be decided if a plot ever wants it.
/// `range(low, high)` takes two **quantile probabilities**, so both ends have to
/// be in 0..1 and the low end has to be below the high one.
///
/// **This is legality rather than arithmetic, and the placement is the point.**
/// `quantile_sorted` clamps, so a probability of 1.5 would quietly become 1.0 and
/// draw a band nobody asked for — the silent drop §12 forbids, wearing a number
/// instead of a channel. Answered here it is fatal and says what to write; left
/// in the transform it could only warn after the fact.
///
/// The equal case is refused with the two directions that actually differ: a
/// reader who wanted one number wanted a reduction, and a reader who wanted a
/// band mistyped one of the ends.
pub(super) fn check_range_params(out: &mut Vec<Diagnostic>, layer: &Layer) {
    let Some(spec) = layer.range.as_ref() else { return };
    let (lo, hi) = RangeSpec::probabilities(Some(spec));

    for (name, p) in [("low", lo), ("high", hi)] {
        if !p.is_finite() || !(0.0..=1.0).contains(&p) {
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: format!(
                    "gog: `range({name} = {p})` is not a probability — the two ends of the \
                     band are quantiles, so each is between 0 and 1. `range(0.25, 0.75)` is \
                     the middle half, `range(0.1, 0.9)` the middle 80 percent, and bare \
                     `range` the whole group. For bounds you computed yourself, in the \
                     data's own units, use `bounds(<lower>, <upper>)`."
                ),
            });
        }
    }

    if lo.is_finite() && hi.is_finite() && (0.0..=1.0).contains(&lo) && (0.0..=1.0).contains(&hi)
        && lo >= hi
    {
        let same = lo == hi;
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `range({lo}, {hi})` has no band to draw — the low end must be below \
                 the high one.{}",
                if same {
                    " Two ends at the same quantile is one number rather than a span: write \
                     `median` for the middle, or `quantile(<p>)` for another point."
                } else {
                    " Swap them: `range(<low>, <high>)` reads low end first."
                },
            ),
        });
    }
}

/// `deviation(k)` is how many standard deviations reach each side of the mean, so
/// the multiplier is a positive number. Zero is refused rather than drawn as a
/// flat line, because a band of width zero is a *point* and the sentence that
/// draws one is `point * mean`.
pub(super) fn check_deviation_params(out: &mut Vec<Diagnostic>, layer: &Layer) {
    let Some(spec) = layer.deviation.as_ref() else { return };
    let Some(k) = spec.multiplier else { return };
    if !k.is_finite() || k <= 0.0 {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `deviation({k})` is not a width — the multiplier counts standard \
                 deviations out from the mean, so it is a positive number. \
                 `deviation` is one, `deviation(2)` is two. For the mean alone, with no \
                 band, that is `point * mean`."
            ),
        });
    }
}

/// `quantile(p)` needs its probability, and it is the one transform whose
/// parameter is **required rather than defaulted** (§5): the only defensible
/// default is 0.5, and that is `median`.
///
/// This also carries the redundancy warning the family decision turns on. At
/// p = 0, 0.5 and 1 the answer is exactly `min`, `median` and `max`, so the plot
/// **draws** and an Assumption names the plain atom. Refusing there was the
/// stricter Law 1 reading and was rejected because a generated sweep over deciles
/// would die at 0.5; saying nothing was rejected because two spellings for one
/// result is what Law 1 exists to notice. §12's standing rule: a contradiction
/// refuses, a proven redundancy warns.
pub(super) fn check_quantile_params(out: &mut Vec<Diagnostic>, layer: &Layer) {
    if !layer.transforms.contains(&Transform::Quantile) { return }
    let p = layer.quantile.as_ref().and_then(|s| s.p);

    let Some(p) = p else {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: "gog: `quantile` needs the probability it reduces to, e.g. \
                      `quantile(0.9)` for the 90th percentile. There is no default, \
                      because the only sensible one is the middle and that already has \
                      a plain name: `median`."
                .to_string(),
        });
        return;
    };

    if !p.is_finite() || !(0.0..=1.0).contains(&p) {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `quantile({p})` is not a probability — a quantile is between 0 and \
                 1. `quantile(0.9)` is the 90th percentile, `quantile(0.5)` the middle."
            ),
        });
        return;
    }

    // The three points where this atom and a plain name meet. Drawn, and said.
    let plain = if p == 0.0 { Some("min") } else if p == 0.5 { Some("median") }
                else if p == 1.0 { Some("max") } else { None };
    if let Some(plain) = plain {
        out.push(Diagnostic {
            kind: DiagnosticKind::Assumption,
            message: format!(
                "gog: `quantile({p})` is the {plain} — write `{plain}` for the plain name. \
                 Drawn as asked."
            ),
        });
    }
}

pub(super) fn check_density_params(out: &mut Vec<Diagnostic>, plot: &PlotSpec, df: Option<&DataFrame>,
                        layer: &Layer, space: SpaceKind) {
    let Some(spec) = layer.density.as_ref() else { return };
    // A *field*, not merely cells: `levels` cuts a continuous surface into level sets
    // and `bandwidth` is a length along one, so neither reaches a tally into
    // categorical slots. Asking `measures_cells` here would let `zone * count +
    // density(levels = 4)` through unremarked, which is the silent drop §12 forbids.
    let field = reads_a_field(&layer.mark, &layer.transforms, space);
    let m = mark_name(&layer.mark);

    // `levels` needs a **plane**, and both marks that have one can use it: it says
    // *cut the field into this many levels*, after which a `path` traces their
    // boundaries and a `zone` fills between them. So the refusal is only for the
    // one-dimensional reading, where there is a single curve and nothing to cut it
    // into. (It was briefly refused on a `zone` too, which closed the silent drop
    // by removing the request rather than answering it; filling the bands answers it,
    // and is the geometry §18 had wrongly assumed needed holes.)
    if spec.levels.is_some() && !field {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `density(levels = )` cuts a field into levels, and {} `{m}` estimates \
                 *one curve* — a curve has no levels to cut. Drop it for a density curve, or \
                 read the density over both axes: `path * density(levels = )` traces the \
                 contours, `zone * density(levels = )` fills the bands between them.",
                article(m),
            ),
        });
    }

    // **A `surface` reads a field and still refuses `levels`**, which is why this is
    // its own arm rather than a third case of the test above. The level sets are
    // regions *in the plane*, and a surface has already spent the third axis on the
    // measurement — so a band on a sheet could only be a color, which is `zone`'s
    // reading of the same request. Refused with direction rather than accepted and
    // ignored, which is what `zone * density(levels = )` itself did for a while
    // (spec §5).
    //
    // **A stepped sheet is drawn, and it is cut on the other axis** (2026-07-28).
    // `surface * bin` cuts the **floor** into cells and lays a plateau on each, which
    // is a different geometry from what `levels` asks for: `levels` would quantize the
    // **measurement**, giving bands of equal height rather than one plateau per cell.
    // So this arm keeps its refusal and gains a direction — the note that used to sit
    // here, *"a terraced surface is drawable and simply is not drawn"*, was true of the
    // floor-cut sheet until it was built and is still true of the height-cut one, which
    // is exactly why an undrawn-but-drawable note has to say **which** thing it means.
    if spec.levels.is_some() && layer.mark == Mark::Surface {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: "gog: `density(levels = )` cuts a field into level sets, which are regions \
                      in the plane — a `surface` draws the field itself, with the estimate as its \
                      height, so it has no axis left to put a band on. Drop `levels` for the \
                      sheet, or `zone * density(levels = )` to fill the bands and \
                      `path * density(levels = )` to trace their boundaries. For a sheet in \
                      steps, cut the floor rather than the height: \
                      `surface * bin * mean + x(<a>) + y(<b>) + z(<column>)` lays a flat \
                      plateau on every cell."
                .to_string(),
        });
    }

    if spec.bandwidth.is_some() && field {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `density(bandwidth = )` is a width in one column's own units, and \
                 `{m} * density` spreads over *two* columns measuring different quantities — one \
                 number cannot be a width in both. Use `density(adjust = )`, which scales the \
                 automatic bandwidth on each axis by the same dimensionless factor."
            ),
        });
    }

    // `reach` is the **fourth** knob to belong to exactly one reading, and it is the
    // slot's second: with no slot there is nothing to measure a reach in. Refused
    // before `compare` so a sentence carrying both hears about both.
    let violin = slot_density(plot, layer, df).is_some();
    if let Some(reach) = spec.reach {
        if !violin {
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: format!(
                    "gog: `density(reach = )` is measured in **slots** — how far each violin \
                     reaches from the line its category is on — and `{m} * density` here has \
                     no slots. Bind a category to give it them: \
                     `area * density(reach = {reach}) + x(<measure>) + y(<category>)` is the \
                     ridgeline. Otherwise drop `reach`."
                ),
            });
        } else if !(reach > 0.0) || !reach.is_finite() {
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: format!(
                    "gog: `density(reach = {reach})` needs a positive number of slots. \
                     `reach = 0.4` (the default) keeps each shape inside its own slot; \
                     past `0.5` they overlap, which is the ridgeline plot."
                ),
            });
        }
    }

    // `compare` is the **third** knob to belong to exactly one reading, and it is
    // refused in the other two on the precedent the first two set. It answers *what
    // does the width mean from slot to slot*, and neither the curve nor the field has
    // slots to compare: a curve is one estimate with a whole axis to itself, and a
    // field measures by color with no width in it at all.
    let Some(how) = spec.compare.as_deref() else { return };
    if !violin {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `density(compare = )` says what a violin's width means from one slot to \
                 the next, and `{m} * density` here has no slots — it estimates {}. Bind a \
                 category to give it slots to compare: `ribbon * density(compare = \"{how}\") + \
                 x(<category>) + y(<measure>)`. Otherwise drop `compare`.",
                if field { "a field over both axes" } else { "one curve along one axis" },
            ),
        });
        return;
    }
    // A value the engine does not know is a silent drop waiting to happen (§12): it
    // would fall through to the default and draw a plot answering the other question.
    if how != crate::ir::COMPARE_SHAPE && how != crate::ir::COMPARE_COUNT {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `density(compare = \"{how}\")` is not a reading this engine has. \
                 `compare = \"shape\"` draws every violin to the same area, so the shapes are \
                 what you compare; `compare = \"count\"` scales each one by how many rows its \
                 group has, so a thin violin is a small group."
            ),
        });
    }
}

/// `places` — how many evenly spaced places a curve, a violin, a field or a fit
/// is computed at — and the three ways to ask for it wrongly.
///
/// **Too few.** One place leaves no spacing to divide the range by, so the count
/// is at least [`crate::ir::MIN_PLACES`]. Two is allowed, and draws a straight
/// segment through a curve: ugly, and legal (Law 8).
///
/// **Too many.** The count is the size of what is drawn, a vertex per place, or on
/// a field a cell per place squared, so each reading has a ceiling
/// ([`crate::ir::places_limit`]). The refusal says the reading's ceiling and its
/// default, since the default is the number a reader most needs and can least
/// guess: it differs between the curve, the violin, the field and the fits.
///
/// **On plain `confidence`.** It computes one interval per group, not a line along
/// an axis, so there are no places to count. The same struct carries the level of
/// `smooth_band` and `straight_band`, which do take places, so the wire can carry
/// the field here; refused rather than ignored (§12).
pub(super) fn check_places(out: &mut Vec<Diagnostic>, plot: &PlotSpec, df: Option<&DataFrame>,
                           layer: &Layer, space: SpaceKind) {
    use crate::ir::{DEFAULT_CURVE_PLACES, DEFAULT_FIELD_PLACES, DEFAULT_FIT_PLACES,
                    DEFAULT_SLOT_PLACES, MIN_PLACES, places_limit};
    use crate::ir::count::shown;

    if let Some(n) = layer.confidence.as_ref().and_then(|c| c.places).map(shown) {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `confidence(places = {n})` has nothing to count: `confidence` \
                 computes one interval for each group, not a line along an axis. \
                 `places` belongs to the bands along a fitted line, \
                 `ribbon * smooth_band(places = {n})` and \
                 `ribbon * straight_band(places = {n})`. Drop it here."
            ),
        });
    }

    // (the call as written, what one place is a place *of*, the default, the ceiling)
    let mut asked: Vec<(String, &str, usize, usize, bool)> = Vec::new();
    if let Some(n) = layer.density.as_ref().and_then(|d| d.places) {
        let call = format!("density(places = {})", shown(n));
        if reads_a_field(&layer.mark, &layer.transforms, space) {
            asked.push((call, "each axis of a field", DEFAULT_FIELD_PLACES, n, true));
        } else if slot_density(plot, layer, df).is_some() {
            asked.push((call, "each violin", DEFAULT_SLOT_PLACES, n, false));
        } else {
            asked.push((call, "a density curve", DEFAULT_CURVE_PLACES, n, false));
        }
    }
    for (word, spec) in [("smooth", layer.smooth.as_ref()), ("straight", layer.straight.as_ref())] {
        if let Some(n) = spec.and_then(|f| f.places) {
            asked.push((format!("{word}(places = {})", shown(n)), "a fitted line", DEFAULT_FIT_PLACES, n, false));
        }
    }
    for (word, spec) in [("smooth_band", layer.smooth_band.as_ref()),
                         ("straight_band", layer.straight_band.as_ref())] {
        if let Some(n) = spec.and_then(|c| c.places) {
            asked.push((format!("{word}(places = {})", shown(n)), "a band along a fitted line", DEFAULT_FIT_PLACES, n, false));
        }
    }

    for (call, of, default, n, field) in asked {
        let limit = places_limit(field);
        if n < MIN_PLACES {
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: format!(
                    "gog: `{call}` computes {of} at fewer than two places, and a line needs \
                     at least two to run between. Give a whole number from {MIN_PLACES} to \
                     {limit}; the default for {of} is {default}."
                ),
            });
        } else if n > limit {
            // Only a count that was written out is squared: one too large to hold
            // has no square worth printing.
            let size = if field && n != usize::MAX {
                format!(
                    " A field is computed on a grid, so {n} places on each axis is {} cells, \
                     and the file grows with every one.",
                    n.saturating_mul(n)
                )
            } else if field {
                " A field is computed on a grid, so its size is the square of the count.".to_string()
            } else {
                String::new()
            };
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: format!(
                    "gog: `{call}` is more places than {of} takes: at most {limit}.{size} \
                     The default for {of} is {default}."
                ),
            });
        }
    }
}

/// `bin`'s count and `density`'s levels have a ceiling, and the one place a count
/// was refused only after it had reached the transform: a bin is kept in memory
/// whether or not a row falls in it, so `bin(1e9)` used up the computer's memory
/// instead of answering.
///
/// **A width is a count too.** `bin(width = 0.000001)` over fifty years cuts fifty
/// million bins, so the width is turned into the count it asks for, on each axis
/// the layer cuts (both of a two-dimensional reading, the one it groups by
/// otherwise, read as the renderer reads it), and held to the same ceiling. A log
/// axis is cut in log units, which no width this small reaches, so it is left out.
pub(super) fn check_counts(out: &mut Vec<Diagnostic>, spec: &PlotSpec, df: Option<&DataFrame>,
                           layer: &Layer, horizontal: bool) {
    use crate::ir::count::shown;
    use crate::ir::{DEFAULT_LEVELS, MAX_BINS, MAX_LEVELS};

    let kept = "Every bin is kept while the plot is drawn, even one that holds no row, so a \
                count this large can use up the computer's memory.";
    if let Some(n) = layer.bin.as_ref().and_then(|b| b.bins).filter(|&n| n > MAX_BINS) {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `bin({})` cuts the axis into more bins than gog draws: at most 1,000,000. \
                 {kept} Leave the count out and gog chooses one by Sturges' rule, or give a \
                 width: `bin(width = 5)`.",
                shown(n)
            ),
        });
    }
    if let Some(n) = layer.density.as_ref().and_then(|d| d.levels).filter(|&n| n > MAX_LEVELS) {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `density(levels = {})` cuts the field into more levels than gog draws: at \
                 most 1,000. Each level is a set of rings, so the file grows with the count. \
                 The default is {DEFAULT_LEVELS}.",
                shown(n)
            ),
        });
    }

    let (Some(df), Some(w)) = (df, layer.bin.as_ref().and_then(|b| b.width)) else { return };
    if !(w > 0.0) || !layer.transforms.contains(&Transform::Bin) { return }
    let axes: Vec<Channel> = if reads_two_dimensions(&layer.mark, &layer.transforms, space_of(spec)) {
        vec![Channel::X, Channel::Y]
    } else if horizontal && reads_orientation(layer) {
        vec![Channel::Y]
    } else {
        vec![Channel::X]
    };
    for ch in axes {
        let Some(field) = spec.position_for(layer, &ch).map(|e| e.field.clone()) else { continue };
        let logged = spec.axis_def(&ch).is_some_and(|d| d.scale == Some(crate::ir::ScaleType::Log));
        let Some(vals) = df.float_col(&field) else { continue };
        if logged { continue }
        let (lo, hi) = vals.iter().filter(|v| v.is_finite())
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), &v| (a.min(v), b.max(v)));
        if !(hi > lo) { continue }
        let bins = ((hi - lo) / w).ceil();
        if bins > MAX_BINS as f64 {
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: format!(
                    "gog: `bin(width = {w})` cuts `{field}` into about {} bins, and gog \
                     draws at most 1,000,000. {kept} Give a wider width, or a count: `bin(30)`.",
                    grouped(bins)
                ),
            });
            return;
        }
    }
}

/// A large whole number with its thousands marked, `7,000,000,000`, so a count in a
/// message can be read at a glance.
fn grouped(v: f64) -> String {
    let digits = format!("{v:.0}");
    let mut out = String::new();
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 { out.push(','); }
        out.push(ch);
    }
    out
}

/// Settings for a transform the layer does not have. A package sends a
/// transform's settings only beside the transform, so this is reached by JSON
/// written by hand, by a tool, or by a binding's own bug, which is how it was found:
/// R's `$` matches a partial name, so a band's layer once sent its level and count
/// under `smooth` too. The engine read nothing from them and drew the plot as if
/// they were not there, the silent drop §12 forbids. A `box` carries its own
/// settings as the mark's, with no transform written.
pub(super) fn check_unread_settings(out: &mut Vec<Diagnostic>, layer: &Layer) {
    let has = |t: Transform| layer.transforms.contains(&t);
    let carried: [(&str, bool, bool); 19] = [
        ("bin", layer.bin.is_some(), has(Transform::Bin)),
        ("density", layer.density.is_some(), has(Transform::Density)),
        ("range", layer.range.is_some(), has(Transform::Range)),
        ("confidence", layer.confidence.is_some(), has(Transform::Confidence)),
        ("smooth_band", layer.smooth_band.is_some(), has(Transform::SmoothBand)),
        ("straight_band", layer.straight_band.is_some(), has(Transform::StraightBand)),
        ("smooth", layer.smooth.is_some(), has(Transform::Smooth)),
        ("straight", layer.straight.is_some(), has(Transform::Straight)),
        ("deviation", layer.deviation.is_some(), has(Transform::Deviation)),
        ("quantile", layer.quantile.is_some(), has(Transform::Quantile)),
        ("box", layer.r#box.is_some(), layer.mark == Mark::Box || has(Transform::Box)),
        ("jitter", layer.jitter.is_some(), has(Transform::Jitter)),
        ("partition", layer.partition.is_some(), has(Transform::Partition)),
        ("flow", layer.flow.is_some(), has(Transform::Flow)),
        ("layout", layer.layout.is_some(), has(Transform::Layout)),
        ("cluster", layer.cluster.is_some(), has(Transform::Cluster)),
        ("stack", layer.stack.is_some(), has(Transform::Stack)),
        ("cumulative", layer.cumulative.is_some(), has(Transform::Cumulative)),
        ("bounds", layer.bounds.is_some(), has(Transform::Bounds)),
    ];
    let m = mark_name(&layer.mark);
    for (word, present, read) in carried {
        if present && !read {
            let how = if word == "box" {
                "a `box` mark".to_string()
            } else {
                format!("the transform, as in `{m} * {word}`")
            };
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: format!(
                    "gog: this `{m}` layer carries settings for `{word}`, and it has no `{word}`, \
                     so nothing would read them and the plot would be drawn as if they were not \
                     there. Write {how}, or leave the settings out."
                ),
            });
        }
    }
}
