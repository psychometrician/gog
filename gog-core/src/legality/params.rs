//! A transform's own arguments: the values `range`, `deviation`, `quantile`,
//! `density` and a mesh accept.

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
                 by construction. Drop the tiling, or bind both axes to numbers for \
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
                     reaches from the line its category sits on — and `{m} * density` here has \
                     no slots. Bind a category to give it them: \
                     `area * density(reach = {reach}) + x(<number>) + y(<category>)` is the \
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
                 x(<category>) + y(<number>)`. Otherwise drop `compare`.",
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
