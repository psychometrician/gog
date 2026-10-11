//! The orthogonality matrix the book's grids are generated from: which mark
//! takes each setting, transform and coordinate space.

use super::*;

// ---------------------------------------------------------------------------
// The rules matrix — a live dump of `rule_for` for the book's grids
//
// The Mark × Channel grid in the book is *generated* from this, not hand-typed,
// so it cannot drift the way the settable grid once shipped missing two marks.
// `gog-cli --rules` serializes it to JSON; a live book chunk renders it as a
// table. Every value here comes from `rule_for`, the one source of truth — this
// only iterates it and names the parts for the wire.
// ---------------------------------------------------------------------------

/// Every mark, in the kernel's teaching order (grammar.qmd's kernel block). One
/// shared list behind the generated grid, the drift test, and
/// `every_mark_channel_pair_has_a_rule`, so none of the three can disagree about
/// the atom set. `mark_name` is the total, compiler-checked match that forces a
/// *new* mark to be handled everywhere it must be.
pub const ALL_MARKS: [Mark; 14] = [
    Mark::Point, Mark::Line, Mark::Area, Mark::Bar, Mark::Step, Mark::Interval,
    Mark::Box, Mark::Ribbon, Mark::Text, Mark::Path, Mark::Rule, Mark::Zone,
    Mark::Surface,
 Mark::Edge,
];

/// Every channel, in the kernel's teaching order.
pub const ALL_CHANNELS: [Channel; 11] = [
    Channel::X, Channel::Y, Channel::Z, Channel::Color, Channel::Size,
    Channel::Shape, Channel::Pattern, Channel::Opacity, Channel::Group,
    Channel::Label, Channel::Play,
];

/// The `style()` settings the book's Mark × Setting grid shows, in row order.
/// Five are the *settable side of a channel* — their legality **is**
/// `rule_for(_).settable`. Five are *style-only* (`border_*`/`caps`/`center`/
/// `nudge`), with no channel of their own; their mark-legality is named in
/// `mark_takes_setting` below and nowhere else.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Setting {
    Color, Opacity, Size, Shape, Pattern,
    BorderColor, BorderSize, Caps, Center, Nudge, Arrow, Reach, Angle, Gap,
}

pub(super) const ALL_SETTINGS: [Setting; 14] = [
    Setting::Color, Setting::Opacity, Setting::Size, Setting::Shape, Setting::Pattern,
    Setting::BorderColor, Setting::BorderSize, Setting::Caps, Setting::Center, Setting::Nudge,
    Setting::Arrow, Setting::Reach, Setting::Angle, Setting::Gap,
];

pub(super) fn setting_name(s: Setting) -> &'static str {
    match s {
        Setting::Color => "color",
        Setting::Opacity => "opacity",
        Setting::Size => "size",
        Setting::Shape => "shape",
        Setting::Pattern => "pattern",
        Setting::BorderColor => "border_color",
        Setting::BorderSize => "border_size",
        Setting::Caps => "caps",
        Setting::Center => "center",
        Setting::Nudge => "nudge",
        Setting::Arrow => "arrow",
        Setting::Reach => "reach",
        Setting::Angle => "angle",
        Setting::Gap => "gap",
    }
}

/// Does this mark carry this setting? The **one** answer the `check_*` refusals
/// and the generated Mark × Setting grid both read, so the grid can never promise
/// a setting the engine refuses (the settable rule, spec §4: a setting spans its
/// geometry class). The channel-backed five defer to `rule_for(_).settable` — the
/// same flag `check_style` gates on; the style-only five name their geometry class
/// here, the sets `check_border`/`check_caps`/`check_center`/`check_nudge` enforce.
pub(super) fn mark_takes_setting(mark: &Mark, setting: Setting) -> bool {
    use Setting::*;
    match setting {
        Color => rule_for(mark, &Channel::Color).settable,
        Opacity => rule_for(mark, &Channel::Opacity).settable,
        Size => rule_for(mark, &Channel::Size).settable,
        Shape => rule_for(mark, &Channel::Shape).settable,
        Pattern => rule_for(mark, &Channel::Pattern).settable,
        // The closed-glyph fills — a rim on a fillable shape. A `surface`'s face is
        // one, and there the rim is the **mesh line**: the settable rule (§4) spanning
        // a setting across its geometry class, landing on the reading a surface plot
        // has always wanted. It cost no machinery — the writer already strokes each
        // face in that face's own shade to close the seam antialiasing leaves between
        // abutting polygons, so this overrides a stroke rather than adding one, and
        // `border_size = 0` gives a seamless sheet.
        //
        // **`zone` joined them 2026-07-27**, reversing the ruling the treemap entry
        // had recorded as settled (spec §15). The refusal's premise — "it marks a
        // region rather than drawing a frame" — described the mark's *first* reading,
        // a highlight behind a line, and the mark has since become the datum itself
        // four times over: the waterfall, the heatmap's cells, the icicle, and the
        // mosaic, which is what forced it. `partition` is `zone`-only, so there was
        // no `bar` route to take the way `nest()` had one, and a mosaic whose cells
        // have no edges is one blob wherever two neighbors share a color (measured
        // 2026-07-27). What still holds is the *other* half of the settable rule:
        // `area`/`ribbon` stay out because one boundary curve is genuinely drawn
        // better by composition (`area + line`), where a region's four sides would be
        // four `rule`s per cell and there are as many cells as the data has.
        //
        // `text` joined 2026-09-26, reversing its "glyphs, not a filled shape — it has
        // no border" refusal. A letter *is* a filled glyph with a perimeter, and its
        // border is realized the way a glyph can carry one: stroked under the fill,
        // so it clears ground around the letters without thinning them (the halo).
        // The engine already drew exactly that for the cube's frame labels, and a
        // label over lines, contours or dense points had no way to ask for it.
        BorderColor | BorderSize =>
            matches!(mark, Mark::Bar | Mark::Box | Mark::Point | Mark::Surface | Mark::Zone
                           | Mark::Text),
        // The crossbars at a whisker's ends, on both marks that draw whiskers: a
        // `box`'s are the same strokes as an `interval`'s, so the settable rule
        // spans them (spec §4). It was `interval`-only until 2026-09-24, and the
        // refusal on a `box` said "a `box` has none" over a box drawing caps.
        Caps => matches!(mark, Mark::Interval | Mark::Box),
        // The center dot a confidence interval draws: `interval`'s alone, since a
        // box's middle is its median bar, which is the box.
        Center => matches!(mark, Mark::Interval),
        // A text label's offset, and its turn: the word is the one glyph read
        // along a direction, so it is the one glyph a turn means anything on.
        Nudge | Angle => matches!(mark, Mark::Text),
        // A head on the marks that have a direction to point in. Not the
        // narrowness it looks like: the settable rule (spec §4) spans a
        // setting across its *geometry class*, and the geometry here is "a
        // stroke whose ends the data chose". A `path` runs from its first row to
        // its last, and an `edge` from its `from` column to its `to` column, the
        // order `layout(from, to)` states; `edge` joined 2026-09-24, when its
        // refusal was found saying the mark had no direction. `line`/`step` sort
        // by x, so their last vertex is an artifact of the sort rather than an
        // end the data chose; `interval` already decorates its ends with `caps`.
        Arrow => matches!(mark, Mark::Path | Mark::Edge),
        // How far the one-position mark reaches across the axis it does not name.
        // Narrow for the same reason `arrow` is: the geometry class is "a mark
        // whose extent the panel supplies", and `rule` is its only member. Every
        // other mark takes both its extents from the data, so a reach would have
        // nothing to describe.
        Reach => matches!(mark, Mark::Rule),
        // The air beside a body standing in a slot (ruled 2026-10-09). A `bar` and
        // a `box` are the two marks whose *body* takes a share of its slot, and that
        // share was a constant on each: four fifths for a bar, less for a box. An
        // `interval` has no body, only a whisker and its caps (`caps` governs
        // those), and the violin's width is already `density(reach = )`, so giving
        // it `gap` as well would be two settings for one width.
        Gap => matches!(mark, Mark::Bar | Mark::Box),
    }
}

/// The legal values for `style(reach = )`: how far a `rule` crosses the axis it
/// does not name. `"panel"` is the default (a reference line); `"edge"` is a
/// short tick at the start of that axis (a rug). The one list the legality check
/// and the renderer both read, on `FILL_TEXTURES`' precedent.
pub(crate) const REACHES: [&str; 2] = ["panel", "edge"];

/// The legal values for `bin(tiling = )`: how the plane is partitioned. `"rect"`
/// is the default (equal-interval cutpoints on each axis); `"hex"` staggers
/// alternate rows so the mesh has no aligned lattice for the eye to mistake for
/// structure. The one list the legality check and the transform both read, on
/// `REACHES`' precedent.
///
/// Wilkinson's `bin` super-class has more members (`tri`, `quantile`, `voronoi`,
/// `dot`, `stem`). They are **not** promised by this list: `tri` would be a third
/// value on the same footing, but `voronoi` is different in kind — data-dependent,
/// one point per cell, needing real computational geometry — and is its own
/// decision rather than something this parameter quietly implies (spec §5).
pub(crate) const TILINGS: [&str; 2] = ["rect", "hex"];

/// Where a pile hangs — `stack(baseline = )`, validated the way `bin(tiling = )` is
/// (spec §5, [`crate::ir::StackSpec`]).
///
/// Three, and deliberately not four. Every reference implementation carries both a
/// plain and a *weighted* wiggle; the weighted one is the layout the readability
/// result is about and the one a streamgraph means, so it is the one that ships. A
/// second, worse spelling of the same idea is the enumeration §5's growth policy
/// exists to refuse, and it is the `tri` ruling arriving on a parameter that already
/// has the value in it.
pub(crate) const BASELINES: [&str; 3] = ["zero", "center", "wiggle"];

// ---------------------------------------------------------------------------
// Mark × Transform legality — the third grid's single source
//
// Transform legality does not live in `rule_for` (which is per *channel*). It is
// a property of the mark's *geometry* against the transform's *class*, and until
// now it was spread across the data-aware `check_*` functions. This function is
// the one place the mark-membership half of that question is answered, so the
// generated Mark × Transform grid and the refusals cannot disagree — the same
// role `mark_takes_setting` plays for the settings grid. The *secondary*,
// data-aware conditions stay in each `check_*` (a split for dodge/stack, a
// categorical axis for jitter, existing columns for bounds, a pair present for
// interval/ribbon); this owns only "does this mark take this transform at all?".
// ---------------------------------------------------------------------------

/// The three states a `(mark, transform)` pair can be in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransformLegality {
    /// The transform does not combine with this mark — refused with direction
    /// toward the transform the mark's geometry actually wants.
    None,
    /// Legal and optional: the mark renders with or without it.
    Combines,
    /// Legal, and the mark's minimum syllable (§7) *requires* a transform of this
    /// class: an `interval`/`ribbon` will not render until one is present
    /// (`check_span_needs_range`). *One* of the class satisfies it, not all three
    /// — the grid's prose says so; the cell only records that the class is needed.
    Required,
}

/// A transform that yields a **low/high pair**, which the *span* marks draw:
/// `interval` whiskers it, `ribbon` fills it, `line`/`step` trace its two
/// boundaries. `range`/`confidence`/`deviation` compute the pair from `y`;
/// `bounds` reshapes a pre-computed one. (`Transform::Box` also emits a pair,
/// but is injected by the `box` mark, never composed, so it is not one of these.)
pub(super) fn is_pair_transform(t: &Transform) -> bool {
    matches!(t, Transform::Range | Transform::Confidence | Transform::Deviation
                | Transform::Bounds | Transform::SmoothBand)
}

/// A collision modifier (Wilkinson §8): an *offset*, not a statistic. They divide
/// by geometry **and by axis** — `dodge` sets marks apart across a slot, `stack`
/// accumulates along a *measure* axis, `jitter` spreads along a *categorical* one —
/// so a mark takes each offset whose precondition it can meet. `point` meets all
/// three: the dot plot is `stack`, the strip plot `jitter`, and the beeswarm
/// `dodge`, which moves each point the least amount that clears its neighbors where
/// `jitter` moves it a seeded amount (spec §5).
///
/// `repel` is the fourth, and it divides on a different question. The first three
/// resolve marks that share a *position*; a label's ink is wider than its position,
/// so two labels collide where their points do not — which is why the mark whose
/// glyph is a word gets an offset of its own rather than a reading of `jitter`.
pub(super) fn is_collision_modifier(t: &Transform) -> bool {
    matches!(t, Transform::Dodge | Transform::Stack | Transform::Jitter | Transform::Repel)
}

/// Which class the book groups this transform under — so the grid can lay the
/// three blocks out without hardcoding membership, the way it reads every other
/// fact off the dump.
pub(super) fn transform_class(t: &Transform) -> &'static str {
    if is_pair_transform(t) {
        "pair"
    } else if is_collision_modifier(t) {
        "collision"
    } else {
        "statistic" // the value statistics; `Box` never reaches the grid
    }
}

/// Does this mark take this transform at all? The **one** answer the Mark ×
/// Transform grid and the `check_*` refusals both read (see the block comment
/// above). Read the classes off `transform::is_value_statistic` / `is_pair_transform` /
/// `is_collision_modifier`, then match the mark's geometry against the class.
pub fn mark_takes_transform(mark: &Mark, transform: &Transform) -> TransformLegality {
    use TransformLegality::*;

    // The five-number summary is constitutive of `box` (spec §6), injected by the
    // mark and never composed — so it is legal on `box` alone and is not a column
    // of the grid. Handled first for totality; `USER_TRANSFORMS` excludes it.
    if *transform == Transform::Box {
        return if *mark == Mark::Box { Required } else { None };
    }

    // `path` takes exactly **one** transform, and both halves of that need saying.
    //
    // *Why it takes almost none.* A path *is* its rows in order, and nearly every
    // transform here replaces the rows. A value statistic reduces each key to one
    // summary, so the order that remains is the order of the keys, which is the sort
    // a path exists not to do; a pair transform emits two rows per key, which is a
    // span rather than a route; and the collision modifiers need a width, a baseline
    // or a cloud, none of which a path has. So `path * mean` is refused toward
    // `line`, the mark that sorts and therefore the mark a statistic is drawn on.
    //
    // *Why `density` is not one of those.* Read on a mark with **no measure axis**,
    // `density` cuts both axes instead of one (spec §5's dimensionality rule, the
    // same one that makes `zone * bin` a heatmap), and a field's iso-lines are not
    // one summary per key — they are *vertices in traversal order*, which is
    // precisely what a path draws and what a `line` would destroy by sorting. So the
    // ruling above was right about the transforms that existed when it was written,
    // and this is the case its reason does not reach: the contour plot.
    //
    // *Why `cluster` is the second, by the same measure.* A merge tree's elbows
    // are not one summary per key either — they are vertices in traversal order,
    // four per merge, each merge its own run, exactly the shape the contour
    // established. A `line` would sort the risers into rubble; the mark that
    // strokes rows in the order given is the only mark that can draw a tree.
    // `path_takes_only_the_field_transform` pins all three halves.
    if *mark == Mark::Path {
        return match transform {
            Transform::Density | Transform::Cluster => Combines,
            _ => None,
        };
    }

    // `surface` needs a floor whose cells **tile without gaps**, and two transforms
    // give it one. That is the whole rule, and it replaced a narrower one that read
    // `density` as the only answer (2026-07-28, spec §15).
    //
    // *The two ways to tile, and they draw the same geometry.* `bin` **cuts** the floor
    // into adjacent cells, and a cut cell asserts one value across its whole extent,
    // so the honest geometry is flat across the cell with a *step* at the boundary: a
    // plateau per cell, the terraced sheet. `density` estimates the field on a grid
    // and publishes it the same way, one cell per block of four grid points with their
    // mean as its value (`transform::density2d_cells`), so its sheet is terraced too,
    // on cells fine enough that the steps read as a slope. The node reading, a face
    // spanning four samples and asserting every value between them, belongs to a bare
    // `surface` over a lattice of rows, which publishes no cells. One mark, one
    // question asked of the floor it was handed — *does this geometry claim anything
    // between its samples?* — and the answer is read off the floor rather than
    // declared. (This paragraph said `density` took the node reading until
    // 2026-09-24; the refusal that repeated it sent readers to an interpolated sheet
    // the engine does not draw.)
    //
    // *Why the old refusal was wrong, and it was wrong on its own terms.* It argued
    // that `bin` emits only non-empty cells, so "a sheet over it would carry holes
    // wherever nothing was counted or interpolate across a gap it cannot see". Both
    // halves are about *interpolation*, which is exactly what a lid does not do: a
    // plateau claims its own cell and nothing beyond it, so an absent cell is simply
    // an absent lid and no gap is ever crossed. The premise described the node reading
    // and was applied to a mark, which is the same defect §5 records for the five value
    // statistics on a floor — a rule true of one reading, enforced as a fact about the
    // geometry. Its direction (`bar * bin + space()`) also sent a reader to the mark
    // whose walls occlude the relief the sheet exists to show.
    //
    // *Why the five value statistics come too.* They compose with `bin` here exactly as
    // they do on a 3-D `bar` — `surface * bin * mean + x + y + z(<column>)` cuts the
    // floor and reduces the named column within each cell (Law 2: `bin * mean` means
    // the same thing on every mark). Alone they still require **categorical** positions,
    // and `rule_for` still refuses a category on either of a surface's, so the bare
    // `surface * mean` stays refused by the one rule that always refused it.
    //
    // *Why a category is still not a floor for this mark.* A cut axis touches and a
    // slotted one leaves air (spec §5). Lids over slots would float apart with gaps
    // between them, and disconnected tiles are not a sheet — so the refusal keeps its
    // reason, and `bar` keeps being the right direction there, where a column under
    // each tile is what says which cell it belongs to.
    //
    // *Why the other five need no word here.* `smooth` is refused in space outright,
    // the pair transforms give a domain two edges (a span, not a sheet), `count` and
    // `proportion` tally into cells two *categories* make, and the collision modifiers
    // need a width, a baseline or a cloud. `surface_takes_the_two_floor_transforms`
    // pins the whole row.
    // The reductions are read off the family predicate `zone` asks, not listed: a
    // list here named five and left out `quantile`, which a zone takes, so a sheet
    // of each cell's median drew and one of its 90th percentile was refused with no
    // reason (ruled 2026-09-29, Law 2).
    if *mark == Mark::Surface {
        return match transform {
            Transform::Density | Transform::Bin => Combines,
            t if crate::transform::is_reduction(t) => Combines,
            _ => None,
        };
    }

    // `rule` takes no transform either, and its reason is `path`'s read off the
    // other axis. Every transform here answers a question about a *measure* laid
    // out along a *domain* — bin and density cut the domain, the aggregations
    // reduce the measure within it, the pair transforms give it two edges, the
    // collision modifiers move marks that would otherwise land on the same slot.
    // A rule has no measure: it names one position and hands the other axis to
    // the panel, so there is nothing for a statistic to compute and nowhere to
    // put it. A line at the mean is therefore the mean *computed in the host* and
    // handed over as a column — which is what "the position is a column" already
    // meant (§18), and it buys several rules from one table.
    if *mark == Mark::Rule {
        return None;
    }

    // A zone takes five transforms, and they answer **two** questions rather than one
    // — which is the distinction the tile plot forced, and the old comment here
    // (three transforms, all answering "where are this rectangle's sides?") had
    // conflated. A cell needs an *extent* and a *measurement*, and a transform may
    // supply either or both:
    //
    // - `bounds` names the sides and measures nothing.
    // - `bin` and `density` cut the sides out of a continuous plane **and** measure
    //   inside them — the heatmap twice over, counted or estimated.
    // - `count` and `proportion` measure only. Their cells come from the categories,
    //   which own their slots already, so there is nothing left to cut: `bin` cuts
    //   and `count` tallies, the same division these two have had in one dimension
    //   since `bar * count` (spec §5).
    //
    // All five read `Required` the way `interval`'s three pair transforms do: the
    // grid's ● means "one of these", not "all of these". And none is required at all
    // when the *axes* bound the mark — two categorical positions are a mesh, so
    // `zone + x(a) + y(b) + color(v)` draws a tile plot with no transform; see
    // `check_zone_extent`.
    //
    // Every one of the four measuring transforms **invents its own measurement**,
    // which is exactly what lets a zone take them: read on a mark with no measure
    // axis the answer goes to `color`.
    //
    // **The other five reduce a column the user names, and they combine rather than
    // bound** (spec §5, the two-dimensional group-by). They were refused here under
    // the claim that a zone has no channel left to name that column with — which was
    // false on its own terms, since a zone *measures by color* and color is the
    // channel (`measure_channel`). What separates them from the four is not legality
    // but what they supply: the four answer *where are my cells?* as well as *what is
    // in them*, where these five only measure. A zone's cells come from its
    // categorical positions in that case — the fourth extent description — so `mean`
    // is `Combines`, never on its own the thing that bounds the mark.
    // `check_zone_extent` still asks where the sides are, `check_pair_summary` that
    // color named a column to reduce.
    //
    // **`partition` goes to the two marks that read an extent description**, and
    // deciding it here rather than inside the `Mark::Zone` block below is the point:
    // it is legal on a mark that is *not* a zone, so a rule written inside the zone's
    // own arm would have had to be written twice.
    //
    // `zone` takes the four edges and draws the rectangle, which bent is the sector —
    // the icicle and the sunburst, one atom apart (spec §15). `text` takes the
    // **center**, which the same computation publishes, and labels the node. That
    // second reader is what makes this a transform at all rather than a capability
    // belonging to `zone`: one computation feeding a rectangle and a label is exactly
    // what `bin` already does for a heatmap's cells and their labels, and a transform
    // legal on one mark would have been the mark's business wearing a transform's
    // name (§5's growth test, read from the other end).
    //
    // `Required` on `zone` for the reason `bounds` and `bin` are: it is one of the
    // ways a zone learns where its sides are, and `check_zone_extent` reads that list
    // rather than restating it. `Combines` on `text`, which renders perfectly well
    // without one — a label at a bound position is the ordinary text mark.
    //
    // Every other mark is `None`. A partition is a *region* description, and the
    // marks that are not regions have no reading for it: a `bar`'s length is measured
    // from a baseline and a node's ring is not one, a `line` sorts by its domain and a
    // partition has no domain to sort, and the span marks want a low/high pair per
    // position rather than one rectangle per node. Each is refused toward `zone`.
    if *transform == Transform::Partition {
        return match mark {
            Mark::Zone => Required,
            Mark::Text => Combines,
            _ => None,
        };
    }

    // `flow` is a layout with three readers, `partition`'s admission argument one
    // reader wider: `ribbon` takes the band between a path's two end intervals,
    // `zone` takes the node slots, and `text` names them. Every other mark is
    // refused toward those three — a flow hands back intervals at stages, and a
    // length from a baseline, a sorted stroke, or a glyph at a point has no
    // reading for a pair of them.
    if *transform == Transform::Flow {
        return match mark {
            Mark::Zone | Mark::Ribbon => Required,
            Mark::Text => Combines,
            _ => None,
        };
    }

    // `layout` has three readers on `partition`'s argument: `edge` takes the
    // endpoint pairs, `point` the nodes, `text` their names. Every other mark
    // is refused toward those three.
    if *transform == Transform::Layout {
        return match mark {
            Mark::Edge => Required,
            Mark::Point | Mark::Text => Combines,
            _ => None,
        };
    }

    // An `edge` takes `layout` and nothing else: its whole geometry is the two
    // endpoints the layout supplies, so a statistic has nothing to feed it and
    // a collision modifier has no coordinate to move it in.
    if *mark == Mark::Edge {
        return None;
    }

    // The pair transforms `range`/`confidence` compute a band across a domain,
    // which is legal grammar and the wrong mark; `check_zone` says so.
    //
    // **`cluster` combines, and it is the first transform here that supplies
    // neither an extent nor a measurement.** A tile plot's cells come from its
    // two categorical axes and its measurement from `color`, both already
    // supplied; what a cluster adds is the *order* of the leaf axis's slots —
    // the seriated heatmap, the tree's other reading. `Combines` rather than
    // `Required` because the mark renders perfectly well without one.
    if *mark == Mark::Zone {
        return match transform {
            Transform::Bounds | Transform::Bin | Transform::Density
            | Transform::Count | Transform::Proportion => Required,
            Transform::Cluster => Combines,
            t if crate::transform::has_reduction(std::slice::from_ref(t)) => Combines,
            _ => None,
        };
    }

    // **`ribbon` takes `density`** — the violin (spec §5), and the one member of the
    // statistic class a span mark accepts. It is `Required` rather than `Combines`
    // for the same reason the pair transforms are, and it is the reason that cell
    // reads as one of a set rather than as an exception: a ribbon renders once
    // *something* has given it two boundaries, and the slot reading of `density`
    // gives it them by reflection. So the mark's minimum syllable is unchanged and
    // gains a fourth way to be satisfied, which `check_span_needs_range` states in
    // one place for the grid and the refusal both.
    //
    // Only `ribbon`: `area` is already `Combines` below (it takes the whole statistic
    // class, the violin included, being a locus mark that happens also to draw one),
    // and no other span mark has a slot reading — an `interval` is whiskers, and a
    // whisker has no width to spread an estimate across.
    if *mark == Mark::Ribbon && *transform == Transform::Density {
        return Required;
    }

    // A tally, a share or an estimate on `text` writes its number, the count on
    // each bar (spec §8, a written column is a column): the label names the column
    // the transform writes, `text * count + label(count)`, and the text stands at
    // the measure's height. **A summary prints its value the same way**: `mean`
    // reduces `life` in place, so `text * mean + label(life)` prints each mean
    // where the bar beside it ends. It was refused as "no label to draw", which the
    // rows rule made the wrong reason: the summary's rows hold `life`, and the key.
    // `check_text_label_rows` refuses a label those rows do not hold.
    if *mark == Mark::Text
        && (matches!(transform, Transform::Count | Transform::Bin | Transform::Proportion
            | Transform::Density)
            || crate::transform::is_reduction(transform))
    {
        return Combines;
    }

    // Value statistics → the locus marks (a value drawn at each x). Refused on the
    // span marks (which need a *pair*, not a single value), on `box` (which carries
    // its own summary), and on `text` except the counting ones and the summaries
    // just above, whose number is the label; `text * smooth` has none to print.
    if is_value_statistic(transform) {
        return match mark {
            Mark::Point | Mark::Line | Mark::Area | Mark::Bar | Mark::Step => Combines,
            _ => None,
        };
    }

    // Pair transforms → the span-capable marks. `interval`/`ribbon` *require* one
    // (it is their minimum syllable); `line`/`step` take one optionally to trace
    // the two boundaries as an unfilled band.
    if is_pair_transform(transform) {
        return match mark {
            Mark::Interval | Mark::Ribbon => Required,
            Mark::Line | Mark::Step => Combines,
            // `Mark::Zone` is answered above: it takes `bounds` and `bin` and
            // nothing else, so it never reaches the class tables.
            _ => None,
        };
    }

    // Collision modifiers → divided by geometry (spec §5).
    match transform {
        Transform::Dodge => match mark {
            // A width to subdivide.
            Mark::Bar | Mark::Box | Mark::Interval => Combines,
            // Glyphs to set apart — the beeswarm (spec §5). A split tiles the slot
            // exactly as it does for a bar, and the points still colliding inside a
            // tile move apart along the slot by the least amount that clears them:
            // Wilkinson's `point.dodge.symmetric`, his Figure 8.26. A bar never
            // collides inside its tile, so the one rule draws bars unchanged and
            // makes points swarm.
            Mark::Point => Combines,
            // Words to set over the tiles a dodged bar stands in. A label has no
            // width of its own to divide; it moves to its group's tile, the place
            // the bar of the same rows stands, so dodged bars can be labeled.
            Mark::Text => Combines,
            _ => None,
        },
        Transform::Stack => match mark {
            // A measured height to accumulate.
            Mark::Bar | Mark::Area => Combines,
            // A tally to pile as glyphs — the dot plot (spec §5). A point has no
            // height of its own, so it cannot accumulate one; what it accumulates
            // is *itself*, one dot per row counted, which is the same span
            // `[base, top]` drawn by a mark whose extent is fixed.
            Mark::Point => Combines,
            _ => None,
        },
        Transform::Jitter => match mark {
            // A widthless cloud of points to spread.
            Mark::Point => Combines,
            _ => None,
        },
        Transform::Repel => match mark {
            // Ink to move. Every other mark's glyph is as wide as the geometry it
            // was given, so its overlap is the one the first three offsets answer;
            // a label is as wide as the word it draws, and only `text` draws one.
            Mark::Text => Combines,
            _ => None,
        },
        // Unreachable: `Box` and the statistics are handled above.
        _ => None,
    }
}

/// Every transform a user composes with `*`, in the grid's teaching order —
/// the eleven value statistics, the five pair transforms, the four whole-picture
/// transforms (`partition`, `flow`, `layout`, `cluster`), then the four collision
/// modifiers: twenty-four. `Transform::Box` is excluded: the `box` mark injects it,
/// it is never typed, so it is not a column of the Mark × Transform grid.
pub const USER_TRANSFORMS: [Transform; 24] = [
    Transform::Bin, Transform::Smooth, Transform::Count, Transform::Density, Transform::Proportion,
    Transform::Sum, Transform::Mean, Transform::Median, Transform::Max, Transform::Min,
    Transform::Quantile,
    Transform::Range, Transform::Confidence, Transform::Deviation, Transform::SmoothBand,
    Transform::Bounds,
    Transform::Partition, Transform::Flow, Transform::Layout, Transform::Cluster,
    Transform::Dodge, Transform::Stack, Transform::Jitter, Transform::Repel,
];

// ---------------------------------------------------------------------------
// Mark × Space legality — the fourth grid's single source
//
// The three grids before this one cross the marks with the *aesthetic* families:
// what a mark maps (`rule_for`), what it sets (`mark_takes_setting`), what it
// derives (`mark_takes_transform`). This one crosses them with the coordinate
// space the whole plot sits in — Wilkinson's chapter 9, and the one axis of the
// orthogonality matrix that had no entries while `flat` was the only space a plot
// could be in. A mark is not redefined by the space: `bar` in `polar` is the same
// bar, asking `render::polar` where a coordinate lands instead of `Layout`.
//
// Two states, like the settings grid rather than the transforms grid: the engine
// draws this mark in this space, or it does not draw it *yet*. No pair is refused
// outright — a geometry with no reading in a space has not turned up, and
// inventing the category before it does would be enumeration.
// ---------------------------------------------------------------------------

/// The coordinate spaces the kernel names (grammar.qmd's `Spaces:` line), in
/// teaching order. The view parameters a space carries (`SpaceView`, `PolarView`)
/// live on `CoordSpace`; this is the bare identity, so it can key a grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpaceKind {
    Flat,
    Space,
    Polar,
    Nest,
    Globe,
    Map,
    Network,
}

pub const ALL_SPACES: [SpaceKind; 7] = [
    SpaceKind::Flat, SpaceKind::Space, SpaceKind::Polar, SpaceKind::Nest,
    SpaceKind::Globe, SpaceKind::Map, SpaceKind::Network,
];

pub fn space_name(s: SpaceKind) -> &'static str {
    match s {
        SpaceKind::Flat => "flat",
        SpaceKind::Space => "space",
        SpaceKind::Polar => "polar",
        SpaceKind::Nest => "nest",
        SpaceKind::Globe => "globe",
        SpaceKind::Map => "map",
        SpaceKind::Network => "network",
    }
}

/// Which space a spec is drawn in. `space` is the *view* — a third dimension is
/// what makes a plot three-dimensional (spec §15) — so a `CoordSpace::Space` with
/// no third dimension still reports `Flat` here, which is what the renderer draws.
///
/// **A synthesized `z` is a third dimension too** (the 3-D histogram, spec §5/§15).
/// Binding `z` is the usual trigger, but `bar * bin + x(a) + y(b) + space()` binds
/// none: `bin` invents the count and writes it to `z`, exactly as it invents the
/// count and writes it to `y` when the plot is flat. Requiring `z(count)` here
/// would make the cube ask for a binding the plane does not — the per-space
/// exception Law 2 forbids — so the axis counts as present when a transform
/// synthesizes it, and `space()` is then the whole of what the user must say.
pub fn space_of(spec: &PlotSpec) -> SpaceKind {
    // **Two ways to have a third dimension, and they are not symmetric.**
    //
    // *Bound* is the original trigger and does not need `space()`: §15 says binding
    // `z` is what makes a plot three-dimensional, and `space()` only sets the angle.
    // This function used to require `CoordSpace::Space` for it, which meant it said
    // `Flat` about plots the renderer had been projecting since M8a — the two had
    // simply never been asked the same question, because the renderer kept its own
    // copy of the test. Reading that copy back into here is what makes them one.
    //
    // *Synthesized* is the 3-D histogram, and it **does** need `space()`: `bin` on a
    // `bar` invents a measure in the plane too, so a flat histogram would otherwise
    // answer `Space` and every 2-D plot with a transform would project. The
    // asymmetry is not an exception — a bound `z` says *there are three columns
    // here*, which nothing else can mean, while a synthesized one says only *this
    // layer invents a measure*, and it takes the coordinate to say where it goes.
    let bound_z = spec.axis_def(&Channel::Z).is_some();
    let synthesized_z = matches!(spec.coord, CoordSpace::Space(_))
        && spec.layers.iter().any(|l| stands_a_measure_on_z(&l.mark, &l.transforms));
    match &spec.coord {
        // Polar wins over a stray `z`: the two are mutually exclusive and
        // `check_polar` refuses the pair, so this only orders the report.
        CoordSpace::Polar(_) => SpaceKind::Polar,
        // The same precedence, for the same reason: `check_nest` refuses `nest()`
        // with a `z`, so this only decides which name the report uses.
        CoordSpace::Nest => SpaceKind::Nest,
        // The same precedence again: `check_globe` refuses `globe()` with a `z`,
        // so this only decides which name the report uses.
        CoordSpace::Globe(_) => SpaceKind::Globe,
        CoordSpace::Map(_) => SpaceKind::Map,
        // The same precedence a fourth time: `check_network` refuses a bound
        // `z` inside `network()`, so this only decides which name the report
        // uses. Flat and cube forms are one SpaceKind — which form is asked
        // for is the view's business, not legality's.
        CoordSpace::Network(_) => SpaceKind::Network,
        _ if bound_z || synthesized_z => SpaceKind::Space,
        _ => SpaceKind::Flat,
    }
}

/// Does this layer stand a measure up on `z` in the cube? Only a tally or an
/// estimate over the floor's cells does: `bin`, `count`, `density` and a tallying
/// `proportion`, which make the 3-D histogram and its kin.
///
/// `synthesizes_measure` answers a wider question, whether a transform invents the
/// value along the mark's measure axis, and four of its answers invent a
/// **position in the plane** rather than a height: `bounds` its two extents,
/// `partition` a ring, `flow` a stacked interval and `cluster` a merge distance.
/// `space_of` once read all of them as heights, so each put a plot with a bare
/// `space()` in a cube it had nothing to stand in. `zone * bounds(lo, hi) + x(t) +
/// space()` drew an empty turned cube with a made-up 0 to 1 vertical axis and no
/// zone, and a cluster tree and a partition did the same, all in silence. Asked
/// this narrower question, each is flat, and `check_space` says a viewing angle
/// with no third dimension is drawn flat, as it says for every other mark. A `flow`
/// was refused rather than drawn empty, but only because the same misreading sent
/// it to the cube: under a bare `space()` it now follows the same rule.
pub(super) fn stands_a_measure_on_z(mark: &Mark, transforms: &[Transform]) -> bool {
    synthesizes_measure(mark, transforms)
        && transforms.iter().any(|t| {
            matches!(t, Transform::Bin | Transform::Count | Transform::Density
                | Transform::Proportion)
        })
}

/// Does the engine draw this mark in this space today? The **one** answer the
/// generated Mark × Space grid and the `check_coord` refusals both read — the
/// role `mark_takes_setting` and `mark_takes_transform` play for their grids.
pub fn mark_draws_in_space(mark: &Mark, space: SpaceKind) -> bool {
    // A mark that draws in no space at all cannot draw in a particular one. No mark
    // is in that state today (`is_drawable`), and this stays as the gate for the next
    // one that is.
    if !is_drawable(mark) {
        return false;
    }
    match space {
        // The plane every mark was built in — with one exception, and it is the
        // first: **`surface` does not draw flat.** Every other mark started in the
        // plane and some have since gained a space; a surface is a sheet through
        // three positions and there is no such thing without the third, so its
        // minimum syllable (Law 7) includes the cube. That makes the missing-`z`
        // failure and the wrong-space failure *one* failure for this mark, reported
        // once by `check_surface` with both routes into the cube named — rather than
        // as a bare "needs `z()`", which would be true and would not tell a reader
        // asking for a flat sheet that `zone` is where the field lives in the plane.
        // …and the second such mark is `edge` (2026-08-17): a stroke whose
        // endpoints mean nothing on data axes has nowhere to stand outside the
        // space that abolishes them, so its flat cell is empty for `surface`'s
        // kind of reason — the minimum syllable includes the space.
        SpaceKind::Flat => !matches!(mark, Mark::Surface | Mark::Edge),
        // `rule_for(_, Z).renders` already says which marks stand in the cube, so
        // this reads it rather than restating a count that would go stale the next
        // time one is added — as a comment here did, four times over.
        SpaceKind::Space => rule_for(mark, &Channel::Z).renders.is_some(),
        // The plane bent into a circle — and since 2026-07-26 **every mark that
        // draws flat draws here**, which makes this the first space with no hole
        // in its column. `surface` is the one blank, and it is not a polar gap:
        // it does not draw flat either, its minimum syllable including the cube.
        //
        // The five that took until then — `step`, `interval`, `box`, `ribbon`,
        // `zone` — were refused together for one recorded reason, *their straight
        // edges would have to become arcs*, and building it showed that reason was
        // wrong for three of them and imprecise for the other two. Kept here in
        // full, because the correction is the useful part:
        //
        // - **`ribbon` never needed an arc.** A band's two boundaries run through
        //   the data's own vertices, so they are the chords `line`/`area`/`path`
        //   have drawn in this space since it shipped. The only ring a band could
        //   have closed along is one it never closes along: it closes on its own
        //   lower boundary, vertex by vertex, which is exactly the retracing path
        //   `write_area` switches to in polar and which `write_ribbon` had been
        //   doing flat since it was written.
        // - **`zone` and `box` needed no *general* arc.** A rectangle bent is an
        //   annular sector, which `Polar::sector` has drawn since `bar` became a
        //   rose; the annulus a panel-spanning zone closes into was already handled
        //   where the arc is written, since an `A` across a whole turn has
        //   coincident ends and is split at the antipode.
        // - **`step` and `interval` needed one thing, and it was not a general arc
        //   either**: a constant-radius arc inside a *stroke* path rather than
        //   inside a filled sector. A tread and a cap **hold** their value across a
        //   span, which is an arc; a riser and a whisker's span change it at one
        //   angle, which is exactly the radius. `Polar::hold_to` is the whole of it.
        //
        // So there is no general arc anywhere in the five, and there never was one
        // to write: a *general* arc is a segment whose radius varies along it, and
        // every such segment in all five marks is a locus segment, which this space
        // decided was a chord when `line` first bent. That decision is inherited
        // here rather than reopened (Law 6).
        //
        // `path` draws for the same reason `line` does — the spiral is the polar
        // reading of a path. The one thing it does *not* inherit is the radar's
        // closing segment: a categorical `line` closes because the categories
        // exhaust the turn with no repeated endpoint, while a path is not indexed
        // by the angular axis at all, so its last vertex is where the data stopped.
        // Closing it would invent a segment nothing asked for; repeat the first row
        // to close one.
        //
        // `rule` shows what "identical in every space" buys. Flat, it spans the
        // axis it does not name; bent, spanning that axis whole is a **ring** when
        // the rule sits on the radius and a **spoke** when it sits on the angle.
        // Neither is a special case — both fall out of the one sentence, which is
        // the bar §4 sets for a Law 7 relaxation.
        SpaceKind::Polar => !matches!(mark, Mark::Surface | Mark::Edge),
        // **A packing has regions, not positions**, so the question this column
        // asks is not the one the other four ask. Everywhere else a mark bends
        // because its geometry survives a map of the plane; here there is no map
        // (spec §15), and a mark draws only if the space can hand it *a rectangle
        // it already knows how to fill*.
        //
        // `bar` can: its identity is a length from a baseline, and this space is
        // the third answer to what carries that length — pixels flat, an angle in
        // polar, an **area** here. Nothing about the sentence changes, which is the
        // test `polar` set for whether a space is a space (ruling 1 above).
        //
        // **`text` draws too, and it is the one mark here that names a region
        // rather than filling one** (2026-07-27, closing the deferral this comment
        // used to record). A label at its cell's centroid is what every published
        // treemap has, and the cell was already computed: `Nest::regions` answers
        // both marks, so a bar and its label cannot disagree about where a region
        // is. What the deferral was waiting on — *what does a label do when its
        // cell is smaller than its own ink?* — is answered where §12 says it has
        // to be. The label is not drawn, and the layer **reports how many it left
        // out**, because a packing of many shares has more regions than legible
        // ones and quietly printing the large ones would let a reader take the
        // labeled cells for all of them.
        //
        // The other eleven are one blank with one reason: they are all placed by a
        // position. A `point` needs somewhere to sit and a packing has no
        // coordinate to give it; a `line`, `step`, `path`, `area` and `ribbon` need
        // an order along an axis, and adjacency here is explicitly *not* a distance
        // (Wilkinson §13.3.4.1: blocks may touch and have different parents), so a
        // segment joining two cells would draw a relation the space does not hold.
        // `rule` and `zone` span an axis that is not there. `box`, `interval` and
        // `surface` measure along one. `text` is not in that list because it is not
        // placed by a position here: it is placed by the *region*, which is the
        // same thing that places the bar.
        SpaceKind::Nest => matches!(mark, Mark::Bar | Mark::Text),
        // **Both positions are spent on the place**, and that one fact decides the
        // whole column. A map is a map of the plane, so unlike a packing it has a
        // coordinate for everything — but longitude and latitude use up both axes,
        // so a mark that needs an axis to *measure along* has none left.
        //
        // Three marks are placed by a position and measure nothing, so they draw
        // exactly as they draw flat: `point` is a place, `path` is a route through
        // places in the table's order, and `text` is a name at a place.
        //
        // `rule` draws for the reason it draws in `polar` — it spans the axis it
        // does not name, and here that makes it a **meridian or a parallel**. The
        // graticule is therefore the mark rather than furniture the space invents,
        // which is the same sentence bending that turns a polar rule into a ring
        // or a spoke.
        //
        // `zone` is the choropleth and is the next one owed: it is the region mark,
        // and rectangularity was never its identity — `bounds` was. What it waits
        // on is an extent supplied by a boundary, which is a fifth source for a
        // zone's sides beside the four it has.
        //
        // The rest measure along an axis — `bar` a length, `interval` a span, `box`
        // a summary, `area`/`ribbon`/`step`/`line` a domain read left to right,
        // `surface` a sheet needing the cube — and that is a ruling rather than a
        // queue. The measure moves to a refining channel instead: `point + size(v)`
        // is the proportional-symbol map, which is what cartography does with a
        // quantity once both positions are gone.
        // `zone` joined the four on 2026-08-01 and is the **choropleth**: the region
        // mark, with its sides supplied by a boundary instead of by a pair per axis.
        SpaceKind::Map => {
            matches!(mark, Mark::Point | Mark::Path | Mark::Text | Mark::Rule | Mark::Zone)
        }
        // **Both positions are spent on the place, on the sphere itself** — the
        // map's fact one space over, so this column is `map`'s being built out
        // mark by mark. `point` is a place, `text` a name at one, `path` a
        // route along great circles (a joined segment is what its endpoints
        // assert lies between them, and on a sphere that is the geodesic), and
        // `rule` holds one coordinate whole — a meridian or a parallel, polar's
        // spoke and ring one space over. All drawn on the facing hemisphere
        // with the hidden count reported. `zone` is the spherical patch — the
        // boundary-supplied choropleth, its rings clipped at the limb and
        // re-closed along it (`check_globe` holds the mesh forms back until an
        // equal-area spherical grid exists). And `bar` is the **spike**: the
        // sphere, unlike its flattening, has one direction to spare — the
        // radius — so a bar stands at its place and measures outward along it,
        // the cube's own `z` reading transplanted (`check_globe` requires the
        // `z` that names its measure). The remaining measure marks refuse for
        // `map`'s one reason, which still holds for them: a *domain* read left
        // to right has no left to right here.
        SpaceKind::Globe => {
            matches!(mark,
                Mark::Point | Mark::Text | Mark::Path | Mark::Rule | Mark::Zone | Mark::Bar)
        }
        // **Positions come from the layout, not the data**, so the question
        // this column asks is `nest`'s: a mark draws only if the layout hands
        // it something it already knows how to draw. `edge` takes the endpoint
        // pairs, `point` the nodes, `text` their names — one computation,
        // three readers (spec §15, the network entry). Everything else is one
        // blank with one reason: it reads a domain, a measure, or an extent
        // along an axis, and this space has no axes to read along.
        SpaceKind::Network => matches!(mark, Mark::Edge | Mark::Point | Mark::Text),
    }
}

/// One mark row of the grid: its name and whether the engine draws it (so the
/// book can show the drawable marks and footnote the two that are grammar-only).
#[derive(serde::Serialize)]
pub struct MarkInfo {
    pub name: &'static str,
    pub drawable: bool,
}

/// One (mark, channel) cell — the raw verdict from `rule_for`, named for the
/// wire. The book turns these four fields into a single glyph; keeping them raw
/// here lets other readers (a lint, autocomplete, the Python book later) reuse
/// the same dump without inheriting one presentation's symbols.
#[derive(serde::Serialize)]
pub struct RuleCell {
    pub mark: &'static str,
    pub channel: &'static str,
    /// `"must"` | `"can"` | `"cannot"`.
    pub obligation: &'static str,
    /// `"continuous"` | `"discrete"` | `"either"` — the types the grammar permits.
    pub accepts: &'static str,
    /// The type the engine draws today, or `null` when the mapping is legal
    /// grammar the renderer cannot do yet (the Unsupported gap — `z` off `point`,
    /// `play`, a mapped `size` on `text`).
    pub renders: Option<&'static str>,
    /// Whether `style()` may *set* this feature to a constant, even where a
    /// per-row mapping is refused (a line's one stroke width).
    pub settable: bool,
}

/// The values a setting accepts **on this mark**, or an empty list when the
/// vocabulary is open (a color, a number of pixels).
///
/// Mark-aware because one setting can be realized differently per geometry: the
/// same `pattern` is a stroke's dash on `line`/`rule` and a fill's hatch on
/// `bar`/`zone` (spec §4, the settable rule). A book table that listed one set for
/// both would be wrong for half the marks, so the split is read off `texture_of`,
/// the same function the refusal reads, instead of being restated per chapter.
pub fn setting_values(mark: &Mark, setting: &str) -> &'static [&'static str] {
    match setting {
        "shape" => SHAPE_NAMES,
        "pattern" => match texture_of(mark) {
            Some(Texture::Dash) => &STROKE_DASHES,
            Some(Texture::Hatch) => &FILL_TEXTURES,
            None => &[],
        },
        // A yes-or-no setting is written in each binding's own spelling of true
        // (`TRUE`, `True`, `true`), so there is no one word to list; the book says
        // "true or false" (ruled 2026-09-29: the engine's words are neutral).
        "caps" | "center" => &[],
        "nudge" => &NUDGES,
        "arrow" => &ARROW_ENDS,
        "reach" => &REACHES,
        // Open vocabularies: any CSS color, any number. Nothing to enumerate.
        _ => &[],
    }
}

/// One (setting, mark) cell of the Mark × Setting grid — can `style()` fix this
/// feature on this mark? Two states, not a channel's five: a setting is available
/// on a mark or it is not.
#[derive(serde::Serialize)]
pub struct SettingCell {
    pub setting: &'static str,
    pub mark: &'static str,
    pub settable: bool,
    /// The values this setting takes on this mark; empty when open-ended. Lets a
    /// per-mark chapter print its own vocabulary without hand-copying it.
    pub values: &'static [&'static str],
}

/// One transform column of the Mark × Transform grid: its name and which of the
/// three classes it belongs to (`"statistic"` / `"pair"` / `"collision"`), so the
/// book can lay the classes out as blocks without hardcoding the membership.
#[derive(serde::Serialize)]
pub struct TransformInfo {
    pub name: &'static str,
    pub class: &'static str,
    /// Which of the four **jobs** this transform fills — the fact that decides what
    /// it can be chained with (spec §5). Orthogonal to `class`, which says what kind
    /// of answer it produces: `bin` and `count` are both statistics, and only one of
    /// them says where the cells are.
    ///
    /// `bounds` is the one entry that reads differently per mark (sides of a
    /// rectangle on a mark that measures by color, the low/high pair everywhere
    /// else), so this lists every job it can fill and `chain_cells` below carries
    /// the verdict that actually applies.
    pub jobs: Vec<&'static str>,
}

/// One (transform, transform) cell of the Transform × Transform grid — **can these
/// two stand together on one mark?**
///
/// Generated from the same predicate the engine refuses with, so the book's chain
/// table cannot drift from what a caller actually gets. `job` names the job they
/// collide on when they cannot; it is absent when they compose.
#[derive(serde::Serialize)]
pub struct ChainCell {
    pub a: &'static str,
    pub b: &'static str,
    pub legal: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job: Option<&'static str>,
}

/// One (transform, mark) cell of the Mark × Transform grid. Three states, named
/// for the wire: `"none"` (no such combination), `"combines"` (legal, optional),
/// `"required"` (legal, and the mark's minimum syllable needs a transform of this
/// class). The book maps each to a glyph, the way it does for the channel grid.
#[derive(serde::Serialize)]
pub struct TransformCell {
    pub transform: &'static str,
    pub mark: &'static str,
    pub state: &'static str,
}

/// One (space, mark) cell of the Mark × Space grid. Two states like the settings
/// grid: the engine draws this mark in this space, or it does not draw it yet.
#[derive(serde::Serialize)]
pub struct SpaceCell {
    pub space: &'static str,
    pub mark: &'static str,
    pub drawn: bool,
}

/// The whole `rule_for` table, ready to serialize for `gog-cli --rules`.
#[derive(serde::Serialize)]
pub struct RulesMatrix {
    pub marks: Vec<MarkInfo>,
    pub channels: Vec<&'static str>,
    pub cells: Vec<RuleCell>,
    /// The Mark × Setting grid (`style.qmd`): `style()` legality across the marks,
    /// the channel-backed settings and the style-only ones under one roof.
    pub settings: Vec<&'static str>,
    pub setting_cells: Vec<SettingCell>,
    /// The Mark × Transform grid (`combinations.qmd`): which statistics and
    /// collision modifiers combine with which marks, from `mark_takes_transform`.
    pub transforms: Vec<TransformInfo>,
    pub transform_cells: Vec<TransformCell>,
    /// The Transform × Transform grid (`combinations.qmd`): which pairs can stand on
    /// one mark, from the same `job_conflict` the refusals read.
    pub chain_cells: Vec<ChainCell>,
    /// The Mark × Space grid (`combinations.qmd`): which marks the engine draws in
    /// which coordinate space, from `mark_draws_in_space`.
    pub spaces: Vec<&'static str>,
    pub space_cells: Vec<SpaceCell>,
}

/// **Can `proportion` rescale what this transform measured?**
///
/// The second pairwise chain rule, and it is not a job collision — `proportion`
/// fills *scale* and these fill *measure*, which are different jobs. What collides
/// is narrower: a share is one number divided by a total, so the measurement has to
/// **be** one number per cell. A pair transform leaves two, and `density`/`smooth`
/// leave a curve sampled between the observations rather than a value in a cell.
///
/// `check_share_composition` owns the sentences, each with its own reason. This owns
/// the *fact*, so `chain_cells` can publish it and the book's chain grid stops
/// claiming pairs the engine refuses — which it did for a few hours on 2026-07-31,
/// the same over-promise a generated grid is supposed to make impossible.
/// `the_published_chain_grid_matches_what_the_engine_refuses` binds the two.
pub(super) fn normalizer_conflict(a: &Transform, b: &Transform) -> bool {
    let pair = |t: &Transform| matches!(t,
        Transform::Density | Transform::Smooth | Transform::Range
            | Transform::Confidence | Transform::Box | Transform::Bounds
            | Transform::SmoothBand);
    (a == &Transform::Proportion && pair(b)) || (b == &Transform::Proportion && pair(a))
}

/// A job's name on the wire, in the words the book uses for it.
pub(super) fn job_wire(j: Job) -> &'static str {
    match j {
        Job::Extent => "extent",
        Job::Measure => "measure",
        Job::Scale => "scale",
        Job::Position => "position",
    }
}

/// Every job this transform can fill, for the wire.
///
/// **The plain reading comes first, and consumers may rely on that.** Two transforms
/// fill a second job only in a particular setting — `bounds` says where a `zone`'s
/// sides are, `stack` rescales when it is given `share = TRUE` — so the default
/// context is asked first and the conditional job is appended. A reader asking "what
/// does this transform *do*" wants element one; a reader asking "what can it collide
/// with" wants the whole list. The book's chain-shape table takes element one, and
/// would otherwise report `stack` as a scale transform and lose the position job
/// entirely.
pub(super) fn transform_jobs_wire(t: &Transform) -> Vec<&'static str> {
    let mut out = Vec::new();
    for ctx in [
        crate::transform::JobContext::default(),
        crate::transform::JobContext { measures_by_color: true, stack_shares: true },
    ] {
        let j = crate::transform::jobs(t, ctx);
        for (fills, name) in [
            (j.extent, "extent"), (j.measure, "measure"),
            (j.scale, "scale"), (j.position, "position"),
        ] {
            if fills && !out.contains(&name) { out.push(name) }
        }
    }
    out
}

pub(super) fn transform_state_wire(t: TransformLegality) -> &'static str {
    match t {
        TransformLegality::None => "none",
        TransformLegality::Combines => "combines",
        TransformLegality::Required => "required",
    }
}

pub(super) fn obligation_wire(o: Obligation) -> &'static str {
    match o {
        Obligation::Must => "must",
        Obligation::Can => "can",
        Obligation::Cannot => "cannot",
    }
}

pub(super) fn vartype_wire(v: VarType) -> &'static str {
    match v {
        VarType::Continuous => "continuous",
        VarType::Discrete => "discrete",
        VarType::Either => "either",
    }
}

/// Build the full Mark × Channel matrix by iterating the shared atom lists, so
/// adding a mark or channel widens the grid with no further edit. This *is*
/// `rule_for`, walked — it cannot say anything the engine would not enforce.
pub fn rules_matrix() -> RulesMatrix {
    let mut cells = Vec::with_capacity(ALL_MARKS.len() * ALL_CHANNELS.len());
    for m in &ALL_MARKS {
        for c in &ALL_CHANNELS {
            let r = rule_for(m, c);
            // **A ruled-out `z` is dumped as the refusal it is** (spec §15). The four
            // path marks keep a `Can` cell with no renderer, so a plot-wide `z` still
            // reaches them and a cube still asks them for one, and each is refused
            // with its own direction (`z_refusal`). But the grammar has ruled the
            // pairing out, so the table says `cannot`: dumped as legal and unbuilt,
            // it was printed as ◌ beside the cells that are only owed.
            let ruled_out = *c == Channel::Z && r.renders.is_none()
                && z_refusal_kind(m) == DiagnosticKind::Illegal;
            cells.push(RuleCell {
                mark: mark_name(m),
                channel: channel_name(c),
                obligation: obligation_wire(if ruled_out { Obligation::Cannot } else { r.obligation }),
                accepts: vartype_wire(r.accepts),
                renders: r.renders.map(vartype_wire),
                settable: r.settable,
            });
        }
    }
    let mut setting_cells = Vec::with_capacity(ALL_SETTINGS.len() * ALL_MARKS.len());
    for s in ALL_SETTINGS {
        for m in &ALL_MARKS {
            setting_cells.push(SettingCell {
                setting: setting_name(s),
                mark: mark_name(m),
                settable: mark_takes_setting(m, s),
                values: setting_values(m, setting_name(s)),
            });
        }
    }
    let mut transform_cells = Vec::with_capacity(USER_TRANSFORMS.len() * ALL_MARKS.len());
    for t in &USER_TRANSFORMS {
        for m in &ALL_MARKS {
            transform_cells.push(TransformCell {
                transform: transform_name(t),
                mark: mark_name(m),
                state: transform_state_wire(mark_takes_transform(m, t)),
            });
        }
    }
    // Every ordered pair of transforms, judged by the predicate the refusals read.
    // Ordered rather than unordered because the grid is read as a square and both
    // halves have to be there; the verdict itself is symmetric, which the book can
    // then show rather than claim.
    let mut chain_cells = Vec::with_capacity(USER_TRANSFORMS.len().pow(2));
    for a in &USER_TRANSFORMS {
        for b in &USER_TRANSFORMS {
            if a == b { continue }
            let conflict = crate::transform::job_conflict(
                &[a.clone(), b.clone()],
                crate::transform::JobContext::default(),
            );
            // Two rules, not one: a job filled twice, or a normalizer with nothing
            // it can divide. Both are refusals a caller meets, so both belong here.
            let why = conflict.map(|(_, _, j)| job_wire(j))
                .or_else(|| normalizer_conflict(a, b).then_some("share"));
            chain_cells.push(ChainCell {
                a: transform_name(a),
                b: transform_name(b),
                legal: why.is_none(),
                job: why,
            });
        }
    }
    let mut space_cells = Vec::with_capacity(ALL_SPACES.len() * ALL_MARKS.len());
    for s in ALL_SPACES {
        for m in &ALL_MARKS {
            space_cells.push(SpaceCell {
                space: space_name(s),
                mark: mark_name(m),
                drawn: mark_draws_in_space(m, s),
            });
        }
    }
    RulesMatrix {
        marks: ALL_MARKS
            .iter()
            .map(|m| MarkInfo { name: mark_name(m), drawable: is_drawable(m) })
            .collect(),
        channels: ALL_CHANNELS.iter().map(channel_name).collect(),
        cells,
        settings: ALL_SETTINGS.iter().map(|s| setting_name(*s)).collect(),
        setting_cells,
        transforms: USER_TRANSFORMS
            .iter()
            .map(|t| TransformInfo {
                name: transform_name(t),
                class: transform_class(t),
                jobs: transform_jobs_wire(t),
            })
            .collect(),
        transform_cells,
        chain_cells,
        spaces: ALL_SPACES.iter().map(|s| space_name(*s)).collect(),
        space_cells,
    }
}
