//! The brush: the reader's bound on a column, and which layers can answer it.

use super::*;

// ---------------------------------------------------------------------------
// Brush — the reader's bound on a column, and which layers can answer it
//
// A selection is a **predicate over rows**, so the question "may this mark be
// brushed?" is really "is one row one *element* here, or is a row a vertex of
// something larger?" A polyline's rows are vertices: brushing half of one would
// have to split it, and splitting a polyline is what `group` already means.
//
// That question is answered in this file already, under another name: a mark
// that says `Cannot` to `group` — `text`, `rule` — has no rows to join, and the
// comments beside those cells give the reason in words: *"a per-row glyph, like
// a point, connects nothing"*. So brushability **derives** rather than earning a
// second table, and a mark added later gets a verdict without anyone remembering
// to come back here (Law 1's completeness enforcement, for free). Three marks
// take `group` for another reason and are pinned (`mark_takes_selection`).
//
// The `accepts`/`renders` split then does the rest of the work, exactly as it
// does for a channel: the grammar says a layer may be brushed, and the engine
// says whether it can draw it (`selection_draws`). A `bar` was the first case:
// its thickness is derived from the smallest gap in the frame it is handed, so
// two passes, selected and unselected, disagreed about how wide a bar is. That
// is a lie rather than a wobble, since a bar's width is what says whether the
// bins are adjacent. Since 2026-10-09 the bar is measured on the whole share and
// each pass draws its own rows (`SvgRenderer::pass_keep`). Spec §15.
// ---------------------------------------------------------------------------

/// May a column be brushed on this mark? One row must be one element.
///
/// Derived from `group`'s rule rather than listed, because they are the same
/// question asked from two directions: `group` splits an element that spans many
/// rows, so a mark that refuses `group` is exactly a mark whose rows are already
/// separate elements.
///
/// **`zone` is the one mark where the two questions come apart** (2026-08-01), and
/// the derivation would have answered the wrong one. Every source of a zone's
/// sides used to describe a single row's rectangle — a category's slot, `bounds`,
/// `bin`, `density` — so refusing `group` was a faithful proxy. A **boundary** is
/// the fifth, it arrives as many rows per shape, and it needs `group` to say which
/// rows those are. Letting the derivation follow that would have quietly made the
/// heatmap cell, the tile and the waterfall step unbrushable, none of which
/// changed: they are still one row each.
///
/// **`bar` and `point` came apart the same way** (2026-10-09). Each took `group`
/// once a statistic could split it (`bar * mean * dodge + group(era)`), and on
/// them `group` splits what the statistic gathers, never one shape across rows: a
/// bare `bar + group()` is refused for changing nothing. Following the derivation
/// would have made every bar and every scatter unbrushable while each row is
/// still its own rectangle or its own point. **`text` followed** (2026-10-10), for
/// the same reason: `group` splits what a summary gathers or what `dodge` sets
/// apart, so the labels of dodged bars stand over their own bars, and each row is
/// still one label.
///
/// So the answer is pinned here rather than derived, and the open case is named
/// rather than left implicit: **brushing a choropleth** would select part of a
/// region's outline, which is not something a reader can mean. It is unreachable
/// by accident, a boundary being read only in `map()`, and it is not yet refused.
pub fn mark_takes_selection(mark: &Mark) -> bool {
    if matches!(mark, Mark::Zone | Mark::Bar | Mark::Point | Mark::Text) {
        return true;
    }
    // An `edge` is one row, one element, and still not brushable — the proxy's
    // second exception, from the other side. A brush names a range of *data*,
    // and an edge's positions are the layout's: coordinates that can be
    // reflected without changing what the graph says, so a rectangle over them
    // is not a thing a reader can mean. The choropleth's vertex problem, as
    // the whole mark.
    if *mark == Mark::Edge {
        return false;
    }
    rule_for(mark, &Channel::Group).obligation == Obligation::Cannot
}

/// Does this transform leave one drawn row standing for one source row?
///
/// A predicate over source rows can only be honest where it does. After a `bin`
/// or a `count` the drawn rectangle stands for forty rows at once, and "twelve of
/// them are selected" has no honest picture — dimming the whole bar would be a
/// lie and dimming part of it would be a second, invented mark. This is the datum
/// provenance debt (§14) made visible instead of silently approximated.
///
/// The collision modifiers do not collapse: `dodge`, `stack`, `jitter` and
/// `repel` move rows without merging them. Nor does `bounds`, which names each
/// row's own two sides, so one row still draws one shape: a zone per recession, a
/// whisker per estimate. It was counted as a summary, and a brush on such a layer
/// was refused for a reason that was false (ruled 2026-09-28: it answers).
pub(super) fn transform_collapses_rows(t: &Transform) -> bool {
    !is_collision_modifier(t) && *t != Transform::Bounds
}

/// Can this layer's rows answer a selection? [`mark_takes_selection`] asks it of
/// the mark, and the layer decides one case the mark cannot: an `interval` under
/// `bounds` draws one whisker per row, the two sides that row names, where the
/// mark in general joins many rows (a summary's spread) or splits by a column.
pub fn layer_takes_selection(layer: &Layer) -> bool {
    mark_takes_selection(&layer.mark)
        || (layer.mark == Mark::Interval && layer.transforms.contains(&Transform::Bounds))
        // A flow's band is one path, and every row on a path holds the same
        // place at every stage. So a bound on a stage keeps or leaves a whole
        // band, which a ribbon's band between two curves never could.
        || (layer.mark == Mark::Ribbon && layer.transforms.contains(&Transform::Flow))
}

/// Can the engine *draw* this layer brushed today? The `renders` half.
///
/// The exclusions are all about geometry derived from a layer's neighbors rather
/// than from the row itself. A two-pass draw hands each pass a different frame,
/// so anything measured off the frame changes between the two. A `bar` was the
/// first and is not one now: its writer measures the whole share and draws one
/// side of it (2026-10-09). The three below move a row by where the others sit,
/// which a measurement of the share cannot fix: the rows themselves would move.
pub(super) fn selection_draws(layer: &Layer) -> Option<&'static str> {
    if layer.transforms.contains(&Transform::Jitter) {
        return Some(
            "a jittered point's offset is seeded from its place in the table, so \
             every point would jump when the selection changed",
        );
    }
    if layer.transforms.contains(&Transform::Repel) {
        return Some(
            "a repelled label is placed by what the other labels in the frame are \
             doing, so every label would move when the selection changed",
        );
    }
    // The swarm is repel's case for points: where one lands depends on every point
    // placed before it in its slot, so a pass holding half of them is a different
    // swarm.
    if layer.mark == Mark::Point && layer.transforms.contains(&Transform::Dodge) {
        return Some(
            "a swarmed point is placed by where the other points in its slot sit, so \
             every point would move when the selection changed",
        );
    }
    None
}

/// Can this layer's rows answer a selection, *and* can the engine draw them
/// answering it? The two halves this file reports separately — one is the
/// grammar's `accepts`, the other the engine's `renders` — asked as the single
/// question the renderer needs. Kept here rather than in the renderer so the
/// picture cannot disagree with the diagnostic the reader was given.
pub fn layer_answers_selection(layer: &Layer) -> bool {
    // **A flow answers by its paths, not by its rows** (2026-10-03, at the
    // author's word). It sums rows into paths, which is what `transform_collapses_rows`
    // refuses everywhere else, but the sum never mixes two paths: a band is the
    // rows that share a place at every stage. A bound on a stage, or on a column
    // that splits the bands, is therefore all or nothing for each band, and
    // `check_brush` refuses every other column on a flow. The slots answer by
    // their own stage (`flow_slot_keeps`), and the positions come from the whole
    // table before either pass is drawn, so both passes put a band in one place.
    if layer.transforms.contains(&Transform::Flow) {
        return true;
    }
    !layer.transforms.iter().any(transform_collapses_rows) && selection_draws(layer).is_none()
}

/// Does this layer draw each row at the value the row holds?
///
/// The page's readout never asks the picture what lies under the pointer. It
/// re-derives every row's position from the row's own value and the two numbers
/// the panel states, then keeps the nearest. That is exact where a mark stands
/// at its value, and wrong everywhere else, so this is **not** the question
/// [`layer_answers_selection`] asks and the two must not be shared.
///
/// They part on both counts, which is the whole reason for a second predicate.
/// A selection keeps `dodge`, `stack` and `jitter`, because those move a row
/// without merging it and a predicate over rows stays honest; a position is
/// precisely what they move, so all three are out here. And a selection refuses
/// `bar` over how two passes measure a width, which says nothing about where a
/// bar stands.
pub fn layer_places_rows(layer: &Layer) -> bool {
    mark_takes_selection(&layer.mark) && layer.transforms.is_empty() && layer.bounds.is_none()
}

/// Why a reader's pointer cannot be answered on this plot, as one word.
///
/// `None` means it can. The question is asked of the **plot** rather than of a
/// layer, because one honest layer is enough: a jittered cloud under a `text`
/// layer still has the text standing at each row's value, and a pointer landing
/// there has found something real. So this reports a reason only when no layer
/// places its rows at all, and the first layer's reason speaks for the rest.
///
/// The engine ships the word and not the sentence. It is the only side that
/// knows a mark was moved away from its value; the page is the only side that
/// knows when a reader is asking.
pub fn why_not_placed(spec: &PlotSpec) -> Option<&'static str> {
    // A network's rows are placed — by the layout — but at coordinates a
    // reader cannot mean, so the readout's word comes first and is its own.
    if matches!(spec.coord, CoordSpace::Network(_)) {
        return Some("network");
    }
    // A flow's bands and slots are placed by the paths' running sums, not by
    // any row's value, and its own word tells the page two things: a pointer
    // names no row there, and a drag selects nothing, since a range over the
    // stages or the running count is not a selection a reader can mean. A
    // click on a slot is what moves a flow's brush.
    if spec.layers.iter().any(|l| l.transforms.contains(&Transform::Flow)) {
        return Some("flow");
    }
    if spec.layers.iter().any(layer_places_rows) {
        return None;
    }
    spec.layers.first().map(|l| {
        if l.transforms.contains(&Transform::Jitter) {
            "jitter"
        } else if l.transforms.contains(&Transform::Repel) {
            "repel"
        } else if l.transforms.contains(&Transform::Dodge) {
            "dodge"
        } else if l.transforms.contains(&Transform::Stack) {
            "stack"
        // Before `summary`: `bounds` is written as a transform, and a shape it
        // places is one row, not a summary of many, which is what the page
        // reads the word as.
        } else if l.bounds.is_some() {
            "bounds"
        } else if !l.transforms.is_empty() {
            "summary"
        } else {
            "mark"
        }
    })
}

/// Which rows a brush keeps — the predicate itself, shared by this check and the
/// renderer so the count a reader is given cannot disagree with the picture.
///
/// `None` means nothing is selected, which is the resting state and the reason an
/// unbrushed plot's bytes are untouched. A row whose value is missing is *outside*
/// every selection and stays in the frame: a brush places nothing, so it can no
/// more drop a row than it can move one.
pub fn brush_keeps(spec: &PlotSpec, df: &DataFrame) -> Option<Vec<bool>> {
    let active: Vec<&crate::ir::BrushDef> =
        spec.brush.iter().filter(|b| !b.is_resting()).collect();
    // The free shape a reader traced, which is the same predicate with a
    // different region: a bound tests one column's value, an outline tests two.
    let region = spec.region.as_ref().filter(|r| !r.is_resting());
    selection_keeps(spec, &active, region, df)
}

/// **One selection per table, across a page** (spec §15). Two plots on a page
/// that read one table and each bound a different column select the rows that
/// both bounds keep, the intersection, which is what the page's count of selected
/// rows has always counted. Drawn plot by plot, each dimmed by its own bound
/// alone, so the count read "2 of 5" over six points at full strength. `beside`
/// is every other brushed plot's selection on this plot's table, handed down by
/// the page (`Fit::selection_beside`); each is tested in *this* plot's space, so a
/// bound on a column this plot draws on a log axis is compared in logarithms.
pub fn page_keeps(
    spec: &PlotSpec,
    df: &DataFrame,
    own: Option<Vec<bool>>,
    beside: &[(Vec<crate::ir::BrushDef>, Option<crate::ir::RegionDef>)],
) -> Option<Vec<bool>> {
    beside.iter().fold(own, |keep, (bounds, region)| {
        let bounds: Vec<&crate::ir::BrushDef> = bounds.iter().collect();
        match (keep, selection_keeps(spec, &bounds, region.as_ref(), df)) {
            (Some(a), Some(b)) => Some(a.iter().zip(&b).map(|(x, y)| *x && *y).collect()),
            (a, b) => a.or(b),
        }
    })
}

/// [`brush_keeps`] for bounds and an outline given rather than read off `spec`,
/// which still says which columns its axes rewrote into logarithms.
fn selection_keeps(
    spec: &PlotSpec,
    active: &[&crate::ir::BrushDef],
    region: Option<&crate::ir::RegionDef>,
    df: &DataFrame,
) -> Option<Vec<bool>> {
    if active.is_empty() && region.is_none() {
        return None;
    }
    let mut keep = vec![true; df.len()];
    let mut read_any = false;
    // **A bound has to be in the same space as the column it tests.** A log
    // scale rewrites its column to logarithms before the renderer ever sees the
    // frame, so a bound written in the data's own units — which is the only way
    // a reader can write one, and what `at` means everywhere else — would be
    // compared against numbers it shares no scale with, and would match nothing
    // at all. Every plot in the manual that draws fully dimmed was log-scaled.
    let logged = |field: &str| -> Option<f64> {
        for ch in [Channel::X, Channel::Y, Channel::Z] {
            if let Some(def) = spec.axis_def(&ch) {
                if def.field == field && crate::scale::is_log(Some(def)) {
                    return Some(crate::scale::log_base(Some(def)));
                }
            }
        }
        None
    };
    for b in active {
        if let Some(at) = b.at {
            let at = match logged(&b.field) {
                Some(base) => [at[0].log(base), at[1].log(base)],
                None => at,
            };
            if let Some(col) = df.float_col(&b.field) {
                read_any = true;
                for (i, v) in col.iter().enumerate().take(keep.len()) {
                    // A non-finite value is outside every bound rather than
                    // inside one: it has no place on the axis to compare.
                    keep[i] &= v.is_finite() && *v >= at[0] && *v <= at[1];
                }
            }
        } else if let Some(levels) = &b.levels {
            if let Some(col) = df.str_col(&b.field) {
                read_any = true;
                for (i, v) in col.iter().enumerate().take(keep.len()) {
                    keep[i] &= levels.contains(v);
                }
            }
        }
    }
    if let Some(r) = region {
        if let (Some(xs), Some(ys)) = (df.float_col(&r.x), df.float_col(&r.y)) {
            read_any = true;
            // The outline arrives in the data's own units, exactly as `at` does,
            // and lands in whatever space each column was rewritten into. Both
            // axes are asked separately: one may be logarithmic and the other not.
            let (bx, by) = (logged(&r.x), logged(&r.y));
            let traced = crate::ir::RegionDef {
                x: r.x.clone(),
                y: r.y.clone(),
                path: r
                    .path
                    .iter()
                    .map(|[px, py]| {
                        [bx.map_or(*px, |b| px.log(b)), by.map_or(*py, |b| py.log(b))]
                    })
                    .collect(),
            };
            for (i, k) in keep.iter_mut().enumerate() {
                *k &= match (xs.get(i), ys.get(i)) {
                    (Some(x), Some(y)) => traced.holds(*x, *y),
                    _ => false,
                };
            }
        }
    }
    // A layer whose table does not carry the brushed column is untouched by the
    // selection rather than emptied by it — the same rule that lets a reference
    // layer stand still while a played layer moves.
    read_any.then_some(keep)
}

/// Which of a flow's slots a brush keeps — [`brush_keeps`] for the rows
/// `flow_nodes` writes, one per place at each stage, which `zone` and `text` read.
///
/// A slot stands for every path through its place, so only a bound that names
/// **its own place** can say whether it is selected: a bound on its own stage
/// (`brush(sex, at = "Female")` keeps `Female` and dims `Male`), on either column
/// of a shared flow, whose slots are places of both, or on the flow's `name` at a
/// stage where the bound names a place. A bound on another stage says nothing about
/// it, since a `1st` slot holds women and men alike, and dimming it either way
/// would be the dishonest picture `transform_collapses_rows` exists to prevent.
/// A layered flow's layers are not stages a path passes through: a slot in
/// another layer need hold nothing of the selection, so a layered flow reads a
/// bound as the shared flow does, and every other place's slot steps back.
/// Such a slot is drawn at full strength with the selection. `None` when no bound
/// speaks about these rows, so the layer is drawn once and whole, as a layer
/// without the brushed column is.
pub fn flow_slot_keeps(spec: &PlotSpec, layer: &Layer, df: &DataFrame) -> Option<Vec<bool>> {
    let name = df.str_col(crate::transform::NODE_NAME)?;
    let stage = df.str_col(crate::transform::FLOW_STAGE);
    let shared = layer.flow_reads_links();
    let columns: Vec<&str> = layer.flow.as_ref()
        .map(|f| f.stages.iter().map(String::as_str).collect()).unwrap_or_default();
    let mut keep = vec![true; df.len()];
    let mut read_any = false;
    let stage_of = |i: usize| stage.and_then(|s| s.get(i));
    for b in spec.brush.iter().filter(|b| !b.is_resting()) {
        // A place is a category, so a bound on one is a list of them.
        let Some(levels) = &b.levels else { continue };
        // A bound on `name` speaks to the stages where it names a place: there the
        // named slots are selected and their siblings are not, and a stage it names
        // nothing at holds selected and unselected paths alike. A shared flow's
        // slots all stand on one axis, so it speaks to every one of them.
        let named_at: Vec<Option<&String>> = (0..df.len())
            .filter(|&i| name.get(i).is_some_and(|n| levels.contains(n)))
            .map(stage_of)
            .collect();
        for (i, k) in keep.iter_mut().enumerate() {
            // A flow read as links has no stages to speak at: each slot is a
            // place of both columns, so a bound on `name` speaks to every one.
            let speaks = if b.field == crate::transform::NODE_NAME {
                shared || named_at.contains(&stage_of(i))
            } else {
                (shared && columns.contains(&b.field.as_str())) || stage_of(i) == Some(&b.field)
            };
            if speaks {
                read_any = true;
                *k &= name.get(i).is_some_and(|n| levels.contains(n));
            }
        }
    }
    read_any.then_some(keep)
}

/// Which of a flow's bands a brush keeps. A bound on a column the bands carry (a
/// stage, either column of a shared flow, or a column that splits them) is
/// [`brush_keeps`] itself. A bound on the flow's **`name`** keeps a band that
/// passes through a named place at *any* of its stages, so on a shared flow it
/// keeps both what a place sends and what it receives: the chord diagram's
/// one-place-at-a-time reading, which no single column can state, since two
/// bounds on two columns would keep only what both select.
pub fn flow_band_keeps(spec: &PlotSpec, layer: &Layer, df: &DataFrame) -> Option<Vec<bool>> {
    let plain = brush_keeps(spec, df);
    let named: Vec<&crate::ir::BrushDef> = spec.brush.iter()
        .filter(|b| !b.is_resting() && b.field == crate::transform::NODE_NAME).collect();
    if named.is_empty() {
        return plain;
    }
    let cols: Vec<&Vec<String>> = layer.flow.as_ref()
        .map(|f| f.stages.iter().filter_map(|c| df.str_col(c)).collect()).unwrap_or_default();
    let mut keep = plain.unwrap_or_else(|| vec![true; df.len()]);
    for b in named {
        let Some(levels) = &b.levels else { continue };
        for (i, k) in keep.iter_mut().enumerate() {
            *k &= cols.iter().any(|c| c.get(i).is_some_and(|v| levels.contains(v)));
        }
    }
    Some(keep)
}

/// The plot-scoped rule, which is `size`'s rule word for word: apply the binding
/// where it fits, say where it did not, and refuse only when it fits nowhere.
pub(super) fn check_brush(out: &mut Vec<Diagnostic>, spec: &PlotSpec, data: &HashMap<String, DataFrame>) {
    // A traced outline reaches the same layers a bound does, and earns the same
    // report about the ones it left whole, so it opens this check too. It carries
    // no refusals of its own: a gesture cannot make the mistakes a written bound
    // can — no reversed range, no range on a column of categories, no misspelled
    // name — because the browser reads every part of it off the panel it was
    // drawn on.
    if spec.brush.is_empty() && spec.region.is_none() {
        return;
    }

    // **A map is not brushed (ruled 2026-09-27).** A map projects longitude and
    // latitude before it draws, and the page and the picture then measured a
    // bound in different units. A written bound was tested against projected
    // units, so the page counted the right rows while the picture dimmed the
    // wrong ones; a dragged bound was projected, so the picture was right and the
    // page counted none; and a drag over a choropleth sliced countries into
    // polygons nobody drew. (Under Equal Earth a range of degrees is not even a
    // rectangle on the page; under Mercator it is, and the disagreement stands.)
    // Three answers, none of them the reader's selection, so the pair is refused
    // and pointed at the same columns without `map()`, where the axes are the
    // degrees themselves.
    if matches!(spec.coord, CoordSpace::Map(_)) {
        // The columns as the layers read them: written after a mark, a position
        // sits on the layer rather than on the plot.
        let read = |ch: Channel| spec.layers.iter()
            .find_map(|l| spec.position_for(l, &ch))
            .map(|c| c.field.as_str());
        let lon = read(Channel::X).unwrap_or("<longitude>");
        let lat = read(Channel::Y).unwrap_or("<latitude>");
        let brushed = spec.brush.iter().find(|b| !b.is_positions())
            .map_or(String::new(), |b| b.field.clone());
        let brushed = if brushed.is_empty() { lon.to_string() } else { brushed };
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `brush` cannot select on a `map()`. A map projects longitude and \
                 latitude before it draws, and a brush on it would count one set of rows \
                 while it dims another. Drop `brush()`, or brush the same columns without \
                 `map()`, where the axes are the degrees themselves: \
                 `point + x({lon}) + y({lat}) + brush({brushed})`."
            ),
        });
        return;
    }

    // **A network refuses the brush itself** (`check_network`: its positions are
    // the layout's, not the data's), so nothing below is asked of it, as nothing is
    // of a map. Asked, a bare `brush` was told to add the `x()` or `y()` a layout
    // refuses, an `edge` to split itself with `group()`, which an edge refuses, and
    // a `point * layout` to brush the layer that draws the rows themselves, which a
    // network has none of.
    if matches!(spec.coord, CoordSpace::Network(_)) {
        return;
    }

    // **A brush that names no bound selects only what a reader drags, and three
    // spaces give the drag another job** (ruled 2026-10-09). On a polar plot the
    // page reads a dragged box as a range on straight axes, which a bent axis does
    // not have, so a drag there never selects; in the cube and on a globe the drag
    // turns the view. Accepted, `brush` or `brush(gdp)` there drew the count line
    // reading "0 of 142 selected" forever, a selection no reader could ever make:
    // the silent drop §12 forbids. A bound written in the sentence still works in
    // all three, tested against each row's values. A flow is not asked: it selects
    // by a click on a slot, in polar too, which is the chord diagram's brush.
    let flows = spec.layers.iter().any(|l| l.transforms.contains(&Transform::Flow));
    let drag_does = if flows {
        None
    } else {
        match space_of(spec) {
            SpaceKind::Polar => Some("On a polar plot a drag never selects, because the page \
                reads a dragged box as a range on straight axes, and a polar plot bends them."),
            SpaceKind::Space => Some("In the cube a drag turns the plot."),
            SpaceKind::Globe => Some("On a globe a drag turns the globe."),
            _ => None,
        }
    };
    if let Some(why) = drag_does {
        for b in spec.brush.iter().filter(|b| b.is_resting()) {
            let message = if b.is_positions() {
                let col = spec.x.as_ref().map(|d| d.field.as_str())
                    .or_else(|| spec.layers.iter()
                        .find_map(|l| l.encodings.get(&Channel::X)).map(|d| d.field.as_str()))
                    .unwrap_or("<column>");
                format!(
                    "gog: `brush` on its own selects only what a reader drags. {why} This \
                     brush would never select a row. Name a column and where the selection \
                     is: `brush({col}, at = ...)`."
                )
            } else {
                format!(
                    "gog: `brush({0})` with no `at` selects only what a reader drags. {why} \
                     This brush would never select a row. Say where the selection is: \
                     `brush({0}, at = ...)`.",
                    b.field
                )
            };
            out.push(Diagnostic { kind: DiagnosticKind::Illegal, message });
        }
    }

    for b in &spec.brush {
        // Bare `brush` says *both positions are selectable*, so there is no
        // single axis for a stated bound to belong to. Naming a column is how
        // you say which axis you meant, and the refusal says exactly that.
        if b.is_positions() {
            if b.at.is_some() || b.levels.is_some() {
                out.push(Diagnostic {
                    kind: DiagnosticKind::Illegal,
                    message: "gog: `brush` on its own lets the reader select a region, so it \
                              cannot also say where the selection opens — there is no one axis \
                              for a range to be on. Name the column: `brush(gdp, at = ...)`."
                        .to_string(),
                });
            }
            // A flow needs no bound position for it: the reader picks a column by
            // clicking one of its slots, so the stages are what is selectable.
            let placed = spec.x.is_some() || spec.y.is_some()
                || spec.layers.iter().any(|l| {
                    l.encodings.contains_key(&Channel::X) || l.encodings.contains_key(&Channel::Y)
                        || l.transforms.contains(&Transform::Flow)
                });
            if !placed {
                out.push(Diagnostic {
                    kind: DiagnosticKind::Illegal,
                    message: "gog: `brush` on its own selects over the positions the plot binds, \
                              and this plot binds none. Give it an `x()` or a `y()`, or brush a \
                              column by name."
                        .to_string(),
                });
            }
            continue;
        }
        if b.at.is_some() && b.levels.is_some() {
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: format!(
                    "gog: `brush({0})` was given both a range and a list of levels. A column \
                     measures or it names categories, never both. Keep the one that matches \
                     `{0}` and drop the other.",
                    b.field
                ),
            });
        }
        if let Some([lo, hi]) = b.at {
            if !(lo < hi) {
                out.push(Diagnostic {
                    kind: DiagnosticKind::Illegal,
                    message: format!(
                        "gog: `brush({})` at {lo} to {hi} selects nothing, because the range \
                         does not run upward. Write the smaller number first.",
                        b.field
                    ),
                });
            }
        }
        // The scrub bar. It picks which moment is on show rather than which rows
        // are selected within one, and every moment is already drawn — so it moves
        // the clock, and the clock belongs to the page rather than to the sentence.
        let played = spec
            .layers
            .iter()
            .filter_map(|l| l.encodings.get(&Channel::Play))
            .chain(spec.channels.get(&Channel::Play))
            .any(|d| d.field == b.field);
        if played {
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: format!(
                    "gog: `{0}` is already the column `play()` cuts the frames on, so \
                     `brush({0})` would select frames rather than rows inside one. That is a \
                     scrub bar, which belongs to the viewer and not to the sentence. Brush a \
                     column the frames are not cut on.",
                    b.field
                ),
            });
        }
        // A column no bound table carries is the silent drop §12 forbids: the
        // brush would be accepted and would select nothing, forever. A flow's
        // `name` is the one exception, a column the flow itself publishes, as
        // `label(name)` reads it.
        let flow_name = b.field == crate::transform::NODE_NAME
            && spec.layers.iter().any(|l| l.transforms.contains(&Transform::Flow));
        let known = flow_name || spec.layers.iter().any(|l| {
            l.data
                .as_ref()
                .or(spec.data.as_ref())
                .and_then(|n| data.get(n))
                .map(|df| df.float_col(&b.field).is_some() || df.str_col(&b.field).is_some())
                .unwrap_or(true)
        });
        if !known {
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: format!(
                    "gog: `brush({0})` names a column no table in this plot has. Check the \
                     spelling, or brush a column the plot already reads.",
                    b.field
                ),
            });
        }
        // **Names to select on a column of numbers select nothing.** A binding sends
        // `at` as names when it is text, and no number in a measured column carries
        // a name, so `brush(gdp, at = "low")` drew the plot with nothing selected
        // and no message, in all four bindings. A measured column takes a range.
        if b.levels.is_some() {
            let measured = spec.layers.iter().any(|l| {
                l.data
                    .as_ref()
                    .or(spec.data.as_ref())
                    .and_then(|n| data.get(n))
                    .is_some_and(|df| df.float_col(&b.field).is_some())
            });
            if measured {
                out.push(Diagnostic {
                    kind: DiagnosticKind::Illegal,
                    message: format!(
                        "gog: `brush({0})` was given names to select, and `{0}` is a column of \
                         numbers, so no row has those names. Give `at` a range instead: two \
                         numbers, the smaller first.",
                        b.field
                    ),
                });
            }
        }
    }

    // **A flow selects whole bands, so its brush names a column every band holds
    // one value of** (2026-10-03, at the author's word): a stage, which each path
    // passes through at one place, or a column of categories that colors or
    // patterns the bands, which splits each path into parts that hold one value
    // each. Any other column cuts across a band, and a band drawn half selected
    // has no honest picture, so the brush would select nothing the reader could
    // see. That is the silent drop §12 forbids, so it is refused, with the two
    // ways to make the column one a band holds.
    let flows: Vec<&Layer> = spec.layers.iter()
        .filter(|l| l.transforms.contains(&Transform::Flow)).collect();
    if !flows.is_empty() {
        let stages: Vec<&str> = flows.iter().filter_map(|l| l.flow.as_ref())
            .flat_map(|f| f.stages.iter().map(String::as_str)).collect();
        let splits: Vec<&str> = flows.iter().filter(|l| l.mark == Mark::Ribbon)
            .flat_map(|l| [Channel::Color, Channel::Pattern].into_iter()
                .filter_map(|ch| l.encodings.get(&ch)).map(|d| d.field.as_str()))
            .collect();
        for b in spec.brush.iter().filter(|b| !b.is_positions()) {
            if stages.contains(&b.field.as_str()) || splits.contains(&b.field.as_str())
                || b.field == crate::transform::NODE_NAME
            {
                continue;
            }
            let first = stages.first().copied().unwrap_or("<stage>");
            // A number cannot split a band (a band's `color` must be a category), so
            // the second way out is offered only for a column of categories.
            let measures = flows.iter().any(|l| {
                l.data.as_ref().or(spec.data.as_ref()).and_then(|n| data.get(n))
                    .is_some_and(|df| df.float_col(&b.field).is_some())
            });
            let message = if measures {
                format!(
                    "gog: `brush({0})` on a flow is valid grammar, but this engine does not \
                     draw it. A flow selects whole bands by a column of categories, and \
                     `{0}` measures, so a range on it would cut across the bands. Brush a \
                     stage, such as `brush({first}, at = ...)`.",
                    b.field
                )
            } else {
                format!(
                    "gog: `brush({0})` on a flow is valid grammar, but this engine does not \
                     draw it. A flow selects whole bands, and `{0}` is neither a stage nor a \
                     column that colors the bands, so it would cut across them. Brush a \
                     stage, such as `brush({first}, at = ...)`, or map `{0}` to the `ribbon` \
                     layer's `color` or `pattern`, which splits each band by its values.",
                    b.field
                )
            };
            out.push(Diagnostic { kind: DiagnosticKind::Unsupported, message });
        }
    }

    // **A stacked bar's measure is a running total** (2026-10-09, when a brushed
    // bar was first drawn). `stack` rewrites the measure column to each segment's
    // top, its own value plus every segment below it, so a bound on that column
    // tested where the pile had reached while the page counts the table's own
    // values: the picture dimmed one set of rows and the count named another. A
    // drag over the positions reads the same column. The category and the split
    // column are not rewritten, so a brush on either draws.
    for layer in spec.layers.iter().filter(|l| {
        l.mark == Mark::Bar && l.transforms.contains(&Transform::Stack)
            && !l.transforms.iter().any(transform_collapses_rows)
    }) {
        let df = layer.data.as_ref().or(spec.data.as_ref()).and_then(|n| data.get(n));
        let ty = |ch: Channel| spec.position_for(layer, &ch)
            .and_then(|c| df.and_then(|d| actual_type(d, &c.field)));
        let (measure_ch, slot_ch) = match slot_orient(ty(Channel::X), ty(Channel::Y)) {
            Orient::Horizontal => (Channel::X, Channel::Y),
            _ => (Channel::Y, Channel::X),
        };
        let Some(measure) = spec.position_for(layer, &measure_ch).map(|c| c.field.as_str()) else {
            continue;
        };
        let Some(b) = spec.brush.iter().find(|b| b.is_positions() || b.field == measure) else {
            continue;
        };
        let (what, reads) = if b.is_positions() {
            ("a brush over the positions of a stacked `bar`".to_string(),
             format!("it reads `{measure}`, and "))
        } else {
            (format!("a brush on `{measure}` over a stacked `bar`"), String::new())
        };
        let slot = spec.position_for(layer, &slot_ch).map_or("<category>", |c| c.field.as_str());
        out.push(Diagnostic {
            kind: DiagnosticKind::Unsupported,
            message: format!(
                "gog: {what} is valid grammar, but this engine does not draw it, because \
                 {reads}`stack` turns each bar's `{measure}` into the running total of the bars \
                 below it. The selection would follow the pile rather than each row's own \
                 value. Brush the category instead, as in `brush({slot}, at = ...)`, or set the \
                 bars side by side with `dodge`, where each bar stands at its own value."
            ),
        });
    }

    // Which layers can answer a selection at all, and which the engine can draw.
    let mut answers = Vec::new();
    let mut undrawn = Vec::new();
    let mut collapsed = Vec::new();
    let mut not_elements = Vec::new();
    for layer in &spec.layers {
        // A layer the graph places has positions only in `network()`, and outside
        // it its own refusal says so; the brush has nothing to add.
        if placed_by_the_graph(layer) {
            continue;
        }
        let m = mark_name(&layer.mark);
        // Every flow layer answers: the bands by their paths, the slots and their
        // names by their own stage. Which columns it answers on is settled above.
        if layer.transforms.contains(&Transform::Flow) {
            answers.push(m);
            continue;
        }
        if !layer_takes_selection(layer) {
            not_elements.push(m);
        } else if let Some(t) = layer.transforms.iter().find(|t| transform_collapses_rows(t)) {
            collapsed.push((m, format!("{t:?}").to_lowercase()));
        } else if selection_draws(layer).is_some() {
            undrawn.push((m, selection_draws(layer).unwrap()));
        } else {
            answers.push(m);
        }
    }

    if !answers.is_empty() {
        // Some layer answers, so the others are an Assumption rather than a
        // refusal: the plot draws, and the engine says what it left alone.
        for (m, why) in &undrawn {
            out.push(Diagnostic {
                kind: DiagnosticKind::Assumption,
                message: format!(
                    "gog: the `{m}` layer is drawn whole, because {why}. The selection still \
                     reads on the rest of the plot."
                ),
            });
        }
        for (m, t) in &collapsed {
            out.push(Diagnostic {
                kind: DiagnosticKind::Assumption,
                message: format!(
                    "gog: the `{m} * {t}` layer is drawn whole, because `{t}` summarizes many \
                     rows into one and a selection of some of them has no honest picture. The \
                     selection still reads on the rest of the plot."
                ),
            });
        }
        // Said out loud for the same reason a plot-scoped `size` says which marks
        // it skipped: a binding applied to some layers and not others is a
        // decision the reader did not make and cannot see in the picture.
        for m in &not_elements {
            out.push(Diagnostic {
                kind: DiagnosticKind::Assumption,
                message: format!(
                    "gog: the `{m}` layer is drawn whole, because it draws one shape through \
                     many rows and there is no single row to select. The selection still reads \
                     on the rest of the plot."
                ),
            });
        }
        return;
    }

    // Nothing in this plot can answer the brush, so it would be accepted and do
    // nothing. Which refusal depends on why, because the two have different fixes.
    if let Some((m, why)) = undrawn.first() {
        out.push(Diagnostic {
            kind: DiagnosticKind::Unsupported,
            message: format!(
                "gog: a brush on {} `{m}` is valid grammar, but this engine does not draw \
                 it, because {why}. Brush a `point` or a `text` layer, or drop the brush.",
                article(m)
            ),
        });
    } else if let Some((m, t)) = collapsed.first() {
        out.push(Diagnostic {
            kind: DiagnosticKind::Unsupported,
            message: format!(
                "gog: a brush on `{m} * {t}` is valid grammar, but this engine does not draw \
                 it: `{t}` summarizes many rows into one, and the engine cannot say which of \
                 them you selected. Brush the layer that draws the rows themselves, or drop \
                 the brush."
            ),
        });
    } else if let Some(m) = not_elements.first() {
        // The layers named are the ones that take a selection and draw it, read off
        // `layer_takes_selection` and `selection_draws` as the refusals above are, so
        // none of them is refused in turn. A mark that answers only under `bounds`
        // or `flow` is named with it, after the marks that answer alone: the layer
        // decides that case, not the mark. The flow was left out until 2026-10-09,
        // while the book said its bands answer.
        // It also said "Use `group()` to split it", and a grouped line is still
        // refused here, since `group` splits one shape into several, never into rows.
        let answers = |layer: &Layer| layer_takes_selection(layer) && selection_draws(layer).is_none();
        let drawable = || ALL_MARKS.iter().filter(|mk| is_drawable(mk));
        let alone: Vec<&Mark> = drawable().filter(|mk| answers(&Layer::new((*mk).clone()))).collect();
        let mut brushable: Vec<String> = alone.iter().map(|mk| mark_name(mk).to_string()).collect();
        for t in [Transform::Bounds, Transform::Flow] {
            for mk in drawable().filter(|mk| !alone.contains(mk)) {
                if answers(&Layer::new(mk.clone()).transform(t.clone())) {
                    brushable.push(format!("{} * {}", mark_name(mk), format!("{t:?}").to_lowercase()));
                }
            }
        }
        let names: Vec<&str> = brushable.iter().map(String::as_str).collect();
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: {} `{m}` draws one shape through many rows, so there is no single row \
                 to select. Brush a layer that draws a selection: {}.",
                article(m), code_list(&names)
            ),
        });
    }
}
