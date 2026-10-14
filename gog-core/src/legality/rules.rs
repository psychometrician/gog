//! The Mark × Channel table: what each mark must, can or cannot take from each
//! channel, and whether the engine draws it.

use super::*;

// ---------------------------------------------------------------------------
// Rule vocabulary
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Obligation {
    /// The mark cannot render without this channel.
    Must,
    /// Optional; the mark renders with or without it.
    Can,
    /// The mark has no such visual feature. Binding it is meaningless.
    Cannot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VarType {
    Continuous,
    Discrete,
    Either,
}

impl VarType {
    /// Does a slot declared as `self` accept a column whose actual type is `actual`?
    pub(super) fn accepts(self, actual: VarType) -> bool {
        matches!(self, VarType::Either) || self == actual
    }

    pub(super) fn describe(self) -> &'static str {
        match self {
            VarType::Continuous => "continuous (numeric)",
            VarType::Discrete => "categorical (text)",
            VarType::Either => "continuous or categorical",
        }
    }
}

/// What the grammar permits for one `(mark, channel)` pair, and what the
/// renderer can actually do with it today.
#[derive(Debug, Clone, Copy)]
pub struct Rule {
    pub obligation: Obligation,
    /// Variable types the grammar permits.
    pub accepts: VarType,
    /// Variable types the renderer handles today — always a subset of
    /// `accepts`. `None` means the channel is not drawn at all yet.
    pub renders: Option<VarType>,
    /// Whether `style()` may *set* this feature to a constant.
    ///
    /// Separate from `obligation` because the two answer different questions.
    /// `obligation` asks "may a column be **mapped** here?"; `settable` asks
    /// "does this mark **have** this visual feature at all?" They come apart
    /// exactly once, and instructively: a polyline has one stroke, so `opacity`
    /// and `size` cannot vary per row (`Cannot`) yet are perfectly meaningful
    /// as one constant for the whole line (`settable`).
    pub settable: bool,
}

pub(super) const fn rule(obligation: Obligation, accepts: VarType, renders: Option<VarType>) -> Rule {
    Rule { obligation, accepts, renders, settable: false }
}

impl Rule {
    /// This mark has the visual feature, so `style()` may set it to a constant.
    const fn settable(mut self) -> Self {
        self.settable = true;
        self
    }
}

/// Shorthand: a channel this mark has no feature for. Neither mapped nor set.
pub(super) const CANNOT: Rule = rule(Obligation::Cannot, VarType::Either, None);

/// The mark *has* this visual feature, but it cannot vary row by row — a
/// polyline is drawn with a single stroke. Mapping is illegal; `style()` sets
/// it once for the whole layer.
pub(super) const SET_ONLY: Rule = CANNOT.settable();

/// The legal fill-texture values for `style(pattern = )` on a fill mark: `solid`
/// (the shared no-texture default) plus the five grayscale- and colorblind-safe
/// hatchings the renderer draws as `<pattern>` tiles (`render::pattern`). The one
/// list both the legality check here and the renderer read — a stroke takes the
/// dash values instead (`solid`/`dashed`/`dotted`), one realization per geometry
/// (spec §4, the settable rule). Small and plain on the `shape` precedent: seven
/// glyphs, five textures.
pub(crate) const FILL_TEXTURES: [&str; 6] =
    ["solid", "hatch", "crosshatch", "stripes", "grid", "dots"];

// ---------------------------------------------------------------------------
// The table
//
// Derived from the Variable × Visual Mapping framework. The
// `renders` column is deliberately narrower than `accepts` in several places —
// that gap is exactly what produces `Unsupported` rather than silent nothing.
//
// **`play` is the one channel whose row is identical on every mark**, and that
// uniformity is a statement rather than a shortcut. Every other channel asks
// something of a mark's geometry — a point has no texture, a line has no glyph,
// a string's form is its content — so its row varies with what the geometry can
// carry. `play` asks nothing of geometry at all: a frame is a **subset of the
// rows**, and every mark can draw a subset, because drawing a subset is what a
// mark already does in every facet panel. So there is no mark for which it could
// be refused without inventing an exception Law 1 exists to catch, and it takes
// `Either` because the column names frames rather than measuring anything —
// exactly as `facet` splits on categories, and see `data::frames_across` for why
// it accepts a number where `facet` refuses one.
// ---------------------------------------------------------------------------

pub fn rule_for(mark: &Mark, channel: &Channel) -> Rule {
    use Channel::*;
    use Obligation::*;
    use VarType::*;

    match mark {
        Mark::Point => match channel {
            X => rule(Must, Either, Some(Either)), // categorical x = strip plot
            Y => rule(Must, Either, Some(Either)), // categorical y = horizontal strip plot
            // Both: a categorical palette for text, a sequential ramp for
            // numbers. `line` and `bar` stay discrete-only — see their rows.
            Color => rule(Can, Either, Some(Either)).settable(),
            Size => rule(Can, Continuous, Some(Continuous)).settable(),
            Shape => rule(Can, Discrete, Some(Discrete)).settable(),
            Pattern => CANNOT, // a point's form is `shape`, not a texture
            Opacity => rule(Can, Continuous, Some(Continuous)).settable(),
            // Points are not connected, so `group` joins nothing here. It splits
            // what a statistic gathers (`point * mean + group(era)`: one mean per
            // era) or what `dodge` sets apart, as `color` does without the color.
            // A bare `point + group()` changes nothing and is refused toward
            // those (`check_group_gathers`).
            Group => rule(Can, Discrete, Some(Discrete)),
            // A label rides `text`, not `point`; a labeled scatter is the
            // superposition `point + text`, so `label` need not ride other marks.
            Label => CANNOT,
            // The third position, now drawn: `point + x + y + z` is a 3-D scatter
            // (spec §15). Continuous only — a categorical z would need a labeled
            // category axis on the cube, which is not built — so a category on z
            // is refused with direction, not half-drawn. Which marks draw in the
            // cube is this column and nothing else (`rule_for(_, Z).renders`), and
            // a blank cell is not one verdict but three: `line`/`step`/`area`/
            // `ribbon` are *ruled out* and point at `path`, `rule`/`zone` wait on
            // occlusion, and `text` alone is the plain "not drawn yet".
            // `z_refusal` is where each says which it is.
            Z => rule(Can, Continuous, Some(Continuous)),
            Play => rule(Can, Either, Some(Either)), // a frame is a subset of rows
        },

        // The path/region family — `line`, `step`, `area`, `ribbon` — shares one
        // rule the glyph marks do not have: **its two axes are different kinds of
        // thing.** `x` is the *domain*, the axis the path is read along (the sort
        // that orders the vertices, the axis a step holds a value across); `y` is
        // the *measure*, the quantity the path traces and the region closes on. So
        // the two positions do not take the same row, and that is not the Law-2
        // gap it looks like: `point` and `text` place a glyph, and a glyph's two
        // axes *are* the same kind, which is why both of theirs read `Either`.
        //
        // The domain takes either type. A category is a place to read a value at,
        // and connecting values across categories in axis order is the profile
        // plot — the same reading a categorical `bar` gives, with the tops joined.
        // (In polar it is the radar.) The measure stays continuous: a mean of
        // category names is not a quantity, and there is no baseline for a region
        // to close on.
        Mark::Line => match channel {
            X => rule(Must, Either, Some(Either)), // categorical x = the profile
            Y => rule(Must, Continuous, Some(Continuous)),
            // Either type, and the two readings are the two questions `color`
            // asks of any mark (spec §6). A **category** says which series this
            // stroke is, and splits the mark into one stroke per group. A
            // **measure** says what the route was carrying as it went, and varies
            // the color along a single stroke.
            //
            // The second reading is what `size` and `opacity` cannot have, and
            // the difference is not arbitrary: a stroke has one width and one
            // opacity because those are properties of the *whole* element, while
            // a stroke's color can be read off the piece of it you are looking
            // at. So the split here is between "a feature of the line" and "a
            // feature of the place on the line", not between channels that were
            // convenient and channels that were not.
            Color => rule(Can, Either, Some(Either)).settable(),
            Group => rule(Can, Discrete, Some(Discrete)),
            // A polyline is drawn with one stroke, so a per-row channel has
            // nothing to vary along — but one stroke still has a width and an
            // opacity, and `style()` sets those. `size` and `opacity` must give
            // the same verdict here; differing would be a per-channel exception.
            Size => SET_ONLY,    // stroke width
            Opacity => SET_ONLY, // stroke opacity
            Shape => CANNOT,     // a line has no glyph
            // A polyline is one stroke per series, so a mapped `pattern` gives one
            // dash per series (the way `color` gives one hue) — mappable, unlike
            // `size`/`opacity` which can't vary within a single stroke.
            Pattern => rule(Can, Discrete, Some(Discrete)).settable(),
            Label => CANNOT,     // no label channel outside `text`
            Z => rule(Can, Either, None),
            Play => rule(Can, Either, Some(Either)), // a frame is a subset of rows
        },

        // `step` is `line` with right-angle interpolation, so its row is `line`'s
        // row — the No Exceptions law made literal. Same obligations, same
        // set-only stroke, same discrete split. The difference is entirely in the
        // renderer (where the path steps instead of slanting), never in what the
        // mark may be bound to.
        Mark::Step => match channel {
            X => rule(Must, Either, Some(Either)), // categorical x = the stepped profile
            Y => rule(Must, Continuous, Some(Continuous)),
            Color => rule(Can, Either, Some(Either)).settable(), // `line`'s row, unchanged
            Group => rule(Can, Discrete, Some(Discrete)),
            Size => SET_ONLY,    // stroke width
            Opacity => SET_ONLY, // stroke opacity
            Shape => CANNOT,     // a stepped line has no glyph
            Pattern => rule(Can, Discrete, Some(Discrete)).settable(), // one dash per series
            Label => CANNOT,     // no label channel outside `text`
            Z => rule(Can, Either, None),
            Play => rule(Can, Either, Some(Either)), // a frame is a subset of rows
        },

        Mark::Bar => match channel {
            // Either axis may carry the categories and either may carry the
            // measure — which is which is a property of the *pair*, not of one
            // channel, so `slot_orient` and `check_slot_shape` decide it. See
            // "Orientation" below.
            X => rule(Must, Either, Some(Either)),
            Y => rule(Must, Either, Some(Either)),
            Color => rule(Can, Discrete, Some(Discrete)).settable(),
            Size => CANNOT,  // bar extent is y; size would be redundant
            Shape => CANNOT, // a bar has no glyph
            Pattern => rule(Can, Discrete, Some(Discrete)).settable(), // one hatch per category
            // Each row is its own rectangle, so `group` joins nothing. It splits
            // what a statistic gathers (`bar * mean * dodge + group(era)`: one
            // mean per era, side by side) or what `dodge` and `stack` arrange, as
            // `color` does without the color. A bare `bar + group()` changes
            // nothing and is refused toward those (`check_group_gathers`).
            Group => rule(Can, Discrete, Some(Discrete)),
            Opacity => rule(Can, Continuous, Some(Continuous)).settable(),
            Label => CANNOT,     // no label channel outside `text`
            // The third position, drawn — a column standing on the floor of the
            // cube. `bar` is the first *slot* mark to take it, and the reason is
            // the one that kept `line`/`step`/`area` out (see `Mark::Path`): those
            // refuse because they read a **domain** left to right, and the cube has
            // no left to right. A bar reads no domain along its length. Its length
            // *is* the measurement, from a baseline, and a baseline is a fact about
            // one axis rather than a direction of travel across two — so it means
            // in the cube exactly what it means in the plane, which is what Law 2
            // asks of a mark that gains a space.
            //
            // **In `space`, `z` measures, always.** Flat, which axis measures is a
            // property of the *pair* and `slot_orient` reads it off the bindings;
            // in the cube `x` and `y` are the two edges of the bar's footprint, so
            // the measurement has nowhere else to go. That is a consequence of the
            // footprint being two-dimensional, not a convention — and it is why
            // there is no 3-D counterpart of the horizontal bar to decide.
            //
            // Continuous, matching `point` and `path`: a categorical `z` would need
            // a labeled category axis on the cube, which is not built. Note this is
            // the type of a *bound* `z`; the histogram binds none, because `bin`
            // synthesizes the count onto it exactly as it synthesizes `y` flat.
            Z => rule(Can, Continuous, Some(Continuous)),
            Play => rule(Can, Either, Some(Either)), // a frame is a subset of rows
        },

        // An area is *one* filled region, bounded by the data above and a
        // baseline below — Wilkinson ch. 8 draws the distinction explicitly:
        // the area graph "represents a single area", where a histogram is "a
        // collection of areas, one for each bar". That single boundary decides
        // this whole row.
        Mark::Area => match channel {
            // The domain takes either type (see `line`) — a filled profile across
            // categories, and the filled radar in polar. The measure does not, and
            // *that* is what makes an area orientation-free where a bar is not: an
            // area always fills down to its baseline, so which axis measures is
            // settled by the mark rather than read off the pair (`slot_orient`).
            X => rule(Must, Either, Some(Either)),
            Y => rule(Must, Continuous, Some(Continuous)),
            // A categorical variable *splits* a graphic (Wilkinson 8.1.5), so
            // `color` draws one region per category — the same split `line`
            // makes into one polyline per category.
            Color => rule(Can, Discrete, Some(Discrete)).settable(),
            Group => rule(Can, Discrete, Some(Discrete)),
            // One region has one fill, so a per-row channel has nothing to
            // vary along: a row here is a *vertex of the boundary*, not a
            // region of its own. That is `line`'s reasoning one dimension up,
            // and it must give the same verdict or it is a per-mark exception.
            Opacity => SET_ONLY,
            // Here `area` parts company with `line`, and the divergence is
            // principled. A stroke's width is free, so `line` makes `size` a
            // setting; an area's extent is pinned by x, y and the baseline, so
            // there is no size left to set. Wilkinson ch. 10: "size for area is
            // a data attribute, not an arbitrary value we may change for
            // aesthetic purposes."
            Size => CANNOT,
            Shape => CANNOT, // a region has no glyph
            Pattern => rule(Can, Discrete, Some(Discrete)).settable(), // one hatch per region
            Label => CANNOT, // no label channel outside `text`
            Z => rule(Can, Either, None),
            Play => rule(Can, Either, Some(Either)), // a frame is a subset of rows
        },

        // Spans low→high at each x — the error-bar / range whisker. Its extents
        // come from a range transform, which `interval` therefore requires
        // (`check_interval`); on its own it has nothing to span. `color` splits it
        // into one whisker per group — the discrete split `line`/`bar` make, with
        // the statistic run within each group. `size`/`opacity` stay set-only: one
        // whisker has a single width and opacity, nothing to vary per row.
        Mark::Interval => match channel {
            // A slot mark's two axes have the *same* role — one holds the slots,
            // the other the measure — and which is which is read off the pair
            // (`slot_orient`), never written as a `flip`. So both rows say Either
            // and `check_slot_shape` enforces "exactly one measures". A category on
            // `y` is the **horizontal error bar**; the relaxation is `bar`'s rule
            // applied to the mark it was always owed to (spec §6).
            X => rule(Must, Either, Some(Either)),
            Y => rule(Must, Either, Some(Either)),
            Color => rule(Can, Discrete, Some(Discrete)).settable(),
            Size => SET_ONLY,    // whisker stroke width
            Opacity => SET_ONLY, // whisker stroke opacity
            Shape => CANNOT,     // a whisker has no glyph
            Pattern => rule(Can, Discrete, Some(Discrete)).settable(), // one dash per whisker
            Group => rule(Can, Discrete, Some(Discrete)),
            Label => CANNOT,     // no label channel outside `text`
            // The third position, drawn (2026-07-26) — and it needed no ruling of
            // its own, because `is_slot_mark` had already made one for all three.
            // A whisker stands in a slot and spans along the other axis; give it a
            // cube and it stands on a *cell* and spans along `z`, which is `bar`'s
            // 3-D histogram with the length replaced by a low→high pair. §5's
            // dimensionality subtraction gives the floor with no change, and
            // `slot_orient` has said since it was written that "a bar's length, a
            // whisker's span and a box's summary are the same question asked of the
            // same pair of axes".
            //
            // Continuous only, matching `point` and `bar`: a categorical `z` would
            // need a labeled category axis on the cube, which is not built.
            Z => rule(Can, Continuous, Some(Continuous)),
            Play => rule(Can, Either, Some(Either)), // a frame is a subset of rows
        },

        // The box-and-whisker glyph — the five-number summary per group (§6). Like
        // `interval` it spans low→high, but it carries its own summary (injected
        // `Transform::Box`), so its minimum syllable is `box + x + y`. `x` places
        // the boxes (a category, or a number); `y` is the measured column the
        // summary reduces. `color` splits it into one box per group — the discrete
        // split `bar`/`interval` make; the summary runs within each group. Fill and
        // width are the box's own; `size`/`opacity` stay set-only (one box, one of
        // each), and there is no glyph to pick, so `shape` is refused.
        Mark::Box => match channel {
            // Either axis may hold the slots and either the measure, as on `bar`
            // and `interval` — a category on `y` is the **horizontal box plot**.
            // `check_slot_shape` refuses the pair that measures nothing.
            X => rule(Must, Either, Some(Either)),
            Y => rule(Must, Either, Some(Either)),
            Color => rule(Can, Discrete, Some(Discrete)).settable(),
            Size => SET_ONLY,    // whisker/box stroke width
            Opacity => SET_ONLY, // box fill/stroke opacity
            Shape => CANNOT,     // a box has no glyph to choose
            Pattern => rule(Can, Discrete, Some(Discrete)).settable(), // one hatch per box
            Group => rule(Can, Discrete, Some(Discrete)),
            Label => CANNOT,     // no label channel outside `text`
            // The third position, drawn (2026-07-26) — `interval`'s row, for
            // `interval`'s reason: the third slot mark stands its summary on a cell
            // and reduces along `z`. See `Mark::Interval`.
            Z => rule(Can, Continuous, Some(Continuous)),
            Play => rule(Can, Either, Some(Either)), // a frame is a subset of rows
        },

        // A filled band from a low boundary to a high one across a continuous x
        // (§6). Its channel row is `area`'s — one filled region, so `color`/`group`
        // split it, `opacity` is set-only (one fill, nothing to vary per row), and
        // `size` cannot even be set (the extent is pinned by x and the two
        // boundaries). It parts from `area` on where the region is bounded, not on
        // what may be mapped: an area closes on a baseline the grammar knows (0), a
        // ribbon on the low/high pair a range transform synthesizes — so the mark
        // requires that transform (`check_span_needs_range`), exactly as `interval`
        // does. It does *not* part from `area` on the domain: a band across
        // categories is the spread profile, and refusing it because `interval`
        // draws the tidier chart would be taste enforced as legality (Law 8).
        // `shape` is refused (a region has no glyph), `label` rides `text`.
        Mark::Ribbon => match channel {
            X => rule(Must, Either, Some(Either)),
            Y => rule(Must, Continuous, Some(Continuous)),
            Color => rule(Can, Discrete, Some(Discrete)).settable(),
            Group => rule(Can, Discrete, Some(Discrete)),
            Opacity => SET_ONLY, // one region, one fill
            Size => CANNOT,      // extent pinned by x and the two boundaries
            Shape => CANNOT,     // a region has no glyph
            Pattern => rule(Can, Discrete, Some(Discrete)).settable(), // one hatch per band
            Label => CANNOT,     // no label channel outside `text`
            Z => rule(Can, Either, None),
            Play => rule(Can, Either, Some(Either)), // a frame is a subset of rows
        },

        // A glyph mark whose glyph is a *string* — `point`'s sibling (§6). Both
        // place one glyph per row at (x, y); they differ in where the glyph comes
        // from. `point` picks it from a closed set of seven (`shape`); `text` takes
        // it from a column — the `label` channel, which `text` therefore
        // **requires** (its minimum syllable, §7): x/y place it, `label` fills it.
        // `shape` is refused (a string is not one of the seven glyphs) and so is
        // `group` (a per-row glyph, like a point, has nothing to connect). `color`
        // maps by category (the palette) or sets. Mapped `size`/`opacity` are
        // valid grammar not yet drawn (`renders: None` → Unsupported, not silent) —
        // set them with `style()`; a per-row font size (a word cloud) is the
        // parked follow-up.
        Mark::Text => match channel {
            X => rule(Must, Either, Some(Either)),
            Y => rule(Must, Either, Some(Either)),
            Label => rule(Must, Either, Some(Either)),
            Color => rule(Can, Discrete, Some(Discrete)).settable(),
            // `point`'s two scaled channels, read the same way (built 2026-10-09):
            // the channel fraction a dot turns into a radius, a label turns into a
            // font size (`encode::text_size_at`), and an opacity is an opacity.
            Size => rule(Can, Continuous, Some(Continuous)).settable(), // font px
            Opacity => rule(Can, Continuous, Some(Continuous)).settable(),
            Shape => CANNOT, // a string is not one of the seven glyphs
            Pattern => CANNOT, // a string's form is its content, not a texture
            // A label is one row's word, as a point is one row's dot, so `group`
            // joins nothing. It splits what a statistic gathers (`text * mean +
            // group(era)`: one mean per era) or what `dodge` sets apart, so the
            // labels of dodged bars stand over their own bars, as `color` does
            // without the color. A bare `text + group()` changes nothing and is
            // refused toward those (`check_group_gathers`).
            Group => rule(Can, Discrete, Some(Discrete)),
            // A label at a place in the cube, `point`'s third position read the
            // same way: continuous only, projected, nearer labels over farther.
            Z => rule(Can, Continuous, Some(Continuous)),
            Play => rule(Can, Either, Some(Either)), // a frame is a subset of rows
        },

        // `path` strokes the rows in the data's own order, and that one
        // difference from `line` decides its whole row. A line is a *function* —
        // it sorts by x, so x is the domain and y the measure, two different
        // roles, which is why the path/region family's positions do not match.
        // A path sorts nothing: it visits row 1, then row 2, and its two axes
        // are the same kind of thing, two positions with neither privileged. Ask
        // §6's role test — *do these two axes have the same role?* — and the
        // answer flips from `line`'s to `point`'s, so **`path` takes `point`'s
        // position row**, either type on both. The same test that gave `box` and
        // `interval` their orientation, run on a different pair.
        //
        // Everything below the positions is `line`'s, because everything below
        // the positions is about a *stroke*, and a path is one: one width, one
        // opacity, one dash, split into several strokes by a discrete channel.
        Mark::Path => match channel {
            X => rule(Must, Either, Some(Either)),
            Y => rule(Must, Either, Some(Either)),
            Color => rule(Can, Either, Some(Either)).settable(), // `line`'s row, unchanged
            Group => rule(Can, Discrete, Some(Discrete)),
            Size => SET_ONLY,    // stroke width
            Opacity => SET_ONLY, // stroke opacity
            Shape => CANNOT,     // a stroke has no glyph
            Pattern => rule(Can, Discrete, Some(Discrete)).settable(), // one dash per series
            Label => CANNOT,     // no label channel outside `text`
            // The third position, drawn — and `path` is the only *stroke* that
            // takes it. That is not the Law-1 gap it looks like, because it is
            // the same distinction that separates `path` from `line` in the
            // plane, read one dimension up.
            //
            // A path strokes its rows in the table's order, and an order is not a
            // property of any axis, so it survives the third dimension untouched:
            // the route is the same route however the cube is turned. A `line`
            // sorts by `x` and draws one `y` for each — a *function*, read left to
            // right along a domain — and in the cube there is no left to right.
            // `x` is one of three equal positions, and at some viewing angles it
            // runs straight into the page and becomes depth rather than position
            // (`project::Scene`'s own test pins that). A line in space would be
            // sorted by an axis the reader cannot see, which is a reading rule
            // that stopped being readable. So the whole **path/region family** —
            // `line`, `step`, `area` and `ribbon`, the four marks whose `x` is a
            // domain — keeps `renders: None` and refuses with direction, and the
            // space curve is `path`'s alone for the reason the connected
            // scatterplot is. `z_refusal` gives them the words; the list there is
            // the same four, and this comment named only three of them until
            // 2026-07-26 for want of anyone checking `ribbon` against it.
            //
            // Continuous only, matching `point`: a categorical z would need a
            // labeled category axis on the cube, which is not built.
            Z => rule(Can, Continuous, Some(Continuous)),
            Play => rule(Can, Either, Some(Either)), // a frame is a subset of rows
        },

        // The one-position mark: `rule` sits at a value on one axis and spans the
        // other, which the panel supplies. Its position row is the only one in the
        // table where both axes are `Can` rather than `Must`, and that is Law 7's
        // second relaxation rather than a gap — a rule needs *a* position, just not
        // both, so the obligation "exactly one of these two" cannot be written per
        // channel. `rule_axis` states it once, and `check_rule` enforces it; the
        // per-channel column here only records that either axis may carry it.
        //
        // Which axis it is decides the orientation, so both accept either type,
        // for `path`'s reason rather than `bar`'s: a rule sorts nothing and
        // measures nothing, so neither axis is a domain and neither is privileged
        // — a rule at a category is as ordinary as a rule at a number.
        //
        // Everything below the positions is a *stroke*'s row (`line`'s, `path`'s):
        // one width, one opacity, one dash per rule. `color` is discrete like the
        // rest of that class rather than either-typed like `point`'s — a rule is
        // paint on a hairline, not a filled glyph, so what it can carry is a
        // handful of named categories (a threshold's severity), not a ramp to
        // read a value off. Where it *does* part from `line` is per row rather
        // than per series: each row here is its own segment, so nothing has to be
        // connected before it can be colored, which is also why `group` is
        // refused.
        Mark::Rule => match channel {
            X => rule(Can, Either, Some(Either)),
            Y => rule(Can, Either, Some(Either)),
            Color => rule(Can, Discrete, Some(Discrete)).settable(),
            Size => SET_ONLY,    // stroke width
            Opacity => SET_ONLY, // stroke opacity
            Shape => CANNOT,     // a stroke has no glyph
            Pattern => rule(Can, Discrete, Some(Discrete)).settable(), // a dashed threshold
            // Nothing to connect: each row is its own segment, as on `point`.
            Group => CANNOT,
            Label => CANNOT, // no label channel outside `text`
            Z => rule(Can, Either, None),
            Play => rule(Can, Either, Some(Either)), // a frame is a subset of rows
        },

        // A filled rectangle — `rule`'s sibling one dimension up. Its position
        // row is `rule`'s and for the same reason: both axes are `Can`, because a
        // zone needs *at least one* bounded axis and takes the other from the
        // panel, and "at least one of these two" cannot be written per channel.
        // `check_zone` states it once.
        //
        // Neither axis is a domain here either — a zone sorts nothing and
        // measures nothing by length, so both take either type. That is not a
        // convenience: it is what makes a **heatmap cell** this mark, a rectangle
        // filling its slot on two categorical axes with the measure moved to
        // `color` (spec §3's objection, answered).
        //
        // Its aesthetics are a *fill*'s, so its row below the positions is
        // `area`'s and `ribbon`'s: one region, `opacity` set-only, no glyph, a
        // hatch rather than a dash. `color` maps **per row** — each row is its own
        // rectangle, nothing has to be connected first — and takes **either**
        // type, unlike the other fills: a zone is the one fill with enough area to
        // decode a ramp from, which is exactly what a heatmap reads.
        Mark::Zone => match channel {
            X => rule(Can, Either, Some(Either)),
            Y => rule(Can, Either, Some(Either)),
            Color => rule(Can, Either, Some(Either)).settable(),
            Opacity => SET_ONLY, // one region, one fill
            Size => CANNOT,      // the extent is the bounds; there is no size
            Shape => CANNOT,     // a rectangle has no glyph
            Pattern => rule(Can, Discrete, Some(Discrete)).settable(), // one hatch per zone
            // **The boundary-supplied extent, and the choropleth with it.** This
            // read `CANNOT — each row is its own rectangle`, which was true of
            // every source of sides this mark had: a category's slot, `bounds`,
            // `bin` and `density` all describe one row's rectangle. A **boundary**
            // is the fifth, and it is the first that arrives as *many rows, one
            // shape*, so it needs a word for which rows are one region — which is
            // what `group` has always meant, split into one series per category
            // without encoding it. A `line` gets one polyline per category and a
            // zone gets one filled region; the channel did not change.
            //
            // Only a boundary can use it, so `check_zone_group` refuses it wherever
            // the sides come from one of the other four. That refusal is the reason
            // this cell can be `Can` without Law 2 being bent: the rule table says
            // the pair is legal, and the sentence that cannot read a boundary says
            // so itself, rather than the table quietly meaning two things.
            Group => rule(Can, Discrete, Some(Discrete)),
            Label => CANNOT,     // no label channel outside `text`
            Z => rule(Can, Either, None),
            Play => rule(Can, Either, Some(Either)), // a frame is a subset of rows
        },

        // The sheet through a field's samples — a mesh of faces between the rows,
        // where a 3-D `bar` stands a column on a cell (spec §15).
        Mark::Surface => match channel {
            // **Three positions, all required, all continuous.** Required because a
            // sheet without a height is not a surface — Law 7's minimum syllable,
            // and `z` is the one it shares with no other mark. Continuous because a
            // face asserts every value *between* two nodes, and between category A
            // and category B there is no between: the cut-versus-slot distinction
            // one dimension up, which is why a `bar`'s floor may be slotted and a
            // surface's may not. `z` is `Must` and still satisfied by
            // `surface * density + space()`, which synthesizes it exactly as
            // `bar * bin` does (`synthesizes_measure`).
            X | Y | Z => rule(Must, Continuous, Some(Continuous)),
            // **Either, and the measured half is what a mesh can do that a region
            // cannot.** `area`/`ribbon` refuse a measured color because a region has
            // one interior and coloring it by a measure is a gradient fill. A face
            // is already small enough to hold one value, so the ramp lands per face
            // — the height-colored surface, with no gradient machinery. A category
            // colors each face too, the way it colors each bar: a mesh is a
            // collection of faces, and the sheet stays one sheet. Sheets are
            // `group`'s, one lattice per value (below). This comment said a category
            // split the mark into sheets, which the engine never did; corrected
            // 2026-10-13 rather than built, on the author's word, because splitting
            // by color would open a gap where `band`'s two colors meet.
            Color => rule(Can, Either, Some(Either)).settable(),
            // **Mapped, and for `color`'s reason one line up rather than `area`'s.**
            // This cell read `renders: None` and cited an area's refusal — *a face has
            // four nodes, so a per-row opacity has no single answer at one* — which
            // stopped being true when the ramp moved to the face's center. An `area`
            // refuses because a row there is a **vertex of one region**, so there is no
            // second region for a per-row value to differ from; a mesh is a *collection*
            // of faces, Wilkinson's own contrast (ch. 8) between "the area graph, which
            // represents a single area" and a "collection of areas, one for each bar",
            // and a face is small enough to hold one value. Which value is settled
            // already: the mean of the four corners, the bilinear patch read at its own
            // center, the same answer `color` takes. Two channels reading one face two
            // ways would have been the per-mark exception Law 2 forbids.
            //
            // What it draws is the sheet that fades where its evidence thins — a fitted
            // surface faded by its own confidence, extrapolated corners barely there.
            // Still settable, so two sheets can be seen through each other.
            Opacity => rule(Can, Continuous, Some(Continuous)).settable(),
            // Splits into one sheet per category without coloring — `line`'s and
            // `path`'s `group`, and it means the same thing here.
            Group => rule(Can, Discrete, Some(Discrete)),
            // A sheet's extent is pinned by its lattice, so there is nothing left to
            // size — `area`'s refusal, for `area`'s reason.
            Size => CANNOT,
            Shape => CANNOT,     // a face is not a glyph
            Label => CANNOT,     // no label channel outside `text`
            // A hatch tile is a texture in *screen* space, and every face of a
            // projected mesh is foreshortened differently, so one tile would read as
            // a different density on every face — a texture that varies with the
            // viewing angle rather than with the data, which is the light this mark's
            // shading is defined not to be.
            Pattern => CANNOT,
            Play => rule(Can, Either, Some(Either)), // a frame is a subset of rows
        },

        // The stroke whose two endpoints the layout supplies (spec §15, the
        // network entry) — after `surface`, the second mark whose flat cell is
        // empty. Its positions are not channels at all: the endpoints ride
        // `layout`-synthesized columns, which is the `ymin`/`ymax` ruling
        // holding for a second endpoint exactly as it held for a second
        // extent. So all three positions are `Cannot` — under `layout` a
        // binding has nothing left to say, and the cube arrives from the
        // *space's* stated view, never from a `z`.
        //
        // Its aesthetics are a stroke's, `rule`'s row with two turns of its
        // own. `opacity` **maps**, continuously — the one stroke where it
        // does, because an edge's weight is the quantity a network reader
        // fades by, and each row is its own stroke with its own value where a
        // `line` is one stroke with one. `size` stays set-only: mapping a
        // column to stroke width is the recorded open linewidth question for
        // the whole stroke family, and one mark must not decide it alone.
        Mark::Edge => match channel {
            X => CANNOT,
            Y => CANNOT,
            Z => CANNOT,
            Color => rule(Can, Discrete, Some(Discrete)).settable(),
            Opacity => rule(Can, Continuous, Some(Continuous)).settable(),
            Size => SET_ONLY,    // stroke width; the linewidth question, whole
            Shape => CANNOT,     // a stroke has no glyph
            // The settable dash arrives with the stroke class (the settable
            // rule), and a *mapped* one draws per row, as `color` does: each row
            // is its own stroke, so each can carry its category's dash.
            Pattern => rule(Can, Discrete, Some(Discrete)).settable(),
            Group => CANNOT,     // one row is one edge already
            Label => CANNOT,     // names belong to `text * layout + label(name)`
            // Valid grammar the engine does not draw (◌): a played layout is not
            // built, and `graph.rs` refuses it toward `| facet()`, where every panel
            // shares one placement. Every other mark plays where it has no layout,
            // but an edge is drawn under a layout or not at all, so its cell never
            // draws. It read ✅ until 2026-10-13, when walking the mark × channel
            // grid with the author found it promising a picture the gate refuses.
            Play => rule(Can, Either, None),
        },
    }
}

/// Does this engine draw this mark at all?
///
/// **Every mark in the kernel answers `true` since 2026-07-26**, `surface` having
/// been the last to gain a renderer. Kept, rather than deleted along with the
/// refusal it used to gate, for two live reasons: `rules_matrix` reads it to decide
/// which rows the generated grids footnote as not-yet-drawn, and the *next* mark
/// added to `Mark` has to declare itself here rather than defaulting to drawable.
/// A total match, so that declaration cannot be forgotten.
pub(super) fn is_drawable(mark: &Mark) -> bool {
    match mark {
        Mark::Point | Mark::Line | Mark::Area | Mark::Bar | Mark::Step
        | Mark::Interval | Mark::Box | Mark::Ribbon | Mark::Text | Mark::Path
        | Mark::Rule | Mark::Zone | Mark::Surface | Mark::Edge => true,
    }
}

/// Returns `true` when the mark was refused, and the caller then skips that
/// layer's remaining checks. One message is the whole story: the per-channel
/// `renders: None` arm would otherwise add "`x(a)` would have no visual effect
/// — remove it", which is both redundant and wrong, since `x` is *required*
/// here and removing it is not a fix a user can apply.
///
/// **No mark reaches the refusal today** — the direction list emptied when `surface`
/// was built, and `every_mark_is_drawn_or_refused` now reads as "every mark is
/// drawn". It stays because the failure it was written for is not hypothetical: an
/// `area` sat in `Mark` with no writer and rendered an empty panel, exiting 0, for as
/// long as it took someone to notice. `render::svg`'s exhaustive match is the compile-time
/// half of that guard and this is the *diagnostic* half — a new mark that cannot draw
/// yet says so here, with its direction, instead of coming out blank.
pub(super) fn check_mark(out: &mut Vec<Diagnostic>, mark: &Mark) -> bool {
    let direction: Option<&str> = match mark {
        // Everything the renderer draws — which is all of it.
        Mark::Point | Mark::Line | Mark::Area | Mark::Bar | Mark::Step | Mark::Interval
        | Mark::Box | Mark::Ribbon | Mark::Text | Mark::Path | Mark::Rule
        | Mark::Zone | Mark::Surface | Mark::Edge => None,
    };
    let Some(direction) = direction else { return false };
    out.push(Diagnostic {
        kind: DiagnosticKind::Unsupported,
        message: format!(
            "gog: `{}` is valid grammar, but this engine does not draw it: the plot \
             would come out empty. {direction}",
            mark_name(mark),
        ),
    });
    true
}
