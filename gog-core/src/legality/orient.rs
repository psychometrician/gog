//! Orientation: which axis a slot mark stands on, read off the types of its two
//! positions.

use super::*;

// ---------------------------------------------------------------------------
// Orientation
//
// A **slot mark** has a *position* axis it sits on and an *extent* axis it
// measures along. Which is which is read off the bindings — there is no `flip`
// atom, because that would be a second way to say one thing:
//
//     bar + x(country) + y(gold)    vertical
//     bar + x(gold) + y(country)    horizontal
//
// This is the first rule that cannot be expressed per channel. `rule_for` sees
// one channel at a time and can only say "y must be continuous"; the real
// constraint is "exactly one of x/y measures, and that one must be a number".
// So the slot marks relax `rule_for(_, Y)` to Either and the pair is judged here.
//
// The family is the three marks that **stand in a slot** — `bar`, `box`,
// `interval` — which is the same set `dodge` subdivides, and not a coincidence:
// having a slot to sit in is exactly what makes a mark dodgeable *and* what
// makes it orientable. `bar` had this from the start; `box` and `interval` were
// pinned to a vertical reading until 2026-07-24, which was a Law-2 gap rather
// than a difference in kind. The test that settles such a question is spec §6's:
// **do these two axes have the same role?** For a slot mark they do: one axis
// holds the slots and the other the measure, and which plays which is a fact
// about the bindings, not about the mark. Same roles, so the same rule; hence one
// function for all three.
//
// A *path* (`line`/`step`/`area`/`ribbon`) has a domain it is read along and a
// measure it traces, and until 2026-10-23 the domain was always `x`. The author
// then ruled that the profile plot goes both ways, as the dot plot does: a
// category on `y` makes `y` the domain, and the profile runs down the page.
// Without a category nothing says which axis to read along, so `x` stays the
// domain, a date on `y` included (`path_orient`).
// ---------------------------------------------------------------------------

/// The marks that stand in a slot on one axis and extend along the other.
///
/// The orientable family, and the same set `dodge` subdivides (see the note
/// above). A path or a glyph is absent: neither has a slot, so neither has an
/// orientation to read.
pub fn is_slot_mark(mark: &Mark) -> bool {
    matches!(mark, Mark::Bar | Mark::Box | Mark::Interval)
}

/// The marks read along a domain and traced along a measure: the path/region
/// family, whose domain axis is read off the pair by [`path_orient`].
pub fn is_path_mark(mark: &Mark) -> bool {
    matches!(mark, Mark::Line | Mark::Step | Mark::Area | Mark::Ribbon)
}

/// Does this layer read its orientation off the types of its two positions?
///
/// The three slot marks always do. A `point` that reduces does too, since a
/// summary dot is a bar's end drawn as a dot (Law 2): `point * median + x(life) +
/// y(year)` summarizes along `x`, one dot per year, as `bar * median` draws one bar
/// per year. Read as vertical, it grouped by the numeric `x`, every row its own
/// group, and drew all 1,704 raw rows as if summarized.
///
/// **A `text` that counts or summarizes reads it too**, for the same reason: its
/// label is printed at a bar's end. Read as vertical, `text * count +
/// y(continent)` beside horizontal bars was refused for naming `y`, and `text *
/// mean + y(continent) + x(life)` grouped by the numeric `x` and printed all 142
/// raw values.
///
/// **And a `point` that counts, as a `text` does.** The dot is drawn where the
/// label is printed, so the two marks take the same transforms both ways (Law 2).
/// `point` had the reductions alone, and `point * count + y(continent)`, the
/// horizontal dot plot `point * bin * stack + y(life)` and their kin were refused
/// for naming `y`, while `bar` and `text` drew them on their side.
///
/// **And a `text` that piles, with no statistic** (2026-11-06). `stack` piles along
/// the measure axis, which is `x` when the category is on `y`; read as vertical,
/// `text * stack + y(stage) + x(n)` grouped by the numeric `x` and left every label
/// at its plain value beside the stacked bars it was written for, where `text * sum
/// * stack` and the upright plot labeled each segment's middle.
pub fn reads_orientation(layer: &Layer) -> bool {
    let ts = &layer.transforms;
    let summarizes = crate::transform::has_reduction(ts)
        || ts.iter().any(|t| matches!(t, Transform::Count | Transform::Bin
            | Transform::Proportion | Transform::Density | Transform::Stack
            | Transform::Change));
    // A path reads it too, except a `density` one, whose violin reading has its
    // own authority (`slot_density`) and whose field reading has no slot at all.
    let path = is_path_mark(&layer.mark) && !ts.contains(&Transform::Density);
    is_slot_mark(&layer.mark) || path
        || (matches!(layer.mark, Mark::Point | Mark::Text) && summarizes)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Orient {
    /// The marks stand on the x axis and measure along y. The default.
    Vertical,
    /// The marks sit on the y axis and measure along x.
    Horizontal,
}

/// Decide orientation from the types of the two bound columns.
///
/// `None` means the axis has no column of its own in the input — a synthesizing
/// transform is about to write one there, which makes it the measured axis.
///
/// One rule for all three slot marks: a bar's length, a whisker's span and a
/// box's summary are the same question asked of the same pair of axes.
pub fn slot_orient(x: Option<VarType>, y: Option<VarType>) -> Orient {
    use Orient::*;
    use VarType::{Continuous, Discrete};
    match (x, y) {
        // Two categorical axes measure nothing. `check_slot_shape` refuses it;
        // the value returned here is only what the renderer would fall back to.
        (Some(Discrete), Some(Discrete)) => Vertical,
        // A categorical axis is one the marks sit on.
        (_, Some(Discrete)) => Horizontal,
        (Some(Discrete), _) => Vertical,
        // Neither is categorical: the axis with no column of its own is the one
        // a transform is filling in, so it is the measure. `bar * bin + y(h)`
        // is a histogram lying on its side.
        (None, Some(Continuous)) => Horizontal,
        // Both continuous, or nothing bound — keep the long-standing default.
        _ => Vertical,
    }
}

/// [`slot_orient`] for two columns of a table, with **a date read as a slot**
/// against a plain number (ruled 2026-10-11, the consistency audit's F7). A
/// moment in time is never a bar's length, so `bar + x(orders) + y(day)` lies on
/// its side as `bar + x(orders) + y(dept)` does. Read as two numbers it stood
/// upright, made the date its length, and was refused for it with the direction
/// to move the date to `x`. Against a category or another date nothing changes:
/// neither is a length either, and the refusals there name the real fault.
///
/// Every place that asks which way a layer stands asks this, so the check, the
/// renderer and the stack's key cannot read a date two ways.
pub fn columns_orient(df: &DataFrame, x_field: &str, y_field: &str) -> Orient {
    let (xt, yt) = (actual_type(df, x_field), actual_type(df, y_field));
    let dated = |f: &str| df.time_unit(f).is_some();
    if xt == Some(VarType::Continuous) && yt == Some(VarType::Continuous)
        && dated(y_field) && !dated(x_field)
    {
        return Orient::Horizontal;
    }
    slot_orient(xt, yt)
}

/// Which axis a **path** is read along, from the types of its two positions.
///
/// `Horizontal` means the domain is `y`: the category is on `y` and the profile
/// runs down the page, the measure on `x`. That is the only case. It is
/// [`slot_orient`] without its two clauses that read a continuous pair: an unbound
/// `x` beside a number on `y` is not a profile on its side but a sentence missing
/// its domain, and a date on `y` is a measure-against-time plot read along `x`, as
/// every line with two continuous positions is (the author, 2026-10-23: only a
/// category turns a line). Two categories measure nothing; `check_slot_shape`
/// refuses that, and `Vertical` is only the fallback.
pub fn path_orient(x: Option<VarType>, y: Option<VarType>) -> Orient {
    match (x, y) {
        (Some(VarType::Discrete), Some(VarType::Discrete)) => Orient::Vertical,
        (_, Some(VarType::Discrete)) => Orient::Horizontal,
        _ => Orient::Vertical,
    }
}

/// [`path_orient`] for one layer: its two columns' types, and one fact the types
/// cannot carry. **Under `bounds` the measure is the two columns `bounds` names**,
/// so a column bound on `x` can only be the domain, and the layer stands upright
/// whatever `y` holds; a category there is then a name on the measure axis, and
/// `synth_axis_names_a_column` refuses it. Read as turned instead, `x(at)` would
/// have been overruled by `bounds` without a word. With no `x`, a band from
/// `bounds` turns down a category on `y` like any other path.
pub(super) fn path_layer_orient(layer: &Layer, df: &DataFrame, x_field: &str, y_field: &str) -> Orient {
    let xt = actual_type(df, x_field);
    if layer.transforms.contains(&Transform::Bounds) && xt.is_some() {
        return Orient::Vertical;
    }
    path_orient(xt, actual_type(df, y_field))
}

/// The orientation **this layer** reads off its two columns: [`path_orient`] for a
/// path, [`columns_orient`] for everything else. Asked only of a layer that
/// [`reads_orientation`]; every place that turns the key to `y` asks this, so a
/// path and a bar can never be read by each other's rule.
pub fn layer_orient(layer: &Layer, df: &DataFrame, x_field: &str, y_field: &str) -> Orient {
    if is_path_mark(&layer.mark) {
        path_layer_orient(layer, df, x_field, y_field)
    } else {
        columns_orient(df, x_field, y_field)
    }
}

/// Is this layer a **turned path**: a `line`/`step`/`area`/`ribbon` read down a
/// category on `y` (`path_orient`)? The renderer asks this, as it asks
/// [`slot_density`], rather than reading column types itself.
pub fn path_turned(spec: &PlotSpec, layer: &Layer, df: &DataFrame) -> bool {
    if !is_path_mark(&layer.mark) || layer.transforms.contains(&Transform::Density) { return false }
    let field = |ch| spec.position_for(layer, &ch).map_or("", |c| c.field.as_str());
    path_layer_orient(layer, df, field(Channel::X), field(Channel::Y)) == Orient::Horizontal
}

/// Which axis a **bounded `zone`**'s measure pair lands on — `Horizontal` when
/// `bounds(lower, upper)` bounds `x` and the slot on `y` bounds the other side.
///
/// [`crate::ir::BoundsSpec`] names a *measure* pair and a *domain* pair, never a
/// screen direction: "left"/"right" would bake in horizontality, and this grammar
/// reads orientation off the bindings and bends into polar, where the domain axis
/// is an angle and "left" means nothing. So the mark has to **ask** which axis is
/// which, exactly as `bar`/`box`/`interval` do and for the reason there is no
/// `flip` atom (§6). Until 2026-07-28 nothing asked: `lower`/`upper` went to `y`
/// and `start`/`end` to `x` whatever the columns held, in two places that had to
/// agree and both guessed the same way — so `zone * bounds(lo, hi) + y(stage)`
/// put the measure on the categorical axis, left `x` with no column to fit, and
/// drew its rectangles thousands of pixels off-panel under a fabricated `0.0 … 1.0`
/// axis. Silently. `interval * bounds(lo, hi) + y(stage)` — the same sentence, one
/// mark over — had been drawing it correctly the whole time, which is what a Law 2
/// exception looks like from the outside.
///
/// **Read off which axis owns a slot, and nothing else.** That is where this parts
/// from [`slot_orient`]'s unbound-axis clause, and it has to: that clause reads a
/// missing column as "the axis a synthesizing transform is about to write", which
/// is `bar * bin + y(h)`, the histogram on its side. For this mark a missing column
/// is the axis the **panel** supplies, which is the whole of what `zone` is for —
/// so an axis that is not a slot tells us nothing about orientation, and is passed
/// as `None` to say exactly that. One authority all the same: the truth table is
/// `slot_orient`'s, asked with the only question a zone can answer.
///
/// Taken as *is this axis categorical* rather than as a column type, because the
/// two callers that must not disagree — `build_axis`'s range and `write_zone`'s
/// rectangles — both hold the axis's own category order and only one holds a
/// frame. A mesh's synthesized sides never reach here: `cell_bounds()` publishes
/// the edges each axis was **cut** on, already assigned to an axis, and only a
/// spec the *sentence* named is a measure/domain pair at all.
pub fn zone_orient(cat_x: bool, cat_y: bool) -> Orient {
    slot_orient(
        cat_x.then_some(VarType::Discrete),
        cat_y.then_some(VarType::Discrete),
    )
}

/// The **violin**: `density` handed a categorical position and a continuous one
/// (spec §5). `None` for every other layer, including every other reading of
/// `density`.
///
/// One authority, asked by six callers that would otherwise each decide for
/// themselves what a violin is — the two refusals that have to stand aside for it
/// (`check_span_needs_range`, `check_distribution_axis`), the parameter check that
/// owns `compare`, the binding-type exemption, the renderer's choice of which axis
/// the transform groups by, and the dispatch that draws it. The `zone`/`path` field
/// reading is what happens when the *same* transform meets two continuous
/// positions, and it is decided by `has_no_measure_axis` for exactly this reason:
/// one question about the sentence, answered in one place, so no two callers can
/// disagree about which reading they are in.
///
/// **Why the mark is `ribbon` and `area` rather than a `violin` atom.** §5's growth
/// test asks whether a proposed atom *derives* or *enumerates*, and the answer here
/// is written in the two marks' existing definitions. A `ribbon` closes on a second
/// data boundary; a violin closes on its own reflection, which is one. An `area`
/// closes on a baseline; a half-violin closes on the slot's center line, which is
/// one. So the geometry is already named twice over, and a `violin` atom would be
/// four things under one word — the mark, the statistic, the mirror, and the slot —
/// which is the enumeration §5 exists to refuse. The statistic stays composed
/// (`* density`) rather than constitutive the way a `box`'s summary is, and the test
/// that separates them is the one `box` recorded: a transform that combines with
/// exactly one mark is constitutive of it, and `density` combines with seven.
///
/// The returned orientation is the layer's own, read off the bindings exactly as
/// `slot_orient` reads a bar's — which is why there is still no `flip` atom (§6).
pub fn slot_density(spec: &PlotSpec, layer: &Layer, df: Option<&DataFrame>) -> Option<Orient> {
    if !takes_slot_reading(&layer.mark) { return None }
    if !layer.transforms.contains(&Transform::Density) { return None }
    let df = df?;
    let typ = |ch| spec.position_for(layer, &ch).and_then(|c| actual_type(df, &c.field));
    match (typ(Channel::X), typ(Channel::Y)) {
        (Some(VarType::Discrete), Some(VarType::Continuous)) => Some(Orient::Vertical),
        (Some(VarType::Continuous), Some(VarType::Discrete)) => Some(Orient::Horizontal),
        _ => None,
    }
}

/// **The marks a violin can be drawn with**: the four that draw the estimate as
/// one connected curve, filled or not. Asked by [`slot_density`], and by the two
/// refusals of a `point`, `bar` or `text` given the violin's bindings, so the
/// marks that draw it and the marks those refusals name cannot drift apart.
pub fn takes_slot_reading(mark: &Mark) -> bool {
    // Four marks, and they are the same four that draw a band: `ribbon` fills the
    // estimate mirrored, `area` fills it against the slot's line, and `line`/`step`
    // **trace** it — the "filled, or two edges" rule this chapter of the grammar
    // already runs on (`line * bounds`), read against a slot instead of an axis.
    //
    // Adding the two stroke marks also closed a silent misdraw rather than only
    // adding a capability. `line * density + x(life) + y(continent)` was legal
    // before and drew the *pooled* curve, because a bound `y` on a synthesizing
    // transform names the output column — so the category was swallowed as a name
    // and the axis came out reading "Continent" over density values. It looked
    // exactly like the stroke half of the ridgeline and was not it.
    matches!(mark, Mark::Area | Mark::Ribbon | Mark::Line | Mark::Step)
}

/// How far, **in slots**, this plot's violins overhang the first and last category
/// on `channel` — what the categorical axis has to grow by to contain them.
///
/// `(0.0, 0.0)` unless something overhangs, which is every plot but a violin's: a
/// bar, a box and a whisker all stand *in* their slot, and half a slot each side
/// holds them. A violin need not, and at `density(reach = 2.5)` deliberately does
/// not — so the axis is told, and the ridgeline's top row stops being clipped by
/// the frame.
///
/// Read off the same `slot_density` the renderer dispatches on, so the axis and
/// the mark cannot disagree about which layers overhang or by how much; and asked
/// per *channel*, because which axis carries the slot is the layer's own answer.
/// A `ribbon` reaches both ways and everything else one way, which is the mirror
/// exactly as `render::marks::violin` draws it.
pub fn slot_reach(
    spec: &PlotSpec, data: &HashMap<String, DataFrame>, channel: Channel,
) -> (f64, f64) {
    let (mut lo, mut hi) = (0.0f64, 0.0f64);
    for layer in &spec.layers {
        let df = layer.data.as_ref().or(spec.data.as_ref()).and_then(|n| data.get(n));
        let Some(orient) = slot_density(spec, layer, df) else { continue };
        // Which axis this layer's slots are on. Vertical violins stand on `x`.
        let slot_channel = match orient { Orient::Vertical => Channel::X, Orient::Horizontal => Channel::Y };
        if slot_channel != channel { continue }
        let reach = layer.density.as_ref().and_then(|d| d.reach)
            .filter(|r| r.is_finite() && *r > 0.0)
            .unwrap_or(crate::ir::DEFAULT_REACH);
        hi = hi.max(reach);
        if layer.mark == Mark::Ribbon { lo = lo.max(reach); }
    }
    (lo, hi)
}

/// The plot's orientation, taken from its first slot-mark layer.
///
/// Layers share one coordinate space, so a plot has a single orientation. A
/// plot with no slot mark is `Vertical`; nothing reads it.
pub fn plot_orient(spec: &PlotSpec, data: &HashMap<String, DataFrame>) -> Orient {
    // **A packing has no orientation to read**, because the space fixes what each
    // position means before any layer is consulted: `y` is the measure it turns
    // into an area and `x` is the outer partition (spec §15). So a bar here is
    // never on its side, split or no split — which is the *unconditional* form of
    // the pie's rule below, and had been getting the conditional one.
    //
    // Found by Law 7's third relaxation, immediately: once `x` stopped being
    // required here, `bar + y(population) + nest()` with no `color` became legal,
    // and `slot_orient` read the unbound `x` as "the axis a transform will fill
    // in" — so the measure became the *key*, `float_col("")` found nothing, and
    // the plot drew an **empty panel**. Exactly the failure the pie's guard was
    // written for, reached from the other side.
    if space_of(spec) == SpaceKind::Nest {
        return Orient::Vertical;
    }
    // A cluster tree's orientation is its leaf axis's: leaves on `y` lay the
    // tree on its side, and the distance axis — the measured one — is `x`,
    // exactly as a lying-down histogram's count is.
    for layer in &spec.layers {
        if layer.mark == Mark::Path && layer.transforms.contains(&Transform::Cluster) {
            let sideways = spec.position_for(layer, &Channel::Y).is_some()
                && spec.position_for(layer, &Channel::X).is_none();
            return if sideways { Orient::Horizontal } else { Orient::Vertical };
        }
    }
    for layer in &spec.layers {
        if !reads_orientation(layer) {
            continue;
        }
        // Read against the columns *this* layer reads, since it may name its own
        // (spec §8). The orientation is still the plot's — one coordinate space
        // — but the types it is read from live in the layer's table.
        let x_field = spec.position_for(layer, &Channel::X).map(|c| c.field.as_str()).unwrap_or("");
        let y_field = spec.position_for(layer, &Channel::Y).map(|c| c.field.as_str()).unwrap_or("");
        // A bar with no position axis has no orientation to read: there is one
        // slot and the split divides it, so the bound column is the measure and it
        // is on `y`. Decided *before* `slot_orient`, because that function's
        // unbound-x rule ("the axis with no column of its own is the one a
        // transform is filling in") is about `bar * bin + y(h)`, a histogram lying
        // on its side. Here x is unbound for the opposite reason — nothing is
        // going to fill it — and reading it as horizontal made the measure the
        // *key*, so the statistic found no column to summarize and the pie came out
        // empty.
        let df = layer.data.as_ref().or(spec.data.as_ref()).and_then(|name| data.get(name));
        if bar_is_one_pile(spec, layer, df) {
            return Orient::Vertical;
        }
        if bar_is_one_pile_sideways(spec, layer, df) {
            return Orient::Horizontal;
        }
        let Some(df) = df else {
            continue;
        };
        // **A path votes only when it is turned.** An upright path says nothing a
        // later bar could not override, and letting it vote would make a plot's
        // direction depend on which layer was written first. A turned one is a
        // category on `y`, which turns a bar the same way; a plot whose layers
        // still disagree is refused (`check_path_direction`).
        if is_path_mark(&layer.mark) {
            match path_layer_orient(layer, df, x_field, y_field) {
                Orient::Horizontal => return Orient::Horizontal,
                Orient::Vertical => continue,
            }
        }
        return columns_orient(df, x_field, y_field);
    }
    Orient::Vertical
}

/// The **direction of each path against the plot's**, and a turned path in polar.
/// A path layer whose own
/// [`path_orient`] differs from [`plot_orient`] would be keyed on one axis and
/// read along the other. Only a date or an unbound axis gets here, since a
/// category turns a bar and a path alike; the author ruled it refused
/// (2026-10-23), naming which layer faces which way.
pub(super) fn check_path_direction(
    out: &mut Vec<Diagnostic>, spec: &PlotSpec, data: &HashMap<String, DataFrame>,
) {
    let plot = plot_orient(spec, data);
    for layer in &spec.layers {
        if !is_path_mark(&layer.mark) || !reads_orientation(layer) { continue }
        let Some(df) = layer.data.as_ref().or(spec.data.as_ref()).and_then(|n| data.get(n)) else { continue };
        let field = |ch| spec.position_for(layer, &ch).map(|c| c.field.clone()).unwrap_or_default();
        let (xf, yf) = (field(Channel::X), field(Channel::Y));
        let own = path_layer_orient(layer, df, &xf, &yf);
        // **A turned profile does not bend into polar** (the author, 2026-10-23:
        // refuse for now, point to the radar). Bent, the measure would go round the
        // circle and the categories out along the radius; the radar is the same
        // profile with the category on `x`, already drawn.
        if own == Orient::Horizontal && matches!(spec.coord, CoordSpace::Polar(_)) {
            let mark = mark_name(&layer.mark);
            let chain = std::iter::once(mark)
                .chain(layer.transforms.iter().map(transform_name))
                .collect::<Vec<_>>().join(" * ");
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: format!(
                    "gog: `{mark}` is read down `y` here, because `y({yf})` is categorical, \
                     and a profile turned that way does not bend into `polar()`. Put the \
                     category on `x`: `{chain} + x({yf}) + y({xf}) + polar()` is the radar."),
            });
            continue;
        }
        if own == plot { continue }
        let mark = mark_name(&layer.mark);
        // The direction depends on why the two disagree: a date on `y` has its own
        // fixes, the date on `x` for both layers, or, for a stroke, `path` through the
        // outer ends of the horizontal bars. The message says what the path mark
        // draws alone, since "upright" left a reader asking whether it could be drawn
        // at all (2026-10-23), and it says horizontal where it said a bar "lies on its
        // side": a mark is drawn, it does not lie or stand.
        let along = if xf.is_empty() { "`x`".to_string() } else { format!("`x({xf})`") };
        let message = match own {
            Orient::Horizontal => format!(
                "gog: `{mark}` is read down `y` here, because `y({yf})` is categorical, but \
                 another layer of the plot is read along `x`. One plot is read in one \
                 direction. Put the category on the same axis in both layers, or draw them as \
                 two plots side by side with `|`."),
            Orient::Vertical if df.time_unit(&yf).is_some() => {
                let (runs, stroke) = match layer.mark {
                    Mark::Line | Mark::Step => ("joins its points", true),
                    _ => ("runs", false),
                };
                let path = if stroke {
                    format!(" To keep the bars horizontal and join their outer ends day by day, \
                             use `path`, which joins the rows in the order the table holds them, \
                             so keep the table sorted by `{yf}`.")
                } else { String::new() };
                format!(
                    "gog: a date on `y` draws a bar horizontally, but `{mark}` still {runs} in \
                     order of {along}, because only a category turns `{mark}`. One plot is read \
                     in one direction. Put the date on `x` for both layers, and both are read \
                     along `x`.{path} Or draw the two as plots side by side with `|`.")
            }
            Orient::Vertical => format!(
                "gog: another layer of the plot is drawn horizontally, but `{mark}` has no \
                 category on `y`, so it is read along `x`. One plot is read in one direction. \
                 Give both layers the same direction, or draw them as two plots side by side \
                 with `|`."),
        };
        out.push(Diagnostic { kind: DiagnosticKind::Illegal, message });
    }
}
