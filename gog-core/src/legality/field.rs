//! A two-dimensional field: the mesh a layer cuts both positions into, and the
//! measure its cells hold.

use super::*;

/// Does this layer read a **field** — a continuous plane, cut or estimated?
///
/// [`measures_cells`]'s narrower half, and the distinction the tile plot forced: a
/// field is a quantity that exists *between* the data points, so it needs two axes
/// with somewhere to spread. A tally into categorical slots is not one, and the
/// checks that guard a field's own vocabulary (`bandwidth`, `levels`, "both axes must
/// be continuous") must not fire on it.
///
/// **The mixed mesh sits inside this and is the reason to read the name carefully.**
/// `zone * bin` over one continuous axis and one categorical is a field in *one*
/// dimension per slot, and it answers `true` — correctly, because what both callers
/// actually want to know is whether a transform published an extent description for
/// the axes it cut. The one check that reads this as "two continuous axes" is
/// `check_density_params`, which is only reachable with a `density` in the layer, and
/// a mixed `density` is refused fatally upstream.
/// **The dimensionality half, so it takes the space** ([`cuts_both_positions`]): a
/// 3-D `bar`'s floor is cut into cells by the same `bin` that cuts a `zone`'s plane,
/// and publishes the same extent description, so the mark writer reads the same four
/// edge columns. That is the whole of what this predicate is for, and it is why the
/// 3-D histogram needed no second code path to find its footprint.
pub fn reads_a_field(mark: &Mark, transforms: &[Transform], space: SpaceKind) -> bool {
    cuts_both_positions(mark, space)
        && transforms.iter().any(|t| matches!(t, Transform::Bin | Transform::Density))
}

/// Did a transform hand this layer the **four edges of its cells**, rather than
/// leaving the mark to find them on the axes or in named columns?
///
/// [`reads_a_field`] answers a narrower question — *was a plane cut and measured*
/// — and a partition cut a **tree**, so it publishes the same four columns for a
/// different reason and has no field anywhere. Both callers need the wider
/// question: `zone` to know its sides were made for it, and `svg`'s axis fit to
/// know the extents live in columns rather than in the bound ones.
///
/// One function because the file that reads it says why: a hand-written list
/// beside a generated one always loses, and this one already lost once when
/// `zone` learned to `bin`.
pub fn publishes_cells(mark: &Mark, transforms: &[Transform], space: SpaceKind) -> bool {
    reads_a_field(mark, transforms, space)
        || transforms.contains(&Transform::Partition)
        || transforms.contains(&Transform::Flow)
}

/// Which geometry a two-dimensional reading draws the field as.
///
/// The `step` ruling, in the one place that decides it: **the mark chooses the
/// geometry, the transform stays constant.** A field is a field; what differs is
/// whether it is shown as the mesh it was sampled on or as the shape of its own
/// level sets. Asked here rather than in `transform.rs`, which stays free of marks,
/// and in one function so the router, the two mark writers, the legend and the
/// legality checks cannot disagree about what a layer is drawing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldGeometry {
    /// One filled rectangle per cell of the mesh — the heatmap, counted or estimated.
    Cells,
    /// The traced level sets: `path` strokes their boundaries (the contour), `zone`
    /// fills them (the filled contour).
    Rings,
}

/// The geometry this layer draws, or `None` if it is not a two-dimensional reading.
///
/// **`levels` is what discretizes a field**, and it means the same thing to both
/// marks: *cut it into this many levels*. A `path` has only one thing it can do with
/// them — trace the boundaries — so it is always `Rings`. A `zone` can do either, and
/// the parameter is the request: without it the field stays continuous and is painted
/// cell by cell; with it the field becomes bands, and a band is bounded by exactly
/// the curve `path` would have drawn. That is why one number serves both marks
/// instead of one of them needing a second name.
pub fn field_geometry(layer: &Layer) -> Option<FieldGeometry> {
    if !measures_cells(&layer.mark, &layer.transforms) {
        return None;
    }
    let banded = layer.density.as_ref().is_some_and(|d| d.levels.is_some());
    Some(match layer.mark {
        Mark::Path => FieldGeometry::Rings,
        _ if banded && layer.transforms.contains(&Transform::Density) => FieldGeometry::Rings,
        _ => FieldGeometry::Cells,
    })
}

/// Which column this reading measured itself by — the one `color` titles itself from
/// and the one a user may name out loud.
///
/// Follows the geometry rather than the mark: rings are measured by the `level` they
/// were cut at whoever draws them, cells by what was tallied or estimated in each.
pub fn field_measure(layer: &Layer) -> Option<&'static str> {
    match field_geometry(layer)? {
        FieldGeometry::Rings => Some(crate::transform::FIELD_LEVEL),
        FieldGeometry::Cells => crate::transform::cell_measure(&layer.transforms),
    }
}

/// The **two-dimensional readings** — `zone * bin`, `zone * density`, `zone * count`,
/// `path * density` — and the two things every one of them needs that a *bounded*
/// zone or a plain path does not.
///
/// One function for all of them because the requirement is not a property of the mark
/// or of the transform but of the *reading*: measuring per cell is what makes both
/// positions compulsory and what spends the measurement on a synthesized column.
/// Written per mark it would be four copies, and the copy that drifted would be
/// the one nobody had a plot for.
///
/// **Both positions.** A bounded zone may name neither axis and still draw: it
/// spans the panel where it is not given a pair, which is the mark's whole point.
/// A cell reading is the opposite case — it has nothing to measure into until both
/// axes name a column. So `x` and `y` stop being optional here, and their absence is
/// refused rather than drawn as an empty panel (the silent-drop §12 forbids, and the
/// failure `area` once had).
///
/// **A color it did not compute.** The measure *is* the count, or the density, or
/// the share, or the level: the transform replaces every row with rows of its own, so
/// any other column the user might have colored by no longer exists downstream.
/// Binding one is not a preference the engine can honor and quietly ignore — it is a
/// request for a column that is gone, so it is refused, and the refusal names the
/// bindings that do mean something.
pub(super) fn check_field(out: &mut Vec<Diagnostic>, spec: &PlotSpec, df: Option<&DataFrame>, layer: &Layer) {
    if !measures_cells(&layer.mark, &layer.transforms) {
        return;
    }
    // A transform the mark does not take is refused on its own, toward the mark
    // that takes it. Read here as a field, `path * count` was also told it traces
    // the density's contours and to write `path * count + x(<column>) +
    // y(<column>)`, a sentence refused in turn.
    if layer.transforms.iter().any(|t| !is_collision_modifier(t)
        && mark_takes_transform(&layer.mark, t) == TransformLegality::None)
    {
        return;
    }
    // A *reduction* composed alongside is the doubly-measured cell, and
    // `check_pair_summary` says so in both transforms' names. Deferred rather than
    // said twice: this message would tell the reader their `color` binding has
    // nothing to read, when in fact it is the binding their `mean` needs and the
    // `count` beside it is the part to drop. One mistake, one refusal — the same
    // deference `check_zone_extent` already makes to the same check.
    if crate::transform::has_reduction(&layer.transforms) {
        return;
    }
    let m = mark_name(&layer.mark);
    // The transform to name back, in the order the reading is decided: a cut beats a
    // tally when both are somehow present, because the cut is what made the cells.
    let t = match &layer.transforms {
        ts if ts.contains(&Transform::Bin) => "bin",
        ts if ts.contains(&Transform::Density) => "density",
        ts if ts.contains(&Transform::Count) => "count",
        _ => "proportion",
    };
    // What this reading draws, and what its measurement is called — both read off
    // `field_geometry`, the one function that decides it, so a message here cannot
    // describe a plot the renderer does not draw.
    let rings = field_geometry(layer) == Some(FieldGeometry::Rings);
    let measure = field_measure(layer).unwrap_or("");
    let shape = match (rings, layer.mark == Mark::Path, t) {
        (true, true, _) => "traces the density's contours",
        (true, false, _) => "fills the density's contour bands",
        // The tally's cells are not cut, they are the categories — so it needs both
        // axes for the reason a cut does, arrived at from the other side.
        (_, _, "count" | "proportion") => "fills the cell where each pair of categories crosses",
        _ => "cuts both axes into cells",
    };

    // Resolved per layer: a zone naming its own columns for the shared axes has
    // both, even with nothing bound at the plot level (spec §8).
    let missing: Vec<&str> = [("x", Channel::X), ("y", Channel::Y)]
        .into_iter()
        .filter(|(_, ch)| spec.position_for(layer, ch).is_none())
        .map(|(n, _)| n)
        .collect();
    if !missing.is_empty() {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `{m} * {t}` {shape}, so it needs both axes — this plot has \
                 no `{}()`. Write `{m} * {t} + x(<column>) + y(<column>)`.",
                missing.join("()` and no `"),
            ),
        });
    }

    if let Some(def) = layer.encodings.get(&Channel::Color) {
        if def.field != measure {
            let did = match (rings, t) {
                (true, _) => "already measures each band by the density it was cut at",
                (_, "bin" | "count") => "already measures each cell by how many rows fell in it",
                (_, "proportion") => "already measures each cell by its share of the rows",
                _ => "already measures each cell by the density it estimated there",
            };
            // A *split* is what a category usually meant here, and on a `path` it is
            // spelled `group` — which runs the whole estimate once per category and
            // leaves color carrying the level. A zone refuses `group` outright
            // (`rule_for`: one row is one rectangle), so it is offered faceting.
            // **A number is not a category to facet on.** Offered for every zone
            // until 2026-10-02, the facet sent `zone * bin + color(life)` to
            // `| life`, refused in turn. A number on `color` is a column the reader
            // wanted each cell colored by, which is a summary of the cell's rows:
            // composed with the cut for `bin` and `density`, the summary heatmap,
            // and in place of the tally for `count` and `proportion`, since two
            // measurements of one cell contradict each other.
            let numeric = df.and_then(|d| actual_type(d, &def.field)) == Some(VarType::Continuous);
            let instead = if layer.mark == Mark::Path {
                format!(" To draw one set of contours per category, `group({})` splits the \
                         estimate and leaves color to the level.", def.field)
            } else if numeric {
                let summary = match t {
                    "count" | "proportion" => format!("{m} * mean"),
                    _ => format!("{m} * bin * mean"),
                };
                format!(" To color each cell by the mean of `{f}`, write `{summary} + \
                         color({f})`; `sum`, `median`, `max` and `min` summarize it the other \
                         ways.", f = def.field)
            } else {
                format!(" To compare across a category, facet on it: `| {}`.", def.field)
            };
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: format!(
                    "gog: `{m} * {t}` {did}, and color is where that measurement goes — so \
                     `color({})` has nothing to read: the transform replaced those rows with \
                     its own. Drop the binding and the measurement colors the {}, or say \
                     `color({measure})` to name it out loud.{instead}",
                    def.field,
                    if rings { "bands" } else { "cells" },
                ),
            });
        }
    }
}
