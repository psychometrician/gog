//! What each coordinate space asks of the layers drawn in it: the cube, polar,
//! nest, map, globe and network.

use super::*;

/// The network space's own gate, `check_nest`'s shape one space over: what may
/// stand in it, and what it has none of to name.
pub(super) fn check_network(out: &mut Vec<Diagnostic>, spec: &PlotSpec) {
    if !matches!(spec.coord, CoordSpace::Network(_)) {
        return;
    }

    // A space no layout feeds is a room with nothing in it — the syllable is
    // the mark, its layout, and the space together.
    if !spec.layers.iter().any(|l| l.transforms.contains(&Transform::Layout)) {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: "gog: `network()` draws what a `layout` places, and nothing \
                      here carries one. Compose the syllable: `edge * layout(from, \
                      to) + network()`, with `point * layout(...)` for the nodes \
                      and `text * layout(...) + label(name)` for their names."
                .to_string(),
        });
        return;
    }

    // The marks that stand here are the layout's three readers; everything else
    // reads a domain, a measure, or an extent along axes this space does not have.
    // A layer that carries `layout` on a mark with no reading for it is refused by
    // `check_layout`, which names the same three marks, so it is not said twice.
    for layer in &spec.layers {
        if !mark_draws_in_space(&layer.mark, SpaceKind::Network)
            && !layer.transforms.contains(&Transform::Layout)
        {
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: format!(
                    "gog: `{}` does not stand in `network()` — it reads an axis, \
                     and this space has none. The layout's readers are `edge`, \
                     `point` and `text`.",
                    mark_name(&layer.mark),
                ),
            });
        }
    }

    // Names in the cube are drawn (2026-10-09). They were refused as unbuilt while
    // the 3-D pass had no text writer; it has one now, the same one every cube
    // uses, so a name stands at its node there as on the flat page.

    // The axes are not there to be named — `nest`'s rule, for `nest`'s reason.
    for (label, atom) in [
        (&spec.x_axis.label, "x_label"),
        (&spec.y_axis.label, "y_label"),
        (&spec.z_axis.label, "z_label"),
    ] {
        if label.is_some() {
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: format!(
                    "gog: `{atom}()` names an axis, and a network has none — its \
                     positions are the layout's, and a name under them would be \
                     read as a promise that the direction measures something. \
                     Remove it; `title()` still names the plot."
                ),
            });
        }
    }

    // A brush names a range of data, and a network's positions are not data.
    if !spec.brush.is_empty() {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: "gog: `brush()` selects by a range of the data's own axes, and \
                      a network's positions are the layout's — a rectangle over \
                      them names nothing a reader can mean. Select by a column \
                      instead, upstream, and color what you selected."
                .to_string(),
        });
    }
}

// ---------------------------------------------------------------------------
// Coord — a space the engine cannot draw in at all
//
// `check_space`, `check_polar` and `check_nest` each gate one *built* space, and
// every one of them asks its question per layer: which marks stand in the cube,
// which mark a packing has no position to give. None of them can answer the
// question one level up — *is this space drawn at all?* — and for the two spaces
// where the answer is no, nothing asked it.
//
// So `map` and `globe` were accepted and drawn flat, in complete silence.
// `space_of` reported them correctly, `mark_draws_in_space` answered `false` for
// all thirteen marks, and the renderer reads neither: it laid out ordinary axes
// and drew the plot as though the atom had never been written. That is the
// accept-and-drop §12 forbids, and it is a failure no test could have caught,
// because the assertion that would have caught it was the missing function
// itself. `mark_draws_in_space`'s doc comment has named `check_coord` as one of
// its two readers since the day it was written; `check_coord` did not exist.
//
// The question is put to the rule table rather than to a list of two names, so
// this stops firing for a space the moment one mark draws there and the
// per-layer gates take over. That is what has to happen when a space lands one
// mark at a time.
// ---------------------------------------------------------------------------

pub(super) fn check_coord(out: &mut Vec<Diagnostic>, spec: &PlotSpec) {
    let space = space_of(spec);
    if ALL_MARKS.iter().any(|m| mark_draws_in_space(m, space)) {
        return;
    }
    let s = space_name(space);
    // The direction is per space, because what a reader should do next differs.
    // Dropping the atom is the answer in both cases, but only one of them can say
    // what the flat plot will then show — and saying it is the point: that flat
    // plot is exactly what the engine drew here without being asked.
    let direction = match space {
        SpaceKind::Map => format!(
            "Drop `{s}()` to draw the plot flat, with longitude across and latitude up."
        ),
        _ => format!("Drop `{s}()` to draw the plot flat."),
    };
    out.push(Diagnostic {
        kind: DiagnosticKind::Unsupported,
        message: format!(
            "gog: `{s}()` is valid grammar, but this engine does not draw it: no mark \
             stands in that space. {direction}"
        ),
    });
}

// ---------------------------------------------------------------------------
// Space — the 3-D coordinate
//
// `z` makes a plot 3-D (spec §15): binding it is the trigger, the way
// orientation is read off the bindings. Which marks stand in the cube is
// `rule_for(_, Z).renders`; a mark whose cell is blank keeps `renders: None`, so
// the per-channel loop already refuses it with direction. What is left to catch
// here is what that loop cannot see: a viewing angle with no third dimension, a
// non-linear z, and `smooth` in a space with no domain to run along — each
// refused or reported rather than half-drawn (§12), never silently dropped.
//
// **A facet is not on that list, and the reason is worth keeping.** It was, for
// as long as the cube existed: `check_space` refused `z` with a facet as "not
// drawn yet — per-panel projection that is not built". That was never true. The
// 3-D branch has sat inside `for panel in &grid.panels` for as long as facets
// and cubes have coexisted, projecting a `Scene` from each panel's own rect, and
// the proof was already shipping — a *synthesized*-z histogram
// (`bar * bin + x + y + space() | facet(g)`) reached the renderer past this
// function's early return on `axis_def(Z)` and drew one cube per panel under
// `GOG_STRICT=1`, while the bound-z sentence beside it was refused. One
// partition, two answers: the Law 2 break was the gate, not the drawing.
// ---------------------------------------------------------------------------

/// A viewing angle without a third dimension: legal, but it projects nothing.
///
/// **Every direction here draws, whatever the plot's marks are.** It said "Add
/// `z(<column>)`" as if the plot's own mark took one, and `line`, `step`, `area`,
/// `ribbon` and a cluster tree refuse it, while `rule`, `zone` and `text` do not
/// draw it yet, so a reader following it could hit a second wall; and its one
/// example, `bar * bin`, is refused over two categories. So the message offers the
/// flat plot first, since that is what was drawn, then `z` on a mark that takes
/// one, which a floor summary on a `bar` needs, and the transform that stands a
/// count up on each kind of floor.
///
/// **Asked only of a plot that draws** (`check` calls it after every gate, and only
/// when nothing was refused), since "so it is drawn flat" is otherwise false.
pub(super) fn note_space_drawn_flat(out: &mut Vec<Diagnostic>, spec: &PlotSpec) {
    if matches!(spec.coord, CoordSpace::Space(_)) && space_of(spec) != SpaceKind::Space
        && !spec.layers.iter().any(placed_by_the_graph)
    {
        out.push(Diagnostic {
            kind: DiagnosticKind::Assumption,
            message: "gog: `space(...)` sets a 3-D viewing angle, but nothing in this plot has \
                      a third dimension, so it is drawn flat. To draw it flat on purpose, drop \
                      `space()`. To stand it in the cube, add `z(<column>)` to a mark that \
                      takes one, such as `point` or `bar`, or use a transform that stands a \
                      count up on `z`: `bar * bin` over two numbers, `bar * count` over two \
                      categories."
                .to_string(),
        });
    }
}

pub(super) fn check_space(out: &mut Vec<Diagnostic>, spec: &PlotSpec) {
    // Whether this plot has a third dimension at all — asked of `space_of`, the one
    // source the renderer's own `is_3d` reads, so the report and the picture cannot
    // disagree. They *did* for one build: this check tested `axis_def(Z)` directly,
    // which the 3-D histogram fails (it binds no `z`; `bin` synthesizes the count
    // onto it), so a correctly projected plot was told it had been drawn flat.
    let projects = space_of(spec) == SpaceKind::Space;

    // **`tilt` is an elevation, and elevation has ends.** `turn` is a compass
    // bearing, so 5000° genuinely *is* 320° and wraps silently under §12's omission
    // rule — the intent is unambiguous, so there is nothing to ask about. Elevation
    // is not periodic in any useful sense: past 90 the eye crosses over the top,
    // the scene hangs upside down, and the three axis names pile onto one corner.
    // There is no sensible value out there to wrap *to*, so this refuses.
    //
    // ±90 was already the range in two places and reachable from neither a
    // sentence nor a check: `interactive.js` clamps the drag to it, pinned by a
    // test that drags past the stop. So a reader dragging was protected and a
    // reader *writing* was not, and `space(tilt = -400)` drew 3081 bytes of
    // nonsense without a word. Law 8: well-formedness is enforced hard.
    if let CoordSpace::Space(view) = &spec.coord {
        let t = view.tilt;
        if !t.is_finite() || t.abs() > 90.0 {
            // A number that is not a number has no nearest view, so the suggestion
            // is the default rather than a clamp — `NaN.clamp(-90, 90)` is `NaN`,
            // and a refusal that recommends `space(tilt = NaN)` is no direction.
            let nearest = if t.is_finite() {
                t.clamp(-90.0, 90.0)
            } else {
                // Read off the IR's own default rather than repeated here, so the
                // suggestion cannot drift from the view a bare `space()` gives.
                crate::ir::SpaceView::default().tilt
            };
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: format!(
                    "gog: `space(tilt = {t})` is outside -90 to 90, which is where an \
                     elevation lives. At 90 the camera looks straight down at the floor \
                     and at -90 straight up from underneath; past either the scene turns \
                     over. Use `space(tilt = {nearest})` for the nearest view, or \
                     `space(turn = )` to swing around the plot instead — a bearing wraps \
                     and an elevation does not."
                ),
            });
        }
    }

    // **`smooth` has no two-dimensional form, so it is refused in the cube rather
    // than run one-dimensionally.** It is the one value statistic that *fits* rather
    // than reduces — a curve of one column against another — and a curve needs a
    // domain to run along. A floor has no left to right, which is precisely why
    // `line`, `step` and `area` cannot stand in the cube either (`rule_for` on
    // `Mark::Path`): what they read *along* an axis, the cube does not offer.
    //
    // Left to run, it grouped by `x` and wrote to `y`, so the plot came out with one
    // column per *row* piled at each slot and a height that was whichever row painted
    // last. Nothing was reported, because every part of it is individually legal; the
    // plot was simply not the plot asked for. That is the §12 sin outright, and it is
    // also a Law-2 break — the same transform on the same mark meaning one thing flat
    // and another in space is a silent letter.
    //
    // **The five that reduce a named column were refused here beside it, and are now
    // the two-dimensional group-by** (spec §5). They group by the positions the mark
    // does *not* measure with, which on a floor is the pair, and reduce the column
    // named on the one it does — `z`. `check_pair_summary` owns them: it asks that `z`
    // named a column, rather than asking the plot to give up.
    //
    // **`point` is asked too, though it cuts no floor.** A smoothed point is the
    // fitted curve drawn as dots, and in the cube it has the same want of a domain:
    // the fit came back with `x` and `y` and no `z`, so every dot lost its third
    // position and the plot was an empty cube with a 0 to 1 axis, in silence. Its
    // way to summarize a floor is the bar's, since a point was never given the
    // pairwise group-by (`cuts_both_positions`), so the direction names `bar`.
    if projects {
        for layer in &spec.layers {
            let cuts = cuts_both_positions(&layer.mark, SpaceKind::Space);
            if !cuts && layer.mark != Mark::Point {
                continue;
            }
            let Some(sm) = layer.transforms.iter().find(|t| match t {
                Transform::Smooth | Transform::Straight => true,
                // A band on a point is refused wherever it stands, since a point
                // draws one value and a band is a span, so it is said there, once.
                Transform::SmoothBand | Transform::StraightBand => cuts,
                _ => false,
            }) else {
                continue;
            };
            let s = transform_name(sm);
            let shape = crate::transform::fit_shape(sm);
            let line = transform_name(&crate::transform::line_of(sm));
            // `smooth`'s sentence keeps its verb; a straight line is fitted, not smoothed.
            let verb = if line == "smooth" { "Smooth" } else { "Fit" };
            let reducer = if cuts { mark_name(&layer.mark) } else { "bar" };
            let m = mark_name(&layer.mark);
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: format!(
                    "gog: `{m} * {s}` fits a {shape} of one column against another, and a \
                     {shape} needs a domain to run along — the cube's floor is a *pair* of \
                     positions with no left to right, which is why `line` and `area` cannot \
                     stand here either. {verb} on the plane, where there is a domain: \
                     `line * {line} + x(<a>) + y(<b>)`. To summarize a column within each \
                     pair of categories instead, the six reductions (`sum`, `mean`, `median`, \
                     `max`, `min`, `quantile`) do read a floor: \
                     `{reducer} * mean + x(<a>) + y(<b>) + z(<column>) + space()`."
                ),
            });
        }
    }

    // **What the cube does not draw yet, said rather than dropped** (2026-10-14,
    // found walking transforms against spaces). Each of these was accepted and drew
    // the plot without it, or drew nothing: `bar * dodge` and `bar * stack` drew the
    // plain bars, solid inside one another; `point * jitter` drew the points
    // unmoved; `point * mean + x(continent) + y(era) + z(life)` drew every row, and
    // over two numbers an empty cube with a 0 to 1 axis; `point * bin + x(life) +
    // space()` put the count on two axes; and `path * density + z(year)` drew no
    // line at all. A point is not given the floor's two-dimensional reading here,
    // because it has two (a dot for each cell, or a pile of dots), and choosing is a
    // design not yet made; the bar already reads the floor, so the direction names it.
    if projects {
        for layer in &spec.layers {
            let m = mark_name(&layer.mark);
            let z = spec.position_for(layer, &Channel::Z).map(|d| d.field.as_str());
            if let Some(o) = layer.transforms.iter()
                .find(|t| matches!(t, Transform::Dodge | Transform::Stack | Transform::Jitter))
            {
                // A point's swarm is said by `check_swarm`, and a mark that does not
                // take the offset is refused by its own check.
                if !(layer.mark == Mark::Point && *o == Transform::Dodge)
                    && mark_takes_transform(&layer.mark, o) != TransformLegality::None
                {
                    let o = transform_name(o);
                    let summarize = if layer.mark == Mark::Bar {
                        " To draw one bar for each cell instead, summarize the rows: \
                         `bar * sum` or `bar * mean`."
                    } else {
                        ""
                    };
                    out.push(Diagnostic {
                        kind: DiagnosticKind::Unsupported,
                        message: format!(
                            "gog: `{m} * {o}` in `space()` is valid grammar, but this engine \
                             does not draw it: `{o}` arranges the marks that share a slot on \
                             the page, and in the cube a slot is a cell of the floor seen at \
                             an angle. To compare the groups, give each its own cube with \
                             `| facet(<column>)`, or its own frame with `play(<column>)`.\
                             {summarize}"
                        ),
                    });
                    continue;
                }
            }
            if matches!(layer.mark, Mark::Point | Mark::Text) {
                if let Some(t) = layer.transforms.iter().find(|t| matches!(t,
                    Transform::Bin | Transform::Count | Transform::Density | Transform::Proportion
                        | Transform::Sum | Transform::Mean | Transform::Median
                        | Transform::Max | Transform::Min | Transform::Quantile))
                {
                    let t = transform_name(t);
                    let reads = crate::transform::has_reduction(&layer.transforms);
                    let column = if reads { " + z(<column>)" } else { "" };
                    let flat = if z.is_some() { "`z()` and `space()`" } else { "`space()`" };
                    out.push(Diagnostic {
                        kind: DiagnosticKind::Unsupported,
                        message: format!(
                            "gog: `{m} * {t}` in `space()` is valid grammar, but this engine \
                             does not draw it: in the cube a statistic summarizes the rows in \
                             each cell of the floor and stands the answer up along `z`, and of \
                             the marks the cube draws, `bar`, `box` and `interval` read the \
                             floor that way. `bar * {t} + x(<a>) + y(<b>){column} + space()` \
                             draws it as a bar in each cell. To draw `{m} * {t}` flat, drop \
                             {flat}."
                        ),
                    });
                    continue;
                }
            }
            // **A hatch on a solid in the cube** (2026-10-14, found walking channels
            // against spaces): `pattern(era)` on a cube's bar or box drew the legend's
            // swatches and every solid plain. The surface refuses the hatch for the
            // reason that holds here: a tile is a texture in screen space, and each
            // projected face is foreshortened differently, so one tile would read as
            // a different density on every face.
            if matches!(layer.mark, Mark::Bar | Mark::Box)
                && (layer.encodings.contains_key(&Channel::Pattern) || layer.style.pattern.is_some())
            {
                out.push(Diagnostic {
                    kind: DiagnosticKind::Illegal,
                    message: format!(
                        "gog: `pattern` textures a fill, and a `{m}` in `space()` is a solid \
                         whose faces the projection foreshortens differently, so one hatch \
                         tile would read as a different density on every face: a texture \
                         that changes with the viewing angle, not with the data. To tell the \
                         groups apart in the cube, use `color(<column>)`."
                    ),
                });
                continue;
            }
            if layer.mark == Mark::Path && layer.transforms.contains(&Transform::Density) {
                if let Some(col) = z {
                    out.push(Diagnostic {
                        kind: DiagnosticKind::Illegal,
                        message: format!(
                            "gog: `path * density` draws contour lines of `x` against `y`, \
                             and `z({col})` names a column it never reads: a contour is a \
                             level of the estimate, not a height. Drop `z({col})` to draw \
                             the contours."
                        ),
                    });
                }
            }
        }
    }

    // A label for an axis that is not drawn. The mirror of the `space()`-with-no-`z`
    // case above, and reported for the same reason: `z_label` is an override for a
    // thing the plot does not have, so accepting it silently would be the §12 drop.
    // Guidance rather than a refusal — the plot is fine, the label is inert.
    if spec.z_axis.label.is_some() && !projects {
        out.push(Diagnostic {
            kind: DiagnosticKind::Assumption,
            message: "gog: `z_label()` names the third axis, but nothing is bound to `z`, \
                      so there is no third axis to label — the label is ignored. Add \
                      `z(<column>)`, or use `x_label()`/`y_label()` for the axes this \
                      plot has."
                .to_string(),
        });
    }

    let Some(zdef) = spec.axis_def(&Channel::Z) else { return };

    // `z` is a linear axis in 3-D for M8a. A log z would be scaled nowhere and
    // then plotted on a linear cube edge — the silent drop §12 forbids — so it
    // is refused with direction instead.
    //
    // **Only in the cube.** Every other space refuses a bound `z` in its own words
    // (a globe reads it as a spike's radius and refuses its `scale`), and this
    // message printed first there, about an axis the plot was never going to have.
    if zdef.scale == Some(ScaleType::Log) && projects {
        out.push(Diagnostic {
            kind: DiagnosticKind::Unsupported,
            message: "gog: a log `z`-axis is valid grammar, but this engine does not draw it: \
                      `z` is linear in 3-D. Drop `scale = \"log\"` on `z`, or put the log \
                      channel on `x`/`y`."
                .to_string(),
        });
    }

    // A facet is *not* checked here. A cube crossed with a facet is one cube per
    // panel, each projected from its own rectangle, and the renderer has always
    // drawn it that way — see this section's header for why the refusal that used
    // to sit here was a stale gate rather than a missing feature.
}

// ---------------------------------------------------------------------------
// Polar — the plane bent into a circle
//
// `polar()` is asked for outright rather than triggered by a binding, because
// unlike `z` it adds no dimension: it re-reads the two the plot already has (`x`
// as the angle, `y` as the radius — Wilkinson §9.1.6). So the trigger is the
// atom, and what has to be checked is which marks the engine can draw once the
// plane is bent, plus the one contradiction the space cannot hold.
// ---------------------------------------------------------------------------

pub(super) fn check_polar(out: &mut Vec<Diagnostic>, spec: &PlotSpec) {
    if !matches!(spec.coord, CoordSpace::Polar(_)) {
        return;
    }

    // A circle and a cube are two different spaces, and a plot sits in one. The
    // cylindrical coordinate that would hold both (Wilkinson §9.3.3) is not built,
    // so this is refused with direction rather than one of the two silently winning.
    if spec.axis_def(&Channel::Z).is_some() {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: "gog: a plot is drawn in one coordinate space, and `polar()` with `z(...)` \
                      asks for two — a circle and a cube. Drop `z(...)` to keep the polar plot, \
                      or drop `polar()` to keep the 3-D one."
                .to_string(),
        });
    }

    for layer in &spec.layers {
        let mark = &layer.mark;
        // **A hexagonal mesh has no polar reading**, and this is `bin(tiling = )`'s
        // third refusal on the same one sentence: a tiling partitions a *plane*.
        // The first two are a one-dimensional bin (no plane to cut) and a
        // categorical axis (slots are an order, not a distance); this is the third
        // way a plane can fail to be there — bent into a circle, the cell a mesh
        // would tile is a **sector**, and a hexagon bent is not a hexagon. Its six
        // sides are equal only against a metric the space no longer has, since a
        // step in angle at the rim is a longer step than the same one at the
        // center, which is the defect `hex` exists to fix arriving from the other
        // side. `rect` survives because a rectangle bent is a sector and the mark
        // draws one.
        if let Some(tiling) = layer.bin.as_ref().and_then(|b| b.tiling.as_deref()) {
            if tiling != "rect" && TILINGS.contains(&tiling) {
                out.push(Diagnostic {
                    kind: DiagnosticKind::Illegal,
                    message: format!(
                        "gog: `bin(tiling = \"{tiling}\")` partitions a *plane*, and `polar()` \
                         bends the plane into a circle — where a cell is a sector, and a \
                         hexagon's six equal sides are equal against a distance the space no \
                         longer has (a step of angle is longer at the rim than at the center). \
                         Drop the tiling for `rect`, whose cells are the sectors a bent \
                         rectangle already is, or drop `polar()` to cut a flat plane."
                    ),
                });
            }
        }
        // `surface` is the only mark that reaches this and does not bend, and its
        // refusal is not a polar one: a sheet through three positions has no
        // reading in a space with two, which is the same sentence `check_surface`
        // gives for the plane and which names both routes into the cube. Saying it
        // twice helps nobody — the rule this loop already applies to `is_drawable`.
        //
        // Kept as a loop over the grid rather than deleted, for the reason
        // `is_drawable` is kept now that every mark draws: it is the forcing
        // function for the *next* mark, which will otherwise reach this space with
        // no message at all.
        if !is_drawable(mark)
            || matches!(mark, Mark::Surface)
            || mark_draws_in_space(mark, SpaceKind::Polar)
            || placed_by_the_graph(layer)
        {
            continue;
        }
        let m = mark_name(mark);
        // Direction toward the marks that *do* bend — never a bare "not supported"
        // (§12) — read off `mark_draws_in_space` rather than typed out. The typed
        // list had already gone stale twice in one day, `path` and then `rule` both
        // learning to bend while it still said "bar, point, line, area or text",
        // which is the same hand-written-list drift as `check_pattern`'s `_` arm.
        let drawn: Vec<String> = ALL_MARKS.iter()
            .filter(|m| mark_draws_in_space(m, SpaceKind::Polar))
            .map(|m| format!("`{}`", mark_name(m)))
            .collect();
        out.push(Diagnostic {
            kind: DiagnosticKind::Unsupported,
            message: format!(
                "gog: `{m}` in `polar()` is valid grammar, but this engine does not draw it. \
                 Drop `polar()` to draw it flat, or use {}.",
                or_list(&drawn)
            ),
        });
    }
}

// ---------------------------------------------------------------------------
// Nest — the panel packed with regions
//
// `nest()` is asked for outright, like `polar()` and for the same reason: it
// adds no dimension, it re-reads the ones the plot has. What it re-reads them
// *as* is the difference. Polar keeps both positions and bends them; nest keeps
// only the **measure**, turns it into an area, and spends the domain axis on the
// outer partition — so a plot in this space has no coordinate at all, and every
// refusal below is one consequence of that single fact (spec §15).
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Map — the sphere on the page (spec §15)
//
// **Every refusal here was a silent drop the moment the space started drawing**,
// and that is the lesson worth keeping rather than the list. While no mark stood
// in `map`, `check_coord` refused the whole space and nothing else had to be
// asked. Giving it four marks turned one gate into five questions, all of which
// the engine answered by drawing something wrong and saying nothing: a `bar` came
// out placed at a longitude with nothing to measure along, a bound `z` was
// ignored, a category was silently unplottable, Mercator quietly relocated
// anything past its cut, and `theme(ratio = )` fought the projection for the
// panel's shape.
//
// A space owes its refusals on the day it draws, not afterwards.
// ---------------------------------------------------------------------------

/// `group` on a `zone` says *these rows are one region*, which only a boundary
/// can mean. Every other source of a zone's sides — a category's slot, `bounds`,
/// `bin`, `density` — describes **one row's** rectangle, so there is nothing for a
/// grouping to gather and accepting it would be accepting a word that changes
/// nothing (§12).
///
/// This is what lets the rule table say `Can` for the pair without Law 2 being
/// bent: the table states the pair is legal, and the sentence that cannot read a
/// boundary refuses in its own words rather than the table quietly meaning two
/// different things in two places.
pub(super) fn check_zone_group(out: &mut Vec<Diagnostic>, spec: &PlotSpec, data: &HashMap<String, DataFrame>) {
    // The two geographic spaces read a boundary the same way: `group` names the
    // region each vertex belongs to, on the flattened sphere and on the sphere
    // itself alike.
    let in_map = matches!(spec.coord, CoordSpace::Map(_) | CoordSpace::Globe(_));
    for layer in &spec.layers {
        if layer.mark != Mark::Zone {
            continue;
        }
        let group = layer.encodings.get(&Channel::Group).map(|d| d.field.as_str());
        match (in_map, group) {
            // A boundary with nothing to say which rows belong to which region.
            // Refused rather than guessed at: one region per table would draw the
            // whole world as a single shape, which looks like a plot and is not one.
            // In `globe` this arm never reports: `check_globe` owns that refusal,
            // with the tiling reason named, and two voices for one gap is noise.
            // **A zone with a transform is a field of cells, not a boundary** (2026-10-14,
            // found walking transforms against spaces): `zone * bin + x(lon) + y(lat) +
            // map()` was told it lacked a `group`, which answers a question it never
            // asked. A cell cut from degrees is no rectangle once projected, which is
            // the reason the globe gives for its own binned bars.
            (true, None) if matches!(spec.coord, CoordSpace::Map(_))
                && !layer.transforms.is_empty() =>
            {
                let t = transform_name(&layer.transforms[0]);
                out.push(Diagnostic {
                    kind: DiagnosticKind::Unsupported,
                    message: format!(
                        "gog: `zone * {t}` on a `map()` is valid grammar, but this engine does \
                         not draw it: a cell cut from degrees of longitude and latitude is not \
                         a rectangle once the map is projected. `path * density + x(<lon>) + \
                         y(<lat>) + map()` draws where the rows gather as contour lines on the \
                         map, or drop `map()` to draw the cells flat."
                    ),
                });
            }
            (true, None) if matches!(spec.coord, CoordSpace::Map(_)) => out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: "gog: a `zone` on a map is a region bounded by a coastline, and \
                          nothing here says which rows are one region — a boundary arrives as \
                          many rows per shape, unlike every other zone, whose sides are its \
                          own row's. Add `group(<column>)` naming the region each vertex \
                          belongs to."
                    .to_string(),
            }),
            (true, None) => {}
            // The transforms named are the ones the grid requires of a zone, read off
            // `mark_takes_transform` as the grid is: typed out, the list named four of
            // the seven and left out `count`, `proportion`, `partition` and `flow`.
            (false, Some(g)) => {
                let sides: Vec<&str> = USER_TRANSFORMS.iter()
                    .filter(|t| mark_takes_transform(&Mark::Zone, t) == TransformLegality::Required)
                    .map(transform_name)
                    .collect();
                out.push(Diagnostic {
                    kind: DiagnosticKind::Illegal,
                    message: format!(
                        "gog: `group({g})` gathers many rows into one region, and a `zone` here \
                         never takes its sides from a group of rows. They come from a \
                         category's slot, or from one of the transforms that give a zone its \
                         sides: {}. Drop `group({g})`, or add `map()` or `globe()` if `{g}` \
                         names regions on a boundary.",
                        code_list(&sides)
                    ),
                });
            }
            // **A ring that never closes.** A boundary closes each ring on the vertex
            // it started from, and that closure is what divides a region's rows into
            // rings — so an unclosed one silently merges two shapes into a shape that
            // is neither. Reported rather than drawn quietly, and reported rather than
            // dropped, since the region is still mostly right.
            (true, Some(g)) => {
                let Some(df) = layer.data.as_ref().or(spec.data.as_ref()).and_then(|n| data.get(n))
                else { continue };
                let (Some(xf), Some(yf)) = (spec.axis_def(&Channel::X), spec.axis_def(&Channel::Y))
                else { continue };
                let (Some(vx), Some(vy), Some(gs)) =
                    (df.float_col(xf.field.as_str()), df.float_col(yf.field.as_str()), df.str_col(g))
                else { continue };
                let n = vx.len().min(vy.len()).min(gs.len());
                let mut open = 0usize;
                let (mut start, mut first) = (0usize, None::<(f64, f64)>);
                for i in 0..=n {
                    let ends = i == n || (i > start && gs[i] != gs[start]);
                    if ends {
                        if first.is_some() {
                            open += 1;
                        }
                        start = i;
                        first = None;
                    }
                    if i == n {
                        break;
                    }
                    let here = (vx[i], vy[i]);
                    match first {
                        None => first = Some(here),
                        Some(f) if here == f => first = None,
                        Some(_) => {}
                    }
                }
                if open > 0 {
                    out.push(Diagnostic {
                        kind: DiagnosticKind::Assumption,
                        message: format!(
                            "gog: {open} region(s) end with a ring that never returns to where \
                             it started, so where one ring stops and the next begins had to be \
                             guessed. A boundary repeats each ring's first point as its last. \
                             Close the rings in `{g}`, or expect an island to be joined to the \
                             mainland."
                        ),
                    });
                }
            }
            (false, None) => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Globe — the sphere viewed, and the refusals it owes on the day it draws
//
// `map`'s fact decides this space too: longitude and latitude spend both
// positions on the place, so the column is `map`'s. What differs is that the
// column is being built out mark by mark, so a blank is one of two different
// sentences — a mark `map` draws is *owed* here (designed, not drawn yet),
// while a mark that measures along an axis is *ruled out* for `map`'s one
// reason. The split is derived from the rule table rather than typed, so it
// shrinks by itself as the owed marks land and cannot go stale in between.
// ---------------------------------------------------------------------------

pub(super) fn check_globe(out: &mut Vec<Diagnostic>, spec: &PlotSpec, data: &HashMap<String, DataFrame>) {
    let CoordSpace::Globe(view) = &spec.coord else { return };
    check_measure_on_a_place(out, spec, "globe");
    // **A spike has no fill and no slot** (2026-10-14, found walking channels and
    // settings against spaces): a globe's bar is drawn as a line standing out from
    // the sphere, so `pattern(continent)` drew the legend's swatches and plain
    // spikes, and `style(border_color =, border_size =, gap =)` changed nothing.
    for layer in &spec.layers {
        if layer.mark != Mark::Bar {
            continue;
        }
        let st = &layer.style;
        let mut asked: Vec<&str> = Vec::new();
        if layer.encodings.contains_key(&Channel::Pattern) { asked.push("pattern()"); }
        if st.pattern.is_some() { asked.push("style(pattern = )"); }
        if st.border_color.is_some() { asked.push("style(border_color = )"); }
        if st.border_size.is_some() { asked.push("style(border_size = )"); }
        if st.gap.is_some() { asked.push("style(gap = )"); }
        if asked.is_empty() {
            continue;
        }
        let list = asked.iter().map(|a| format!("`{a}`")).collect::<Vec<_>>().join(", ");
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: a `bar` on a globe is a spike, a line standing out from the sphere, \
                 so it has no fill to texture or border and no slot to leave a gap in, and \
                 {list} would change nothing. Drop it. To tell the groups apart, use \
                 `color(<column>)`."
            ),
        });
    }

    // **`z` here is the radius, and `bar` is what reads it** — a spike standing
    // at its place, measuring outward from the surface: the cube's own `z`
    // reading transplanted onto the one direction the sphere has that its
    // flattening does not. (Wilkinson's §9.3.1, one sentence past the constant
    // radius the earlier ruling cited: spherical coordinates with a varying
    // radius are for "bundles of vectors at a common origin".) Bound with no
    // bar to read it, `z` would be accepted and ignored — the drop §12
    // forbids — so that is refused with both readers named.
    let has_bar = spec.layers.iter().any(|l| l.mark == Mark::Bar);
    if spec.axis_def(&Channel::Z).is_some() && !has_bar {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: "gog: `z(...)` on a globe is the radius, and only a `bar` reads it — a \
                      spike standing at its place, measuring outward from the surface. Add a \
                      `bar` layer to raise spikes, drop `z(...)` to keep the marks drawn on \
                      the sphere, or drop `globe()` for the cube, where every position mark \
                      reads `z`."
                .to_string(),
        });
    }

    // A `bar` here is the spike, and a spike needs its measure named. With a
    // transform instead of a column it is the binned field on a sphere, which
    // waits on the hexagonal tiling; with nothing at all it has nothing to
    // stand up.
    for layer in &spec.layers {
        if layer.mark != Mark::Bar {
            continue;
        }
        if !layer.transforms.is_empty() {
            out.push(Diagnostic {
                kind: DiagnosticKind::Unsupported,
                message: "gog: a `bar` with a transform on the globe is valid grammar, but \
                          this engine does not draw it: it is the binned field on a sphere, \
                          whose correct tiling is hexagonal, since rectangular bins do not \
                          cover equal area there. Compute the \
                          measure in the host and bind it: `bar + x(<lon>) + y(<lat>) + \
                          z(<column>)` raises a spike per row."
                    .to_string(),
            });
        } else if spec.axis_def(&Channel::Z).is_none() {
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: "gog: a `bar` on the globe stands at its place and measures along \
                          the radius — the one axis the sphere has to spare — and nothing \
                          names its measure. Bind `z(<column>)` to raise a spike per row, or \
                          carry the quantity on `point + size(<column>)`."
                    .to_string(),
            });
        }
    }

    // A spike measures straight from the surface to its value against the
    // fitted top, so there is no axis on `z` to shape either. Shaping is the
    // host's line of code away, and the direction says so.
    if let Some(def) = spec.axis_def(&Channel::Z) {
        let param = if def.limits.is_some() {
            Some("limits")
        } else if def.scale.is_some() {
            Some("scale")
        } else if def.tick_count.is_some() {
            Some("tick_count")
        } else if def.free {
            Some("free")
        } else {
            None
        };
        if let Some(param) = param {
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: format!(
                    "gog: `z(<column>, {param} = )` shapes an axis, and a `globe()` plot \
                     draws none — a spike measures straight from the surface to its value \
                     against the fitted top. Compute the shaping in the host and bind the \
                     result: a square root or a logarithm of the column is one line there."
                ),
            });
        }
    }

    // **`tilt` is a latitude, and latitude has ends** — `check_space`'s
    // elevation ruling, one space over, with the same asymmetry: `turn` is a
    // longitude and wraps at any value, so only `tilt` refuses.
    let t = view.tilt;
    if !t.is_finite() || t.abs() > 90.0 {
        let nearest = if t.is_finite() {
            t.clamp(-90.0, 90.0)
        } else {
            crate::ir::GlobeView::default().tilt
        };
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `globe(tilt = {t})` is outside -90 to 90, which is where a latitude \
                 lives. At 90 the view faces straight down on the north pole and at -90 on \
                 the south; there is no place past either. Use `globe(tilt = {nearest})` for \
                 the nearest view, or `globe(turn = )` to swing around the earth instead — a \
                 longitude wraps and a latitude does not."
            ),
        });
    }

    // The marks, split by the rule table: owed (map draws it, this space not
    // yet) against ruled out (it measures along an axis neither space has).
    for layer in &spec.layers {
        let mark = &layer.mark;
        if !is_drawable(mark) || mark_draws_in_space(mark, SpaceKind::Globe)
            || placed_by_the_graph(layer)
        {
            continue;
        }
        let m = mark_name(mark);
        let drawn: Vec<String> = ALL_MARKS
            .iter()
            .filter(|m| mark_draws_in_space(m, SpaceKind::Globe))
            .map(|m| format!("`{}`", mark_name(m)))
            .collect();
        // The first branch is valid grammar the engine does not draw; the second is
        // the space's own rule, the map's, and Illegal (2026-09-29).
        let (kind, message) = if mark_draws_in_space(mark, SpaceKind::Map) {
            // Designed and owed: the spec's staged build order, said honestly
            // rather than worn as a ruling.
            (DiagnosticKind::Unsupported, format!(
                "gog: `{m}` on `globe()` is valid grammar, but this engine does not draw it. \
                 Use {}, drop `globe()` to draw `{m}` flat, or use `map()`, which draws it on \
                 the flattened sphere.",
                or_list(&drawn)
            ))
        } else {
            (DiagnosticKind::Illegal, format!(
                "gog: `{m}` measures along an axis, and a `globe()` plot has none to spare — \
                 longitude and latitude use both. Drop `globe()` to draw `{m}` flat, or use \
                 {}. To carry a quantity on the globe, put it on a channel instead of an \
                 axis: `point + size(<column>)` sizes each place by it, and \
                 `color(<column>)` shades it.",
                or_list(&drawn)
            ))
        };
        out.push(Diagnostic { kind, message });
    }

    // A place is two numbers — the column-type question is legality's, the
    // ruling the binned-category bug settled, and `check_map`'s own check one
    // space over.
    for (channel, axis) in [(Channel::X, "x"), (Channel::Y, "y")] {
        let Some(def) = spec.axis_def(&channel) else { continue };
        let field = def.field.as_str();
        let categorical = spec
            .layers
            .iter()
            .filter_map(|l| l.data.as_ref().or(spec.data.as_ref()).and_then(|n| data.get(n)))
            .any(|df| df.float_col(field).is_none() && df.str_col(field).is_some());
        if categorical {
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: format!(
                    "gog: `{axis}({field})` is a category, and a `globe()` position is a \
                     place — `x` is longitude and `y` is latitude, both in degrees. A \
                     category has neither. Give the globe the coordinates and put `{field}` \
                     on a channel that decodes it: `color({field})`."
                ),
            });
        }
    }

    // **A globe draws no axes, so nothing may shape or name one.** Accepting an
    // axis parameter here and ignoring it would be the silent drop §12 forbids —
    // degrees are already placed on the sphere, and what is *seen* is chosen by
    // the view, not by a domain.
    for (channel, axis) in [(Channel::X, "x"), (Channel::Y, "y")] {
        let Some(def) = spec.axis_def(&channel) else { continue };
        let param = if def.limits.is_some() {
            Some("limits")
        } else if def.scale.is_some() {
            Some("scale")
        } else if def.tick_count.is_some() {
            Some("tick_count")
        } else if def.free {
            Some("free")
        } else {
            None
        };
        if let Some(param) = param {
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: format!(
                    "gog: `{axis}(<column>, {param} = )` shapes an axis, and a `globe()` plot \
                     draws none — a position here is a place on the sphere, already in \
                     degrees. Drop `{param}`; the view chooses what is seen: `globe(turn =, \
                     tilt = )` faces a place, and looking closer is a zoom, which is a view."
                ),
            });
        }
    }
    if spec.x_axis.label.is_some() || spec.y_axis.label.is_some()
        || spec.z_axis.label.is_some()
    {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: "gog: an axis label names an axis, and a `globe()` plot draws none — a \
                      sphere has no straight side to write one along, and the graticule is the \
                      reference instead. Drop it; `title()` still names the plot."
                .to_string(),
        });
    }

    // **`zone` on the sphere takes its sides from a boundary, and only from
    // one.** The mesh and `bounds` forms are held back: the binned distribution
    // on a sphere is this space's recommended eventual build, and its correct
    // tiling is hexagonal (rectangular bins do not cover equal area on a
    // sphere) — an equal-area spherical grid the engine does not have yet.
    // Refused rather than drawn as flat rectangles pretending to be patches.
    for layer in &spec.layers {
        if layer.mark == Mark::Zone
            && (layer.encodings.get(&Channel::Group).is_none()
                || !layer.transforms.is_empty()
                || layer.bounds.is_some())
        {
            out.push(Diagnostic {
                kind: DiagnosticKind::Unsupported,
                message: "gog: `zone` on the globe takes its sides from a boundary — \
                          `zone + x(<lon>) + y(<lat>) + group(<region>)` fills each region \
                          on the sphere. A binned or aggregated field here is valid grammar, \
                          but this engine does not draw it: its correct tiling is \
                          hexagonal, because rectangular bins do not cover equal area on a \
                          sphere. Give the zone a boundary, or drop `globe()`."
                    .to_string(),
            });
        }
    }

    // The rug stands its ticks on an axis edge, and a globe has no axes — a
    // rule's reading here is the whole meridian or parallel. Refused rather
    // than drawn whole in silence, since the sentence asked for something
    // shorter than what it would get.
    for layer in &spec.layers {
        if layer.mark == Mark::Rule && layer.style.reach.as_deref() == Some("edge") {
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: "gog: `style(reach = \"edge\")` stands a rule's ticks on an axis \
                          edge, and a `globe()` plot has no axes — a rule here is the whole \
                          meridian or parallel it holds. Drop `reach` to draw it whole, or \
                          drop `globe()` for the flat rug."
                    .to_string(),
            });
        }
    }

    // A latitude beyond ±90 is not a place on the sphere, so those rows cannot
    // be drawn — said with a count rather than skipped in silence.
    if let Some(def) = spec.axis_def(&Channel::Y) {
        let field = def.field.as_str();
        let beyond = spec
            .layers
            .iter()
            .filter_map(|l| l.data.as_ref().or(spec.data.as_ref()).and_then(|n| data.get(n)))
            .flat_map(|df| df.float_col(field).into_iter().flatten())
            .filter(|v| v.is_finite() && v.abs() > 90.0)
            .count();
        if beyond > 0 {
            out.push(Diagnostic {
                kind: DiagnosticKind::Assumption,
                message: format!(
                    "gog: {beyond} row(s) have latitudes beyond ±90, which is not a place on \
                     the sphere, so they are left undrawn. Latitude runs -90 to 90; check the \
                     column, or swap `x` and `y` if longitude is bound to `y`."
                ),
            });
        }
    }
}

/// **A count is not a latitude** (2026-10-14, found walking transforms against
/// spaces). On a map or a globe both positions are the place, so a point or a label
/// whose transform writes its own measurement put it on latitude: `point * count +
/// x(lon) + map()` drew each longitude's count of rows as degrees north, on a panel
/// a few degrees tall, and `globe()` the same near the equator. `cumulative` is
/// refused here for the same reason in its own words. A reduction keeps the column's
/// own degrees, and `path * density` writes a contour level rather than a position,
/// so both still draw.
fn check_measure_on_a_place(out: &mut Vec<Diagnostic>, spec: &PlotSpec, space: &str) {
    for layer in &spec.layers {
        if !matches!(layer.mark, Mark::Point | Mark::Text)
            || !synthesizes_measure(&layer.mark, &layer.transforms)
        {
            continue;
        }
        let m = mark_name(&layer.mark);
        let chain = std::iter::once(m).chain(layer.transforms.iter().map(transform_name))
            .collect::<Vec<_>>().join(" * ");
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `{chain}` writes its own measurement on a position, and a `{space}()` \
                 plot spends both positions on the place: `x` is longitude and `y` is \
                 latitude, so the measurement would be drawn as degrees. Drop `{space}()` to \
                 draw `{chain}` flat. To show where rows gather on the {space}, `path * \
                 density + x(<lon>) + y(<lat>) + {space}()` draws it as contour lines."
            ),
        });
    }
}

pub(super) fn check_map(out: &mut Vec<Diagnostic>, spec: &PlotSpec, data: &HashMap<String, DataFrame>) {
    let CoordSpace::Map(view) = &spec.coord else { return };
    check_measure_on_a_place(out, spec, "map");

    // **A map and a cube are two spaces, and a plot is drawn in one** —
    // `check_polar`'s ruling and `check_nest`'s, restated because the reason is
    // the same and not a shared implementation. Longitude and latitude have
    // already spent both positions; a third is not a deeper map, it is a
    // different one.
    if spec.axis_def(&Channel::Z).is_some() {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: "gog: a plot is drawn in one coordinate space, and `map()` with `z(...)` \
                      asks for two — a map and a cube. Drop `z(...)` to keep the map, or drop \
                      `map()` to keep the 3-D plot. To carry a third number on a map, put it \
                      on a channel: `size(<column>)` or `color(<column>)`."
                .to_string(),
        });
    }

    // **Both positions are spent on the place**, so a mark that measures along an
    // axis has none left. The direction is the one cartography already uses: the
    // quantity moves to a refining channel, which is the proportional-symbol map.
    for layer in &spec.layers {
        let mark = &layer.mark;
        if !is_drawable(mark) || mark_draws_in_space(mark, SpaceKind::Map)
            || placed_by_the_graph(layer)
        {
            continue;
        }
        let m = mark_name(mark);
        let drawn: Vec<String> = ALL_MARKS
            .iter()
            .filter(|m| mark_draws_in_space(m, SpaceKind::Map))
            .map(|m| format!("`{}`", mark_name(m)))
            .collect();
        // `zone` used to be answered here separately, as *owed* rather than ruled
        // out. It draws now — the choropleth, its sides supplied by a boundary — so
        // the branch went with it and every mark left in this loop is refused for
        // the one reason.
        let why = format!(
            "`{m}` measures along an axis, and a `map()` plot has none to spare — \
             longitude and latitude use both"
        );
        // A rule of the space, Illegal since 2026-09-29: both positions are the
        // place, so a mark that measures along one has nothing to measure on.
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: {why}. Drop `map()` to draw `{m}` flat, or use {}. To carry a quantity \
                 on a map, put it on a channel instead of an axis: \
                 `point + size(<column>)` sizes each place by it, and `color(<column>)` \
                 shades it.",
                or_list(&drawn)
            ),
        });
    }

    // **The panel's shape belongs to the projection here.** Refused rather than
    // silently overridden: the renderer takes the panel's proportions from the
    // projected extent whatever this says, so accepting the setting would mean
    // accepting a number and ignoring it, which is the one thing §12 forbids.
    if spec.theme.ratio.is_some() {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: "gog: `theme(ratio = )` shapes a panel, and a `map()` panel is shaped by \
                      the projection — an equal-area map stretched to fit a box is no longer \
                      equal-area, though it still looks like a map. Drop it. To change how much \
                      room the plot takes, use `theme(width =, height =)`."
                .to_string(),
        });
    }

    // A place is two numbers. A category has no longitude, so this is a question
    // about the column's *type*, which makes it legality rather than something a
    // transform should discover — the ruling the binned-category bug settled.
    for (channel, axis) in [(Channel::X, "x"), (Channel::Y, "y")] {
        let Some(def) = spec.axis_def(&channel) else { continue };
        let field = def.field.as_str();
        let categorical = spec
            .layers
            .iter()
            .filter_map(|l| l.data.as_ref().or(spec.data.as_ref()).and_then(|n| data.get(n)))
            .any(|df| df.float_col(field).is_none() && df.str_col(field).is_some());
        if categorical {
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: format!(
                    "gog: `{axis}({field})` is a category, and a `map()` position is a place — \
                     `x` is longitude and `y` is latitude, both in degrees. A category has \
                     neither. Give the map the coordinates and put `{field}` on a channel that \
                     decodes it: `color({field})`."
                ),
            });
        }
    }

    // **Mercator has a latitude it cannot draw past, and a clamped row is a moved
    // row.** Reported as an Assumption rather than applied in silence: a plot that
    // quietly relocated a place would be the silent drop, and one that dropped the
    // row would misreport how many places there are.
    if let Some(limit) = crate::render::geo::Geo::new(view).limit() {
        let Some(def) = spec.axis_def(&Channel::Y) else { return };
        let field = def.field.as_str();
        let beyond = spec
            .layers
            .iter()
            .filter_map(|l| l.data.as_ref().or(spec.data.as_ref()).and_then(|n| data.get(n)))
            .flat_map(|df| df.float_col(field).into_iter().flatten())
            .filter(|v| v.is_finite() && v.abs() > limit)
            .count();
        if beyond > 0 {
            out.push(Diagnostic {
                kind: DiagnosticKind::Assumption,
                message: format!(
                    "gog: `map(preserve = \"angle\")` sends the poles infinitely far away, so it \
                     stops at {limit:.2}° — {beyond} row(s) beyond that were drawn at the edge \
                     rather than where they belong. Use `map(preserve = \"area\")`, which reaches \
                     both poles, or leave those rows out on purpose."
                ),
            });
        }
    }
}

pub(super) fn check_nest(out: &mut Vec<Diagnostic>, spec: &PlotSpec, data: &HashMap<String, DataFrame>) {
    if !matches!(spec.coord, CoordSpace::Nest) {
        return;
    }

    // **The layers of a packing play together** (2026-10-14, found walking channels
    // against spaces). A `text` names the regions the `bar` packs, and each layer
    // packs the rows it is handed, so a `play()` written after one mark packed that
    // layer from one frame's rows and the other from every row: in the Asia frame the
    // names of Asian countries were drawn inside Europe's and Africa's regions. A
    // `play()` the whole plot shares packs every layer from the same frame.
    if !spec.channels.contains_key(&Channel::Play) {
        let plays: Vec<&Layer> = spec.layers.iter()
            .filter(|l| l.encodings.contains_key(&Channel::Play)).collect();
        if let Some(first) = plays.first() {
            if plays.len() < spec.layers.len() {
                let col = &first.encodings[&Channel::Play].field;
                let m = mark_name(&first.mark);
                out.push(Diagnostic {
                    kind: DiagnosticKind::Illegal,
                    message: format!(
                        "gog: `play({col})` is written after the `{m}`, so in `nest()` that \
                         layer would be packed from one frame's rows while the others are \
                         packed from every row, and their regions would not line up. Write \
                         `play({col})` before the marks, so that every layer plays the same \
                         frames."
                    ),
                });
            }
        }
    }

    // **A packing already draws shares** (2026-10-14, found walking transforms
    // against spaces): each region's area is its share of the panel, so `proportion`
    // on a bar here divides by the total and draws the same regions. A word that
    // changes nothing is said, as `count * proportion` is; the plot is the one asked
    // for, so it is a note.
    for layer in &spec.layers {
        if layer.mark == Mark::Bar && layer.transforms.contains(&Transform::Proportion) {
            out.push(Diagnostic {
                kind: DiagnosticKind::Assumption,
                message: "gog: `proportion` on a `bar` in `nest()` draws the same regions as \
                          without it: a packing already sizes each region by its share of \
                          the whole. Drop it. To print each share, a `text * proportion` \
                          layer reads it."
                    .to_string(),
            });
        }
    }

    // A packing and a cube are two spaces, and a plot sits in one — `check_polar`'s
    // ruling, restated because the reason is the same one and not a shared
    // implementation. A nested space *could* one day pack boxes into a box; that is
    // a different space and would be asked for differently.
    if spec.axis_def(&Channel::Z).is_some() {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: "gog: a plot is drawn in one coordinate space, and `nest()` with `z(...)` \
                      asks for two — a packing and a cube. Drop `z(...)` to keep the packed \
                      plot, or drop `nest()` to keep the 3-D one."
                .to_string(),
        });
    }

    // **The axes are not there to be named.** A label names an axis, and this
    // space has none: the two directions of a packing carry no variable, and
    // Wilkinson is explicit that they can be reordered without changing anything
    // the plot means (§13.3.4.1). Refused rather than accepted-and-dropped, which
    // is the one thing no feature may do (§12) — and refused rather than *drawn*,
    // which would be worse: a label is read as a promise that the direction under
    // it measures something.
    for (label, atom) in [(&spec.x_axis.label, "x_label"), (&spec.y_axis.label, "y_label")] {
        if label.is_some() {
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: format!(
                    "gog: `{atom}()` names an axis, and a `nest()` plot has none — its two \
                     directions carry no variable and can be reordered without changing what \
                     the plot says. Use `title()` for the plot's own name, and the color \
                     legend to say what the regions are."
                ),
            });
        }
    }

    for layer in &spec.layers {
        // **A collision modifier has nothing to modify here**, and this is the
        // sharpest way the space differs from every other one. `dodge`, `stack` and
        // `jitter` answer "two marks landed in the same place" (spec §5) — a
        // question that presupposes *places*. A packing gives every piece its own
        // region by construction, so the overlap those three resolve cannot occur,
        // and accepting one would mean accepting a word that changes nothing.
        //
        // This is where the build diverged from the design, which had read the
        // treemap as the pie's sentence with the space swapped — `bar * count *
        // stack + color(g) + polar()` becoming the same with `nest()`. The pie needs
        // `stack` because polar *is* a map of the plane: two wedges at the same
        // angle genuinely overlap, and stacking is what lays them end to end. Nest
        // is not a map of the plane, so it has to resolve the collision itself, and
        // once it does, `stack` is a word with no work. Recorded in §15.
        if let Some(t) = [Transform::Stack, Transform::Dodge, Transform::Jitter]
            .into_iter()
            .find(|t| layer.transforms.contains(t))
        {
            let n = transform_name(&t);
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: format!(
                    "gog: `{n}` decides what happens when two marks land in the same place, and \
                     in a `nest()` plot none can — the packing gives every piece its own region, \
                     which is what makes the areas add up to the panel. Drop `{n}`; the shares \
                     are the same without it. For pieces laid end to end instead, `{n}` is at \
                     home flat and in `polar()`."
                ),
            });
        }

        // **And repel has nothing to move apart.** The fourth offset is refused
        // here for a reason of its own, which is why it is not a fifth name in the
        // list above. Its collision is *ink* rather than a place, and a packing
        // resolves that one too: a name is drawn inside the region it names, at the
        // size that fits, and regions do not overlap, so neither can two names. What
        // is left is a share too small to have a region at all, which no move can
        // help, and moving a name out of its own rectangle would break the only thing
        // saying which region it belongs to. That share is reported instead (§12).
        if layer.transforms.contains(&Transform::Repel) {
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: "gog: `repel` moves labels that overlap one another, and in a `nest()` \
                          plot none can — each name is drawn inside its own region, smaller when \
                          it does not fit at full size, and the regions do not overlap. A name \
                          moved outside its own rectangle would no longer say which region it \
                          belongs to. Drop `repel`. A name is left out only when its share is too \
                          small to have a region at all, and the plot reports how many. A larger \
                          plot (`theme(width =, height =)`) gives the small regions room for \
                          larger names."
                    .to_string(),
            });
        }

        // **A nudge has nothing to move away from here.** The setting exists so a
        // label can step off the point it would otherwise cover (spec §7), and a
        // packed label sits in a region with no dot under it — so honoring it would
        // push the name toward its own border for no reason, and ignoring it would
        // be the accept-and-drop §12 forbids. Refused, with what a reader might
        // have wanted instead.
        if layer.style.nudge.is_some() && matches!(layer.mark, Mark::Text) {
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: "gog: `style(nudge = )` moves a label off the point it would cover, \
                          and a `nest()` label covers no point — it sits at the center of its \
                          own region, which is the only place that says which region it names. \
                          Drop it. A name that does not fit its region is already drawn smaller; \
                          a larger plot (`theme(width =, height =)`) gives the small regions \
                          room for larger names."
                    .to_string(),
            });
        }

        let mark = &layer.mark;
        if !is_drawable(mark) || mark_draws_in_space(mark, SpaceKind::Nest)
            || placed_by_the_graph(layer)
        {
            continue;
        }
        let m = mark_name(mark);
        let drawn: Vec<String> = ALL_MARKS.iter()
            .filter(|m| mark_draws_in_space(m, SpaceKind::Nest))
            .map(|m| format!("`{}`", mark_name(m)))
            .collect();
        // One sentence now, where there were two. The second was `text`'s, held
        // apart because it was *owed* rather than refused; it drew on 2026-07-27 and
        // the branch went with it. What is left is one verdict with one reason: a
        // packing has no positions, so a mark placed by one cannot be drawn here —
        // a rule of the space, and Illegal since 2026-09-29.
        let (kind, why) = (
            DiagnosticKind::Illegal,
            format!("`{m}` is placed by a position, and a packing has none to give it — its \
                     two directions are not axes, and two neighboring regions are not near \
                     each other in the data (Wilkinson §13.3.4.1)"),
        );
        out.push(Diagnostic {
            kind,
            message: format!(
                "gog: {why}. Drop `nest()` to draw `{m}` flat, or use {}.",
                or_list(&drawn)
            ),
        });
    }

    // **Something has to be the area.** A bar in this space is its measure and
    // nothing else — there is no slot for it to stand in and no baseline to stand
    // on — so a sentence that names no measure and synthesizes none has nothing to
    // pack. Flat, the same sentence draws a row of bars of no height, which is
    // empty but not wrong; here it is the whole plot.
    let has_measure = spec.axis_def(&Channel::X).is_some()
        || spec.axis_def(&Channel::Y).is_some()
        || spec.layers.iter().any(|l| synthesizes_measure(&l.mark, &l.transforms));
    if !has_measure && !spec.layers.is_empty() && !spec.layers.iter().any(placed_by_the_graph) {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: "gog: a `nest()` plot packs each row's measure into an area, and this one \
                      has no measure — nothing says how big a region should be. Give it a \
                      value to size the regions by (`y(revenue)`), or count the rows \
                      (`bar * count + color(region)`)."
                .to_string(),
        });
    }

    // **A log scale would be accepted and do nothing**, which is the silent drop
    // §12 forbids, so it is refused here. A region's size is its *share of the
    // total*, and a share is arithmetic on the raw values: re-spacing them changes
    // what a distance means, and a packing has no distances. Flat, `scale = "log"`
    // moves a bar's tip; here there is no tip to move, so the plot would be
    // byte-identical with the scale and without it — `zone * density(levels = )`'s
    // defect exactly (§18), caught this time before it shipped.
    for ch in [Channel::X, Channel::Y] {
        let Some(enc) = spec.axis_def(&ch) else { continue };
        if matches!(enc.scale, Some(ScaleType::Log)) {
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: format!(
                    "gog: `{}(scale = \"log\")` with `nest()` — a packed region's size is its \
                     **share of the total**, which is arithmetic on the values themselves, so a \
                     log scale would change nothing about the picture. Drop the scale, or drop \
                     `nest()` to read the values against a log axis.",
                    channel_name(&ch)
                ),
            });
        }
    }

    // **One packing for every layer.** Each layer packs its own rows, so a summed
    // layer and a plain one beside it pack two different sets of regions over the
    // same rectangle: `bar * sum + color(continent) + text + label(continent)`
    // drew five continent regions and packed the text's 142 rows on its own, and
    // 26 of the 37 names that fit sat inside another continent's region, with only
    // a note that some names were too wide. The host language sums the table, and
    // both layers then pack one row per region, which is how a treemap is labeled.
    let packs = |l: &&Layer| is_drawable(&l.mark) && mark_draws_in_space(&l.mark, SpaceKind::Nest);
    let summed: Vec<&Layer> = spec.layers.iter().filter(packs)
        .filter(|l| crate::transform::has_reduction(&l.transforms)).collect();
    let plain: Vec<&Layer> = spec.layers.iter().filter(packs)
        .filter(|l| !crate::transform::has_reduction(&l.transforms)).collect();
    if let (Some(s), Some(p)) = (summed.first(), plain.first()) {
        let reduction = s.transforms.iter().find(|t| crate::transform::is_reduction(t))
            .map(transform_name).unwrap_or("sum");
        let (sm, pm) = (mark_name(&s.mark), mark_name(&p.mark));
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: in a `nest()` plot every layer packs its own rows, and here `{sm} * \
                 {reduction}` packs one region per group while `{pm}` packs every row of its \
                 table, so the two lay out different regions over the same rectangle and the \
                 `{pm}` pieces land in regions of the other packing. Summarize the table in \
                 the host language first and draw both layers from it without `{reduction}`, \
                 so both pack one row per region."
            ),
        });
    }

    // **Nor would the furniture of an axis.** Gridlines, turned tick labels, the
    // place an axis's name goes and the lines that bound a panel all belong to
    // axes, and a packing has none, so each of these was accepted and drew the
    // same bytes as the sentence without it: `x_label()`'s case, one level down.
    // Read off what was written rather than the resolved theme, so a preset that
    // fills one in (`theme("minimal")` sets `grid`) is not refused for it.
    for (name, stated, what) in [
        ("grid", spec.theme.grid.is_some(), "draws lines across the panel at an axis's ticks"),
        ("tick_angle", spec.theme.tick_angle.is_some(), "turns an axis's tick labels"),
        ("axis_label", spec.theme.axis_label.is_some(), "places an axis's name"),
        ("frame", spec.theme.frame.is_some(), "draws the lines that bound the panel"),
    ] {
        if stated {
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: format!(
                    "gog: `theme({name} = )` {what}, and a `nest()` plot has no axes — its two \
                     directions carry no variable and can be reordered without changing what \
                     the plot says. Drop it; the packing is drawn the same without it."
                ),
            });
        }
    }

    // A count of ticks asks for ticks, which the same absence rules out.
    let positions = |ch: Channel| -> Vec<&ChannelDef> {
        let plot = if ch == Channel::X { spec.x.as_ref() } else { spec.y.as_ref() };
        plot.into_iter().chain(spec.layers.iter().filter_map(move |l| l.encodings.get(&ch))).collect()
    };
    for ch in [Channel::X, Channel::Y] {
        let c = channel_name(&ch);
        for def in positions(ch.clone()) {
            if let Some(n) = def.tick_count {
                out.push(Diagnostic {
                    kind: DiagnosticKind::Illegal,
                    message: format!(
                        "gog: `{c}({}, tick_count = {n})` asks for ticks along an axis, and a \
                         `nest()` plot has no axes — its regions are read by their areas, not \
                         against a scale. Drop `tick_count`.",
                        def.field
                    ),
                });
            }
        }
    }

    // **A domain under a summary bounds nothing.** Without one, `limits` leaves out
    // the rows it excludes, and says so. Under `bar * sum` a region's size is its
    // group's total as a share of the whole, so no axis is fitted for a domain to
    // bound and no row is left out: the plot was byte for byte the one without it,
    // for every range tried.
    for layer in &spec.layers {
        let Some(reduced) = layer.transforms.iter().find(|t| crate::transform::is_reduction(t)) else {
            continue;
        };
        for ch in [Channel::X, Channel::Y] {
            let Some(def) = spec.position_for(layer, &ch) else { continue };
            if def.limits.is_none() {
                continue;
            }
            let (c, m, r) = (channel_name(&ch), mark_name(&layer.mark), transform_name(reduced));
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: format!(
                    "gog: `{c}({}, limits = …)` states the domain of an axis, and under `{m} * \
                     {r}` in a `nest()` there is none to bound — each region's size is its \
                     group's {r} as a share of the whole. Drop `limits`, or leave rows out of \
                     the table before `data()`.",
                    def.field
                ),
            });
        }
    }

    // **An area cannot be negative**, and unlike the rulings above this one is
    // about the data rather than the sentence, so it is asked here where the
    // frames are in hand. A length can run below a baseline and read as a loss; an
    // area has no direction to run in, so a negative measure has no packing — and
    // silently dropping those rows would draw a plot whose regions no longer sum
    // to the whole, which is the one thing the reader of a treemap is entitled to.
    for layer in &spec.layers {
        let Some(df) = layer.data.as_ref().or(spec.data.as_ref()).and_then(|n| data.get(n))
        else { continue };
        for ch in [Channel::X, Channel::Y] {
            let Some(enc) = spec.axis_def(&ch) else { continue };
            let Some(col) = df.float_col(&enc.field) else { continue };
            if col.iter().any(|v| v.is_finite() && *v < 0.0) {
                out.push(Diagnostic {
                    kind: DiagnosticKind::Illegal,
                    message: format!(
                        "gog: `{}` has negative values, and a `nest()` plot turns a measure into \
                         an **area**, which cannot be negative. Flat, a bar below the baseline \
                         reads as a loss; a region has no direction to run in. Filter or offset \
                         the column, or draw it flat where the sign is readable.",
                        enc.field
                    ),
                });
                break;
            }
        }
    }
}

/// **A cell of the cube's floor holds one solid** (2026-10-14, found walking
/// transforms against spaces). A bar, box or interval stands in a cell as a solid,
/// and two solids in one cell are drawn inside one another, so only the largest
/// shows. Flat, the same overlap draws see-through (the overlay rule), and `dodge`
/// sets the groups side by side; in the cube neither is drawn, so
/// `bar + x(food) + y(nutrient) + z(amount) + color(method)` showed one method per
/// cell and hid two, in silence. Refused, counting the cells, toward the ways that
/// draw: a statistic for one bar per cell, a facet or a frame for each group.
///
/// What shares a cell is a row for a plain bar, and a group (a combination of the
/// splitting columns) for a mark that summarizes its rows first. A facet panel and a
/// `play` frame are part of the cell, as in `check_zone_cells`. On a floor that `bin`
/// or `density` cuts, a cell is known only by its category positions, so a split is
/// asked of those: `bar * bin + x(life) + y(continent) + color(continent)` holds one
/// continent in each cell and draws, where `color(continent)` over two numbers puts
/// every continent in every bin.
pub(super) fn check_cube_cells(out: &mut Vec<Diagnostic>, spec: &PlotSpec, df: &DataFrame, layer: &Layer) {
    if space_of(spec) != SpaceKind::Space
        || !matches!(layer.mark, Mark::Bar | Mark::Box | Mark::Interval)
        || layer.transforms.iter().any(|t| matches!(t,
            Transform::Dodge | Transform::Stack | Transform::Jitter))
    {
        return;
    }
    let field = |ch: Channel| spec.position_for(layer, &ch).map(|d| d.field.as_str());
    let (Some(xf), Some(yf)) = (field(Channel::X), field(Channel::Y)) else { return };
    let splits: Vec<&str> = split_fields_of(layer).into_iter()
        .filter(|f| df.str_col(f).is_some())
        .collect();
    let summarizes = layer.mark != Mark::Bar || !layer.transforms.is_empty();
    let cuts = layer.transforms.iter().any(|t| matches!(t, Transform::Bin | Transform::Density));
    if cuts {
        if splits.is_empty() { return }
    }
    let text = |f: &str, r: usize| -> String {
        df.str_col(f).and_then(|c| c.get(r).cloned())
            .or_else(|| df.float_col(f).and_then(|c| c.get(r)).map(|v| v.to_string()))
            .unwrap_or_default()
    };
    let mut keys: Vec<&str> = if cuts {
        [xf, yf].into_iter().filter(|f| df.str_col(f).is_some()).collect()
    } else {
        vec![xf, yf]
    };
    if let Some(f) = &spec.facet {
        keys.extend(f.col.as_deref());
        keys.extend(f.row.as_deref());
    }
    if let Some(p) = layer.encodings.get(&Channel::Play) {
        keys.push(p.field.as_str());
    }
    let mut cells: HashMap<Vec<String>, std::collections::HashSet<Vec<String>>> = HashMap::new();
    for r in 0..df.len() {
        let key: Vec<String> = keys.iter().map(|k| text(k, r)).collect();
        let unit: Vec<String> = if summarizes {
            splits.iter().map(|s| text(s, r)).collect()
        } else {
            vec![r.to_string()]
        };
        cells.entry(key).or_default().insert(unit);
    }
    let crowded = cells.values().filter(|u| u.len() > 1).count();
    if crowded == 0 {
        return;
    }
    let m = mark_name(&layer.mark);
    let unit = if summarizes { "group" } else { "row" };
    let mut ways: Vec<String> = Vec::new();
    if let Some(s) = splits.first() {
        ways.push(format!("To show each `{s}`, give it its own cube with `| facet({s})`, \
                           or its own frame with `play({s})`."));
    }
    if !summarizes {
        ways.push(match splits.first() {
            Some(s) => format!("To draw one bar for each cell, leave out the split by `{s}` \
                                and summarize the rows: `bar * sum` or `bar * mean`."),
            None => "To draw one bar for each cell, summarize the rows: `bar * sum` or \
                     `bar * mean`.".to_string(),
        });
    }
    let place = if cuts && crowded == cells.len() && keys.is_empty() {
        "the floor's cells".to_string()
    } else {
        format!("{} of its {} cells", crowded, cells.len())
    };
    out.push(Diagnostic {
        kind: DiagnosticKind::Illegal,
        message: format!(
            "gog: a `{m}` in `space()` stands one solid in each cell of the floor, and \
             {place} hold more than one {unit}, so the solids would be drawn inside one \
             another and only the largest in each cell would show. {}",
            ways.join(" ")
        ),
    });
}
