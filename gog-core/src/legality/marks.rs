//! The checks a mark's own geometry asks of its rows: a slot's shape, a span's
//! range, a surface's grid, a zone's cells, a path's order.

use super::*;

/// A slot mark needs something to measure.
///
/// Two refusals, and they are deliberately not the same shape. The
/// **both-categorical** one covers all three marks: whichever of them is drawn,
/// one axis has to be the measure, and two categories leave none. The
/// **date-on-the-measure** one is `bar`-only, because it is about a *length*,
/// not about a slot mark in general: a bar is read from a baseline, and a bar
/// reaching the year 2007 would be measured from the epoch, an origin nobody
/// chose. A box or a whisker has no baseline — it spans between two moments —
/// so the median of a set of dates is a perfectly good quantity and stays legal.
///
/// **Both refusals are about a mark with two axes, so neither reaches the cube.**
/// They read "one of these two must be the measure", which is true wherever a slot
/// mark has exactly two positions to divide between a slot and a length — and false
/// in `space`, where `x` and `y` are both edges of a footprint and `z` is the
/// length. Two categorical floors is then the *tile plot standing up*
/// (`bar * count + x(<a>) + y(<b>) + space()`), not a bar with nothing to measure;
/// and a date is an ordinary thing to slot a floor by. Left in place this refused
/// the plot the dimensionality rule had just made legal, and pointed at
/// `bar * count` as the fix — which it refused for the same reason.
pub(super) fn check_slot_shape(out: &mut Vec<Diagnostic>, spec: &PlotSpec, df: &DataFrame, layer: &Layer) {
    if !is_slot_mark(&layer.mark) || cuts_both_positions(&layer.mark, space_of(spec)) {
        return;
    }
    let m = mark_name(&layer.mark);

    // A bar's length is an amount, and a moment in time is not an amount —
    // a bar "reaching" the year 2007 would be measured from the epoch, which
    // is an arbitrary origin nobody chose. Dates belong on the axis the bars
    // *sit* on, where a time series of bars is a perfectly ordinary plot.
    let xd = spec.position_for(layer, &Channel::X);
    let yd = spec.position_for(layer, &Channel::Y);
    let xt = xd.and_then(|c| actual_type(df, &c.field));
    let yt = yd.and_then(|c| actual_type(df, &c.field));
    let (measured, pos_name) = match slot_orient(xt, yt) {
        Orient::Horizontal => (xd, "y"),
        Orient::Vertical => (yd, "x"),
    };
    if layer.mark == Mark::Bar {
        if let Some(meas) = measured {
            if df.time_unit(&meas.field).is_some() {
                out.push(Diagnostic {
                    kind: DiagnosticKind::Illegal,
                    message: format!(
                        "gog: `bar` measures its length along `{}`, but that is a date column — a \
                         bar's length is an amount, and a moment in time is not an amount. Put the \
                         date on `{pos_name}()` and measure a number, or use `point`/`line` for \
                         values that are dates.",
                        meas.field
                    ),
                });
                return;
            }
        }
    }

    let (Some(x), Some(y)) = (xd, yd) else { return };
    if actual_type(df, &x.field) != Some(VarType::Discrete)
        || actual_type(df, &y.field) != Some(VarType::Discrete)
    {
        return;
    }
    // **A tally or an estimate is not short of a measure: it makes one.** What is
    // wrong is the category named on the axis it draws along, and
    // `synth_axis_names_a_column` says so, with the splits that keep that column.
    // Refused here as well, `bar * count + x(<a>) + y(<b>)` was told there was
    // nothing to measure and to use `bar * count`, the sentence it was.
    if synthesizes_measure(&layer.mark, &layer.transforms) {
        return;
    }
    // What the measure would have *been* — named per mark, so the direction
    // points at the thing that mark draws rather than at a generic "a number".
    let (what, measure) = match layer.mark {
        Mark::Box => ("summarize", "the column the five-number summary reduces"),
        Mark::Interval => ("span", "the column the low and high extents come from"),
        _ => ("measure", "the length of the bar"),
    };
    let (a, b) = (&x.field, &y.field);
    // **A written `space()` with no `z` has a floor and nothing standing on it.** In
    // the cube `x` and `y` are the footprint, not a slot and a measure, so neither
    // has to be a number: `z` is the one missing. Told "one axis must be a number"
    // and to count instead, a floor summary (`bar * mean + x(<a>) + y(<b>) + space()`)
    // was sent away from the plot it was one position short of.
    let fix = if matches!(spec.coord, CoordSpace::Space(_))
        && mark_draws_in_space(&layer.mark, SpaceKind::Space)
    {
        let count = if layer.mark == Mark::Bar && layer.transforms.is_empty() {
            " To count the rows on each pair of categories instead, `bar * count` stands \
             the count up on `z`."
        } else {
            ""
        };
        format!(
            "In the cube, `x` and `y` are the floor and `z` is {measure}: add \
             `z(<column>)`.{count}"
        )
    } else if layer.mark == Mark::Bar && a == b {
        // One column twice has no pairs to split or stand on: the count is of it.
        format!(
            "One axis must be a number: that is {measure}. To count the rows instead, \
             `bar * count + x({a})` draws one bar for each `{a}`."
        )
    } else if layer.mark == Mark::Bar {
        // Every count offered draws. "Use `bar * count`" kept `y({b})` and was refused
        // for it, since a count draws on `y`. Either column can be the one counted,
        // and which one the reader wants is not in the sentence: a country's
        // continent groups it, never the other way round, and nothing here says so.
        format!(
            "One axis must be a number: that is {measure}. To count the rows instead, \
             count one column, `bar * count + x({a})` or `bar * count + x({b})`, and \
             `color()` splits each bar by the other. To keep both, \
             `bar * count + x({a}) + y({b}) + space()` stands one bar on each pair in the \
             cube."
        )
    } else {
        format!("One axis must be a number: that is {measure}.")
    };
    out.push(Diagnostic {
        kind: DiagnosticKind::Illegal,
        message: format!(
            "gog: `{m}` has categorical columns on both axes — `x({a})` and `y({b})` — so \
             there is nothing for it to {what}. {fix}"
        ),
    });
}

/// The two marks that **float between a low and a high** — `interval` (a whisker)
/// and `ribbon` (a filled band) — need a range-producing transform to supply those
/// extents; on their own they have only single y values, nothing to span. Refused
/// with direction rather than drawn empty — the same duty `check_mark` performs for
/// a mark with no renderer, here for a mark whose minimum syllable is unmet (spec
/// §6, §7). The refusal names the mark's own idiomatic form so the fix reads back
/// as the thing the user was reaching for.
pub(super) fn check_span_needs_range(out: &mut Vec<Diagnostic>, spec: &PlotSpec, df: Option<&DataFrame>, layer: &Layer) {
    // A `zone` asks the same question — *where are my sides?* — but it has four ways
    // to answer it and one of them is the axis itself, which cannot be seen without
    // the data. So it is checked in `check_zone_extent`, in the df-gated block.
    if layer.mark == Mark::Zone {
        // Named *and* cut says where the sides are twice — and that is the extent job
        // filled twice, which `check_chain_jobs` refuses for every mark rather than
        // for this one. The sentence it gives is this arm's, moved rather than
        // rewritten. Kept as a comment because the *reason* this arm returns early is
        // still the one below: a zone has four ways to answer "where are my sides?"
        // and one of them is the axis itself, which cannot be seen without the data.
        return;
    }
    // **A space that never draws the mark refuses it in its own words.** On a
    // `map()` or a `globe()` an `interval` was told to add a range transform, and
    // `interval * range` was then refused by the space: the direction led to a
    // second refusal. The space's refusal stands alone.
    if !mark_draws_in_space(&layer.mark, space_of(spec)) {
        return;
    }
    let example = match layer.mark {
        Mark::Interval => "`interval * range + x(group) + y(value)` draws the min–max range per group, or `interval * bounds(lo, hi)` a pre-computed one",
        Mark::Ribbon => "`ribbon * range + x(t) + y(value)` draws a band between the min and max at each x, or `ribbon * bounds(lo, hi)` a pre-computed one",
        _ => return,
    };
    // `range`/`confidence` compute the extents; `bounds` supplies pre-computed ones.
    // Any of the three satisfies the mark's minimum syllable.
    if layer.transforms.iter().any(is_pair_transform) {
        return;
    }
    // A **flow** answers the same question a fifth way: the band's two boundaries
    // are the path's intervals at two adjacent stages, both laid by the layout.
    if layer.transforms.contains(&Transform::Flow) {
        return;
    }
    // And so does the **violin** — a fourth way to answer the same question, which is
    // why the requirement is stated as *where are the boundaries* rather than as a
    // list of three transforms. `density` in its slot reading produces them by
    // reflection: the estimate is one boundary and its mirror is the other, so the
    // band is bounded exactly as `range` bounds it, from a statistic rather than from
    // a pair of columns. Data-aware because only the column types say which reading
    // of `density` this is — a bare `ribbon * density + x(life)` is the *curve*, which
    // produces one boundary and no second, and falls through to the refusal below.
    if slot_density(spec, layer, df).is_some() {
        return;
    }
    // A `density` that reached here is the **curve** — one boundary, drawn against a
    // continuous axis — and the reader is one binding away from the violin, so say
    // which binding rather than sending them to `range`.
    if layer.transforms.contains(&Transform::Density) {
        let m = mark_name(&layer.mark);
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `{m} * density` estimates one curve, and {} {m} needs two boundaries to \
                 span between. Bind a category and the estimate is drawn across its slot — \
                 `{m} * density + x(<category>) + y(<number>)` is the violin, one distribution \
                 per group. For the curve itself, `area * density + x(<number>)` fills it to \
                 the baseline and `line * density + x(<number>)` traces it.",
                article(m)
            ),
        });
        return;
    }
    out.push(Diagnostic {
        kind: DiagnosticKind::Illegal,
        message: format!(
            "gog: `{}` draws a span from a low value to a high one, but nothing here \
             produces those extents. Add a range transform — {example}.",
            mark_name(&layer.mark),
        ),
    });
}

/// What a `surface` needs before it can be a sheet: the cube, and a grid to span
/// (spec §15). The mark's whole story in one check, on `check_span_needs_range`'s
/// precedent.
///
/// **The cube first, and it is one failure rather than two.** A surface is a sheet
/// through three positions and there is no such thing without the third, so a flat
/// `surface` and a `surface` with no `z()` are the same mistake, and the direction
/// names both routes in — bind `z`, or let `density` invent it under `space()` — plus
/// `zone`, which is where a field lives in the plane. `mark_draws_in_space` says the
/// same thing per space; this is where a reader hears about it.
///
/// **Then a grid, and the fatal case is a condition rather than a threshold.** A
/// table of scattered points has no repeated `x` and no repeated `y`, so the lattice
/// it describes is *n*×*n* holding *n* nodes and **not one complete block of four**.
/// A sheet over it is an empty panel — the failure this project refuses above all
/// others, arriving as geometry rather than as a dropped binding. So the test is
/// exactly *can one face be drawn*, which needs no fraction to be tuned and is right
/// at every table size. Direction is `point` for a cloud, or `surface * density` to
/// estimate a field **from** the scatter, which is the sentence a reader with
/// scattered data actually wanted.
///
/// **A hole is legitimate and draws**, with an Assumption counting the open crossings
/// (§12): a response surface can be missing a cell, and a mesh with a gap in it is a
/// true picture of that. Refusing it would be taste enforced as legality (Law 8);
/// staying quiet about it would let a lattice recovered from near-miss coordinates
/// look like a mesh with a designed opening.
///
/// Takes the frame as an `Option`, `check_tiling`'s shape: the cube question is
/// answered by the sentence and must be asked even of a spec with no table behind
/// it, while the grid question can only be answered by the rows.
pub(super) fn check_surface(
    out: &mut Vec<Diagnostic>,
    spec: &PlotSpec,
    df: Option<&DataFrame>,
    layer: &Layer,
) {
    if layer.mark != Mark::Surface {
        return;
    }
    if space_of(spec) != SpaceKind::Space {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: "gog: a `surface` is a sheet through three positions, so it needs the cube. \
                      Bind the height — `surface + x(a) + y(b) + z(h)` — or let a transform \
                      invent it: `surface * density + x(a) + y(b) + space()`. For the same field \
                      drawn in the plane, `zone` paints it as cells and `path` traces its \
                      contours."
                .to_string(),
        });
        return;
    }

    // **Only the *user's* mesh is checked**, and forgetting that was a live bug: this
    // ran against the raw frame, so `surface * density` — whose whole job is to make a
    // mesh out of a scatter — was refused *as* a scatter, with a message advising the
    // sentence that had just been written. A transform that cuts a plane emits every
    // cell of a rectangular lattice, so downstream of one the grid is a fact rather
    // than a question; the question is only whether a table the reader built is one.
    if reads_a_field(&layer.mark, &layer.transforms, SpaceKind::Space) {
        return;
    }

    // The two position columns as the mesh's axes. A categorical position is refused
    // by `rule_for` (a face asserts every value between two nodes, and categories have
    // no between), so by here both are numbers or the layer is already refused —
    // reading them as floats and returning quietly is not a silent drop.
    let Some(df) = df else { return };
    let (Some(xf), Some(yf)) = (
        spec.position_for(layer, &Channel::X).map(|c| c.field.as_str()),
        spec.position_for(layer, &Channel::Y).map(|c| c.field.as_str()),
    ) else {
        return;
    };
    let (Some(xc), Some(yc)) = (df.float_col(xf), df.float_col(yf)) else {
        return;
    };
    let Some(lattice) = crate::data::Lattice::of(xc, yc) else { return };
    let (crossings, filled) = lattice.fill();

    if lattice.faces().is_empty() {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `surface` found no complete cell to draw a face on — \
                 {filled} rows over {} distinct `{xf}` and {} distinct `{yf}` values, which is a \
                 scatter rather than a grid. A surface needs one row per (x, y) crossing: every \
                 `{xf}` value paired with every `{yf}` value. For a cloud of scattered points use `point` with \
                 `z({})`; to estimate a field *from* them use \
                 `surface * density + x({xf}) + y({yf}) + space()`.",
                lattice.xs.len(),
                lattice.ys.len(),
                spec.position_for(layer, &Channel::Z).map_or("<height>", |c| c.field.as_str()),
            ),
        });
        return;
    }

    if filled < crossings {
        out.push(Diagnostic {
            kind: DiagnosticKind::Assumption,
            message: format!(
                "gog: `surface` has {} of {crossings} crossings filled, so the sheet is drawn \
                 with {} open — a face needs all four of its corners. That is a true picture of \
                 a grid with gaps; if you expected a whole sheet, check that `{xf}` and `{yf}` \
                 repeat exactly across rows rather than to a rounded value.",
                filled,
                crossings - filled,
            ),
        });
    }
}

/// Where a `zone`'s sides come from — the mark's minimum syllable (Law 7), and the
/// one check that has to read a column's **type** to answer it.
///
/// **Four extent descriptions, and the fourth costs no columns** (spec §5). A zone is
/// the region mark, and rectangularity was never its identity — the *extent
/// description* was. Three have shipped: `bounds` **names** the sides from columns
/// you hold, `bin`/`density` **cut** them out of a continuous plane, and a level set
/// publishes the curve itself. The fourth is the axis: **a categorical position
/// bounds its axis, because a category owns a slot.** A continuous position does not,
/// because a number is a point — which is the whole of why `zone + x(gdp) + y(life)`
/// is still refused while `zone + x(continent) + y(decade)` draws the tile plot.
///
/// It falls out of that sentence, rather than being four cases, that:
/// - two categorical positions bound both axes — the **tile plot**, no transform at
///   all, its measurement in `color(v)`;
/// - one bounds one axis and the panel supplies the other — a **column highlight**,
///   which is `rule`'s relaxation arriving a third time and was never designed for;
/// - a pair on the same axis simply wins, being the more specific request (Law 5).
///   `zone * bounds(start = "Mar", end = "Jun")` says two named categories, and the
///   slot default was never a request to override.
/// **A cell holds one row** (2026-10-03, at the author's word). A plain zone over
/// categorical positions draws one opaque rectangle per row, so rows that share a
/// cell are drawn on top of one another and the cell shows only the last of them.
/// `zone + x(year) + y(continent) + color(life)`, with `year` stored as text, drew
/// each continent's year in the color of its last country, while the legend's
/// range came from rows nobody could see. That is the silent drop §12 forbids, and
/// a contradiction the engine would settle by discarding rows, so it is refused.
/// Only cells whose rows disagree on a mapped column count: rows that agree draw
/// the same rectangle, and nothing is hidden. Rows in different panels or frames
/// are in different cells. The message counts what would be hidden, which is what
/// makes the refusal appealable.
pub(super) fn check_zone_cells(out: &mut Vec<Diagnostic>, spec: &PlotSpec, df: &DataFrame, layer: &Layer) {
    if layer.mark != Mark::Zone || !layer.transforms.is_empty()
        || layer.encodings.contains_key(&Channel::Group)
    {
        return;
    }
    let positions: Vec<&ChannelDef> = [Channel::X, Channel::Y].iter()
        .filter_map(|ch| spec.position_for(layer, ch))
        .collect();
    if positions.is_empty()
        || positions.iter().any(|d| actual_type(df, &d.field) != Some(VarType::Discrete))
    {
        return;
    }
    let painted: Vec<(Channel, &ChannelDef)> = [Channel::Color, Channel::Opacity, Channel::Pattern]
        .into_iter()
        .filter_map(|ch| layer.encodings.get(&ch).map(|d| (ch, d)))
        .filter(|(_, d)| actual_type(df, &d.field).is_some())
        .collect();
    if painted.is_empty() {
        return;
    }
    let text = |field: &str, r: usize| -> String {
        df.str_col(field).and_then(|c| c.get(r).cloned())
            .or_else(|| df.float_col(field).and_then(|c| c.get(r)).map(|v| v.to_string()))
            .unwrap_or_default()
    };
    let mut keys: Vec<&str> = positions.iter().map(|d| d.field.as_str()).collect();
    if let Some(f) = &spec.facet {
        keys.extend(f.col.as_deref());
        keys.extend(f.row.as_deref());
    }
    if let Some(p) = layer.encodings.get(&Channel::Play) {
        keys.push(p.field.as_str());
    }
    let mut cells: HashMap<Vec<String>, Vec<usize>> = HashMap::new();
    for r in 0..df.len() {
        let key: Vec<String> = keys.iter().map(|k| text(k, r)).collect();
        // A row with no category on a position has no slot, and draws nowhere.
        if positions.iter().any(|d| text(&d.field, r).is_empty()) {
            continue;
        }
        cells.entry(key).or_default().push(r);
    }
    let (mut crowded, mut hidden) = (0usize, 0usize);
    for rows in cells.values() {
        let disagree = rows.len() > 1 && painted.iter().any(|(_, d)| {
            let first = text(&d.field, rows[0]);
            rows.iter().any(|&r| text(&d.field, r) != first)
        });
        if disagree {
            crowded += 1;
            hidden += rows.len() - 1;
        }
    }
    if crowded == 0 {
        return;
    }
    let (ch, def) = &painted[0];
    let call = format!("{}({})", channel_name(ch), def.field);
    let advice = if actual_type(df, &def.field) == Some(VarType::Continuous) {
        format!("For one value per cell, summarize the rows: `zone * mean + {call}` draws \
                 each cell's mean, and `median`, `sum`, `max` and `min` do the same.")
    } else {
        "Keep one row per cell before plotting, or split the rows into panels with \
         `facet(<column>)`."
            .to_string()
    };
    let (holds, them, rows) = match (crowded, hidden) {
        (1, 1) => ("holds", "that cell", "row"),
        (1, _) => ("holds", "that cell", "rows"),
        _ => ("hold", "each of those cells", "rows"),
    };
    out.push(Diagnostic {
        kind: DiagnosticKind::Illegal,
        message: format!(
            "gog: a `zone` draws one opaque rectangle per row, and {crowded} of its {} \
             cells {holds} rows with different `{call}`, so {them} would show only its \
             last row, and {hidden} {rows} would be hidden. {advice}",
            cells.len(),
        ),
    });
}

pub(super) fn check_zone_extent(out: &mut Vec<Diagnostic>, spec: &PlotSpec, df: &DataFrame, layer: &Layer) {
    if layer.mark != Mark::Zone {
        return;
    }
    // **A boundary is the fifth source of sides**, and this function asks about the
    // other four. `group` names the rows that form one region, so a zone carrying
    // it has an extent — just not one written as a pair per axis. Answered here
    // rather than left to fall through, because the message below lists the four
    // and would tell a reader with a coastline to go and find a rectangle.
    if layer.encodings.contains_key(&Channel::Group) {
        return;
    }
    // A tallying transform means the cells were *meant* to be slots. If the axes are
    // not categorical, `check_distribution_axis` refuses in that transform's own
    // words; saying it here too would be two refusals for one mistake.
    //
    // A **reduction** says the same thing and is deferred for the same reason: it
    // measures into cells it did not cut, so its cells can only be slots, and
    // `check_pair_summary` says so in that transform's words (spec §5).
    if layer.transforms.iter().any(|t| matches!(t, Transform::Count | Transform::Proportion)) {
        return;
    }
    if crate::transform::has_reduction(&layer.transforms) {
        return;
    }
    let slotted = |ch: &Channel| {
        spec.position_for(layer, ch)
            .and_then(|c| actual_type(df, &c.field))
            == Some(VarType::Discrete)
    };
    // **A category beside a number is not a cell either.** One category bounds its
    // own axis, and the column highlight lets the panel supply the other, but only
    // when that other axis is left unsaid. Bound to a number, it was dropped:
    // `zone + x(year) + y(country) + color(life)` drew each row as twelve bands
    // across the whole panel, `year` read for the axis and for nothing else, the
    // silent drop §12 forbids. Refused with the three sentences that give the
    // zone sides on that axis. A category scale on the number is refused by
    // `check_scale` in its own words, so it is not said twice here.
    if layer.transforms.is_empty() {
        for (cat, num) in [(Channel::X, Channel::Y), (Channel::Y, Channel::X)] {
            if !slotted(&cat) {
                continue;
            }
            let Some(def) = spec.position_for(layer, &num) else { continue };
            if def.scale == Some(ScaleType::Category)
                || actual_type(df, &def.field) != Some(VarType::Continuous)
            {
                continue;
            }
            let (n, nf) = (channel_name(&num), def.field.as_str());
            let cf = spec.position_for(layer, &cat).map_or("", |c| c.field.as_str());
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: format!(
                    "gog: `{n}({nf})` is a number, and on a `zone` a number is a point with \
                     no width, so it gives the zone no sides on `{n}`: each zone would span \
                     the panel and `{nf}` would be dropped. To cut `{nf}` into ranges, write \
                     `zone * bin * mean + {n}({nf}) + color(<column>)`, which colors each cell \
                     by a mean. For one cell per value, store `{nf}` as text or categories \
                     before plotting, and write `zone * mean` if a cell would hold more than \
                     one row. To shade each `{cf}` slot across the whole panel, leave \
                     `{n}` out of the zone: a position written before the marks reaches every \
                     layer, so if another layer needs `{n}({nf})`, write it after that \
                     layer's mark."
                ),
            });
            return;
        }
    }
    if layer.transforms.contains(&Transform::Bounds)
        || layer.transforms.contains(&Transform::Partition)
        || layer.transforms.contains(&Transform::Flow)
        || reads_a_field(&layer.mark, &layer.transforms, space_of(spec))
        || slotted(&Channel::X)
        || slotted(&Channel::Y)
    {
        return;
    }
    out.push(Diagnostic {
        kind: DiagnosticKind::Illegal,
        // Five, and the fifth is the one this function returns early for above:
        // the message listed four until 2026-09-23 while the book taught five, so a
        // reader holding a coastline was sent to find a rectangle.
        message: "gog: `zone` shades a rectangle, but nothing here says where its sides \
                  are. Five things can: a categorical position, whose category owns a \
                  slot — `zone + x(method) + y(dataset) + color(score)` fills every cell \
                  where two categories cross; `bounds`, which names the sides from \
                  columns you hold — `zone * bounds(lo, hi)` on the measure axis, \
                  `zone * bounds(start = a, end = b)` on the domain axis, all four for a \
                  box, and the axis you leave out spans the panel; `bin`, which cuts them \
                  out of two continuous axes and counts the rows in each cell; \
                  `density`, which cuts the same cells and estimates a value at each; and \
                  a boundary, many rows around one region, named by `group` in `map()` \
                  or `globe()` — `zone + x(lon) + y(lat) + group(<region>) + map()`. A continuous \
                  position on its own is none of them — a number is a point, and a point \
                  has no width.".to_string(),
    });
}

pub(super) fn check_box(out: &mut Vec<Diagnostic>, layer: &Layer) {
    if layer.mark != Mark::Box {
        return;
    }
    let extra: Vec<&str> = layer.transforms.iter()
        .filter(|t| **t != Transform::Box && !is_collision_modifier(t))
        .map(transform_name)
        .collect();
    if !extra.is_empty() {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `box` already summarizes each group into a box — quartiles, median, \
                 whiskers — so it takes no transform, but `{}` was added. Drop it: \
                 `box + x(group) + y(value)` draws the box-and-whisker on its own.",
                extra.join("`, `"),
            ),
        });
    }
    // The one knob: the whisker rule. Anything but the two names is refused with
    // direction rather than quietly treated as the default.
    if let Some(w) = layer.r#box.as_ref().and_then(|b| b.whiskers.as_deref()) {
        if w != "tukey" && w != "range" {
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: format!(
                    "gog: `box(whiskers = \"{w}\")` is not a whisker rule. Use \"tukey\" (the \
                     default — whiskers to 1.5·IQR, points beyond drawn as outliers) or \
                     \"range\" (whiskers to the true minimum and maximum, no outliers)."
                ),
            });
        }
    }
}

/// A `zone`'s numbered sides on a categorical axis are slots, and a side past every
/// slot is a rectangle drawn off the panel.
///
/// `bounds` names two pairs, and which axis each lands on is the bindings' answer
/// (`zone_orient`). On a continuous axis a side is a value, and the axis widens to
/// hold it. On a categorical axis a number is read as a place among the slots, 0
/// for the first category's center, which is how the waterfall and the candlestick
/// draw blocks narrower than their slots (`left = slot - 0.3`). The axis does not
/// widen for those: it runs from half a step before the first slot to half a step
/// after the last, whatever the zone asks. So a side meant for the *other* axis,
/// `bounds(start = lo, end = hi)` with life expectancies over twelve countries on
/// `y`, put each rectangle near slot 70 and drew it some 2,800 pixels above the
/// panel, with nothing said. Refused, per row: a rectangle with any part inside
/// the slots is drawn and clipped, so only one wholly outside is lost.
///
/// Read across layers, because the zone's own table rarely holds the categorical
/// column (the band above is one row of two numbers); the categories are those of
/// every layer that binds the axis to text, the set the renderer lays out.
pub(super) fn check_zone_slots(out: &mut Vec<Diagnostic>, spec: &PlotSpec, data: &HashMap<String, DataFrame>) {
    // The axis's categories, and the column a sentence names for it.
    let slots = |ch: Channel| -> Option<(String, usize)> {
        let mut field = None;
        let mut frames: Vec<&DataFrame> = Vec::new();
        for layer in &spec.layers {
            let Some(cd) = spec.position_for(layer, &ch) else { continue };
            let Some(df) = layer.data.as_ref().or(spec.data.as_ref()).and_then(|n| data.get(n))
            else { continue };
            if df.str_col(&cd.field).is_some() && df.time_unit(&cd.field).is_none() {
                field.get_or_insert_with(|| cd.field.clone());
                frames.push(df);
            }
        }
        let field = field?;
        let n = crate::data::categories_across(&frames, &field).len();
        (n > 0).then_some((field, n))
    };
    let (cat_x, cat_y) = (slots(Channel::X), slots(Channel::Y));
    if cat_x.is_none() && cat_y.is_none() {
        return;
    }
    let turned = zone_orient(cat_x.is_some(), cat_y.is_some()) == Orient::Horizontal;
    for layer in &spec.layers {
        if layer.mark != Mark::Zone {
            continue;
        }
        let Some(b) = layer.bounds.as_ref() else { continue };
        let Some(df) = layer.data.as_ref().or(spec.data.as_ref()).and_then(|n| data.get(n))
        else { continue };
        // (pair, its names as written, the axis it lands on)
        let pairs = [
            (b.measure(), ("lower", "upper"), if turned { Channel::X } else { Channel::Y }),
            (b.domain(), ("start", "end"), if turned { Channel::Y } else { Channel::X }),
        ];
        for (pair, (ka, kb), ch) in pairs {
            let Some((a, c)) = pair else { continue };
            let Some((field, n)) = (if ch == Channel::X { &cat_x } else { &cat_y }).clone()
            else { continue };
            let (Some(va), Some(vc)) = (df.float_col(a), df.float_col(c)) else { continue };
            let edge = n as f64 - 0.5;
            let outside: Vec<usize> = (0..va.len().min(vc.len()))
                .filter(|&i| {
                    let (lo, hi) = (va[i].min(vc[i]), va[i].max(vc[i]));
                    lo.is_finite() && hi.is_finite() && (hi < -0.5 || lo > edge)
                })
                .collect();
            let Some(&first) = outside.first() else { continue };
            let axis = channel_name(&ch);
            let other = if ch == Channel::X { Channel::Y } else { Channel::X };
            let fmt = |v: f64| {
                let s = format!("{v:.2}");
                s.trim_end_matches('0').trim_end_matches('.').to_string()
            };
            let (total, k) = (va.len(), outside.len());
            let (x, y) = (fmt(va[first]), fmt(vc[first]));
            // "The first" alone read as the table's first row, which may be inside.
            let lost = match (total, k) {
                (1, _) => format!("Its one row lies outside every slot: it runs from {x} to \
                                   {y}, so the zone would be drawn off the panel."),
                (t, 1) => format!("One of its {t} rows lies outside every slot (it runs from \
                                   {x} to {y}), so that rectangle would be drawn off the panel."),
                (t, k) if k == t => format!("All {t} of its rows lie outside every slot (the \
                                   first of them runs from {x} to {y}), so the zone would be \
                                   drawn off the panel."),
                (t, k) => format!("{k} of its {t} rows lie outside every slot (the first of \
                                   them runs from {x} to {y}), so those rectangles would be \
                                   drawn off the panel."),
            };
            // The likely meaning, when there is one: a measure pair left unnamed and
            // a continuous axis beside this one, which is what the numbers fit.
            let measure_free = b.measure().is_none() && ka == "start"
                && (if other == Channel::X { &cat_x } else { &cat_y }).is_none();
            let o = channel_name(&other);
            let values = spec.layers.iter()
                .find_map(|l| spec.position_for(l, &other))
                .map_or(format!("values on `{o}`"), |cd| format!("values of `{}`, on `{o}`", cd.field));
            let slots = if n == 1 {
                "keep them between -0.5 and 0.5, the outer edges of the slot".to_string()
            } else {
                format!("keep them between -0.5 and {}, the outer edges of the first and the \
                         last slot", fmt(edge))
            };
            let advice = if measure_free {
                format!("If `{a}` and `{c}` are {values}, they bound the measure axis: write \
                         `bounds({a}, {c})`. If they are slot positions, {slots}.")
            } else {
                format!("To place the zone among the slots, {slots}.")
            };
            let holds = if n == 1 {
                format!("holds one category, `{field}`'s only value, so the pair's numbers are \
                         slot positions: 0 is that category's slot")
            } else {
                format!("holds the {n} categories of `{field}`, so the pair's numbers are slot \
                         positions: 0 is the first category's slot and {} the last's", n - 1)
            };
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: format!(
                    "gog: `bounds({ka} = {a}, {kb} = {c})` places the zone on `{axis}`, and \
                     `{axis}` {holds}. {lost} {advice}",
                ),
            });
            return;
        }
    }
}

/// `line`, `area` and `step` join their rows in x order, and rows that share one
/// `x` value make the stroke or the outline zigzag inside it.
///
/// `area + x(continent) + y(life)` over a year's countries drew a 145-vertex
/// outline running up and down inside each continent's slot, and `step` did the
/// same. Said as an Assumption, and only when it happens: some `x` value holds
/// more than one row within one panel at one moment, and nothing splits or
/// summarizes them. `line` said it from the renderer on its row count, so every
/// single series of more than five points heard it, 23 of the 26 sentences in
/// the book that printed it, and so did one country's rows under
/// `facet(country)`; it asks this question now. Rows are counted within a panel
/// and a moment because that is where they are joined.
pub(super) fn check_joined_rows(out: &mut Vec<Diagnostic>, spec: &PlotSpec, df: &DataFrame, layer: &Layer) {
    if !matches!(layer.mark, Mark::Line | Mark::Area | Mark::Step) || !layer.transforms.is_empty() {
        return;
    }
    // What splits the rows into series: a group, a pattern, or a color of
    // categories. A color of numbers ramps along the one stroke instead.
    let ramped = layer.encodings.get(&Channel::Color).filter(|c| df.float_col(&c.field).is_some());
    if layer.encodings.contains_key(&Channel::Group)
        || layer.encodings.contains_key(&Channel::Pattern)
        || (layer.encodings.contains_key(&Channel::Color) && ramped.is_none())
    {
        return;
    }
    let Some(x) = spec.position_for(layer, &Channel::X) else { return };
    let facet = spec.facet.as_ref();
    let apart: Vec<&str> = [
        facet.and_then(|f| f.col.as_deref()),
        facet.and_then(|f| f.row.as_deref()),
        layer.encodings.get(&Channel::Play).map(|c| c.field.as_str()),
        Some(x.field.as_str()),
    ].into_iter().flatten().collect();
    let cell = |field: &str, i: usize| -> Option<String> {
        match (df.str_col(field), df.float_col(field)) {
            (Some(vals), _) => vals.get(i).cloned(),
            (_, Some(vals)) => vals.get(i).filter(|v| v.is_finite()).map(|v| v.to_bits().to_string()),
            _ => None,
        }
    };
    let mut counts: HashMap<Vec<String>, usize> = HashMap::new();
    for i in 0..df.len() {
        let key: Option<Vec<String>> = apart.iter().map(|f| cell(f, i)).collect();
        if let Some(key) = key {
            *counts.entry(key).or_default() += 1;
        }
    }
    let most = counts.values().copied().max().unwrap_or(0);
    if most < 2 {
        return;
    }
    let m = mark_name(&layer.mark);
    let (what, ramp) = match (&layer.mark, ramped) {
        (Mark::Line, Some(c)) => ("the stroke", format!(
            " `color({})` holds numbers, so it colors along the stroke rather than \
             splitting it.", c.field)),
        (Mark::Line, None) => ("the stroke", String::new()),
        _ => ("the outline", String::new()),
    };
    let split = if ramped.is_some() { "`group(<field>)`" } else { "`color(<field>)` or `group(<field>)`" };
    out.push(Diagnostic {
        kind: DiagnosticKind::Assumption,
        message: format!(
            "gog: up to {most} rows of `{m}` share one value of `x({f})`, and nothing splits \
             them, so they are joined in x order and {what} zigzags inside that value.{ramp} \
             If you have several series, add {split}; for one value per `x`, summarize them \
             with `{m} * mean`.",
            f = x.field,
        ),
    });
}
