//! Legality — which `(mark, channel)` bindings are grammatical, and which the
//! engine can actually render today.
//!
//! This is the "syllable formation rule" of the grammar. In Hangeul a syllable
//! block requires an initial consonant and a vowel; you cannot write `ㅏㅏ`, and
//! the writing system rejects it structurally rather than rendering a blank.
//! A gog *layer* is the same unit: a mark plus the channels that mark requires.
//!
//! Two orthogonal questions per binding:
//!
//! 1. **Obligation** — must / can / cannot be bound to this mark
//! 2. **Variable type** — continuous / discrete / either
//!
//! Diagnostics separate three cases, because the reader needs a different
//! action from each:
//!
//! | Kind | Meaning | What the reader should do |
//! |---|---|---|
//! | `Illegal` | the grammar forbids it | rewrite the expression |
//! | `Unsupported` | grammar allows it, engine can't do it yet | use a different atom, or wait |
//! | `Assumption` | it renders, but a choice was made for you | confirm the default is what you meant |
//!
//! Keeping `Illegal` and `Unsupported` distinct matters: telling someone a
//! valid combination is "illegal" teaches them a rule that isn't real.

use crate::color::{css_rgb, is_valid_color, nearest_color, numbered_shade};
use crate::data::DataFrame;
use crate::ir::{
    Channel, ChannelDef, CoordSpace, Figure, Layer, Mark, PaletteDef, PlotSpec, ScaleType,
    StyleSpec, Transform,
};
use crate::transform::Job;
use std::collections::HashMap;

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
    fn accepts(self, actual: VarType) -> bool {
        matches!(self, VarType::Either) || self == actual
    }

    fn describe(self) -> &'static str {
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

const fn rule(obligation: Obligation, accepts: VarType, renders: Option<VarType>) -> Rule {
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
const CANNOT: Rule = rule(Obligation::Cannot, VarType::Either, None);

/// The mark *has* this visual feature, but it cannot vary row by row — a
/// polyline is drawn with a single stroke. Mapping is illegal; `style()` sets
/// it once for the whole layer.
const SET_ONLY: Rule = CANNOT.settable();

/// The legal fill-texture values for `style(pattern = )` on a fill mark: `solid`
/// (the shared no-texture default) plus the four grayscale- and colorblind-safe
/// hatchings the renderer draws as `<pattern>` tiles (`render::pattern`). The one
/// list both the legality check here and the renderer read — a stroke takes the
/// dash values instead (`solid`/`dashed`/`dotted`), one realization per geometry
/// (spec §4, the settable rule). Small and plain on the `shape` precedent: five
/// glyphs, five textures.
pub(crate) const FILL_TEXTURES: [&str; 5] = ["solid", "hatch", "crosshatch", "grid", "dots"];

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
            Group => CANNOT, // points are not connected
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
            Group => CANNOT,
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
        // from. `point` picks it from a closed set of five (`shape`); `text` takes
        // it from a column — the `label` channel, which `text` therefore
        // **requires** (its minimum syllable, §7): x/y place it, `label` fills it.
        // `shape` is refused (a string is not one of the five glyphs) and so is
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
            Size => rule(Can, Continuous, None).settable(),    // font px: set now, map later
            Opacity => rule(Can, Continuous, None).settable(), // set now, map later
            Shape => CANNOT, // a string is not one of the five glyphs
            Pattern => CANNOT, // a string's form is its content, not a texture
            Group => CANNOT, // a per-row glyph, like a point, connects nothing
            Z => rule(Can, Either, None),
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
            Group => CANNOT,     // each row is its own rectangle
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
            // splits the mark into one sheet per group, interleaved correctly because
            // the depth sort runs over every face of every series at once.
            Color => rule(Can, Either, Some(Either)).settable(),
            // Set, not mapped, for `area`'s reason: a face has four nodes and a
            // per-row opacity has no single answer at one. Settable so two sheets
            // can be seen through each other.
            Opacity => rule(Can, Continuous, None).settable(),
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
    }
}

/// Does this engine draw this mark at all?
///
/// The per-channel `renders: None` mechanism cannot answer this. It reports a
/// *binding* that would have no effect, so it only ever fires for a channel the
/// layer actually carries — and `x`/`y` are plot-scoped, so a layer with no
/// encodings of its own produced no diagnostics whatsoever. `area` therefore
/// rendered an empty panel and exited 0, which is precisely the silent drop
/// this crate exists to refuse. The question "can this mark be drawn?" belongs
/// to the mark, so it is asked once, here.
///
/// Split areas overlap, and the engine chose that on the caller's behalf.
///
/// A split `line` draws polylines, which cross without hiding one another; a
/// split `area` draws opaque regions, and the last one drawn can bury the rest
/// completely. That is a *default* — overlay rather than stack — and §12 says a
/// default only stays silent when one sensible value exists. Two do: `stack`
/// (now built — `area * stack` piles the regions into abutting bands) and a set
/// opacity you can see through.
///
/// Silent when the caller has already answered the question — a `* stack` that
/// resolves the overlap outright, or a `style(opacity = )` they can see through.
/// An Assumption, never fatal: the picture is
/// legitimate, and Law 8 forbids blocking the ugly-but-legal.
fn check_area_overlap(
    out: &mut Vec<Diagnostic>,
    spec: &PlotSpec,
    df: &DataFrame,
    layer: &Layer,
) {
    if layer.mark != Mark::Area
        || layer.style.opacity.is_some()
        || layer.transforms.contains(&Transform::Stack)
    {
        return;
    }
    // **A split violin does not overlap**, so the warning that a split `area` hides
    // itself is false there and was firing on every colored ridgeline. The regions
    // an ordinary split area draws share one domain and stack up on the page; a
    // violin's each stand in their own category's slot, which is what a slot is for.
    // (Two violins genuinely *can* share a slot — when `color` splits *within* a
    // category — and the renderer already answers that the way `box` does, by
    // drawing them translucent, so there is nothing for this warning to add.)
    if slot_density(spec, layer, Some(df)).is_some() {
        return;
    }
    let Some(def) = [Channel::Color, Channel::Group]
        .iter()
        .find_map(|c| binding_of(spec, layer, c))
    else {
        return;
    };
    let n = crate::data::categories_across(&[df], &def.field).len();
    if n < 2 {
        return;
    }
    out.push(Diagnostic {
        kind: DiagnosticKind::Assumption,
        message: format!(
            "gog: `area` split by `{}` draws {n} regions on top of one another, and \
             the last one drawn can hide the rest. Add `style(opacity = 0.5)` to see \
             through them, or use `line` to compare shapes without filling.",
            def.field,
        ),
    });
}

/// Does this engine draw this mark at all?
///
/// **Every mark in the kernel answers `true` since 2026-07-26**, `surface` having
/// been the last to gain a renderer. Kept, rather than deleted along with the
/// refusal it used to gate, for two live reasons: `rules_matrix` reads it to decide
/// which rows the generated grids footnote as not-yet-drawn, and the *next* mark
/// added to `Mark` has to declare itself here rather than defaulting to drawable.
/// A total match, so that declaration cannot be forgotten.
fn is_drawable(mark: &Mark) -> bool {
    match mark {
        Mark::Point | Mark::Line | Mark::Area | Mark::Bar | Mark::Step
        | Mark::Interval | Mark::Box | Mark::Ribbon | Mark::Text | Mark::Path
        | Mark::Rule | Mark::Zone | Mark::Surface => true,
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
/// long as it took someone to notice. `svg.rs`'s exhaustive match is the compile-time
/// half of that guard and this is the *diagnostic* half — a new mark that cannot draw
/// yet says so here, with its direction, instead of coming out blank.
fn check_mark(out: &mut Vec<Diagnostic>, mark: &Mark) -> bool {
    let direction: Option<&str> = match mark {
        // Everything the renderer draws — which is all of it.
        Mark::Point | Mark::Line | Mark::Area | Mark::Bar | Mark::Step | Mark::Interval
        | Mark::Box | Mark::Ribbon | Mark::Text | Mark::Path | Mark::Rule
        | Mark::Zone | Mark::Surface => None,
    };
    let Some(direction) = direction else { return false };
    out.push(Diagnostic {
        kind: DiagnosticKind::Unsupported,
        message: format!(
            "gog: `{}` is part of the grammar, but this engine does not draw it \
             yet — the plot would come out empty. {direction}",
            mark_name(mark),
        ),
    });
    true
}

/// "a bar", but "an area".
///
/// Every mark name went into these templates behind a hardcoded "a" until
/// `area` shipped and three diagnostics started reading "a area has no size to
/// set". A message that tells you what to do should not also be the reason you
/// stop trusting it.
fn article(word: &str) -> &'static str {
    match word.chars().next() {
        Some('a' | 'e' | 'i' | 'o' | 'u') => "an",
        _ => "a",
    }
}

fn mark_name(mark: &Mark) -> &'static str {
    match mark {
        Mark::Point => "point",
        Mark::Line => "line",
        Mark::Area => "area",
        Mark::Bar => "bar",
        Mark::Step => "step",
        Mark::Interval => "interval",
        Mark::Box => "box",
        Mark::Ribbon => "ribbon",
        Mark::Text => "text",
        Mark::Path => "path",
        Mark::Rule => "rule",
        Mark::Zone => "zone",
        Mark::Surface => "surface",
    }
}

fn transform_name(t: &Transform) -> &'static str {
    match t {
        Transform::Bin => "bin",
        Transform::Smooth => "smooth",
        Transform::Count => "count",
        Transform::Density => "density",
        Transform::Sum => "sum",
        Transform::Mean => "mean",
        Transform::Median => "median",
        Transform::Max => "max",
        Transform::Min => "min",
        Transform::Proportion => "proportion",
        Transform::Range => "range",
        Transform::Confidence => "confidence",
        Transform::Box => "box",
        Transform::Bounds => "bounds",
        Transform::Dodge => "dodge",
        Transform::Stack => "stack",
        Transform::Jitter => "jitter",
        Transform::Partition => "partition",
    }
}

fn channel_name(channel: &Channel) -> &'static str {
    match channel {
        Channel::X => "x",
        Channel::Y => "y",
        Channel::Z => "z",
        Channel::Color => "color",
        Channel::Size => "size",
        Channel::Shape => "shape",
        Channel::Pattern => "pattern",
        Channel::Opacity => "opacity",
        Channel::Group => "group",
        Channel::Label => "label",
        Channel::Play => "play",
    }
}

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
pub const ALL_MARKS: [Mark; 13] = [
    Mark::Point, Mark::Line, Mark::Area, Mark::Bar, Mark::Step, Mark::Interval,
    Mark::Box, Mark::Ribbon, Mark::Text, Mark::Path, Mark::Rule, Mark::Zone,
    Mark::Surface,
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
enum Setting {
    Color, Opacity, Size, Shape, Pattern,
    BorderColor, BorderSize, Caps, Center, Nudge, Arrow, Reach,
}

const ALL_SETTINGS: [Setting; 12] = [
    Setting::Color, Setting::Opacity, Setting::Size, Setting::Shape, Setting::Pattern,
    Setting::BorderColor, Setting::BorderSize, Setting::Caps, Setting::Center, Setting::Nudge,
    Setting::Arrow, Setting::Reach,
];

fn setting_name(s: Setting) -> &'static str {
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
    }
}

/// Does this mark carry this setting? The **one** answer the `check_*` refusals
/// and the generated Mark × Setting grid both read, so the grid can never promise
/// a setting the engine refuses (the settable rule, spec §4: a setting spans its
/// geometry class). The channel-backed five defer to `rule_for(_).settable` — the
/// same flag `check_style` gates on; the style-only five name their geometry class
/// here, the sets `check_border`/`check_caps`/`check_center`/`check_nudge` enforce.
fn mark_takes_setting(mark: &Mark, setting: Setting) -> bool {
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
        BorderColor | BorderSize =>
            matches!(mark, Mark::Bar | Mark::Box | Mark::Point | Mark::Surface | Mark::Zone),
        // Interval's own display toggles.
        Caps | Center => matches!(mark, Mark::Interval),
        // A text label's offset.
        Nudge => matches!(mark, Mark::Text),
        // A head on the one mark that has a direction to point in. Not the
        // narrowness it looks like: the settable rule (spec §4) spans a
        // setting across its *geometry class*, and the geometry here is "a
        // stroke whose vertex order is the data's". `line`/`step` sort by x,
        // so their last vertex is an artifact of the sort rather than an end
        // the data chose; `interval` already decorates its ends with `caps`.
        Arrow => matches!(mark, Mark::Path),
        // How far the one-position mark reaches across the axis it does not name.
        // Narrow for the same reason `arrow` is: the geometry class is "a mark
        // whose extent the panel supplies", and `rule` is its only member. Every
        // other mark takes both its extents from the data, so a reach would have
        // nothing to describe.
        Reach => matches!(mark, Mark::Rule),
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

/// A statistic that reduces the data to **one value per x-group** (or per bin),
/// which any *locus* mark can then draw. "One `bin`, three marks" (spec §5)
/// generalized: `point`/`line`/`area`/`bar`/`step` all draw a value at each x.
fn is_value_statistic(t: &Transform) -> bool {
    matches!(
        t,
        Transform::Bin | Transform::Smooth | Transform::Count | Transform::Density
            | Transform::Proportion | Transform::Sum | Transform::Mean | Transform::Median
            | Transform::Max | Transform::Min
    )
}

/// A transform that yields a **low/high pair**, which the *span* marks draw:
/// `interval` whiskers it, `ribbon` fills it, `line`/`step` trace its two
/// boundaries. `range`/`confidence` compute the pair from `y`; `bounds` reshapes a
/// pre-computed one. (`Transform::Box` also emits a pair, but is injected by the
/// `box` mark, never composed, so it is not one of these.)
fn is_pair_transform(t: &Transform) -> bool {
    matches!(t, Transform::Range | Transform::Confidence | Transform::Bounds)
}

/// A collision modifier (Wilkinson §8): an *offset*, not a statistic. The three
/// divide by geometry **and by axis** — `dodge` subdivides a width, `stack`
/// accumulates along a *measure* axis, `jitter` spreads along a *categorical* one
/// — so a mark takes each offset whose precondition it can meet. `point` meets two
/// of them, on different axes, which is why the dot plot is `stack` and the strip
/// plot is `jitter` (spec §5).
fn is_collision_modifier(t: &Transform) -> bool {
    matches!(t, Transform::Dodge | Transform::Stack | Transform::Jitter)
}

/// Which class the book groups this transform under — so the grid can lay the
/// three blocks out without hardcoding membership, the way it reads every other
/// fact off the dump.
fn transform_class(t: &Transform) -> &'static str {
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
/// above). Read the classes off `is_value_statistic` / `is_pair_transform` /
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
    // `path_takes_only_the_field_transform` pins both halves.
    if *mark == Mark::Path {
        return if *transform == Transform::Density { Combines } else { None };
    }

    // `surface` needs a floor whose cells **tile without gaps**, and two transforms
    // give it one. That is the whole rule, and it replaced a narrower one that read
    // `density` as the only answer (2026-07-28, spec §15).
    //
    // *The two ways to tile, and they draw different geometry.* `density` estimates a
    // value at every **node** of the mesh it cut, so the field is defined everywhere
    // and the sheet interpolates between samples — the founding reading. `bin` **cuts**
    // the floor into adjacent cells, and a cut cell asserts one value across its whole
    // extent, so the honest geometry is flat across the cell with a *step* at the
    // boundary: a plateau per cell, the terraced sheet. One mark, one question asked of
    // the floor it was handed — *does this geometry claim anything between its
    // samples?* — and the answer is read off the floor rather than declared.
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
    if *mark == Mark::Surface {
        return match transform {
            Transform::Density | Transform::Bin => Combines,
            Transform::Sum
            | Transform::Mean
            | Transform::Median
            | Transform::Max
            | Transform::Min => Combines,
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

    // The pair transforms `range`/`confidence` compute a band across a domain,
    // which is legal grammar and the wrong mark; `check_zone` says so.
    if *mark == Mark::Zone {
        return match transform {
            Transform::Bounds | Transform::Bin | Transform::Density
            | Transform::Count | Transform::Proportion => Required,
            t if crate::transform::reduces_column(std::slice::from_ref(t)).is_some() => Combines,
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

    // Value statistics → the locus marks (a value drawn at each x). Refused on the
    // span marks (which need a *pair*, not a single value), on `box` (which carries
    // its own summary), and on `text` (whose glyph is its `label`, not a statistic).
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
        // Unreachable: `Box` and the statistics are handled above.
        _ => None,
    }
}

/// Every transform a user composes with `*`, in the grid's teaching order —
/// the ten value statistics, then the three pair transforms, then the three
/// collision modifiers. `Transform::Box` is excluded: the `box` mark injects it,
/// it is never typed, so it is not a column of the Mark × Transform grid.
pub const USER_TRANSFORMS: [Transform; 17] = [
    Transform::Bin, Transform::Smooth, Transform::Count, Transform::Density, Transform::Proportion,
    Transform::Sum, Transform::Mean, Transform::Median, Transform::Max, Transform::Min,
    Transform::Range, Transform::Confidence, Transform::Bounds, Transform::Partition,
    Transform::Dodge, Transform::Stack, Transform::Jitter,
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
}

pub const ALL_SPACES: [SpaceKind; 6] = [
    SpaceKind::Flat, SpaceKind::Space, SpaceKind::Polar, SpaceKind::Nest,
    SpaceKind::Globe, SpaceKind::Map,
];

pub fn space_name(s: SpaceKind) -> &'static str {
    match s {
        SpaceKind::Flat => "flat",
        SpaceKind::Space => "space",
        SpaceKind::Polar => "polar",
        SpaceKind::Nest => "nest",
        SpaceKind::Globe => "globe",
        SpaceKind::Map => "map",
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
        && spec.layers.iter().any(|l| synthesizes_measure(&l.mark, &l.transforms));
    match &spec.coord {
        // Polar wins over a stray `z`: the two are mutually exclusive and
        // `check_polar` refuses the pair, so this only orders the report.
        CoordSpace::Polar(_) => SpaceKind::Polar,
        // The same precedence, for the same reason: `check_nest` refuses `nest()`
        // with a `z`, so this only decides which name the report uses.
        CoordSpace::Nest => SpaceKind::Nest,
        CoordSpace::Globe => SpaceKind::Globe,
        CoordSpace::Map => SpaceKind::Map,
        _ if bound_z || synthesized_z => SpaceKind::Space,
        _ => SpaceKind::Flat,
    }
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
        SpaceKind::Flat => *mark != Mark::Surface,
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
        SpaceKind::Polar => *mark != Mark::Surface,
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
        // Designed vocabulary, no renderer: the empty columns are the honest edge
        // of the engine, and they are in the grid so that edge is visible.
        SpaceKind::Globe | SpaceKind::Map => false,
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
        "caps" | "center" => &["TRUE", "FALSE"],
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
fn normalizer_conflict(a: &Transform, b: &Transform) -> bool {
    let pair = |t: &Transform| matches!(t,
        Transform::Density | Transform::Smooth | Transform::Range
            | Transform::Confidence | Transform::Box | Transform::Bounds);
    (a == &Transform::Proportion && pair(b)) || (b == &Transform::Proportion && pair(a))
}

/// A job's name on the wire, in the words the book uses for it.
fn job_wire(j: Job) -> &'static str {
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
fn transform_jobs_wire(t: &Transform) -> Vec<&'static str> {
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

fn transform_state_wire(t: TransformLegality) -> &'static str {
    match t {
        TransformLegality::None => "none",
        TransformLegality::Combines => "combines",
        TransformLegality::Required => "required",
    }
}

fn obligation_wire(o: Obligation) -> &'static str {
    match o {
        Obligation::Must => "must",
        Obligation::Can => "can",
        Obligation::Cannot => "cannot",
    }
}

fn vartype_wire(v: VarType) -> &'static str {
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
            cells.push(RuleCell {
                mark: mark_name(m),
                channel: channel_name(c),
                obligation: obligation_wire(r.obligation),
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

// ---------------------------------------------------------------------------
// Diagnostics
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticKind {
    /// The grammar forbids this. It will never render.
    Illegal,
    /// The grammar allows it; this engine cannot draw it yet.
    Unsupported,
    /// It renders, but a default was chosen on the caller's behalf.
    Assumption,
}

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub kind: DiagnosticKind,
    /// Directional message: says what to do, not only what went wrong.
    pub message: String,
}

impl Diagnostic {
    /// `true` when the plot must not be rendered.
    pub fn is_fatal(&self) -> bool {
        matches!(self.kind, DiagnosticKind::Illegal | DiagnosticKind::Unsupported)
    }
}

/// Actual type of a bound column, or `None` when the column is absent.
fn actual_type(df: &DataFrame, field: &str) -> Option<VarType> {
    if df.float_col(field).is_some() {
        Some(VarType::Continuous)
    } else if df.str_col(field).is_some() {
        Some(VarType::Discrete)
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Constant values — colors and glyph names
//
// A set value has no data to check it against, so the value itself is the only
// thing that can be wrong. That makes validation matter *more* here than for a
// mapped channel: a misspelt column name already fails loudly, but a misspelt
// color is accepted by SVG and silently painted black. Silence is the one
// outcome the working agreement forbids.
// ---------------------------------------------------------------------------

/// The glyphs `point` can draw, in the order `shape` assigns them.
pub const SHAPE_NAMES: &[&str] = &["circle", "square", "triangle", "diamond", "cross"];




/// The complete advice for a rejected color.
///
/// Returns the whole tail rather than a fragment, so a caller cannot append a
/// second, blander sentence that contradicts the specific one.
fn color_advice(s: &str) -> String {
    if let Some(stem) = numbered_shade(s) {
        // Only offer shades that actually exist: CSS has `lightsteelblue` but
        // no `darksteelblue`, and a suggestion that fails on retry is worse
        // than none at all.
        let shades: Vec<String> = ["light", "dark", ""]
            .iter()
            .map(|p| format!("{p}{stem}"))
            .filter(|c| css_rgb(c).is_some())
            .map(|c| format!("\"{c}\""))
            .collect();
        return format!(
            " `{s}` is an R color name. gog uses CSS colors, which have no numbered \
             shades — use {}, or a hex value like \"#cccccc\" for an exact shade.",
            or_list(&shades)
        );
    }
    match nearest_color(s) {
        Some(near) => format!(
            " Did you mean \"{near}\"? Use a CSS color name, or a hex value like \"#4e79a7\"."
        ),
        None => " Use a CSS color name like \"steelblue\", or a hex value like \"#4e79a7\".".into(),
    }
}

/// `a`, `a or b`, `a, b, or c` — so a suggestion list reads as a sentence.
fn or_list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [a] => a.clone(),
        [a, b] => format!("{a} or {b}"),
        [rest @ .., last] => format!("{}, or {last}", rest.join(", ")),
    }
}


/// Transforms that invent the measured column, so the user need not bind it.
///
/// Named for the *role*, not for `y`: on a horizontal slot mark the axis being
/// invented is `x` (`interval * bounds(lo, hi) + y(term)` is the forest plot),
/// and the caller picks the channel via `synth_axis`.
///
/// `bounds` belongs here: its two named columns *are* the extents, so — like
/// `count` tallying rows — the user binds no measure. It differs from
/// `range`/`confidence`, which reduce an existing measure the user must name and
/// so are absent.
///
/// `pub` because the renderer asks the same question when deciding whether an
/// unbound axis is a mistake worth warning about. Two copies of this list drifted
/// once already: the renderer's omitted `bounds` and told a correctly-drawn forest
/// plot it was "rendering empty chart".
///
/// **It takes the mark**, because a transform only invents a measure where the mark
/// left an axis free for one. Read on a `path` or a `zone` — the marks with no
/// measure axis — the same `bin`/`density` cuts *both* positions and invents
/// neither, so both must be bound and both must be checked against the data. Ask
/// this without the mark and `path * density + x(a) + y(typo)` skips its column
/// check and draws nothing, which is the silent drop §12 forbids.
/// **A composed `proportion` synthesizes nothing**, and getting that wrong is how
/// `bar * sum * proportion + x(continent) + y(pop)` — with no `pop` column in the
/// table — drew an **empty panel on fabricated 0..1 axes** instead of refusing, the
/// exact §12 silent drop this function's own note describes. A normalizer invents an
/// output column only when it had to make the tally itself; standing after a
/// statistic it rescales the column the *user* named, so that name is an **input**
/// and has to be in the data. `bar * sum + y(pop)` refused correctly throughout,
/// which is what made the pair diagnostic: adding a word that measures nothing
/// cannot turn a misspelling into a legal name.
pub fn synthesizes_measure(mark: &Mark, transforms: &[Transform]) -> bool {
    if measures_cells(mark, transforms) {
        return false;
    }
    transforms.iter().any(|t| {
        match t {
            Transform::Proportion => !crate::transform::measures_beside_share(transforms),
            Transform::Bin | Transform::Count | Transform::Density | Transform::Bounds => true,
            // A partition writes the **ring** to the measure axis, which is a
            // synthesized column exactly as a tally is: `y(depth, …)` names an
            // output. It reaches this list rather than being special-cased because
            // every consumer wants the same answer — the axis names itself, the
            // binding is well-formed with nothing in the input to check it against,
            // and Law 7's requirement stands down.
            Transform::Partition => true,
            _ => false,
        }
    })
}

/// Does this mark have **no measure axis**, so that what a transform measures has
/// no position to be written to and goes to `color` instead?
///
/// **This is the *destination* question, not the dimensionality one**, and the two
/// were one predicate until `bar` stood up in the cube (spec §5/§15). They agreed
/// on every mark that existed: a `zone` and a `path` cut both positions *and* have
/// nowhere to put the answer, a flat `bar` cuts one *and* has `y`. A 3-D `bar` is
/// the case that separates them — it cuts **two** positions and still measures
/// along one, because `space` gave it a third. So the conflated predicate split in
/// two: this one names where the measurement goes, [`cuts_both_positions`] how many
/// axes it was read over, and each caller now asks the one it meant.
///
/// A mark's answer here is a property of the mark alone, in every space: a bar
/// measures by length wherever it stands, and a zone measures by color.
pub fn has_no_measure_axis(mark: &Mark) -> bool {
    matches!(mark, Mark::Zone | Mark::Path)
}

/// Does a distributional transform on this mark, in this space, cut **both** of the
/// axes it is read over rather than one?
///
/// Spec §5's dimensionality rule, as a predicate: *how many dimensions a transform
/// cuts is read off the mark, never asked for*. The rule is a subtraction — the
/// positions the space offers, less the one the mark measures along:
///
/// | | positions | measures along | cuts |
/// |---|---|---|---|
/// | `zone`/`path`, flat | 2 | none | **2** |
/// | `bar`, flat | 2 | `y` | 1 |
/// | `bar`, `space` | 3 | `z` | **2** |
/// | `surface`, `space` | 3 | `z` | **2** |
///
/// So the 3-D histogram is this rule read with the third axis present, not a new
/// one: `bar * bin + x(a) + y(b) + space()` cuts the floor into cells for the same
/// reason `zone * bin` cuts the plane, and the count rises along `z` for the same
/// reason it rises along `y` when the plot is flat.
///
/// Asked in one place so the transform stage, the axis fitting, the legality checks
/// and the mark writers cannot disagree about how many dimensions a layer is in.
///
/// Restricted to the marks that **take an extent on the floor and measure up**,
/// rather than stated for every mark with a measure axis. `point` also leaves an axis
/// free (the dot plot's pile is `point * bin * stack`), so the subtraction would give
/// it two dimensions in space as well — a reading nobody has designed, and claiming it
/// here would be the enumeration §5 refuses. `box`/`interval` inherit the widening for
/// free on the day they learn to stand in the cube, because that is exactly what
/// `mark_draws_in_space` will then say.
///
/// The set is the slot marks **plus `surface`**, and the two halves differ in what
/// they take off the floor rather than in the subtraction: a slot mark stands *in* a
/// cell and reads its edges, a surface spans the whole floor and reads its rows as
/// nodes (spec §15). Both spend the third axis on the measurement, which is the only
/// thing this predicate asks.
pub fn cuts_both_positions(mark: &Mark, space: SpaceKind) -> bool {
    has_no_measure_axis(mark)
        || (space == SpaceKind::Space
            && (is_slot_mark(mark) || *mark == Mark::Surface)
            && mark_draws_in_space(mark, space))
}

/// Is this layer a two-dimensional reading — a measurement made per **cell** rather
/// than along an axis?
///
/// The conjunction of [`has_no_measure_axis`] and a transform that invents its own
/// measurement. [`field_measure`] names what such a layer measures; this says
/// whether there is one.
///
/// **Asked of the destination, not of the dimensionality**, which is what keeps a
/// 3-D bar out of it: `bar * bin + x + y + space()` reads its bin over two axes
/// exactly as the heatmap does, and is still not a per-cell *measurement by color*
/// — it has `z` to stand its tally up along. So it answers `false` here and `true`
/// to [`cuts_both_positions`], and every consumer of this predicate (the `color`
/// exemption, the ramp, the legend, the field checks) correctly leaves it alone.
///
/// Four transforms qualify and they divide by where the cells come from, not by what
/// they measure: `bin` and `density` **cut** a continuous plane into cells, `count`
/// and `proportion` tally into the cells two categorical axes already are. Cutting is
/// the *field* half and slotting is the *tile* half, which is why this is no longer
/// called `reads_a_field` — a confusion matrix is cells all the way down and a
/// density estimate nowhere in it.
pub fn measures_cells(mark: &Mark, transforms: &[Transform]) -> bool {
    has_no_measure_axis(mark) && crate::transform::measures_cells(transforms)
}

/// Is this layer read over **two** positions — the dimensionality question, where
/// [`measures_cells`] just above is the destination one?
///
/// The two are the same predicate on every mark but one, and the exception is what
/// forced them apart: a 3-D `bar` reads its `bin` over two axes (`true` here) and
/// still stands its tally up along `z` rather than painting it (`false` there). So
/// the transform stage asks *this* — it decides whether to run `bin2d` or `bin` —
/// while the `color` exemption, the ramp and the legend ask the other.
///
/// Every mark answers both the same way in the plane, which is why one predicate
/// served until the cube had a mark with a measure axis in it.
///
/// **Either class of statistic gets here**, and that is the two-dimensional group-by
/// (spec §5). The four that invent their own measurement read over two axes because
/// nobody handed them a column; the five that reduce a named one read over two
/// because the mark measures with a channel that is not either of them. Both are the
/// same subtraction — *the positions, less the one the mark measures with* — so both
/// belong to the same predicate, and a layer's dimension stays a fact about the mark
/// rather than about which transform was typed.
pub fn reads_two_dimensions(mark: &Mark, transforms: &[Transform], space: SpaceKind) -> bool {
    cuts_both_positions(mark, space)
        && (crate::transform::measures_cells(transforms)
            || crate::transform::reduces_column(transforms).is_some()
            // **A pair is a reduction too** — it reduces to two numbers instead of
            // one, which is a fact about the statistic and not about how many axes
            // it is read over. Left out until 2026-07-26, when `interval` and `box`
            // stood in the cube and needed the floor grouped: without it a whisker
            // fell through to the one-key branch, grouped by `x` alone, and drew one
            // per *row* — 75 whiskers on a 6-cell floor. Same subtraction, same
            // cells, same absent-pair rule as `agg2d`; only the arity of the answer
            // differs (`transform::pairs2d`).
            || crate::transform::pairs_a_column(transforms))
}

/// Which channel a mark makes its **measurement** with, when it is read over both
/// positions — the other half of §5's subtraction, and the whole of the
/// two-dimensional group-by.
///
/// The dimensionality rule is *the positions the space offers, less the one the mark
/// measures along*. Read left to right it says how many axes a distributional
/// transform cuts; read right to left it names the leftover — the channel that
/// carries the measurement, which is exactly the channel a **value statistic** takes
/// its column from. A value statistic has always named its column with the channel it
/// writes back to, because it reduces *in place*: on a flat `bar * mean + x(continent)
/// + y(life)`, `y` both names `life` and receives the mean.
///
/// | mark | positions | measures with | groups by | reduces |
/// |---|---|---|---|---|
/// | `bar`, flat | x, y | `y` | x | y |
/// | `bar`, `space` | x, y, z | `z` | **x, y** | z |
/// | `zone` | x, y | `color` | **x, y** | color |
///
/// So the claim these five were refused under — *both positions are spoken for, so
/// there is no channel left to say which column to summarize* — was false on its own
/// terms. A zone measures by color; color is the channel. Nothing was missing but
/// reading the subtraction's remainder as a name.
///
/// `None` for a mark that is not read over both positions, where the answer is
/// `slot_orient`'s and depends on the bindings rather than on the mark.
pub fn measure_channel(mark: &Mark, space: SpaceKind) -> Option<Channel> {
    if !cuts_both_positions(mark, space) {
        return None;
    }
    // A mark with no measure *axis* measures by color, in every space — which is
    // what `has_no_measure_axis` has always said, now read as a name rather than as
    // an absence. Anything else that cuts both positions did so by gaining a third
    // axis, and in the cube `z` measures, always (spec §15).
    Some(if has_no_measure_axis(mark) { Channel::Color } else { Channel::Z })
}

/// The **column** a two-dimensional value statistic reduces — [`measure_channel`]'s
/// binding, resolved on this layer.
///
/// Positions go through `position_for` (a layer may name its own column for a shared
/// axis, spec §8) and everything else through the layer's own encodings, which is
/// `binding_of`'s split and is asked here so the transform stage and the checks read
/// one answer.
pub fn measure_field<'a>(spec: &'a PlotSpec, layer: &'a Layer) -> Option<&'a str> {
    let ch = measure_channel(&layer.mark, space_of(spec))?;
    binding_of(spec, layer, &ch).map(|d| d.field.as_str())
}

/// Is this layer's `color` carrying the layer's **own measurement**, rather than a
/// column the reader bound for its own sake?
///
/// The question anything reading a color *domain* has to ask, because the answer
/// decides which frame holds the numbers: a measurement exists only downstream of the
/// transform that made it, and the raw table either lacks the column or — the case
/// that hid this — holds the *unreduced* values under the same name.
///
/// Both halves of §5's division say yes. The four that invent a measurement publish it
/// under a synthesized name (`count`, `density`, `proportion`, `level`), so the raw
/// table has no such column at all; the five that reduce a named one rewrite the
/// reader's own column, so the raw table has it with every original row still in it.
/// Stating the rule as "is the column synthesized" caught only the first, and the
/// second then drew a heatmap of cell means beside a key that spanned the raw
/// column's range — self-consistent fills under a legend that decoded them wrongly,
/// which is §12's silent wrongness rather than a cosmetic slip.
pub fn color_is_the_measurement(spec: &PlotSpec, layer: &Layer) -> bool {
    reads_two_dimensions(&layer.mark, &layer.transforms, space_of(spec))
        && measure_channel(&layer.mark, space_of(spec)) == Some(Channel::Color)
}

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
    reads_a_field(mark, transforms, space) || transforms.contains(&Transform::Partition)
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
// **do these two axes have the same role?** For a *path* (`line`/`area`/…) they
// do not — `x` is permanently the domain and `y` the measure — so a path's row
// is asymmetric by right. For a slot mark they do: one axis holds the slots and
// the other the measure, and which plays which is a fact about the bindings, not
// about the mark. Same roles, so the same rule; hence one function for all three.
// ---------------------------------------------------------------------------

/// The marks that stand in a slot on one axis and extend along the other.
///
/// The orientable family, and the same set `dodge` subdivides (see the note
/// above). A path or a glyph is absent: neither has a slot, so neither has an
/// orientation to read.
pub fn is_slot_mark(mark: &Mark) -> bool {
    matches!(mark, Mark::Bar | Mark::Box | Mark::Interval)
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
    if !matches!(layer.mark, Mark::Area | Mark::Ribbon | Mark::Line | Mark::Step) { return None }
    if !layer.transforms.contains(&Transform::Density) { return None }
    let df = df?;
    let typ = |ch| spec.position_for(layer, &ch).and_then(|c| actual_type(df, &c.field));
    match (typ(Channel::X), typ(Channel::Y)) {
        (Some(VarType::Discrete), Some(VarType::Continuous)) => Some(Orient::Vertical),
        (Some(VarType::Continuous), Some(VarType::Discrete)) => Some(Orient::Horizontal),
        _ => None,
    }
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
    for layer in &spec.layers {
        if !is_slot_mark(&layer.mark) {
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
        if spec.position_for(layer, &Channel::X).is_none() && bar_divides_one_slot(layer) {
            return Orient::Vertical;
        }
        let Some(df) = layer
            .data
            .as_ref()
            .or(spec.data.as_ref())
            .and_then(|name| data.get(name))
        else {
            continue;
        };
        return slot_orient(actual_type(df, x_field), actual_type(df, y_field));
    }
    Orient::Vertical
}

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
fn check_slot_shape(out: &mut Vec<Diagnostic>, spec: &PlotSpec, df: &DataFrame, layer: &Layer) {
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
    // What the measure would have *been* — named per mark, so the direction
    // points at the thing that mark draws rather than at a generic "a number".
    let (what, fix) = match layer.mark {
        Mark::Box => (
            "summarize",
            "One axis must be a number: that is the column the five-number summary reduces.",
        ),
        Mark::Interval => (
            "span",
            "One axis must be a number: that is the column the low and high extents come from.",
        ),
        _ => (
            "measure",
            "One axis must be a number: that is the length of the bar. To count rows per \
             category instead, use `bar * count`.",
        ),
    };
    out.push(Diagnostic {
        kind: DiagnosticKind::Illegal,
        message: format!(
            "gog: `{m}` has categorical columns on both axes — `x({})` and `y({})` — so there \
             is nothing for it to {what}. {fix}",
            x.field, y.field
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
fn check_span_needs_range(out: &mut Vec<Diagnostic>, spec: &PlotSpec, df: Option<&DataFrame>, layer: &Layer) {
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
    let example = match layer.mark {
        Mark::Interval => "`interval * range + x(group) + y(value)` draws the min–max range per group, or `interval * bounds(lo, hi)` a pre-computed one",
        Mark::Ribbon => "`ribbon * range + x(t) + y(value)` draws a band between the min and max at each x, or `ribbon * bounds(lo, hi)` a pre-computed one",
        _ => return,
    };
    // `range`/`confidence` compute the extents; `bounds` supplies pre-computed ones.
    // Any of the three satisfies the mark's minimum syllable.
    if layer.transforms.iter().any(|t| matches!(t, Transform::Range | Transform::Confidence | Transform::Bounds)) {
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
                "gog: `{m} * density` estimates one curve, and a {m} needs two boundaries to \
                 span between. Bind a category and the estimate is drawn across its slot — \
                 `{m} * density + x(<category>) + y(<number>)` is the violin, one distribution \
                 per group. For the curve itself, `area * density + x(<number>)` fills it to \
                 the baseline and `line * density + x(<number>)` traces it."
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
fn check_surface(
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
                 scatter rather than a grid. A surface needs one row per (x, y) crossing, the \
                 shape `expand.grid()` makes. For a cloud of scattered points use `point` with \
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
fn check_zone_extent(out: &mut Vec<Diagnostic>, spec: &PlotSpec, df: &DataFrame, layer: &Layer) {
    if layer.mark != Mark::Zone {
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
    if crate::transform::reduces_column(&layer.transforms).is_some() {
        return;
    }
    let slotted = |ch: &Channel| {
        spec.position_for(layer, ch)
            .and_then(|c| actual_type(df, &c.field))
            == Some(VarType::Discrete)
    };
    if layer.transforms.contains(&Transform::Bounds)
        || layer.transforms.contains(&Transform::Partition)
        || reads_a_field(&layer.mark, &layer.transforms, space_of(spec))
        || slotted(&Channel::X)
        || slotted(&Channel::Y)
    {
        return;
    }
    out.push(Diagnostic {
        kind: DiagnosticKind::Illegal,
        message: "gog: `zone` shades a rectangle, but nothing here says where its sides \
                  are. Four things can: a categorical position, whose category owns a \
                  slot — `zone + x(method) + y(dataset) + color(score)` fills every cell \
                  where two categories cross; `bounds`, which names the sides from \
                  columns you hold — `zone * bounds(lo, hi)` on the measure axis, \
                  `zone * bounds(start = a, end = b)` on the domain axis, all four for a \
                  box, and the axis you leave out spans the panel; `bin`, which cuts them \
                  out of two continuous axes and counts the rows in each cell; and \
                  `density`, which cuts the same cells and estimates a value at each. A \
                  continuous position on its own is none of them — a number is a point, \
                  and a point has no width.".to_string(),
    });
}

/// The **two-dimensional group-by** (spec §5): the five reductions read over a *pair*
/// of keys, on the marks whose measurement is not either of them.
///
/// A value statistic groups by every position the mark does **not** measure with, and
/// reduces the column named on the one it does — writing the answer back into it,
/// because a reduction is *in place*. That is §5's subtraction read a second time, and
/// nothing about it is new except running it where the remainder is two:
///
/// | mark | positions | measures with | groups by | reduces |
/// |---|---|---|---|---|
/// | `bar`, flat | x, y | `y` | x | y |
/// | `bar`, `space` | x, y, z | `z` | **x, y** | z |
/// | `zone` | x, y | `color` | **x, y** | color |
///
/// So this check asks the three things the pair reading needs and the one-key reading
/// got for free from `slot_orient`: that the **measure channel names a column** (there
/// is no `y` to fall back on when the measurement rides `color`), that the column is
/// **numeric** (it is reduced, not counted), and that **both positions are
/// categorical** (a cell is a slot a category owns — a number is a point, and there is
/// nothing to cut here, since these five measure without cutting).
///
/// Data-aware, so it sits in the df-gated block beside `check_zone_extent`, which
/// defers its own "where are the sides?" refusal to this one for the reason a tally
/// does: a transform that measures into cells it did not cut can only mean slots, and
/// two refusals for one mistake is worse than the right one.
fn check_pair_summary(out: &mut Vec<Diagnostic>, spec: &PlotSpec, df: &DataFrame, layer: &Layer) {
    let space = space_of(spec);
    // Not a two-dimensional reading, or not one of the five — the one-key form, which
    // `check_slot_shape` and `check_summary` between them already judge.
    if !cuts_both_positions(&layer.mark, space) {
        return;
    }
    let Some(t) = layer.transforms.iter().find(|t| {
        crate::transform::reduces_column(std::slice::from_ref(*t)).is_some()
    }) else { return };
    let (m, t) = (mark_name(&layer.mark), transform_name(t));
    // Which channel this mark measures with, so the message names the binding the
    // reader must write rather than a generic "the measure". `Some` because
    // `cuts_both_positions` is true above.
    let Some(ch) = measure_channel(&layer.mark, space) else { return };
    let c = channel_name(&ch);

    // 0a. A **bounded** zone is one rectangle per row, its sides named by its own
    //     four columns — not a mesh, so there are no cells to group into. That
    //     refusal moved to `check_chain_jobs` on 2026-07-31, sentence intact, because
    //     sitting here it was **mis-scoped**: this whole function is gated on
    //     `cuts_both_positions` above, so `interval * bounds(lo, hi) * mean` never
    //     reached it and drew in silence — the identical mistake this file already
    //     records at 0b, where the composition rule lived inside the two-dimensional
    //     group-by and every one-key reading walked past it.
    if layer.transforms.contains(&Transform::Bounds) {
        return;
    }

    // 0b. Two transforms cannot both be the cell's measurement — unless one of them
    //    is only describing an **extent**, which is `check_cut_composition`'s
    //    question and is asked of every mark in every dimension rather than here.
    //    This check ran that refusal until 2026-07-26, which is why the same mistake
    //    was fatal on a `zone` and silent on a `bar`: the refusal lived inside the
    //    *two-dimensional* group-by, and the one-key reading never reached it.
    //
    //    `bin` deliberately falls through to the three questions below. Composed, it
    //    supplies the cells and this statistic supplies their measurement — so the
    //    column still has to be named (1) and still has to be a number (2), and only
    //    the *both positions categorical* question (3) is answered differently,
    //    because a cut axis owns cells too.
    if crate::transform::measures_cells(&layer.transforms)
        && !layer.transforms.contains(&Transform::Bin)
    {
        return;
    }

    // 1. The column to reduce. On the plane a value statistic never had to ask: `y`
    //    is required by every mark that takes one. Here the measurement rides a
    //    channel that is optional — `color` on a zone, `z` on a bar — and accepting
    //    the layer without it would reduce nothing and draw an empty mesh, which is
    //    the §12 drop.
    let Some(field) = measure_field(spec, layer) else {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `{m} * {t}` reduces a column within each cell, but nothing says which \
                 column — `{m}` measures by `{c}`, and no `{c}()` is bound. Name it: \
                 `{m} * {t} + x(<a>) + y(<b>) + {c}(<column>)`. To count the rows in each \
                 cell instead of reducing a column, `count` needs no such binding: \
                 `{m} * count + x(<a>) + y(<b>)`."
            ),
        });
        return;
    };

    // 2. It has to be a number, because it is reduced rather than counted. A legality
    //    question about a column's type, so it is asked here and fatally (spec §12).
    if df.float_col(field).is_none() && !df.is_empty() {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `{m} * {t}` reduces `{c}({field})` within each cell, and `{field}` is \
                 not a numeric column — there is no {t} of a category. Name the column you \
                 want summarized, or count the rows in each cell instead: \
                 `{m} * count + x(<a>) + y(<b>)`."
            ),
        });
        return;
    }

    // 3. Every position must own a cell, because these five **measure without
    //    cutting**: they answer *what is in this cell*, never *where are my cells*.
    //    A category arrives already cut — it owns its slot — and a number does not.
    //    So a continuous axis needs something to cut it, and since 2026-07-26 there
    //    is a transform that does: composed with `bin`, these five keep the
    //    measurement and `bin` keeps the cut (spec §5). Asking whether the layer
    //    cuts rather than listing the marks is what makes the summary heatmap and
    //    the confusion matrix one sentence read on two kinds of axis.
    if layer.transforms.contains(&Transform::Bin) {
        return;
    }
    let loose: Vec<String> = [Channel::X, Channel::Y].into_iter()
        .filter_map(|ch| spec.position_for(layer, &ch).map(|d| (ch, d)))
        .filter(|(_, d)| actual_type(df, &d.field) == Some(VarType::Continuous))
        .map(|(ch, d)| format!("{}({})", channel_name(&ch), d.field))
        .collect();
    if !loose.is_empty() {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `{m} * {t}` summarizes `{field}` inside the cell each pair of \
                 categories owns, and `{}` carries numbers — a number is a point, so it \
                 owns no cell to summarize into. Either put a category on both positions, \
                 or cut the numeric axis into cells first: `{m} * bin * {t} + x(<a>) + \
                 y(<b>) + {c}({field})` bins where your data lives and {t}s `{field}` \
                 inside each cell.",
                loose.join("` and `"),
            ),
        });
    }
}

/// **Two transforms that do the same job contradict** (spec §5) — asked of every
/// mark, in every dimension, because the answer is a fact about the transforms
/// rather than about the geometry reading them.
///
/// There are four jobs, and the book names three of them before this check does:
/// a transform says **where the cells are**, says **what is in them**, says **what
/// scale the answer is read on**, or says **where the marks sit**. The frame holds
/// one of each per cell. So a second transform doing a job the first already did is
/// a request the engine can only answer by throwing one of the two away, and
/// throwing one away in silence is the drop §12 forbids.
///
/// `transform::job_conflict` finds the pair; everything below is what to *say* about
/// it. The split is the point: a transform's job is a fact about frames, which is
/// that module's subject, while a refusal is a sentence addressed to a person.
///
/// **Why this is one rule and not four lists.** It was four lists. Three checks each
/// enumerated the family it knew — the five reductions here, `[bin, count, density]`
/// here, `bounds` against a cut over in the zone's own check — and every transform
/// outside those families composed with everything, silently. `range * confidence`
/// drew exactly `confidence`; `sum * range` drew exactly `range`; `smooth * mean`
/// drew exactly `smooth`; `dodge * stack` drew groups both side by side *and* piled.
/// A survey on 2026-07-31 found that of 582 two-transform chains that drew, only 271
/// had every transform doing something. None of those were decisions. They were the
/// cost of a rule written three times instead of once, and a fourth family was going
/// to be forgotten in exactly the same way.
///
/// **The messages stay per-pair.** A generic sentence per job is the fallback, not
/// the goal — a refusal that does not say what to do instead is a §12 failure one
/// level down. Every message names what would have been discarded, which is what
/// makes the refusal appealable: a reader who has a reading in mind can see the plot
/// the engine declined to draw and say so.
///
/// A transform can supply two different things, and §5's division of the nine value
/// statistics into *the four that invent a measurement* and *the five that reduce a
/// named one* hid the second question: which of them also says **where the cells
/// are**. Only `bin` gives its measurement up. It **cuts**, and its tally is a
/// by-product of the cut rather than the cut itself, so it can hand the measurement
/// over and still have something left to contribute — and it hands it only to a
/// transform that was handed a *column*, which is why `bar * bin * mean` composes and
/// `bar * bin * count` does not. The others cannot:
///
/// - `count` and `proportion` tally into cells the **positions** already own (the
///   fourth extent description — a category owns its slot), so taking their
///   measurement away leaves nothing at all.
/// - a `density` cell is a **sample point of an estimate**, which exists *between*
///   the data points rather than partitioning rows, so there are no rows inside one
///   to reduce. It is the same property that makes `bandwidth` a length and `bin`'s
///   width a boundary.
///
/// **Why this is a check and not a renderer branch.** Until 2026-07-26 all four
/// compositions ran, and in one dimension every one of them silently dropped the
/// statistic: `bin` overwrote the named column with its own tally, the reduction then
/// averaged a single count per cell and handed it back unchanged, and the axis was
/// relabeled to the column nobody had read. A histogram under an axis reading
/// `Life`. That is the silent drop §12 forbids, arriving through a *composition*
/// rather than through a binding, and it was invisible to every test because the plot
/// rendered and exited 0.
fn check_chain_jobs(out: &mut Vec<Diagnostic>, spec: &PlotSpec, layer: &Layer) {
    use crate::transform::{JobContext, job_conflict};
    let ts = &layer.transforms;
    let m = mark_name(&layer.mark);

    // **No early return for a transform the mark does not take**, even though that
    // reads like the better message and two refusals for one mistake is usually
    // worse than the right one. Deferring here assumed a refusal that does not
    // always exist: `mark_takes_transform(interval, mean)` is `None`, but the only
    // check that spoke up was the *minimum syllable* one — and `interval * bounds *
    // mean` satisfies the syllable, so `mean` went through unrefused and unread. The
    // grid says `none` and the engine drew it anyway. An extra sentence costs a
    // reader one confused moment; a silent drop costs them a wrong plot they believe.
    let ctx = JobContext {
        measures_by_color: has_no_measure_axis(&layer.mark),
        stack_shares: layer.stack.as_ref().is_some_and(|s| s.share.unwrap_or(false)),
    };
    let Some((a, b, job)) = job_conflict(ts, ctx) else {
        // **A transform that changes nothing is still a transform nobody read.** Two
        // that do different jobs compose, but one of them can still turn out to be a
        // no-op — and saying so is the same duty as the refusals above, one step
        // softer. It is a warning rather than a refusal because the plot drawn is
        // exactly the plot asked for: nothing was discarded, one atom was simply not
        // needed. Refusing it would forbid the ugly-but-legal (Law 8).
        //
        // The list is short on purpose and grows only as a case is understood well
        // enough to say why in one sentence. A no-op nobody can explain is a refusal,
        // not a warning — a plot that quietly means something other than it says is
        // the more expensive mistake, and a refusal can be relaxed later while a
        // drawing people have built on cannot be taken back.
        if ts.contains(&Transform::Count) && ts.contains(&Transform::Proportion) {
            out.push(Diagnostic {
                kind: DiagnosticKind::Assumption,
                message: format!(
                    "gog: `{m} * count * proportion` draws the same plot as \
                     `{m} * proportion` — a share is a share *of* a tally, so \
                     `proportion` already counts the rows and `count` adds nothing. \
                     Drop it, or keep `{m} * count` on its own to read the tallies \
                     themselves rather than their shares."
                ),
            });
        }
        return;
    };
    let (an, bn) = (transform_name(&a), transform_name(&b));

    // Which binding names the column, so the way out is a sentence the reader can
    // type. A mark that reads over two positions measures with a channel the reader
    // has to bind (`color` on a zone, `z` in the cube); a one-key mark measures along
    // its own y, which `check_summary` already requires by name, so naming it twice
    // would be the restating-a-rule-from-elsewhere defect the refusal audit found.
    let bind = measure_channel(&layer.mark, space_of(spec))
        .map(|ch| format!(" + {}(<column>)", channel_name(&ch)))
        .unwrap_or_default();

    let message = chain_message(&layer.mark, m, job, (&a, an), (&b, bn), &bind);
    out.push(Diagnostic { kind: DiagnosticKind::Illegal, message });
}

/// What to say about a pair of transforms that do the same job.
///
/// Ordered most specific first: the pairs that earned their own sentence keep it, and
/// the per-job fallback catches the rest. The fallback is what makes this a rule — a
/// transform added next year collides correctly on the day it arrives instead of
/// composing silently until somebody notices, which is what happened to the six that
/// were outside every check until 2026-07-31.
fn chain_message(
    mark: &Mark,
    m: &str,
    job: Job,
    (ta, a): (&Transform, &str),
    (tb, b): (&Transform, &str),
    bind: &str,
) -> String {
    use crate::transform::is_reduction;
    let has = |t: &Transform| ta == t || tb == t;
    // The transform that is *not* the one a bespoke message names.
    let other = |t: &Transform| if ta == t { b } else { a };

    // `smooth` fits a curve of one column against another, and LOESS already averages
    // locally along that curve — so cutting the domain into cells first buys it
    // nothing it was not doing, and the sentence reads as two answers to one
    // question. (In two dimensions §5 refuses it a different way, on a floor having
    // no left to right; here there is a domain, and the redundancy is what rules.)
    // `smooth * proportion` is refused too but not here: a normalizer asks no second
    // question, so its reason is its own and `check_share_composition` gives it.
    if has(&Transform::Smooth) {
        let o = other(&Transform::Smooth);
        let did = if is_reduction(if ta == &Transform::Smooth { tb } else { ta }) {
            format!("reducing them with `{o}` as well")
        } else {
            "cutting them into cells first".to_string()
        };
        return format!(
            "gog: `{m} * {a} * {b}` asks one question twice — `smooth` fits a \
             curve through the rows and already averages locally as it goes, so \
             {did} changes nothing it was not doing. Keep \
             whichever you meant: `{m} * smooth + x(<a>) + y(<b>)` for the fitted \
             curve, or `{m} * {o}` for the shape `{o}` measures. For a summary per \
             cell rather than a fitted curve, name the statistic: \
             `{m} * bin * mean + x(<a>) + y(<b>)`."
        );
    }

    // A `zone` names its own sides. Named *and* cut says where they are twice, and
    // the two would disagree the moment the mesh moved.
    if job == Job::Extent && has(&Transform::Bounds) && has_no_measure_axis(mark) {
        let t = other(&Transform::Bounds);
        return format!(
            "gog: `zone * {a} * {b}` says where the sides are twice — `bounds` names \
             them from columns you have, `{t}` cuts them from the data. Keep whichever \
             you meant: `zone * bounds(...)` to shade a rectangle you chose, \
             `zone * {t}` to tile the panel with measured cells."
        );
    }

    // A **bounded** zone is one rectangle per row, its sides named by its own four
    // columns — not a mesh, so there are no cells to group into. A reduction needs
    // cells the *positions* make, which is the extent description `bounds` replaces
    // rather than supplies. Said in `bounds`' own terms, because the reader who wrote
    // it was shading a region they chose, not summarizing.
    //
    // This sentence was mis-scoped rather than missing until 2026-07-31: it lived
    // behind `cuts_both_positions` in `check_pair_summary`, so `interval * bounds *
    // mean` never reached it and drew in silence. The identical mistake, and the
    // identical fix, that this file already records for the composition rule at large.
    if has(&Transform::Bounds) && (is_reduction(ta) || is_reduction(tb)) {
        let t = other(&Transform::Bounds);
        let c = measure_channel(mark, SpaceKind::Flat).map(|ch| channel_name(&ch)).unwrap_or("y");
        return format!(
            "gog: `{m} * {a} * {b}` says what this rectangle is twice — `bounds` names \
             its sides from columns you hold, one rectangle per row, and `{t}` \
             summarizes a column within the cells your *positions* make. Keep whichever \
             you meant: `{m} * bounds(...)` to shade a region you chose, or \
             `{m} * {t} + x(<a>) + y(<b>) + {c}(<column>)` to summarize one within every \
             cell two categories cross. To shade a band the data computed, \
             `ribbon * range` is the mark that spans a statistic."
        );
    }

    match job {
        // Two members of the aggregation family each reduce *the column you named*,
        // and a cell still holds one number. Neither was handed a different column to
        // give way to, so the tie `bin` breaks has nothing to break here.
        Job::Measure if is_reduction(ta) && is_reduction(tb) => format!(
            "gog: `{m} * {a} * {b}` measures each cell twice — `{a}` and `{b}` both \
             reduce the column you named, and a cell holds one number, so there is no \
             reading that keeps both. Keep whichever you meant: `{m} * {a}` or \
             `{m} * {b}`. To show two summaries of one column, draw them as two \
             layers: `{m} * {a} + {m} * {b}`."
        ),
        // Two transforms that each invent their own measurement from the rows, with
        // neither side handed a column, so the tie cannot be broken the way the one
        // below is.
        Job::Measure if !reads_a_column(ta) && !reads_a_column(tb) => format!(
            "gog: `{m} * {a} * {b}` measures each cell twice — `{a}` and `{b}` each \
             invent their own measurement from the rows, and neither was handed a \
             column to give way to, so there is no reading that keeps both. Keep \
             whichever you meant: `{m} * {a}` or `{m} * {b}`. To cut an axis into \
             cells and measure something else inside them, the second transform has \
             to be one you hand a column: `{m} * bin * mean + x(<number>) + \
             y(<column>)`. To read either as shares of the whole rather than as \
             counts, `proportion` rescales whichever you keep: `{m} * {a} * proportion`."
        ),
        // One that supplies no extent, against one that was handed a column. Each
        // synthesizer gets its own reason, because they are not refused for the same
        // one — and a message that restates a rule from elsewhere instead of this one
        // is the defect the 2026-07-26 refusal audit went looking for.
        Job::Measure if !reads_a_column(ta) || !reads_a_column(tb) => {
            let (o, t) = if reads_a_column(ta) { (b, a) } else { (a, b) };
            let why = if has(&Transform::Density) {
                format!(
                    "a `density` cell is a point where the estimate was sampled, not a bucket \
                     holding rows — the estimate exists *between* your observations — so there \
                     is nothing inside one for `{t}` to reduce"
                )
            } else {
                format!(
                    "`{o}` supplies only a measurement: its cells are the slots the positions \
                     already own, so with `{t}` measuring them too the cell is measured twice \
                     and `{o}` has nothing left to contribute"
                )
            };
            format!(
                "gog: `{m} * {a} * {b}` measures each cell twice — {why}. Keep whichever you \
                 meant: `{m} * {o}` to measure what `{o}` computes, or `{m} * {t}{bind}` to \
                 reduce the column you name. To cut a continuous axis into cells and reduce a \
                 column inside each, `bin` is the transform that cuts without keeping the \
                 measurement: `{m} * bin * {t} + x(<number>)`."
            )
        }
        // Both were handed a column. The pair transforms land here, and until
        // 2026-07-31 nothing looked: `interval * range * confidence` drew exactly
        // `interval * confidence`, and reversed, exactly `interval * range`.
        Job::Measure => format!(
            "gog: `{m} * {a} * {b}` measures each cell twice — `{a}` and `{b}` both \
             reduce the column you named, and a cell holds one answer, so drawing \
             both would mean drawing one of them and discarding the other. Keep \
             whichever you meant: `{m} * {a}` or `{m} * {b}`. To show both, draw them \
             as two layers: `{m} * {a} + {m} * {b}`."
        ),
        Job::Extent => format!(
            "gog: `{m} * {a} * {b}` says where the cells are twice — `{a}` and `{b}` \
             each carve the panel their own way, and the marks sit in one set of cells, \
             so one of the two would be discarded. Keep whichever you meant: \
             `{m} * {a}` or `{m} * {b}`. To measure something inside cells one of them \
             cuts, name a statistic instead: `{m} * {a} * mean{bind}`."
        ),
        Job::Scale => format!(
            "gog: `{m} * {a} * {b}` rescales the measurement twice — `{a}` and `{b}` \
             each divide it into shares, and dividing twice does not read as shares of \
             anything. Keep whichever you meant: `{a}` for shares of the whole plot, \
             `{b}` for shares within each pile."
        ),
        Job::Position => format!(
            "gog: `{m} * {a} * {b}` arranges the same marks twice — `{a}` and `{b}` \
             each decide where colliding groups go, and a mark sits in one place, so \
             one of the two would be discarded. Keep whichever you meant: `{m} * {a}` \
             or `{m} * {b}`. To show both readings, draw them as two plots side by \
             side rather than as one layer."
        ),
    }
}

/// Was this transform handed a column to measure, rather than inventing its own
/// number from the rows? The distinction the measure job turns on — see
/// [`crate::transform::Jobs::reads_a_column`].
fn reads_a_column(t: &Transform) -> bool {
    crate::transform::jobs(t, crate::transform::JobContext::default()).reads_a_column
}


/// What a **normalizer** cannot rescale (spec §5).
///
/// `proportion` divides the measurement present by its total, which means it needs
/// the measurement to *be* a total's worth of parts: one number per cell, summing to
/// something a share is a share of. Three kinds of transform fail that, each for its
/// own reason, and each says so in its own words — a message that restates a rule
/// from elsewhere is the defect the 2026-07-26 refusal audit went looking for.
///
/// The five value statistics and `bin`/`count` are all absent from this list, which
/// is the point: `bar * bin * proportion` is the relative-frequency histogram and
/// `bar * sum * proportion` is each slot's share of a summed column. A normalizer
/// composes with everything that leaves one number per cell behind it (Law 1).
fn check_share_composition(out: &mut Vec<Diagnostic>, layer: &Layer) {
    let ts = &layer.transforms;
    if !ts.contains(&Transform::Proportion) { return }
    let m = mark_name(&layer.mark);

    // 1. A density is already normalized — it integrates to 1 by construction — and
    //    its cells are sample points rather than parts of a whole, so the sum of its
    //    heights is not a quantity anything is a share *of*. Dividing by it would
    //    hand back a number with no reading at all.
    if ts.contains(&Transform::Density) {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `{m} * density * proportion` has nothing to normalize — a density \
                 already integrates to 1, and its cells are points where the estimate was \
                 sampled rather than parts of a whole, so the sum of its heights is not a \
                 quantity anything is a share of. For the estimated shape, \
                 `{m} * density + x(<number>)`; for shares of the rows themselves, cut them \
                 into cells first: `bar * bin * proportion + x(<number>)` is the same curve's \
                 histogram, read as fractions."
            ),
        });
        return;
    }

    // 2. A fitted curve is a value *at* each x, not a part of any total. Summing the
    //    fitted values answers nothing, so there is no denominator to be found — the
    //    same absence as `density`, arrived at from the other direction (an estimate
    //    between the observations against a fit through them).
    if ts.contains(&Transform::Smooth) {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `{m} * smooth * proportion` has no total to divide by — a fitted curve \
                 gives a value *at* each x rather than a part of a whole, so adding those \
                 values up answers nothing and a share of the sum means nothing. Keep the \
                 fit: `{m} * smooth + x(<a>) + y(<b>)`. If the column you are fitting is \
                 already a share, compute it in your own table and plot it as an ordinary y."
            ),
        });
        return;
    }

    // 3. A pair transform leaves **two** numbers per cell, an extent rather than a
    //    quantity, and a share of an extent is not defined — normalizing both ends
    //    separately would move them by different factors and stop the pair being one
    //    span. (`bounds` reshapes two columns you already have, and lands here for the
    //    same reason it lands anywhere: what it supplies is a pair.)
    if let Some(p) = [Transform::Range, Transform::Confidence, Transform::Box,
                      Transform::Bounds].into_iter().find(|t| ts.contains(t)) {
        let p = transform_name(&p);
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `{m} * {p} * proportion` cannot take a share of a span — `{p}` leaves \
                 two numbers per cell, a low and a high, and an extent has no total it is a \
                 part of. For a single value per cell read as a share, name the statistic \
                 that makes one: `bar * mean * proportion + x(<a>) + y(<column>)`."
            ),
        });
    }
}

/// `bounds` supplies a *pre-computed* low/high pair (`bounds(lower, upper)`), so it
/// belongs to the marks that draw such a pair: `ribbon` fills it, `interval` draws a
/// whisker across it, and `line`/`step` trace its two boundaries (the unfilled band).
/// On any other mark it names nothing to draw. And because it *reshapes* rather than
/// computes, its two columns must actually exist and be numeric — a data-aware check,
/// so it sits in the df-gated block beside `check_jitter`.
fn check_bounds(out: &mut Vec<Diagnostic>, df: &DataFrame, layer: &Layer) {
    if !layer.transforms.contains(&Transform::Bounds) {
        return;
    }
    // 1. A mark that draws a low/high pair — the four band marks, read off the
    //    shared table (bounds is a pair transform, so `None` marks the rest).
    if mark_takes_transform(&layer.mark, &Transform::Bounds) == TransformLegality::None {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `bounds` supplies a pre-computed low/high pair, which the band marks draw — \
                 `ribbon` fills it, `interval` whiskers it, `line`/`step` trace its two boundaries \
                 — but `{}` is none of those. For a single value per group, a summary like `mean` \
                 is the transform you want.",
                mark_name(&layer.mark),
            ),
        });
        return;
    }
    // 2. Which pairs this mark needs. A band mark spans the *measure* axis and has
    //    no extent along the domain, so it requires `lower`/`upper` and has
    //    nothing to do with `start`/`end`. A `zone` is a rectangle and takes
    //    either pair, or both — whichever it is not given, the panel supplies,
    //    which is the whole reason the mark exists.
    let Some(spec) = layer.bounds.as_ref() else {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: if layer.mark == Mark::Zone {
                "gog: `zone` needs at least one pair of columns to bound it — \
                 `bounds(lower, upper)` for the measure axis, `bounds(start, end)` for the \
                 domain axis, or all four for a box. The axis you leave out spans the panel."
                    .to_string()
            } else {
                "gog: `bounds` needs two column names — `bounds(lower, upper)`.".to_string()
            },
        });
        return;
    };

    let is_zone = layer.mark == Mark::Zone;
    if !is_zone && (spec.start.is_some() || spec.end.is_some()) {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `bounds(start, end)` bounds a rectangle along the domain axis, and {} `{}` \
                 has no extent there — it spans the measure axis at each position. Keep \
                 `bounds(lower, upper)`, or use `zone` to shade a rectangle.",
                article(mark_name(&layer.mark)), mark_name(&layer.mark),
            ),
        });
        return;
    }
    if is_zone && spec.measure().is_none() && spec.domain().is_none() {
        // A half-named pair is the likely typo, and saying which half is missing
        // beats repeating the whole grammar at someone who nearly had it.
        let named: Vec<&str> = [("lower", &spec.lower), ("upper", &spec.upper),
                                ("start", &spec.start), ("end", &spec.end)]
            .iter().filter(|(_, v)| v.is_some()).map(|(k, _)| *k).collect();
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: if named.is_empty() {
                "gog: `zone` needs at least one complete pair — `bounds(lower, upper)` bounds it \
                 on the measure axis, `bounds(start, end)` on the domain axis, and all four make \
                 a box. The axis you leave out spans the panel.".to_string()
            } else {
                format!(
                    "gog: `zone` was given `{}` but not the other half of any pair. A rectangle \
                     needs both ends of a side: `bounds(lower, upper)`, `bounds(start, end)`, or \
                     all four.",
                    named.join("`, `"),
                )
            },
        });
        return;
    }
    if !is_zone && spec.measure().is_none() {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: "gog: `bounds` needs two column names — `bounds(lower, upper)`.".to_string(),
        });
        return;
    }

    // 3. Every named column must exist and be numeric — it reshapes, never computes.
    for col in [&spec.lower, &spec.upper, &spec.start, &spec.end].into_iter().flatten() {
        if df.float_col(col).is_none() {
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: format!(
                    "gog: `bounds` reads a pre-computed numeric column, and `{col}` is not one in \
                     the data. Check the name, or compute the bound first — gog draws it, it does \
                     not fit the model."
                ),
            });
        }
    }
}

/// The distributional transforms — `bin`, `density`, `smooth` — describe how values
/// are spread *along* an axis, so the axis they read must carry a number.
///
/// This question was answered in `transform.rs` for several sessions, one `eprintln!`
/// per transform, and answering it *there* made it unrefusable: the transform stage
/// runs after the legality gate, so it could only warn and then hand the renderer
/// something to draw. `bin` and `density` returned an empty frame, which drew an empty
/// panel with fabricated 0..1 axes; `smooth` returned its input unchanged, which drew
/// the raw scatter as if it were the fitted curve — the worse of the two, because it
/// looks finished. Either way a binding was accepted and silently dropped, which §12
/// forbids outright, and `writing.qmd` documented the `bin` case as a refusal that
/// never happened. It is a legality question, so it is asked here, once, and fatally.
///
/// Which axis is read is *relational*, the way `slot_orient` is: a vertical `bar * bin`
/// cuts `x`, a horizontal one cuts `y`. `smooth` reads **both** — it fits `y` against
/// `x` — so both must be numeric. Data-aware, so it sits in the df-gated block.
fn check_distribution_axis(
    out: &mut Vec<Diagnostic>,
    spec: &PlotSpec,
    df: &DataFrame,
    layer: &Layer,
) {
    let xd = spec.position_for(layer, &Channel::X);
    let yd = spec.position_for(layer, &Channel::Y);
    let xt = xd.and_then(|c| actual_type(df, &c.field));
    let yt = yd.and_then(|c| actual_type(df, &c.field));
    // How many axes this layer's transform is read over — the dimensionality rule,
    // which needs the space now that a `bar` cuts one axis flat and two in the cube.
    let both = cuts_both_positions(&layer.mark, space_of(spec));

    // The axis a synthesizing transform *writes* to is the measured one; it reads
    // the other. For a bar, orientation decides which — the same call `check` makes
    // to pick `synth_axis`, so the two can never disagree about the key axis.
    let key = match layer.mark {
        ref m if is_slot_mark(m) && slot_orient(xt, yt) == Orient::Horizontal => Channel::Y,
        _ => Channel::X,
    };
    let named = |ch: &Channel| -> Option<(String, VarType)> {
        let cd = match ch {
            Channel::Y => yd?,
            _ => xd?,
        };
        let t = if matches!(ch, Channel::Y) { yt } else { xt }?;
        Some((format!("{}({})", channel_name(ch), cd.field), t))
    };

    for t in &layer.transforms {
        // Which axes this transform reads, and **which type it refuses there**.
        //
        // The second half is new with the tile plot, and it is what makes the pair
        // `bin`/`count` one rule rather than two: *`bin` cuts, `count` tallies.* A
        // cutting transform needs room to cut, so it refuses a category; a tallying
        // one needs cells that already exist, so it refuses a number. Same axis, same
        // question, opposite answers — and stating the polarity here keeps both in the
        // one function that asks a position's type.
        let (required, refuses): (&[Channel], VarType) = match t {
            Transform::Smooth => (&[Channel::X, Channel::Y], VarType::Discrete),
            // A **field** is a quantity that exists between the data points, so it
            // needs two axes with somewhere to spread and refuses a category on
            // either. Asked of `has_no_measure_axis` rather than of a mark name, so
            // the `zone` heatmap and the `path` contour cannot answer it differently.
            Transform::Density if both => {
                (&[Channel::X, Channel::Y], VarType::Discrete)
            }
            // A **cut** asks for less, and the difference is the mixed mesh. `bin`
            // does not need two continuous axes, it needs *something to cut*: it cuts
            // the axis with a width and leaves alone the one that arrives already
            // cut, because a category is a cell. So it refuses only where **neither**
            // axis can be cut — where there is no bin to make and the transform the
            // user wanted is `count` (spec §5).
            //
            // Why `density` is not the same case, since the two are otherwise one
            // rule: a tally over a product mesh is **joint**, so every cell holds the
            // same kind of measurement and the ramp compares them honestly. A density
            // per category would be **conditional** — each slot's estimate integrating
            // to 1 on its own — so the cells would not be comparable across slots and
            // the color scale would say they were. That is the *which margin
            // normalizes* question the 100% stacked bar asks, and it is not answered
            // by giving one transform two meanings depending on the types it was
            // handed.
            Transform::Bin
                if both
                    && (xt, yt) == (Some(VarType::Discrete), Some(VarType::Discrete)) =>
            {
                (&[Channel::X, Channel::Y], VarType::Discrete)
            }
            Transform::Bin if both => continue,
            // The **violin**: a category is one slot, and a slot is exactly what this
            // reading spreads the estimate *across* rather than along. So the type
            // that refuses `density` everywhere else is the one that selects this
            // reading, which is why the exemption is asked of `slot_density` rather
            // than written out again here — the fourth reading of one transform, on
            // the pattern `path`/`zone` set (spec §5).
            Transform::Density if slot_density(spec, layer, Some(df)).is_some() => continue,
            Transform::Bin | Transform::Density => (std::slice::from_ref(&key), VarType::Discrete),
            // A cut supplies the cells, so the tally has somewhere to land and the
            // axis's own type stops mattering — the same exemption `check_pair_summary`
            // gives the five value statistics, and for the identical reason. Without
            // it `zone * bin * proportion` was refused *as* `zone * proportion`, quoting
            // a continuous `x` and advising the `bin` the sentence already had.
            Transform::Count | Transform::Proportion
                if layer.transforms.contains(&Transform::Bin) => continue,
            // The tally read in two dimensions — the tile plot. Its cells are the
            // categories, so *both* axes must have them, and a continuous position is
            // the mistake here rather than a categorical one. On a mark that has a
            // measure axis `count` is unconstrained (a `bar` tallies distinct numbers
            // happily), so this fires only where the tally is also the extent.
            Transform::Count | Transform::Proportion if both => {
                (&[Channel::X, Channel::Y], VarType::Continuous)
            }
            _ => continue,
        };
        for ch in required {
            let Some((binding, vt)) = named(ch) else { continue };
            if vt != refuses {
                continue;
            }
            let message = match t {
                // A zone bins to *make* its cells, and a category's cell exists
                // already — so the direction is `count`, the transform that tallies
                // into cells rather than cutting them. The one-dimensional pair says
                // the same thing (`bar * bin` against `bar * count`); this is that
                // sentence with both axes in it.
                //
                // It fires only where **both** axes are categorical, since the mixed
                // mesh is legal: one category left for `bin` to work around is a plot,
                // and two is nothing to cut at all.
                Transform::Bin if layer.mark == Mark::Zone => format!(
                    "gog: `zone * bin` cuts an axis into cells, and there is no axis here \
                     it can cut — `{binding}` is categorical, and so is the other one. A \
                     category is one slot, with no width to cut. To tally rows into the \
                     cells two categorical axes already make, `count` is the transform that \
                     does it: `zone * count + x(<a>) + y(<b>)` draws a cell per pair, \
                     colored by how many rows fell there. (With *one* categorical axis \
                     `zone * bin` draws the mixed mesh — the continuous axis cut into \
                     cells, one row of them per category.)"
                ),
                // The mirror, and the reason it reads as one rule: a tally needs cells
                // that exist, a cut makes them. Naming `bin` back is the whole fix.
                Transform::Count | Transform::Proportion => format!(
                    "gog: `zone * {}` tallies rows into the cells that two *categorical* axes \
                     already make, and `{binding}` is continuous — a number is a point, and a \
                     point owns no cell. To cut a continuous axis into cells first, `bin` is \
                     the transform that does it: `zone * bin` counts the rows in each, which \
                     is the heatmap.",
                    transform_name(t),
                ),
                // The same sentence for a field, whose plane is the two axes rather
                // than a mesh cut across them: an estimate needs somewhere to spread,
                // and a category is one slot with no room to spread in.
                Transform::Density if both => format!(
                    "gog: `{} * density` estimates a density over the *plane*, and `{binding}` \
                     is categorical — a category is one slot, with no interval for the estimate \
                     to spread along. Both axes must be continuous. To compare one continuous \
                     distribution across categories, `line * density + color({})` draws a curve \
                     per group.",
                    mark_name(&layer.mark),
                    binding.split_once('(').map_or("<column>", |(_, f)| f.trim_end_matches(')')),
                ),
                Transform::Bin => format!(
                    "gog: `bin` cuts a continuous axis into intervals, and `{binding}` is \
                     categorical — a category is one slot, with no width to cut. To tally rows \
                     per category, `count` is the transform that does it: `bar * count`."
                ),
                Transform::Density => format!(
                    "gog: `density` estimates a continuous distribution, and `{binding}` is \
                     categorical — there is no number line for the curve to spread along. For \
                     the share of rows in each category, that is `bar * proportion`."
                ),
                _ => format!(
                    "gog: `smooth` fits a curve of `y` against `x`, and `{binding}` is \
                     categorical — a fit needs a number line to run along. For a typical value \
                     per category, `bar * mean` (or `point * mean`) says it directly."
                ),
            };
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message,
            });
        }
    }
}

/// The pair transforms `range`/`confidence` produce a low/high pair, which only
/// the *span* marks draw (`interval` whiskers it, `ribbon` fills it, `line`/`step`
/// trace its two boundaries). Composed onto a *locus* mark — `point`, `bar`,
/// `area` — they used to render nonsense (two points or bars per group, or a
/// region wanting to be a ribbon): a silent Law-1 gap the Mark × Transform grid
/// surfaced, and `mark_takes_transform` records as `None`. Refused here with
/// direction toward the mark whose geometry the pair actually wants. The other
/// `None` marks are refused by a check that already owns them, so this handles
/// only these three: `box` by `check_box`, `text` by the `label` requirement,
/// `bounds` on any mark by `check_bounds`; `interval`/`ribbon`/`line`/`step` take
/// the pair legally.
fn check_pair_transform_marks(out: &mut Vec<Diagnostic>, layer: &Layer) {
    if !matches!(layer.mark, Mark::Point | Mark::Bar | Mark::Area) {
        return;
    }
    for t in &layer.transforms {
        // `bounds` on these marks is `check_bounds`'s; only `range`/`confidence`
        // reach a locus mark unrefused, which is the gap this closes.
        if !matches!(t, Transform::Range | Transform::Confidence) {
            continue;
        }
        let fix = match layer.mark {
            Mark::Area =>
                "To fill the space between a low and a high boundary, that is `ribbon`; \
                 an `area` fills to a baseline at 0 and draws one value per x",
            _ =>
                "The span marks draw a low/high pair — `interval` whiskers it, `ribbon` \
                 fills it. For a single summary value per group, a statistic like `mean` \
                 is what a point or bar draws",
        };
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `{}` produces a low value and a high one per group, but `{}` draws a \
                 single value at each x, not a span. {fix}.",
                transform_name(t),
                mark_name(&layer.mark),
            ),
        });
    }
}

/// A `box` carries its own five-number summary (spec §6), so — unlike `interval`,
/// which *requires* a range transform — it takes **none**. `resolve_scopes`
/// injects `Transform::Box`; any further *summary* the user composed onto a box
/// (`box * mean`) is refused with direction rather than silently run before the
/// summary. **All three collision modifiers are exempt here** and judged by their
/// own checks, which give geometry-aware direction: `box * dodge` is legal (the
/// grouped boxes are set side by side), while `box * stack` and `box * jitter` are
/// refused by `check_stack`/`check_jitter` toward `dodge` (a box subdivides its
/// slot, it does not pile or scatter) rather than by the blunter "box takes no
/// transform" here (spec §5). This owns only "a `box` takes no *statistic*."
/// `path` takes no transform, and the refusal explains why rather than only
/// saying no.
///
/// This is `check_box`'s mirror image, and the pair is worth reading together:
/// `box` refuses every transform because it *already carries* one, while `path`
/// refuses every transform because there is nothing left of it afterwards. A
/// value statistic reduces each key to a single row, so what would remain is one
/// point per key in key order — which is a `line`, drawn the long way round. A
/// pair transform emits two rows per key, a span rather than a route. And the
/// collision modifiers each need geometry a path lacks: a width, a baseline, a
/// cloud. So every arm of the grid's `None` for `path` has the same cause, and
/// the direction is always the same mark.
/// The marks with an **empty** transform row, refused here so the grid's `—` is
/// a promise the engine keeps.
///
/// Three marks are in this set, and their reasons rhyme: a transform replaces the
/// rows, and each of these three needs something about the *original* rows that
/// does not survive that. `path` needs their order. `rule` has no measure to
/// compute, having handed one whole axis to the panel. `text` needs a string per
/// row, and a summary per key has none.
///
/// Checked here rather than left to the transform stage for the reason spec §12
/// gives: a transform can only warn and then hand the renderer whatever it made,
/// and warn-then-draw is the silent drop the grammar refuses. Both non-`path`
/// arms were found by `every_none_cell_of_the_transform_grid_actually_refuses`
/// and both were live defects — `rule * mean` warned twice about a missing `x`
/// and drew an empty panel, and `text * mean` drew nine glyphs with no complaint
/// at all, for combinations the book already prints as impossible.
///
/// **The collision modifiers are exempt**, exactly as they are in `check_box`,
/// and for the same reason: their own checks give geometry-aware direction
/// (`dodge` wants a width, `stack` a baseline, `jitter` a cloud), where this
/// function can only say "no transform". Not exempting them was a live wording
/// bug: `path * dodge` used to answer with *both* messages, the first of them
/// claiming `dodge` "replaces those rows with one summary per key", which a
/// collision modifier does not do.
/// Everything a hierarchy can be wrong about, answered in one place.
///
/// **Why it is a check and not a warning inside the transform** (§12, and the
/// `bin`-on-a-category lesson): a column-type question is legality, and a
/// transform that discovers one can only warn and then hand the renderer an empty
/// frame — the warn-then-draw silent drop the grammar refuses. Every branch here
/// is fatal and names what to do.
///
/// The five failures, in the order a caller meets them: the mark cannot read a
/// partition; no levels were named; a level is missing or is not a category; a
/// branch has a **hole** in it; and an interior node carries a value of its own,
/// which is the one genuine ambiguity in the set and the reason Law 5 makes it a
/// refusal rather than a default.
fn check_partition(
    out: &mut Vec<Diagnostic>, spec: &PlotSpec, df: Option<&DataFrame>, layer: &Layer,
) {
    if !layer.transforms.contains(&Transform::Partition) {
        return;
    }

    // 1. A mark with no reading for a region description. Refused toward the two
    //    that have one, and toward what each of them does with it, so the reader
    //    learns the sentence rather than only its verdict.
    if mark_takes_transform(&layer.mark, &Transform::Partition) == TransformLegality::None {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `partition` divides a whole among nested parts and hands back one \
                 **region** per node — four edges and a center — and `{}` has no reading \
                 for a region. Two marks do: `zone` takes the edges and draws the \
                 rectangle, which in `polar()` is the sector, so `zone * partition(<a>, \
                 <b>) + x(<measure>)` is the icicle and the same sentence `+ polar()` is \
                 the sunburst; and `text` takes the center, so `text * partition(<a>, \
                 <b>) + label(name)` names each node where it sits. A length from a \
                 baseline, a route through its rows, or a low-and-high pair are the \
                 other geometries, and a node's ring is none of them.",
                mark_name(&layer.mark),
            ),
        });
        return;
    }

    // 2. The atom with nothing named. Its own message rather than a missing-column
    //    one, because the caller wrote a word and gave it no arguments.
    let levels: &[String] = match layer.partition.as_ref() {
        Some(p) if !p.levels.is_empty() => &p.levels,
        _ => {
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: "gog: `partition` needs the hierarchy's columns, outermost \
                          first — `partition(group, item, detail)` puts `group` on the \
                          innermost ring and `detail` on the rim. One row of the table \
                          is one leaf, and those columns spell the path down to it."
                    .to_string(),
            });
            return;
        }
    };

    let Some(df) = df else { return };

    // 3. A level that is not there, or is not a category. A number cannot name a
    //    level: a node is a *name* the rows share, and no two rows share a
    //    measurement the way they share a word.
    for l in levels {
        match actual_type(df, l) {
            None => {
                out.push(Diagnostic {
                    kind: DiagnosticKind::Illegal,
                    message: format!(
                        "gog: `partition({l}, …)` names a level the table does not have. \
                         Every level is a column of the same table, outermost first."
                    ),
                });
                return;
            }
            Some(VarType::Continuous) => {
                out.push(Diagnostic {
                    kind: DiagnosticKind::Illegal,
                    message: format!(
                        "gog: `partition` reads `{l}` as a level of the hierarchy, and \
                         `{l}` holds numbers. A level is a **name** the rows of a branch \
                         share; a measurement is what the branch is *weighed* by, which \
                         is `x({l})`. If `{l}` is a code rather than a quantity, make it \
                         text where your data lives."
                    ),
                });
                return;
            }
            // `Either` is a column the data cannot decide about (an all-empty one,
            // in practice), and it is let through rather than refused: a level with
            // no values reads as a branch that ends, which is the ragged rim.
            Some(VarType::Discrete) | Some(VarType::Either) => {}
        }
    }

    // 4 and 5 are facts about the rows, read by the same function the layout uses,
    //   so the refusal and the picture cannot disagree about what the tree is.
    let paths = crate::transform::paths(df, levels);

    if let Some(p) = paths.gap {
        let shown = p.iter().filter(|s| !s.is_empty()).cloned().collect::<Vec<_>>().join(" / ");
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: this table has a branch with a hole in it — a row reaching \
                 `{shown}` with a level above it left empty. A blank level *ends* a \
                 branch, which is what gives a hierarchy its ragged rim, and nothing \
                 below an ending can be reached. Fill the missing level, or move the \
                 row up to the depth it really sits at."
            ),
        });
        return;
    }

    if let Some(p) = paths.interior {
        let shown = p.join(" / ");
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `{shown}` has a value of its own *and* children with values, and \
                 those two readings draw different pictures: either its arc is the \
                 children's total (its own number already counted among them), or its \
                 own number sits beside them and widens it. gog will not pick — the \
                 arithmetic is the accounts', not the grammar's. Put every number on a \
                 **leaf** and let the parents be the sums, which is what a partition \
                 computes."
            ),
        });
        return;
    }

    // The measure is the bound `x`, so it has to be a number. Refused here rather
    // than by `check_distribution_axis`, whose message is about cutting an axis and
    // would send the caller looking for a bin.
    if let Some(x) = spec.position_for(layer, &Channel::X) {
        if actual_type(df, &x.field) == Some(VarType::Discrete) {
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: format!(
                    "gog: `x({0})` is what each branch is weighed by, and `{0}` holds \
                     categories rather than numbers. Name the column that carries the \
                     amount — `x(<amount>)` — or bind nothing at all, in which case \
                     every leaf weighs 1 and the partition tallies them.",
                    x.field,
                ),
            });
        }
    }
}

fn check_marks_that_take_no_transform(out: &mut Vec<Diagnostic>, layer: &Layer) {
    if !matches!(
        layer.mark,
        Mark::Path | Mark::Rule | Mark::Text | Mark::Zone | Mark::Surface
    ) {
        return;
    }
    let names: Vec<&str> = layer.transforms.iter()
        .filter(|t| !is_collision_modifier(t))
        // What a mark *does* take is asked of the shared table, never typed out
        // here. This filter used to name `zone`'s one transform by hand, and a
        // hand-maintained list beside a generated one always loses: the moment
        // `zone` learned to `bin`, the list still said `bounds` and the refusal
        // fired on a plot the grid advertised as legal. Same failure as the polar
        // fallback's, which is why that one is generated now too.
        .filter(|t| mark_takes_transform(&layer.mark, t) == TransformLegality::None)
        .map(transform_name)
        .collect();
    if names.is_empty() {
        return;
    }
    let listed = names.join("`, `");
    let message = match layer.mark {
        Mark::Path => format!(
            "gog: `path` strokes the rows in the order the table gives them, and `{listed}` \
             replaces those rows with one summary per key — after which the order is \
             the keys' and the path is a `line`. Use `line * {}`, which sorts by `x` \
             and is the mark a statistic is drawn on.",
            names[0],
        ),
        Mark::Rule => format!(
            "gog: `rule` is placed by one column and spans the other axis, so it has no \
             measure for `{listed}` to compute and nowhere to put the answer. Compute the \
             value where your data lives and give the rule a table of the results — one row \
             per line — which is what a rule's position always is: a column."
        ),
        // What is left on this arm once the two-dimensional group-by shipped:
        // `smooth`, which fits along a domain a cell has none of, and the two pair
        // transforms, which produce a low/high band rather than a value per cell.
        // The five reductions are no longer here — a zone measures by *color*, so
        // color is the channel that names their column (`measure_channel`, spec §5).
        Mark::Zone => format!(
            "gog: a `zone` measures each cell by *color*, one value at a time, and \
             `{listed}` does not produce one — `smooth` fits a curve along a domain, which \
             a cell has none of, and `range`/`confidence`/`bounds` produce a low and a high \
             where a cell has room for a single number. A zone takes the four that invent \
             their own measurement — `count` and `proportion` tally rows into the cells \
             your categories make, `bin` cuts cells out of two continuous axes and counts \
             them, `density` estimates a value at each — and the five that reduce a column \
             color names: `zone * mean + x(<a>) + y(<b>) + color(<column>)` averages it \
             within every cell. To shade a band the data computed, `ribbon * range` is the \
             mark that spans a statistic."
        ),
        // What is left on this arm once the terraced sheet shipped: `smooth`, the two
        // pair transforms, `count`/`proportion`, and the collision modifiers. `bin` is
        // no longer here and neither are the five reductions — a surface takes both
        // ways of tiling a floor now (spec §15).
        Mark::Surface => format!(
            "gog: a `surface` is a sheet over a floor whose cells tile without gaps, and \
             `{listed}` does not give it one — `smooth` fits a curve along a domain a cell \
             has none of, `range`/`confidence`/`bounds` give a low and a high where a cell \
             holds one height, and `count`/`proportion` tally into the cells two *categories* \
             make, which a surface refuses because slots leave air between them and \
             disconnected tiles are not a sheet. A surface takes the two transforms that do \
             tile: `bin` cuts the floor into adjacent cells and lays a flat lid on each — \
             `surface * bin * mean + x(<a>) + y(<b>) + z(<column>)` reduces the column you \
             name inside every cell — and `density` estimates a value at every node, which \
             the sheet then interpolates between: \
             `surface * density + x(<a>) + y(<b>) + space()`. Over categories, `bar` is the \
             mark, where the column under each tile says which cell it belongs to."
        ),
        _ => format!(
            "gog: `text` draws one string per row, taken from `label`, and `{listed}` \
             replaces those rows with one summary per key — which has no label to draw. \
             Compute the summary where your data lives and give `text` the resulting table, \
             so every row still carries the string it is drawn from."
        ),
    };
    out.push(Diagnostic { kind: DiagnosticKind::Illegal, message });
}

fn check_box(out: &mut Vec<Diagnostic>, layer: &Layer) {
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

/// `dodge` is a **collision modifier** (spec §5): it sets side by side the groups a
/// `color`/`group` split would otherwise stack at one shared position. Two things
/// make it well-formed, each refused with direction otherwise:
///
/// 1. **A width to subdivide.** The three offsets divide the mark set by geometry —
///    `dodge` narrows a mark's *width* (bar, box, interval), `stack` accumulates
///    along the measure axis (bar, area, and `point` as a pile of dots), `jitter`
///    spreads a mark with no width along a category (point). So `dodge` is legal
///    only on the width-bearing marks; elsewhere the
///    refusal names the offset that fits (point → jitter, line/area → stack). This
///    is Law 1 read correctly — a modifier combines with the marks whose geometry
///    it was defined for.
/// 2. **A split to separate.** With no `color`/`group` binding there is one mark per
///    slot and nothing to set beside anything, so `dodge` is refused toward adding
///    the split rather than accepted as a silent no-op (spec §12).
fn check_dodge(out: &mut Vec<Diagnostic>, layer: &Layer) {
    if !layer.transforms.contains(&Transform::Dodge) {
        return;
    }
    let name = mark_name(&layer.mark);
    // 1. A width for dodge to subdivide — read off the shared table so the grid
    //    and this refusal name the same mark set.
    if mark_takes_transform(&layer.mark, &Transform::Dodge) == TransformLegality::None {
        let fix = match layer.mark {
            Mark::Point => "A point has no width to subdivide — to spread overlapping points, `jitter` is the tool",
            Mark::Line | Mark::Area => "A connected path is offset by *accumulating* (`stack`), not by subdividing a width",
            Mark::Ribbon => "A filled band has no width to subdivide — overlapping bands are told apart by transparency (`style(opacity = )`)",
            _ => "dodge subdivides a mark's width across groups, which this mark does not have",
        };
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `dodge` sets side by side the bars, boxes or whiskers that a `color` split \
                 stacks at one position — but `{name}` is not one of those. {fix}."
            ),
        });
        return;
    }
    // 2. A split to separate.
    if !(layer.encodings.contains_key(&Channel::Color) || layer.encodings.contains_key(&Channel::Group)) {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `{name} * dodge` has no groups to set side by side — nothing splits the \
                 mark. Add `color(<field>)` (or `group(<field>)`) so there are groups to dodge."
            ),
        });
    }
}

/// `stack` is `dodge`'s sibling (spec §5) — the collision modifier for the marks
/// that *accumulate* rather than sit side by side. It hands every element the span
/// `[base, top]` along the measure axis, and each mark draws that span as its own
/// geometry. Well-formed on two conditions, each refused with direction otherwise:
///
/// 1. **A measure to accumulate along.** The three offsets divide the mark set by
///    geometry — `stack` accumulates (`bar`, `area`, `point`), `dodge` subdivides a
///    width (`bar`, `box`, `interval`), `jitter` spreads a widthless mark along a
///    category (`point`). Elsewhere the refusal names the offset that fits.
/// 2. **Something at one position to pile** — and that is a different sentence for
///    a mark whose element *is* a quantity than for one whose element is a row:
///    - `bar`/`area` measure a height, so what piles is a `color`/`group` split.
///      With no split there is one mark per position and nothing to accumulate, so
///      it is refused toward adding the split rather than accepted as a silent
///      no-op (spec §12).
///    - `point` has no height. Its contribution to a measure is that it is *there*,
///      so a pile of points is a **count of rows** and the layer needs the transform
///      that counts them — `bin` on a continuous axis, `count` on a categorical
///      one. It needs no split: the dot plot piles a single series.
fn check_stack(out: &mut Vec<Diagnostic>, layer: &Layer) {
    if !layer.transforms.contains(&Transform::Stack) {
        return;
    }
    let name = mark_name(&layer.mark);
    // 1. A measure for stack to accumulate along — read off the shared table.
    if mark_takes_transform(&layer.mark, &Transform::Stack) == TransformLegality::None {
        let fix = match layer.mark {
            Mark::Line | Mark::Step => "An unfilled path has nothing to fill and pile — use `area`, which stacks as filled bands",
            Mark::Box | Mark::Interval => "A box or whisker is set beside its neighbors by subdividing the slot — that is `dodge`, not `stack`",
            Mark::Ribbon => "A ribbon already spans a low to a high, so it measures no height from a baseline to pile — overlapping bands are told apart by transparency (`style(opacity = )`)",
            _ => "stack piles a mark's measured height across groups, which this mark does not have",
        };
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `stack` piles on top of one another the bars or areas that a `color` split \
                 draws at one position — but `{name}` is not one of those. {fix}."
            ),
        });
        return;
    }
    // 2a. A point's pile is a count of rows, so it asks for the transform that
    // counts them — not for a split, which it does not need.
    if layer.mark == Mark::Point {
        if !layer.transforms.iter().any(|t| matches!(t, Transform::Bin | Transform::Count)) {
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message:
                    "gog: `point * stack` piles one dot per observation along the measure axis — \
                     the dot plot — so the measure has to be a count of rows, and nothing here \
                     counts them. Add the transform that does: `point * bin * stack` for a \
                     continuous axis (dots per interval), `point * count * stack` for a \
                     categorical one (dots per category)."
                    .to_string(),
            });
        // **A share cannot be piled, because a dot is a whole observation.** The
        // pile's height is `round(top - base)` dots, and every share is below one, so
        // it rounds to none: `point * bin * proportion * stack` drew an **empty
        // panel** with a fabricated 0..1 axis and exited 0. That is the same failure
        // the book's binned-category chunk had — a plot with nothing in it standing
        // where a refusal belonged — and it is not the job rule's to catch, because
        // `proportion` (scale) and `stack` (position) really are different jobs. What
        // collides is narrower: this mark's pile counts *units*, and a fraction of an
        // observation is not one.
        } else if layer.transforms.contains(&Transform::Proportion) {
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message:
                    "gog: `point * proportion * stack` has no dots to pile — a dot plot stacks \
                     one dot per observation, and `proportion` turns the count into a fraction, \
                     which is less than one whole dot. Keep whichever you meant: \
                     `point * count * stack` to pile the observations themselves, or \
                     `bar * count * proportion` to read the same shape as shares."
                    .to_string(),
            });
        }
        return;
    }
    // 2b. A split to separate.
    if !(layer.encodings.contains_key(&Channel::Color) || layer.encodings.contains_key(&Channel::Group)) {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `{name} * stack` has no groups to pile up — nothing splits the mark. Add \
                 `color(<field>)` (or `group(<field>)`) so there are groups to stack."
            ),
        });
    }
}

/// Where a pile hangs — `stack(baseline = )`, checked for a name that exists and for a
/// space with an origin to spend (spec §5).
///
/// **Displacing a pile costs the measure axis's origin**, and that is only ever free in
/// the plane. In `polar` the measure axis is an angle or a radius, and neither origin
/// is a choice: a radius of zero is the center of the circle, and moving a pile off it
/// asks for a negative radius, which is not a place. In `space` the pile stands on the
/// floor, and in `nest` the space has already fixed what each position means before any
/// layer is read. So a displaced baseline is refused outside `flat` — the same shape of
/// ruling as the donut (§18), where a want that is meaningless in five spaces out of
/// six turned out to belong to the scale rather than to the space.
///
/// `"zero"` is legal everywhere, being what every pile has always done.
fn check_baseline(out: &mut Vec<Diagnostic>, spec: &PlotSpec, layer: &Layer) {
    let Some(b) = layer.stack.as_ref().and_then(|s| s.baseline.as_deref()) else { return };
    if !BASELINES.contains(&b) {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `{b}` is not a baseline. `stack(baseline = )` takes {}. \
                 `\"zero\"` stands every pile on the axis, which is the plain stacked \
                 bar; `\"center\"` hangs each pile so its middle is at zero; \
                 `\"wiggle\"` chooses the foot that makes the bands as flat as it can, \
                 which is the streamgraph.",
                BASELINES.iter().map(|t| format!("`\"{t}\"`")).collect::<Vec<_>>().join(", "),
            ),
        });
        return;
    }
    if b == "zero" {
        return;
    }
    let space = space_of(spec);
    if space != SpaceKind::Flat {
        let s = space_name(space);
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `stack(baseline = \"{b}\")` moves every pile off the origin of the \
                 measure axis, and `{s}()` has no origin to spare. A displaced pile is a \
                 statement about a plane: in `{s}()` the measure is an angle or a radius \
                 or a height the space has already fixed, so moving the foot asks for a \
                 place that is not on the plot. Keep the space and drop the baseline \
                 (`stack` alone, or `stack(share = TRUE)` for composition), or keep the \
                 baseline and draw it flat."
            ),
        });
    }
}

/// **A pile has one direction** — the third condition on `stack`, and the only one
/// of the three that has to read the numbers (spec §5, §12).
///
/// `stack_frame` gives each element the span `[base, base + value]`, where `base` is
/// everything piled below it at the same position. That is the whole of stacking, and
/// it is correct for a pile whose members agree in sign: all-positive grows up from
/// zero, all-negative grows *down* from zero, and in both the bands sit end to end
/// with the axis reading the total. Mix the signs at one position and the arithmetic
/// turns on itself — `top < base`, so the band is drawn **inside** the ones beneath
/// it, and a reader sees ink of length `|value|` sitting where the data said the pile
/// shrank. Measured before this refusal existed: `a = 5`, `b = -3` drew `b` from 2 up
/// to 5, entirely overlapping `a`, with nothing said. Not a poor picture but an
/// **inverted** one, which is why it is fatal rather than an Assumption; a
/// warn-then-draw here is the silent drop §12 exists to forbid.
///
/// `stack(share = true)` fails in the same place and more obviously: a share is a
/// fraction of the pile's total, and with `5` and `-3` the total is `2`, so the two
/// "shares" come out `2.5` and `-1.5`. There is no normalization that rescues it,
/// which is the confirming sign that the defect is in the *pile* and not in how it
/// is drawn.
///
/// **Per position, not per column**, because the pile is what has to agree: piles
/// pointing up at one x and down at another are two coherent piles and stay legal
/// (the diverging stacked bar). Refusing the whole column would forbid that to catch
/// this, which is Law 8 — enforce well-formedness hard, never forbid the ugly-but-legal.
///
/// **Asked of the values `stack` will actually pile**, which is why this runs the
/// layer's other transforms first rather than reading the bound column: `bar * sum *
/// stack` piles sums, and a column of mixed signs whose *cells* come out all-positive
/// is a plot with nothing wrong with it. Running `transform::apply` minus `Stack` is
/// the one way to be sure the check and the renderer are looking at the same numbers,
/// on the same reasoning as `limit_cut` — a count the user is given must not disagree
/// with the picture. It costs a second pass over the frame, and only on layers that
/// carry `stack`.
fn check_stack_signs(out: &mut Vec<Diagnostic>, spec: &PlotSpec, df: &DataFrame, layer: &Layer) {
    if !layer.transforms.contains(&Transform::Stack) {
        return;
    }
    // The conditions `check_stack` already refuses. Asking again here would be a
    // second refusal for one mistake, and `apply` on a mark that cannot stack would
    // be answering a question nobody may ask.
    if mark_takes_transform(&layer.mark, &Transform::Stack) == TransformLegality::None {
        return;
    }
    let group_field = layer.encodings.get(&Channel::Color)
        .or_else(|| layer.encodings.get(&Channel::Group))
        .map(|e| e.field.as_str());
    let Some(gf_name) = group_field else { return }; // refused by `check_stack` 2b

    // Which axis the pile stands on, read off the bindings exactly as the renderer
    // reads it (`slot_orient`): the key groups, the measure accumulates.
    let field = |c: Channel| spec.position_for(layer, &c).map(|e| e.field.as_str()).unwrap_or("");
    let (x_field, y_field) = (field(Channel::X), field(Channel::Y));
    let (key_field, out_field) = match slot_orient(actual_type(df, x_field), actual_type(df, y_field)) {
        Orient::Vertical => (x_field, y_field),
        Orient::Horizontal => (y_field, x_field),
    };

    let piled = crate::transform::apply(
        df,
        &layer.transforms.iter().filter(|t| **t != Transform::Stack).cloned().collect::<Vec<_>>(),
        key_field, out_field,
        layer.bin.as_ref(), None, layer.density.as_ref(), layer.confidence.as_ref(),
        layer.r#box.as_ref(), layer.bounds.as_ref(), None, group_field,
    );
    let Some(vals) = piled.float_col(out_field) else { return };
    let Some(groups) = piled.str_col(gf_name) else { return };

    // Two elements share a position on the same terms `stack_frame` uses, so the piles
    // this check inspects are the piles that will be drawn: string equality for a
    // categorical key, a tolerance compare for a numeric one, and *everything* in one
    // pile when no key is named (the share-of-total bar, and the pie).
    let key_str = piled.str_col(key_field);
    let key_num = piled.float_col(key_field);
    let same_pos = |a: usize, b: usize| -> bool {
        if key_field.is_empty() { return true; }
        if let Some(k) = key_str { return k[a] == k[b]; }
        if let Some(k) = key_num { return (k[a] - k[b]).abs() < 1e-9; }
        a == b
    };

    for i in 0..vals.len() {
        if !(vals[i] > 0.0) { continue }
        // The first negative sharing this position, if there is one. Reported with
        // both offending groups and both numbers, because "some values are negative"
        // sends a reader to scan the whole table for a pile they cannot see.
        let Some(j) = (0..vals.len()).find(|&j| vals[j] < 0.0 && same_pos(i, j)) else { continue };
        let name = mark_name(&layer.mark);
        let at = match (key_str, key_num) {
            _ if key_field.is_empty() => "this position".to_string(),
            (Some(k), _) => format!("{key_field} = {}", k[i]),
            (_, Some(k)) => format!("{key_field} = {}", k[i]),
            _ => "this position".to_string(),
        };
        // The direction divides by geometry, the way every `stack` refusal does: a
        // bar can be set beside its neighbors and keep its own baseline, where an
        // area has no baseline of its own to keep and simply stops piling.
        let fix = match layer.mark {
            Mark::Bar => "`bar * dodge` sets the groups side by side instead, and a dodged bar \
                          keeps its own baseline, so a negative one hangs below the axis where \
                          a reader can see that it is negative",
            Mark::Area => "drop `stack`: overlaid areas each run from their own baseline, \
                           translucent so the ones behind show through",
            _ => "drop `stack`: without it each group is drawn from its own baseline",
        };
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `{name} * stack` piles the groups at one position on top of one another, \
                 and at {at} they disagree in sign — `{}` is {} and `{}` is {}. A pile has one \
                 direction: the band going the other way is drawn *inside* the ones below it, so \
                 the plot would show a block of {} where the number is negative and read its sign \
                 backwards. Either keep the sign and drop the pile — {fix} — or keep the pile and \
                 drop the sign, stacking magnitudes you took the absolute value of in the host. \
                 (A pile whose members agree stays legal both ways: all-negative grows downward \
                 from zero exactly as all-positive grows up.)",
                groups[i], vals[i], groups[j], vals[j], vals[j].abs(),
            ),
        });
        return; // one refusal per layer — the first pile names the defect
    }
}

/// `jitter` is the third collision modifier (spec §5): the offset for a mark with
/// no width to subdivide, along an axis with no magnitude to spend — `point` on a
/// *category*. It spreads a strip plot's coincident points apart. Two things make it
/// well-formed, and — unlike `dodge`/`stack` — the second is *data-aware*, which
/// is why this check takes `df` and lives beside `check_slot_shape` rather than in
/// the structural block:
///
/// 1. **A point cloud to spread.** The three offsets divide by geometry, and
///    jitter's mark is `point` alone; every other mark is refused toward the offset
///    its geometry wants (the width marks → `dodge`). It is `point`'s answer to
///    overlap along the **categorical** axis; `stack` is its answer along the
///    **measure** axis, where the offset can be a count instead of a nudge — one
///    mark, two overlap problems, one modifier each (spec §5).
/// 2. **A categorical band to spread within.** jitter offsets *only* along a
///    categorical position axis — never along one carrying a measured value, which
///    it would falsify (the ggplot2 divergence, spec §5). With both axes
///    continuous there is neither a band nor an axis it may honestly move, so it is
///    refused toward `style(opacity = )`, the honest tool for continuous
///    overplotting. It does **not** require a `color`/`group` split: it resolves
///    same-position overlap of individual points, which needs no grouping.
fn check_jitter(out: &mut Vec<Diagnostic>, spec: &PlotSpec, df: &DataFrame, layer: &Layer) {
    if !layer.transforms.contains(&Transform::Jitter) {
        return;
    }
    // 1. A scatter of individual points to spread — read off the shared table.
    if mark_takes_transform(&layer.mark, &Transform::Jitter) == TransformLegality::None {
        let name = mark_name(&layer.mark);
        let fix = match layer.mark {
            Mark::Bar | Mark::Box | Mark::Interval =>
                "Those marks have a width; to set their `color` groups apart, `dodge` sets them side by side",
            Mark::Line | Mark::Area | Mark::Step | Mark::Ribbon =>
                "A connected path or filled region is one shape, not a cloud of separate points to spread",
            _ => "jitter spreads a scatter of individual points, which this mark does not draw",
        };
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `jitter` spreads overlapping points sideways so a strip plot's density \
                 shows — but `{name}` is not a point mark. {fix}."
            ),
        });
        return;
    }
    // 2. A categorical band to spread within: at least one position axis must be a
    //    category. Both continuous → there is nothing jitter may honestly move.
    let xt = spec.position_for(layer, &Channel::X).and_then(|c| actual_type(df, &c.field));
    let yt = spec.position_for(layer, &Channel::Y).and_then(|c| actual_type(df, &c.field));
    if xt == Some(VarType::Continuous) && yt == Some(VarType::Continuous) {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message:
                "gog: `point * jitter` spreads points sideways within a categorical band, but \
                 here both `x` and `y` are continuous — there is no band to spread within, and \
                 nudging a measured value would misplace it. For overplotting on two continuous \
                 axes, `style(opacity = )` reveals density without moving any point off its value."
                    .to_string(),
        });
    }
}

// ---------------------------------------------------------------------------
// Scope resolution
//
// Position decides scope. A channel written before any mark is plot-scoped and
// reaches every layer that can accept it; written after a mark it binds to that
// mark alone. A layer's own binding always wins — the nearest-wins rule that
// already governs `data()`.
//
// The rule lives here, in one function, because the renderer and the checker
// must never disagree about which bindings a layer has. This replaces a
// broadcast in the R front end that reached *backwards* over committed layers:
// it made `point + color(g) + line` and `line + point + color(g)` mean
// different things, and it left no way to scope a channel that only one mark
// accepts — `line + color(country) + point + size(population)` put `size` on
// the line, which has no size, and refused to render.
// ---------------------------------------------------------------------------

/// Does this mark have the feature at all? Plot-scoped channels reach only the
/// layers that do — a `size` written for the whole plot means the points, not
/// the lines drawn beside them.
fn accepts_binding(mark: &Mark, channel: &Channel) -> bool {
    rule_for(mark, channel).obligation != Obligation::Cannot
}

/// Materialize plot-scoped channels onto the layers that accept them.
///
/// Idempotent: layer bindings take precedence, so resolving an already-resolved
/// spec changes nothing. Both `check` and the renderer call it on entry, which
/// is what keeps them in step.
pub fn resolve_scopes(spec: &PlotSpec) -> PlotSpec {
    let mut out = spec.clone();
    for layer in &mut out.layers {
        for (channel, def) in &spec.channels {
            if layer.encodings.contains_key(channel) || !accepts_binding(&layer.mark, channel) {
                continue;
            }
            layer.encodings.insert(channel.clone(), def.clone());
        }
        // A `box` carries its five-number summary intrinsically (spec §6): the
        // statistic that draws as exactly one mark is constitutive of it, not an
        // orthogonal transform the user composes. So the mark injects it here —
        // the one pass both `check` and the renderer run, which is what keeps
        // them agreeing about what a box layer contains. Idempotent, and a
        // user-typed transform is left in place for `check_box` to refuse.
        if layer.mark == Mark::Box && !layer.transforms.contains(&Transform::Box) {
            layer.transforms.push(Transform::Box);
        }
    }
    out
}

/// Explain a binding that names no column.
///
/// The R front end deparses the argument, so a quoted string arrives with its
/// quotes intact: `color(species)` gives `species`, `color("red")` gives
/// `"red"`. That surviving quote is a reliable signal that the caller passed a
/// *value* where the grammar expects a *column name* — which is almost always
/// someone reaching for a constant. Saying "check the spelling" to that reader
/// sends them looking for a typo that isn't there.
fn missing_column_message(c: &str, m: &str, field: &str, channel: &Channel) -> String {
    let literal = field.len() >= 2 && field.starts_with('"') && field.ends_with('"');
    if !literal {
        return format!(
            "gog: `{c}({field})` refers to a column that is not in the data. \
             Check the spelling of `{field}`."
        );
    }

    let inner = &field[1..field.len() - 1];

    // A color where a column belongs: they meant to set, not map.
    if *channel == Channel::Color && is_valid_color(inner) {
        return format!(
            "gog: `{c}({field})` names a column, and there is no column called `{inner}`. \
             To paint every {m} {inner}, that is a setting rather than a mapping: \
             use `style(color = {field})`."
        );
    }

    format!(
        "gog: `{c}({field})` is quoted, so it names a column called `{inner}` — and there \
         is no such column. Column names are written bare: `{c}({inner})`. If you meant a \
         fixed value rather than a column, that is `style()`."
    )
}

/// Check a figure — one plot, or a page of them (spec §11).
///
/// A page is checked plot by plot, because that is what a page *is*: separate
/// plots, each grammatical on its own terms. What is left for this level is the
/// one thing no plot can see from inside itself — whether the sizes the plots
/// asked for fit on the page they were arranged onto.
pub fn check_figure(figure: &Figure, data: &HashMap<String, DataFrame>) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for spec in figure.plots() {
        out.extend(check(spec, data));
    }
    check_page_fits(&mut out, figure, crate::render::svg::CANVAS);
    out
}

/// A page whose cells ask for more room than the page has.
///
/// Checked against the **whole** canvas rather than against each node's share of
/// it, deliberately: a nested page has less room than that, so this can only
/// ever miss an over-full page, never refuse one that fits. A refusal that fires
/// on a legal sentence is a much worse failure than a squeeze that draws.
fn check_page_fits(out: &mut Vec<Diagnostic>, figure: &Figure, canvas: (f64, f64)) {
    let Figure::Page(page) = figure else { return };

    let horizontal = page.arrange == crate::ir::Arrange::Beside;
    let (limit, dimension, word) = if horizontal {
        (canvas.0, "width", "beside")
    } else {
        (canvas.1, "height", "below")
    };
    let claimed: f64 = page.cells.iter().filter_map(|c| c.ask(horizontal)).sum();
    if claimed > limit {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: the plots {word} each other ask for {claimed:.0}px of {dimension} \
                 between them, and the page has {limit:.0}. A `theme({dimension} = )` on a \
                 composed plot is how much of the page that plot takes, so the ones that \
                 state it must leave room for the ones that do not."
            ),
        });
    }
    for cell in &page.cells {
        check_page_fits(out, cell, canvas);
    }
}

/// Check every layer of `spec` against the table.
///
/// Returns diagnostics in spec order. An empty vector means the plot is
/// grammatical and fully renderable.
pub fn check(spec: &PlotSpec, data: &HashMap<String, DataFrame>) -> Vec<Diagnostic> {
    let mut out = Vec::new();

    check_plot_scope(&mut out, spec);
    let spec = &resolve_scopes(spec);

    for layer in &spec.layers {
        let mark = &layer.mark;
        let m = mark_name(mark);

        // A mark this engine cannot draw makes every later question about the
        // layer moot — and answering them anyway produces advice that does not
        // apply. Say the one true thing and move on.
        if check_mark(&mut out, mark) {
            continue;
        }

        // A layer may name its own *column* for a shared axis; a layer naming its
        // own *scale* is a second axis, which §18 refuses.
        check_layer_position(&mut out, spec, layer);

        // The layer's own table, resolved once. Read this early because the very
        // first refusal below is data-aware: which reading of `density` a layer is in
        // is a question only the column types answer.
        let df = layer
            .data
            .as_ref()
            .or(spec.data.as_ref())
            .and_then(|name| data.get(name));

        // An interval or a ribbon floats between two extents, and only a range
        // transform — or the violin's reflection — supplies them; without either
        // there is nothing to span.
        check_span_needs_range(&mut out, spec, df, layer);

        // A box carries its own summary, so it takes no transform — the inverse
        // refusal to `check_interval`'s.
        check_box(&mut out, layer);
        check_marks_that_take_no_transform(&mut out, layer);

        // `dodge` is legal only on the width-bearing marks, and only with a group
        // split to separate — refused with direction otherwise (spec §5).
        check_dodge(&mut out, layer);

        // `stack` is `dodge`'s sibling: legal only on the accumulating marks
        // (`bar`, `area`), and only with a group split to pile — the same two
        // conditions, refused with the geometry-aware direction (spec §5).
        check_stack(&mut out, layer);
        // Where the pile hangs: a baseline that names one of the three, and a space
        // with an origin to spend. Structural, so it sits with `check_stack` rather
        // than in the df-gated block below.
        check_baseline(&mut out, spec, layer);

        // The pair transforms (`range`/`confidence`) draw a span, so a locus mark
        // (`point`/`bar`/`area`) that draws one value per x is refused toward the
        // span mark whose geometry the pair wants — the Law-1 gap the grid found.
        check_pair_transform_marks(&mut out, layer);

        // A bar with no position axis takes only the statistics that mean something
        // without one (spec §15) — `count`/`sum`, not `bin`/`density`/`smooth`.
        check_keyless_statistic(&mut out, spec, layer);

        // Two transforms that do the same job contradict (spec §5). Mark-agnostic and
        // dimension-agnostic, so it sits out here rather than in `check_pair_summary`,
        // where the same refusal reached only the marks that read over two positions
        // and left every one-key composition drawing a silent lie.
        check_chain_jobs(&mut out, spec, layer);
        // What a normalizer cannot rescale (spec §5). Its own check because
        // `proportion` composes with far more than it refuses — the question is not
        // *which transform owns the measurement* but *is there one number per cell
        // for a share to be a share of*.
        check_share_composition(&mut out, layer);
        // A mark that reads a domain, cut by the column that supplies it — the
        // subset holds one position and there is nothing left to read between.
        // Asked per layer because the mark decides it, and it covers both
        // partitions: `play`'s frames and `facet`'s panels (§11).
        check_domain_split(&mut out, spec, layer);
        // A two-dimensional reading — `zone * bin`, `zone * density`, `path *
        // density` — needs both axes and colors itself by what it measured. None
        // of that is true of a bounded zone, so it is checked apart from
        // `check_bounds`.
        check_field(&mut out, spec, layer);
        // Every way a hierarchy can be malformed, from the mark that cannot read one
        // down to an interior node carrying its own value. Takes the frame as an
        // `Option` rather than sitting in the block below, because its first two
        // branches — the wrong mark, and no levels named — are facts about the
        // sentence and must refuse even when the table is missing.
        check_partition(&mut out, spec, df, layer);

        // Which mesh, and whether this mark has a plane to tile at all. Takes the
        // frame because the third answer is data-aware: a mixed mesh has two axes and
        // still no plane, one of them being categorical.
        check_tiling(&mut out, spec, df, layer);
        // Whether a `surface` has the cube and a grid to span. Takes the frame the same
        // way and for the same reason: the cube is a fact about the sentence, the grid
        // one only the rows can state.
        check_surface(&mut out, spec, df, layer);
        // `levels` needs a plane to cut and `bandwidth` needs a single axis, so each
        // is refused in the reading it cannot mean.
        check_density_params(&mut out, spec, df, layer, space_of(spec));

        // The trio's third offset, `jitter`, is checked in the df-gated block
        // below (`check_jitter`) — unlike these two it must read the axis types.

        if let Some(df) = df {
            check_slot_shape(&mut out, spec, df, layer);
            check_area_overlap(&mut out, spec, df, layer);
            // `jitter` is point-only and legal only when a position axis is
            // categorical — a data-aware check, so it sits with the others (§5).
            check_jitter(&mut out, spec, df, layer);
            // `bounds` is legal only on the span marks, and its two columns must
            // exist — a data-aware check, so it sits here too.
            check_bounds(&mut out, df, layer);
            // Where a `zone`'s sides come from. Data-aware because one of the four
            // answers is the axis itself: a category owns a slot, a number is a point.
            check_zone_extent(&mut out, spec, df, layer);
            // The two-dimensional group-by: a reduction read over a pair of keys
            // needs its measure channel bound to a numeric column, and both
            // positions categorical. Data-aware for the same reason `check_zone_extent`
            // is, and placed after it because it takes over that refusal for the five.
            check_pair_summary(&mut out, spec, df, layer);
            // `bin`/`density`/`smooth` describe a spread along an axis, so the axis
            // they read must carry a number. Answered here rather than in
            // `transform.rs`, which could only warn and then draw anyway (§12).
            check_distribution_axis(&mut out, spec, df, layer);
            // Which axis a `rule` sits on — Law 7's second relaxation, and a
            // data-aware question for the same reason `check_bounds` is: it is
            // answered by which of the plot's position columns this layer's own
            // table holds.
            check_rule(&mut out, spec, df, layer);
            // `stack`'s third condition, and the one that has to read the numbers:
            // a pile's members must agree in sign, or the band going the other way
            // is drawn inside the ones below it. Data-aware, and it runs the layer's
            // other transforms first because the pile is made of *their* output.
            check_stack_signs(&mut out, spec, df, layer);
            // What a stated domain actually cuts — counted aloud, and fatal when
            // it cuts everything. Data-aware by nature, and last because it is
            // the only check that reports on rows rather than on the sentence.
            check_limit_rows(&mut out, spec, df, layer);
        }

        let synth_axis = synth_axis(spec, layer, df);

        // Effective bindings: the positions this layer actually reads, plus its
        // own channels. A layer naming its own column for a shared axis takes
        // precedence over the plot's — nearest wins, as with `data()` (spec §8) —
        // and it must, or the plot's column would be looked for in the layer's
        // table and reported missing, which is how the designed annotation
        // sentence used to fail.
        let mut bound: Vec<(Channel, &str)> = Vec::new();
        for ch in [Channel::X, Channel::Y, Channel::Z] {
            if let Some(cd) = spec.position_for(layer, &ch) {
                bound.push((ch, &cd.field));
            }
        }
        for (ch, cd) in &layer.encodings {
            if matches!(ch, Channel::X | Channel::Y | Channel::Z) {
                continue; // already resolved above
            }
            bound.push((ch.clone(), &cd.field));
        }

        // An axis a mark *spans* is not one it reads, so the plot's column for that
        // axis is expected to be absent from the layer's own table — a thresholds
        // table holds the threshold and nothing else. Checking it as an ordinary
        // binding would report that normal case as a missing column, which is how
        // the designed sentence would have failed.
        //
        // The two relaxed marks answer it differently because they read position
        // differently. A `rule` reads *one* of the plot's position columns, so
        // only the other is skipped. A `zone` reads **neither** — its four sides
        // are its own columns, named by `bounds` — so both are skipped and
        // `check_bounds` owns its whole position story.
        let spans_axis = |channel: &Channel| {
            if !matches!(channel, Channel::X | Channel::Y) {
                return false;
            }
            match mark {
                Mark::Zone => true,
                Mark::Rule => df.is_none_or(|d| rule_axis(spec, d, layer).as_ref() != Some(channel)),
                _ => false,
            }
        };

        // --- bindings that are present ------------------------------------
        for (channel, field) in &bound {
            if spans_axis(channel) {
                continue;
            }
            let c = channel_name(channel);
            let r = rule_for(mark, channel);

            if r.obligation == Obligation::Cannot {
                out.push(Diagnostic {
                    kind: DiagnosticKind::Illegal,
                    message: format!(
                        "gog: `{c}` cannot be bound to `{m}` — {} {m} has no {c} feature. \
                         Remove `{c}({field})`, or use a mark that has one.",
                        article(m)
                    ),
                });
                continue;
            }

            // A synthesizing transform *writes* y: `y(count)` names the output
            // column, not an input. There is nothing in the data to check it
            // against, so the binding is always well-formed.
            // The **violin** is the one layer carrying a synthesizing transform that
            // writes to no axis at all: both positions name real columns (the slot and
            // the measure) and the estimate rides a width instead. So the exemption is
            // withheld here and `y(life)` is type-checked like any other binding — a
            // misspelled measure column on a violin would otherwise pass unremarked.
            if *channel == synth_axis && synthesizes_measure(&layer.mark, &layer.transforms)
                && slot_density(spec, layer, df).is_none() {
                continue;
            }

            // The same exemption one channel over. A two-dimensional reading writes
            // its measurement to `color` rather than to an axis — both positions are
            // spoken for — so `color(count)`/`color(density)`/`color(level)` names an
            // output column too. `check_field` has already refused every *other*
            // field here, so reaching this line means the name is the synthesized one.
            if *channel == Channel::Color && measures_cells(&layer.mark, &layer.transforms) {
                continue;
            }

            // The same exemption a third time, and this one is on `label`. A
            // partition writes a node's own name into a column of its own, so
            // `text * partition(…) + label(name)` names an *output* — the counterpart
            // of `color(count)` one channel over, for a mark that measures with
            // neither an axis nor a ramp but with a string. The other columns it
            // publishes (`depth`, and the levels carried down) are real inputs or are
            // checked on their own axes, so only this one needs saying.
            if *channel == Channel::Label
                && layer.transforms.contains(&Transform::Partition)
                && *field == crate::transform::NODE_NAME
            {
                continue;
            }

            let Some(df) = df else { continue };
            let Some(actual) = actual_type(df, field) else {
                out.push(Diagnostic {
                    kind: DiagnosticKind::Illegal,
                    message: missing_column_message(c, m, field, &channel),
                });
                continue;
            };

            // **A violin reads its orientation off the bindings**, exactly as
            // `bar`/`box`/`interval` do (§6), so the sideways form — the category on
            // `y`, with room for long names — is a plot rather than a mistake. The
            // static rule table cannot say that, because it is keyed on (mark,
            // channel) and this is a fact about the *transform*: an `area`'s `y` is
            // its measure in every other reading, and the refusal below is right
            // there. `slot_density` has already established that the two positions
            // are one category and one number, which is the whole of what this check
            // would ask, so skipping it drops nothing.
            let violin_position = matches!(channel, Channel::X | Channel::Y)
                && slot_density(spec, layer, Some(df)).is_some();

            if !r.accepts.accepts(actual) && !violin_position {
                // The direction, where the type alone does not imply it. Keyed on the
                // mark as well as the channel since `surface` joined: which column
                // type a *position* wants is a fact about the geometry, so the fix is
                // a different mark rather than a different channel — the one shape
                // this hint could not express while it matched on the channel alone.
                let hint = match (mark, channel, actual) {
                    (_, Channel::Size, VarType::Discrete) => {
                        " Use `color`, `shape`, or `pattern` to distinguish categories."
                    }
                    (_, Channel::Shape, VarType::Continuous)
                    | (_, Channel::Pattern, VarType::Continuous) => {
                        " Use `size` or `color` to show a numeric column."
                    }
                    // A face spans the gap between two samples and so asserts every
                    // value in it; between two categories there is no value to assert.
                    // A column *stands in* its category's cell and claims nothing
                    // about the space between cells, which is why the same floor a
                    // surface refuses is exactly what the 3-D histogram is drawn on.
                    // *The direction said `bar * bin` until 2026-07-28, and that
                    // sentence is itself refused* — `bin` cuts a **continuous** axis,
                    // so a reader who followed it hit a second wall naming the same
                    // category. Over two categorical positions the transform that
                    // makes cells is `count`, which tallies into the slots the
                    // categories already are. The book's copy of this sentence had
                    // been corrected earlier the same day and the engine's had not,
                    // which is the drift a message duplicated in prose always risks.
                    (Mark::Surface, Channel::X | Channel::Y, VarType::Discrete) => {
                        " A face spans the gap between two samples, and between two \
                         categories there is nothing to span. For a mesh over categories \
                         use `bar * count + x(<a>) + y(<b>) + space()` — a column stands \
                         in its own cell and claims nothing in between."
                    }
                    // **The path family's measure axis**, and this arm exists because
                    // these four were documented as "refused with direction" while
                    // the message gave none — a claim the engine did not meet, found
                    // by auditing the messages the book actually
                    // prints. The direction is worth giving because the fix is not
                    // "supply a number": it is *which axis the category belongs on*.
                    // A path's two axes have fixed roles (spec §6) — `x` is the domain
                    // and `y` the measure — so a category on `y` is nearly always a
                    // category that wanted to be the domain, and saying so turns a
                    // dead end into the profile plot.
                    (
                        Mark::Line | Mark::Step | Mark::Area | Mark::Ribbon,
                        Channel::Y,
                        VarType::Discrete,
                    ) => {
                        " On these marks `x` is the domain and `y` the measure, and a \
                         category is not a quantity to measure: a mean of category names \
                         is not a number, and a region has no categorical baseline to \
                         close on. Put the category on `x` instead — \
                         `line * mean + x(<category>) + y(<number>)` is the profile plot, \
                         and `area * mean` fills it. Unlike `bar`/`box`/`interval`, these \
                         marks do not read their orientation off the bindings, because \
                         their two axes do not have the same role."
                    }
                    _ => "",
                };
                out.push(Diagnostic {
                    kind: DiagnosticKind::Illegal,
                    message: format!(
                        "gog: `{c}({field})` maps a {} column, but `{c}` on `{m}` needs a {} \
                         column.{hint}",
                        actual.describe(),
                        r.accepts.describe(),
                    ),
                });
                continue;
            }

            // A scale override, when there is one. The raw column is the right
            // thing to judge it against *except* on the axis a transform writes,
            // where the values that will be scaled do not exist yet.
            if let Some(def) = binding_of(spec, layer, channel) {
                check_scale(
                    &mut out, def, c, field, df, actual,
                    reads_a_scale(channel),
                    layer.transforms.is_empty() || *channel != synth_axis,
                );
                // A stated domain is a scale property, so it is judged against
                // the same two facts and on the same axis-role distinction.
                check_limits(&mut out, def, c, field, actual, reads_a_scale(channel));
                // A stated tick count is a scale property too, but on a narrower
                // set — `limits` needs a domain and this needs an axis.
                check_tick_count(&mut out, def, c, field, actual, channel);
                // Narrower still: only `play` has a duration to scale.
                check_speed(&mut out, def, c, field, channel);
                // A free scale needs an axis to free *and* panels to free it
                // across, so it is judged against the plot as well as the binding.
                check_free(&mut out, def, c, field, channel, spec);
            }

            match r.renders {
                // `z` is the one channel whose blanks are not all one thing, so it
                // does not take the generic wording. Three different sentences are
                // true of the marks that refuse it, and a message that says "not
                // drawn yet" for a **decided** refusal promises a feature the design
                // has already declined — the same defect as a book chunk claiming a
                // refusal that stopped happening, one layer down. See `z_refusal`.
                None if *channel == Channel::Z => out.push(Diagnostic {
                    kind: z_refusal_kind(mark),
                    message: z_refusal(mark, field),
                }),
                None => out.push(Diagnostic {
                    kind: DiagnosticKind::Unsupported,
                    message: format!(
                        "gog: `{c}` is valid grammar for `{m}`, but this engine does not draw \
                         it yet — `{c}({field})` would have no visual effect. \
                         Remove it, or use a channel that renders."
                    ),
                }),
                // The violin's slot again, and skipped for the same reason one check
                // up: the rule table describes the mark, this is the transform's
                // reading, and the pair has already been established well-formed.
                Some(_) if violin_position => {}
                Some(supported) if !supported.accepts(actual) => out.push(Diagnostic {
                    kind: DiagnosticKind::Unsupported,
                    message: format!(
                        "gog: `{c}({field})` is a {} column. `{c}` on `{m}` accepts {}, but this \
                         engine only renders {} so far.",
                        actual.describe(),
                        r.accepts.describe(),
                        supported.describe(),
                    ),
                }),
                Some(_) => {}
            }
        }

        // --- required bindings that are missing ---------------------------
        for channel in [Channel::X, Channel::Y, Channel::Label] {
            let r = rule_for(mark, &channel);
            if r.obligation != Obligation::Must {
                continue;
            }
            if bound.iter().any(|(c, _)| *c == channel) {
                continue;
            }
            // A synthesizing transform supplies y itself.
            if channel == synth_axis && synthesizes_measure(&layer.mark, &layer.transforms) {
                continue;
            }
            // Law 7's three relaxations of a missing `x`, stated once in
            // `x_needs_no_binding` because the warning in `render/svg.rs` reads the
            // same question and had been answering it from its own shorter list.
            // The third of them is `nest`'s, added 2026-07-27 with `text`: `bar`
            // reads `Can` on `x` in every space and so never asked, while `text`
            // reads `Must` because flat a glyph has to be put *somewhere*. In a
            // packing nothing is put anywhere — a row gets a **region**, and `x` is
            // what subdivides the panel when it is bound at all.
            if channel == Channel::X && x_needs_no_binding(spec, layer) {
                continue;
            }
            // `y` relaxes for a partition alone, and never for the packing: there
            // `y` is the **measure**, the one thing a space with no coordinates
            // cannot do without. A partition is the other way about — it supplies
            // both positions, so naming either is optional and still means
            // something when you do (`x(amount)` weighs the arcs, `y(depth,
            // limits = c(0, 4))` is the hole).
            if channel == Channel::Y && layer.transforms.contains(&Transform::Partition) {
                continue;
            }
            let c = channel_name(&channel);
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: format!(
                    "gog: `{m}` needs `{c}()` but none is set. \
                     Add `{c}(<column>)` — {} {m} cannot be drawn without it.",
                    article(m)
                ),
            });
        }

        // --- constant settings from `style()` -----------------------------
        check_style(&mut out, mark, &layer.style, &bound);
        check_border(&mut out, mark, &layer.style);
        check_caps(&mut out, mark, &layer.style);
        check_arrow(&mut out, mark, &layer.style);
        check_center(&mut out, mark, &layer.style);
        check_nudge(&mut out, mark, &layer.style);
        check_reach(&mut out, mark, &layer.style);
        // A mapped `pattern()` and a `style(pattern = )` setting are contradictory
        // — honoring one silently drops the other, exactly the conflict `check_style`
        // catches for `color`/`shape`. Refuse, and skip the value check.
        if let (Some(_), Some((_, field))) =
            (&layer.style.pattern, bound.iter().find(|(ch, _)| *ch == Channel::Pattern))
        {
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: format!(
                    "gog: `pattern({field})` maps the texture and `style(pattern = )` sets it — \
                     one layer cannot do both. Keep `pattern({field})` to vary it by category, \
                     or `style(pattern = )` to fix one texture for every {}.",
                    mark_name(mark)
                ),
            });
        } else {
            check_pattern(&mut out, mark, &layer.style);
        }
    }

    check_palette(&mut out, spec, data);
    check_facet(&mut out, spec, data);

    check_play(&mut out, spec, data);
    // Ahead of the three gates below, because it answers the wider question: they
    // ask which marks stand in a space, and this asks whether the space draws.
    check_coord(&mut out, spec);
    check_brush(&mut out, spec, data);
    check_space(&mut out, spec);
    check_polar(&mut out, spec);
    check_order(&mut out, spec, data);
    check_nest(&mut out, spec, data);
    check_theme(&mut out, spec);

    out
}

// ---------------------------------------------------------------------------
// Theme — the page, not the ink (spec §7)
//
// The bindings validate too, so that a caller is told at the line they wrote.
// This exists anyway, and for the reason §14 gives: a rule implemented in a
// binding is a rule the other three get wrong. There are four of them now, and
// a fifth would inherit this check by construction and the bindings' by copying.
// ---------------------------------------------------------------------------

fn check_theme(out: &mut Vec<Diagnostic>, spec: &PlotSpec) {
    const GRID: &[&str] = &["both", "x", "y", "none"];

    if let Some(name) = spec.theme.preset.as_deref() {
        if !crate::ir::THEME_PRESETS.contains(&name) {
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: format!(
                    "gog: `theme(\"{name}\")` is not a theme. gog has {}. A theme is a \
                     named preset you can then adjust — `theme(\"minimal\", ratio = 1)`.",
                    or_list(&crate::ir::THEME_PRESETS.iter()
                        .map(|s| format!("`{s}`")).collect::<Vec<_>>())
                ),
            });
        }
    }

    if let Some(grid) = spec.theme.grid.as_deref() {
        if !GRID.contains(&grid) {
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: format!(
                    "gog: `theme(grid = \"{grid}\")` is not a gridline setting. gog has {} \
                     — named by the *axis* whose ticks they mark, so `\"x\"` keeps the \
                     lines that run up from the x axis and drops the rest.",
                    or_list(&GRID.iter().map(|s| format!("`{s}`")).collect::<Vec<_>>())
                ),
            });
        }
    }

    if let Some(frame) = spec.theme.frame.as_deref() {
        const FRAME: &[&str] = &["full", "axes", "none"];
        if !FRAME.contains(&frame) {
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: format!(
                    "gog: `theme(frame = \"{frame}\")` is not a way to bound a panel. gog has \
                     {} — `\"full\"` closes the axis lines into a rectangle, which is the \
                     look a journal usually asks for and what `theme(\"bw\")` sets.",
                    or_list(&FRAME.iter().map(|s| format!("`{s}`")).collect::<Vec<_>>())
                ),
            });
        }
    }

    // The strip band's fill, on `background`'s validator and for its reason: one
    // color vocabulary across the four bindings, `"transparent"` included free.
    if let Some(strip) = spec.theme.strip.as_deref() {
        if !crate::color::is_valid_color(strip) {
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: format!(
                    "gog: `theme(strip = \"{strip}\")` is not a color. gog takes CSS color \
                     names, hex, and the functional forms. The strip is the band above a \
                     panel naming the level it holds — `theme(\"bw\")` sets it white, which \
                     is what a printed figure usually wants."
                ),
            });
        }
    }

    if let Some(ink) = spec.theme.strip_text.as_deref() {
        if !crate::color::is_valid_color(ink) {
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: format!(
                    "gog: `theme(strip_text = \"{ink}\")` is not a color. gog takes CSS \
                     color names, hex, and the functional forms. It is the ink of the \
                     strip's label; leave it out and gog picks the one that reads on the \
                     band, so `theme(strip = \"black\")` already gives white type."
                ),
            });
        }
    }

    if let Some(background) = spec.theme.background.as_deref() {
        // The same validator `style()` and `palette()` use, which is what keeps
        // one color vocabulary across the four bindings (spec §7).
        if !crate::color::is_valid_color(background) {
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: format!(
                    "gog: `theme(background = \"{background}\")` is not a color. gog takes \
                     CSS color names, hex, and the functional forms — and `\"transparent\"`, \
                     which is what a figure destined for a journal's own page usually wants."
                ),
            });
        }
    }

    if let Some(ratio) = spec.theme.ratio {
        if !(ratio.is_finite() && ratio > 0.0) {
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: format!(
                    "gog: `theme(ratio = {ratio})` is not a shape a panel can have — it is \
                     the panel's width divided by its height, so it must be a positive \
                     number. `ratio = 1` is a square."
                ),
            });
        }
    }

    if let Some(angle) = spec.theme.tick_angle {
        if !angle.is_finite() || angle.abs() > 90.0 {
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: format!(
                    "gog: `theme(tick_angle = {angle})` is not an angle a tick label can \
                     be read at — it turns the x labels between -90 and 90 degrees. \
                     `tick_angle = 45` is the usual answer to names that overlap."
                ),
            });
        }
    }

    // A type scale is stated in **pixels**, like every other size in the grammar,
    // and the floor is here to catch the one mistake that unit invites: reading
    // `font_size` as a multiplier and writing `1.5`. Nothing above the floor is
    // refused — a 40px axis label is ugly, not malformed, and Law 8 says never to
    // forbid the ugly-but-legal.
    if let Some(size) = spec.theme.font_size {
        if !(size.is_finite() && size >= MIN_FONT_SIZE) {
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: format!(
                    "gog: `theme(font_size = {size})` is not a type size — it is how many \
                     pixels a tick label is, not a multiplier, so it must be at least \
                     {MIN_FONT_SIZE}. The default is {}, and the axis names and the title \
                     are derived from it, so one number sets the whole plot's text.",
                    crate::render::svg::FONT_BASE
                ),
            });
        }
    }

    // A size has to be a size. Both are checked by one closure because they are
    // one property asked twice, and a rule stated twice is a rule that drifts.
    for (name, value) in [("width", spec.theme.width), ("height", spec.theme.height)] {
        if let Some(v) = value {
            if !(v.is_finite() && v >= MIN_PLOT_SIZE) {
                out.push(Diagnostic {
                    kind: DiagnosticKind::Illegal,
                    message: format!(
                        "gog: `theme({name} = {v})` is not a size — it is how many pixels \
                         the plot asks for, so it must be at least {MIN_PLOT_SIZE}. On its \
                         own it sizes the image; composed onto a page it sizes the cell, \
                         which is how a marginal plot says it is thin."
                    ),
                });
            }
        }
    }
}

/// The smallest plot worth drawing, in pixels. Below this there is no room for a
/// panel once the margins have taken theirs, so the answer is a refusal with the
/// number in it rather than an empty rectangle.
const MIN_PLOT_SIZE: f64 = 40.0;

/// The smallest type size that is still type, in pixels. Its job is less to rule
/// out tiny text than to catch `theme(font_size = 1.5)` — the reading in which
/// the number is a multiplier rather than a measurement.
const MIN_FONT_SIZE: f64 = 4.0;

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

fn check_coord(out: &mut Vec<Diagnostic>, spec: &PlotSpec) {
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
            "gog: `{s}()` names a coordinate space the engine cannot draw in — no mark \
             stands there today. {direction}"
        ),
    });
}

// ---------------------------------------------------------------------------
// Brush — the reader's bound on a column, and which layers can answer it
//
// A selection is a **predicate over rows**, so the question "may this mark be
// brushed?" is really "is one row one *element* here, or is a row a vertex of
// something larger?" A polyline's rows are vertices: brushing half of one would
// have to split it, and splitting a polyline is what `group` already means.
//
// That question is answered in this file already, under another name. Five marks
// say `Cannot` to `group` — `point`, `bar`, `text`, `rule`, `zone` — and the
// comments beside those cells give the reason in words: *"points are not
// connected"*, *"a per-row glyph, like a point, connects nothing"*, *"each row is
// its own rectangle"*. So brushability **derives** rather than earning a second
// table, and a mark added later gets a verdict without anyone remembering to
// come back here (Law 1's completeness enforcement, for free).
//
// The `accepts`/`renders` split then does the rest of the work, exactly as it
// does for a channel: the grammar says a `bar` may be brushed, and the engine
// says not yet, because a bar's thickness is derived from the smallest gap in
// the frame it is handed — draw the selected rows and the unselected rows as two
// passes and the two would disagree about how wide a bar is, which is a lie
// rather than a wobble, since a bar's width is what says whether the bins are
// adjacent. Spec §15.
// ---------------------------------------------------------------------------

/// May a column be brushed on this mark? One row must be one element.
///
/// Derived from `group`'s rule rather than listed, because they are the same
/// question asked from two directions: `group` splits an element that spans many
/// rows, so a mark that refuses `group` is exactly a mark whose rows are already
/// separate elements.
pub fn mark_takes_selection(mark: &Mark) -> bool {
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
/// The collision modifiers do not collapse: `dodge`, `stack` and `jitter` move
/// rows without merging them.
fn transform_collapses_rows(t: &Transform) -> bool {
    !matches!(t, Transform::Dodge | Transform::Stack | Transform::Jitter)
}

/// Can the engine *draw* this layer brushed today? The `renders` half.
///
/// Two mechanical exclusions, and both are about geometry derived from a layer's
/// neighbors rather than from the row itself. A two-pass draw hands each pass a
/// different frame, so anything measured off the frame changes between the two.
fn selection_draws(layer: &Layer) -> Option<&'static str> {
    if layer.mark == Mark::Bar {
        return Some(
            "a bar's thickness is measured from the smallest gap between its \
             neighbors, so the selected and unselected bars would be drawn at \
             different widths",
        );
    }
    if layer.transforms.contains(&Transform::Jitter) {
        return Some(
            "a jittered point's offset is seeded from its place in the table, so \
             every point would jump when the selection changed",
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
    !layer.transforms.iter().any(transform_collapses_rows) && selection_draws(layer).is_none()
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
    if active.is_empty() {
        return None;
    }
    let mut keep = vec![true; df.len()];
    let mut read_any = false;
    for b in active {
        if let Some(at) = b.at {
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
    // A layer whose table does not carry the brushed column is untouched by the
    // selection rather than emptied by it — the same rule that lets a reference
    // layer stand still while a played layer moves.
    read_any.then_some(keep)
}

/// The plot-scoped rule, which is `size`'s rule word for word: apply the binding
/// where it fits, say where it did not, and refuse only when it fits nowhere.
fn check_brush(out: &mut Vec<Diagnostic>, spec: &PlotSpec, data: &HashMap<String, DataFrame>) {
    if spec.brush.is_empty() {
        return;
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
            let placed = spec.x.is_some() || spec.y.is_some()
                || spec.layers.iter().any(|l| {
                    l.encodings.contains_key(&Channel::X) || l.encodings.contains_key(&Channel::Y)
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
                        "gog: `brush({}, at = c({lo}, {hi}))` selects nothing, because the \
                         range does not run upward. Write the smaller number first.",
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
        // brush would be accepted and would select nothing, forever.
        let known = spec.layers.iter().any(|l| {
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
    }

    // Which layers can answer a selection at all, and which the engine can draw.
    let mut answers = Vec::new();
    let mut undrawn = Vec::new();
    let mut collapsed = Vec::new();
    let mut not_elements = Vec::new();
    for layer in &spec.layers {
        let m = mark_name(&layer.mark);
        if !mark_takes_selection(&layer.mark) {
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
                "gog: a `{m}` cannot be brushed yet, because {why}. Brush a `point` or a \
                 `text` layer, or drop the brush."
            ),
        });
    } else if let Some((m, t)) = collapsed.first() {
        out.push(Diagnostic {
            kind: DiagnosticKind::Unsupported,
            message: format!(
                "gog: `{m} * {t}` cannot be brushed, because `{t}` summarizes many rows into \
                 one and the engine cannot say which of them you selected. Brush the layer \
                 that draws the rows themselves, or drop the brush."
            ),
        });
    } else if let Some(m) = not_elements.first() {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: a `{m}` draws one shape through many rows, so there is no single row to \
                 select. Use `group()` to split it, or brush a mark that draws one shape per \
                 row: `point`, `text`, `rule` or `zone`."
            ),
        });
    }
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

fn check_space(out: &mut Vec<Diagnostic>, spec: &PlotSpec) {
    // Whether this plot has a third dimension at all — asked of `space_of`, the one
    // source the renderer's own `is_3d` reads, so the report and the picture cannot
    // disagree. They *did* for one build: this check tested `axis_def(Z)` directly,
    // which the 3-D histogram fails (it binds no `z`; `bin` synthesizes the count
    // onto it), so a correctly projected plot was told it had been drawn flat.
    let projects = space_of(spec) == SpaceKind::Space;

    // A viewing angle without a third dimension: legal, but it projects nothing.
    if matches!(spec.coord, CoordSpace::Space(_)) && !projects {
        out.push(Diagnostic {
            kind: DiagnosticKind::Assumption,
            message: "gog: `space(...)` sets a 3-D viewing angle, but nothing is bound to `z`, \
                      so there is no third dimension to project — drawn flat. Add `z(<column>)`, \
                      or a transform that invents one: `bar * bin + x(<a>) + y(<b>) + space()` \
                      cuts the floor into cells and stands the count up on `z`."
                .to_string(),
        });
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
    if projects {
        for layer in &spec.layers {
            if !cuts_both_positions(&layer.mark, SpaceKind::Space) {
                continue;
            }
            if !layer.transforms.contains(&Transform::Smooth) {
                continue;
            }
            let m = mark_name(&layer.mark);
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: format!(
                    "gog: `{m} * smooth` fits a curve of one column against another, and a \
                     curve needs a domain to run along — the cube's floor is a *pair* of \
                     positions with no left to right, which is why `line` and `area` cannot \
                     stand here either. Smooth on the plane, where there is a domain: \
                     `line * smooth + x(<a>) + y(<b>)`. To summarize a column within each \
                     pair of categories instead, the five reductions do read a floor: \
                     `{m} * mean + x(<a>) + y(<b>) + z(<column>) + space()`."
                ),
            });
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
    if zdef.scale == Some(ScaleType::Log) {
        out.push(Diagnostic {
            kind: DiagnosticKind::Unsupported,
            message: "gog: a log `z`-axis is not drawn yet — `z` is linear in 3-D for now. \
                      Drop `scale = \"log\"` on `z`, or put the log channel on `x`/`y`."
                .to_string(),
        });
    }

    // A facet is *not* checked here. A cube crossed with a facet is one cube per
    // panel, each projected from its own rectangle, and the renderer has always
    // drawn it that way — see this section's header for why the refusal that used
    // to sit here was a stale gate rather than a missing feature.
}

// ---------------------------------------------------------------------------
// The bar with no position axis — one slot, divided by its split
//
// Law 7 says a visual is a mark plus its *required* positions, and `bar` requires
// an `x`. There is exactly one shape where it does not: when a `color`/`group`
// split supplies the segmentation, the bar has one slot and the split divides it.
// Flat, that draws the share-of-total column (one bar, segmented). In `polar` it
// is the pie, because a plot with one bound position reads that position as the
// angle (Wilkinson's one-argument `polar.theta`, §9.1.6.1).
//
// This is a relaxation, not an exception: it is stated once, it is relational (it
// reads the bindings, the way `slot_orient` does), and it means the same thing in
// both coordinate spaces, so Law 2 holds. Without the split it stays refused —
// every row would pile into one place with nothing to tell them apart.
// ---------------------------------------------------------------------------

pub fn bar_divides_one_slot(layer: &Layer) -> bool {
    layer.mark == Mark::Bar
        && (layer.encodings.contains_key(&Channel::Color)
            || layer.encodings.contains_key(&Channel::Group))
}

/// Is a missing `x` well formed on this layer? **The one answer**, read by the
/// check that refuses and by the warning that advises.
///
/// Law 7 has three relaxations now, and they had been stated in two places with
/// only the first copied across — so `warn_missing_bindings` told a `text` in
/// `nest` it was "rendering empty chart" while the check beside it had already
/// blessed the plot. A warning that contradicts the check is the drift the master
/// grids are generated to avoid, and this is its fourth appearance in that one
/// function; the fix is the same as theirs, one source both readers ask.
///
/// The three, and why each is not an exception:
///
/// - **A `bar` whose split is its segmentation** has one slot and the split
///   divides it — the share-of-total column flat, the pie bent. *Conditional:*
///   take the split away and the requirement comes back.
/// - **A `partition`** places its own nodes; one that did not would not be a
///   partition. *Constitutive.*
/// - **A packing** places by region rather than by coordinate, and `x` is what
///   subdivides the panel when it is bound at all. *Constitutive*, and the only
///   one of the three that is a property of the **space** rather than of the
///   layer — which is why it is asked of `spec` and not of `layer` alone.
pub fn x_needs_no_binding(spec: &PlotSpec, layer: &Layer) -> bool {
    bar_divides_one_slot(layer)
        || layer.transforms.contains(&Transform::Partition)
        || space_of(spec) == SpaceKind::Nest
}

// ---------------------------------------------------------------------------
// The rule's axis — Law 7's second relaxation
//
// A `rule` is placed by *one* position and spans the other, so "which one?" has
// to be answered before anything else about the layer makes sense. It is
// answered the way `bar`/`box`/`interval` answer orientation (`slot_orient`) and
// for the same reason there is no `flip` atom (§6): **read it off the bindings**,
// never off a second spelling of the mark.
//
// What it reads is which of the plot's two position columns this layer's own
// table actually has. That is not a fallback for something better — it is the
// binding, seen from the layer. Positions are plot-scoped (one x scale, one y
// scale, shared by every layer, spec §8), so a rule cannot name a *different*
// column; what it can do is be handed a table that answers one axis and not the
// other, and that is exactly the sentence §18 designed: put the thresholds in
// their own table and layer a rule over them.
//
// Two shapes are refused rather than guessed, both with direction. A table with
// *neither* column has no position at all (Law 7's floor: no mark without a
// position). A table with *both* is the genuinely ambiguous one — it is the rug
// written over the plot's own data — and it is refused for the same reason
// `check_slot_shape` refuses a bar with two categorical axes: the grammar cannot
// tell which axis is meant, and picking one would teach a rule that isn't real.
// ---------------------------------------------------------------------------

/// One axis, its own column — never its own scale.
///
/// A layer naming its own position column is a *data-resolution* move: the note
/// table's value is in the same units on the same axis and differs only in what
/// it is called (spec §8). A layer naming its own **scale** would be two
/// coordinate spaces wearing one panel, which is the secondary axis spec §18
/// refuses outright — so the line between the owed thing and the refused thing
/// is drawn here, and it is exactly one field wide.
///
/// The scale belongs to the axis, so the axis's own binding may carry one; what
/// is refused is a *second, different* scale arriving from a layer.
///
/// **`limits` is held to the same line** (spec §10). A domain is a scale
/// property, so a layer stating its own is the same two-coordinate-spaces
/// failure arriving through the other parameter: the axis would be drawn over
/// one range while a layer's rows were filtered against another, and every mark
/// would read against an axis the expression never named.
///
/// **And so is `tick_count`** (2026-07-26), for a reason one step milder and the
/// same in kind: two layers asking for different tick counts is one axis asked to
/// carry two sets of ticks, and whichever layer happened to be first would win
/// silently. It is the third parameter through this door, which is what the field
/// list below is for — a fourth scale property added to `ChannelDef` and not to
/// this check would be accepted per layer and quietly ignored.
///
/// **Unstated is *inherit*, not *different* (fixed 2026-07-27).** Every property
/// here is an `Option`, and until this was fixed the check compared the two
/// options directly — so a layer that said **nothing** compared unequal to a plot
/// axis that carried a scale property, and was refused for "its own limits" it
/// had never asked for. The sentence the docstring above promises is fine —
/// `y(v)` on its own — was therefore refused outright whenever the axis had a
/// domain: `y(v, limits = c(0, 10)) + point + x(a) + y(v)`. What is refused is a
/// *second, different* scale, and a layer with no opinion is not a second
/// anything; the axis's value is what it draws against either way. So each
/// property is compared **only when the layer stated one**, which is also the
/// reading that makes the "a fourth property must be added here" note above
/// correct rather than merely strict.
fn check_layer_position(out: &mut Vec<Diagnostic>, spec: &PlotSpec, layer: &Layer) {
    /// Did the layer state this property, *and* state it differently? `None` on
    /// the layer means it inherits the axis, which is never a disagreement.
    fn overrides<T: PartialEq>(own: &Option<T>, axis: &Option<T>) -> bool {
        own.is_some() && own != axis
    }

    for ch in [Channel::X, Channel::Y, Channel::Z] {
        let Some(own) = layer.encodings.get(&ch) else { continue };
        let Some(axis) = spec.axis_def(&ch) else { continue };
        if !overrides(&own.scale, &axis.scale)
            && !overrides(&own.base, &axis.base)
            && !overrides(&own.limits, &axis.limits)
            && !overrides(&own.tick_count, &axis.tick_count)
        {
            continue;
        }
        let c = channel_name(&ch);
        let f = &own.field;
        // Which parameter it arrived through, so the direction names the one the
        // caller actually wrote rather than the commoner of the several.
        let (what, example) = if overrides(&own.limits, &axis.limits) {
            ("its own limits", format!("{c}(<column>, limits = c(0, 24))"))
        } else if overrides(&own.tick_count, &axis.tick_count) {
            ("its own tick count", format!("{c}(<column>, tick_count = 8)"))
        } else {
            ("its own scale", format!("{c}(<column>, scale = \"log\")"))
        };
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: this layer's `{c}({f}, …)` gives it {what}, and the layers of one \
                 plot share one {c} axis — two scales on it would be two coordinate spaces in one \
                 panel, where every mark reads against an axis the expression never named. Set the \
                 scale once for the plot, before the marks: `{example}`. A \
                 layer may name its own *column* for the shared axis — `{c}({f})` on its own is \
                 fine — but not its own scale. If the two series are genuinely in different units, \
                 facet on the measure instead."
            ),
        });
    }
}

/// Which axis a `rule` sits on: the one whose column this layer's table has.
/// `None` when neither or both do — the two shapes `check_rule` refuses, so a
/// caller that gets `None` should draw nothing.
///
/// **A layer that names its own position has already answered this**, which is
/// the payoff spec §18 predicted when it recorded the ambiguous case as "the
/// sharpest live cost of the parked per-layer position bindings". A thresholds
/// table carrying both of the plot's columns used to be refused for being
/// unreadable; `rule + x(cut)` now says which axis it means, and the guess is
/// never made rather than being made better.
/// Is a mark's blank `z` cell a **decision** or a **gap**?
///
/// `Illegal` where the grammar has ruled the reading out, `Unsupported` where it is
/// owed and unbuilt. The distinction matters for more than tone: `GOG_STRICT=0`
/// downgrades both to warnings, but a reader deciding whether to wait for a release
/// is asking exactly this question, and until 2026-07-26 every one of these answered
/// "not drawn yet" — including the four the spec had argued *against* since M8a.
fn z_refusal_kind(mark: &Mark) -> DiagnosticKind {
    match mark {
        // The path/region family: ruled out, not owed. See `z_refusal`.
        Mark::Line | Mark::Step | Mark::Area | Mark::Ribbon => DiagnosticKind::Illegal,
        _ => DiagnosticKind::Unsupported,
    }
}

/// What to say when a mark has no `z`, and there are three true sentences rather
/// than one (spec §15).
fn z_refusal(mark: &Mark, field: &str) -> String {
    let m = mark_name(mark);
    match mark {
        // **Decided, and argued in `rule_for`'s `Mark::Path` row since M8a.** A
        // line is a *function*: it sorts by `x` and draws one `y` for each, read
        // left to right along a domain — and a cube has no left to right. `x` is
        // one of three equal positions, and at some viewing angles it runs into the
        // page and becomes depth (`project::Scene`'s own test pins that), so the
        // sort would be along an axis the reader cannot see. The direction is
        // `path`, which is exactly `line` with that sort removed.
        Mark::Line | Mark::Step | Mark::Area | Mark::Ribbon => format!(
            "gog: `{m}` reads a *domain* left to right — it sorts by `x` and draws one value \
             for each — and a cube has no left to right: `x` is one of three equal positions, \
             and at some viewing angles it runs into the page and becomes depth. A `{m}` in \
             space would be sorted by an axis the reader cannot see, so this is refused rather \
             than drawn. For a route through three dimensions use `path`, which is `{m}` with \
             that sort removed: `path + x(<a>) + y(<b>) + z({field})`."
        ),
        // **Blocked, and on the one thing M8a deliberately does not have.** A rule
        // spans the axes it does not name; in a cube that is *two* of them, so the
        // mark is a plane. Marks are depth-sorted by footprint, and a plane's
        // footprint is the whole floor — so it can only land wholly in front of or
        // wholly behind every mark, and a reference plane's entire job is to cut
        // through them. That needs per-element occlusion, which is M8b (§16).
        Mark::Rule => format!(
            "gog: `rule` marks a value on one axis and spans the ones it does not name — in a \
             cube that is *two* axes, so a rule here is a **plane**. Marks in space are sorted \
             by their footprint, and a plane's footprint is the whole floor, so it could only \
             be drawn wholly in front of or wholly behind the data when its job is to cut \
             through it. Drawing it would mean working out, piece by piece, what hides what, \
             and the engine cannot do that yet. Draw it flat, \
             or mark the threshold on a floor axis with `bar`/`point` at `z({field})`."
        ),
        // **Blocked for the same reason, plus a vocabulary gap that has to be
        // decided before it could be drawn at all.** `bounds` names two pairs — a
        // measure pair and a domain pair — so a zone can never be bounded on a
        // *third* axis, and one it does not bound it spans whole. Every zone in a
        // cube is therefore a slab, with a plane's sorting problem.
        Mark::Zone => format!(
            "gog: `zone` shades the region its `bounds` name and spans the axes they do not — \
             and `bounds` names two pairs, so in a cube a zone always spans one axis whole and \
             is a **slab**. Like a 3-D `rule` it has no footprint to sort by, so it cannot be \
             placed among the data until the engine can tell, piece by piece, what hides what. \
             Draw it flat, or stand a solid on the floor with `bar + x(<a>) + y(<b>) + z({field})`."
        ),
        // Anything else is an ordinary unbuilt cell, and says so.
        _ => format!(
            "gog: `z` is valid grammar for `{m}`, but this engine does not draw it yet — \
             `z({field})` would have no visual effect. Remove it, or use a channel that renders."
        ),
    }
}

pub fn rule_axis(spec: &PlotSpec, df: &DataFrame, layer: &Layer) -> Option<Channel> {
    for ch in [Channel::X, Channel::Y] {
        if layer.encodings.contains_key(&ch) {
            return Some(ch);
        }
    }
    let here = |c: Option<&ChannelDef>| c.is_some_and(|d| actual_type(df, &d.field).is_some());
    match (here(spec.axis_def(&Channel::X)), here(spec.axis_def(&Channel::Y))) {
        (true, false) => Some(Channel::X),
        (false, true) => Some(Channel::Y),
        _ => None,
    }
}

fn check_rule(out: &mut Vec<Diagnostic>, spec: &PlotSpec, df: &DataFrame, layer: &Layer) {
    if layer.mark != Mark::Rule || rule_axis(spec, df, layer).is_some() {
        return;
    }
    let named = |c: Option<&ChannelDef>| c.map(|d| d.field.clone());
    let message = match (named(spec.axis_def(&Channel::X)), named(spec.axis_def(&Channel::Y))) {
        // Both axes resolve here: the ambiguous case. Say which columns collided
        // and how to break the tie, because the fix is a smaller table rather
        // than a different atom.
        (Some(xf), Some(yf)) if actual_type(df, &xf).is_some() && actual_type(df, &yf).is_some() => {
            format!(
                "gog: `rule` marks a value on one axis and spans the other, but this table \
                 has a column for both — `x({xf})` and `y({yf})` — so there is nothing to \
                 say which axis is meant. Say it on the layer: `rule + x({xf})` places every \
                 line by `{xf}`, `rule + y({yf})` by `{yf}`. Giving the rule its own table \
                 holding just the one column works too."
            )
        }
        // Neither resolves: Law 7's floor. Name the axes so the reader can see
        // which two columns were looked for and did not turn up.
        _ => {
            let looked = match (named(spec.axis_def(&Channel::X)), named(spec.axis_def(&Channel::Y))) {
                (Some(xf), Some(yf)) => format!(" — this table has neither `{xf}` nor `{yf}`"),
                (Some(f), None) | (None, Some(f)) => format!(" — this table has no `{f}`"),
                (None, None) => String::new(),
            };
            format!(
                "gog: `rule` needs a position and has none{looked}. A rule is placed by one \
                 column and spans the other axis, so its table must hold the column the plot \
                 is placed by: `x(<column>)` stands it up, `y(<column>)` lays it down."
            )
        }
    };
    out.push(Diagnostic { kind: DiagnosticKind::Illegal, message });
}

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
fn check_tiling(
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
fn check_field(out: &mut Vec<Diagnostic>, spec: &PlotSpec, layer: &Layer) {
    if !measures_cells(&layer.mark, &layer.transforms) {
        return;
    }
    // A *reduction* composed alongside is the doubly-measured cell, and
    // `check_pair_summary` says so in both transforms' names. Deferred rather than
    // said twice: this message would tell the reader their `color` binding has
    // nothing to read, when in fact it is the binding their `mean` needs and the
    // `count` beside it is the part to drop. One mistake, one refusal — the same
    // deference `check_zone_extent` already makes to the same check.
    if crate::transform::reduces_column(&layer.transforms).is_some() {
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
            let instead = if layer.mark == Mark::Path {
                format!(" To draw one set of contours per category, `group({})` splits the \
                         estimate and leaves color to the level.", def.field)
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
fn check_density_params(out: &mut Vec<Diagnostic>, plot: &PlotSpec, df: Option<&DataFrame>,
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

/// `style(reach = )` — how far a rule crosses the axis it does not name. The
/// mark half is `mark_takes_setting`, so this and the generated grid cannot
/// disagree; the value half is `REACHES`, the list the renderer reads too.
fn check_reach(out: &mut Vec<Diagnostic>, mark: &Mark, style: &StyleSpec) {
    let Some(reach) = style.reach.as_deref() else { return };
    let m = mark_name(mark);

    if !mark_takes_setting(mark, Setting::Reach) {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `style(reach = )` says how far a `rule` crosses the axis it does not \
                 name, and {} {m} has no such axis — both its extents come from the data. \
                 Remove it.",
                article(m)
            ),
        });
        return;
    }

    if !REACHES.contains(&reach) {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `style(reach = \"{reach}\")` is not a reach. Use \"panel\" (the default \
                 — a line all the way across, a reference line) or \"edge\" (a short tick at \
                 the start of that axis, a rug)."
            ),
        });
    }
}

/// The statistics that still need a position axis even when the bar does not.
/// `count` and the aggregations answer "one value for these rows", which is
/// meaningful with nothing to group by; `bin`, `density` and `smooth` describe how
/// values are *distributed along* an axis, so with no axis there is nothing for
/// them to say. Refused with direction rather than quietly returning the input.
fn check_keyless_statistic(out: &mut Vec<Diagnostic>, spec: &PlotSpec, layer: &Layer) {
    if spec.position_for(layer, &Channel::X).is_some() || !bar_divides_one_slot(layer) {
        return;
    }
    for t in &layer.transforms {
        let needs_axis = matches!(t, Transform::Bin | Transform::Density | Transform::Smooth);
        if !needs_axis {
            continue;
        }
        let n = transform_name(t);
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `{n}` describes how values are spread along an axis, and this `bar` has \
                 no `x()` to spread them along. Add `x(<column>)` to draw the distribution, \
                 or use `count`/`sum` for one value per group."
            ),
        });
    }
    // A pair transform has the same problem one level up: it produces a low and a
    // high *per position*, and there is no position here.
    if layer.transforms.iter().any(is_pair_transform) {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: "gog: a range transform gives a low and a high at each position, and this \
                      `bar` has no `x()` to place them at. Add `x(<column>)`, or drop the range."
                .to_string(),
        });
    }
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

fn check_polar(out: &mut Vec<Diagnostic>, spec: &PlotSpec) {
    if !matches!(spec.coord, CoordSpace::Polar(_)) {
        return;
    }

    // A circle and a cube are two different spaces, and a plot sits in one. The
    // cylindrical coordinate that would hold both (Wilkinson §9.3.3) is not built,
    // so this is refused with direction rather than one of the two silently winning.
    if spec.axis_def(&Channel::Z).is_some() {
        out.push(Diagnostic {
            kind: DiagnosticKind::Unsupported,
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
                "gog: `{m}` is not drawn in polar coordinates yet. Drop `polar()` to draw it \
                 flat, or use {}.",
                or_list(&drawn)
            ),
        });
    }
}

// ---------------------------------------------------------------------------
// Order — and the axis it needs in order to mean anything
//
// `order()` sorts the plot's **categorical position axis** (spec §9), so a plot
// that has no such axis has nothing for it to sort, and until 2026-07-26 it was
// accepted and dropped in silence there — the §12 sin, sitting in plain sight
// because every sentence in the book that used it happened to have a categorical
// `x`. The packed space is what surfaced it: the one-level treemap binds no
// position at all, so `order()` on it looked like it was doing the sorting the
// packing deliberately does not do for you.
//
// Refused in every space rather than only in `nest`, because the rule is not the
// packing's — it is `order()`'s, and it was always true.
// ---------------------------------------------------------------------------

fn check_order(out: &mut Vec<Diagnostic>, spec: &PlotSpec, data: &HashMap<String, DataFrame>) {
    let Some(order) = &spec.order else { return };
    let Some(df) = spec.data.as_ref().and_then(|n| data.get(n)) else { return };

    let categorical = [Channel::X, Channel::Y].iter().any(|ch| {
        spec.axis_def(ch)
            .and_then(|enc| actual_type(df, &enc.field))
            .is_some_and(|t| t == VarType::Discrete)
    });
    if categorical {
        return;
    }
    out.push(Diagnostic {
        kind: DiagnosticKind::Illegal,
        message: format!(
            "gog: `order({})` sorts a **categorical position axis**, and this plot has none — \
             so there is nothing for it to put in order. Bind a category to `x` or `y`, or, if \
             what you meant was the order of a color split, set the column's factor levels \
             where the data lives.",
            order.field
        ),
    });
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

fn check_nest(out: &mut Vec<Diagnostic>, spec: &PlotSpec, data: &HashMap<String, DataFrame>) {
    if !matches!(spec.coord, CoordSpace::Nest) {
        return;
    }

    // A packing and a cube are two spaces, and a plot sits in one — `check_polar`'s
    // ruling, restated because the reason is the same one and not a shared
    // implementation. A nested space *could* one day pack boxes into a box; that is
    // a different space and would be asked for differently.
    if spec.axis_def(&Channel::Z).is_some() {
        out.push(Diagnostic {
            kind: DiagnosticKind::Unsupported,
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

        // **A nudge has nothing to move away from here.** The setting exists so a
        // label can step off the point it would otherwise cover (spec §7), and a
        // packed label sits in a region with no dot under it — so honoring it would
        // push the name toward its own border for no reason, and ignoring it would
        // be the accept-and-drop §12 forbids. Refused, with the two things a reader
        // might have wanted instead.
        if layer.style.nudge.is_some() && matches!(layer.mark, Mark::Text) {
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: "gog: `style(nudge = )` moves a label off the point it would cover, \
                          and a `nest()` label covers no point — it sits at the center of its \
                          own region, which is the only place that says which region it names. \
                          Drop it. To fit more names in, make the plot larger \
                          (`theme(width =, height =)`) or the text smaller \
                          (`style(size = )`)."
                    .to_string(),
            });
        }

        let mark = &layer.mark;
        if !is_drawable(mark) || mark_draws_in_space(mark, SpaceKind::Nest) {
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
        // a ruling, not a queue.
        let (kind, why) = (
            DiagnosticKind::Unsupported,
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
    if !has_measure && !spec.layers.is_empty() {
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

fn check_facet(out: &mut Vec<Diagnostic>, spec: &PlotSpec, data: &HashMap<String, DataFrame>) {
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
                Some(VarType::Continuous) => {
                    out.push(Diagnostic {
                        kind: DiagnosticKind::Illegal,
                        message: format!(
                            "gog: `facet({field})` splits on a number column, but a facet \
                             variable names the panels, so it must be a category column. \
                             Make `{field}` text — in R, `factor({field})` — or cut it \
                             into named groups first."
                        ),
                    });
                    return;
                }
                Some(VarType::Discrete | VarType::Either) => found = true,
                None => absent_from.push(name.map(String::as_str).unwrap_or("?")),
            }
        }

        if !found {
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: format!(
                    "gog: `{op} facet({field})` refers to a column that is not in the data. \
                     Check the spelling of `{field}`."
                ),
            });
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

/// Above this many frames, say how long the loop will run before drawing it.
///
/// Under it the default is unambiguous and §12 says use it silently: a dozen
/// frames at the default pace is ten seconds, which is what anyone writing
/// `play(year)` over a decade already expects. Above it the number stops being
/// obvious, and the caller is choosing a two-minute loop without being told.
const FRAMES_WORTH_MENTIONING: usize = 30;

fn check_play(out: &mut Vec<Diagnostic>, spec: &PlotSpec, data: &HashMap<String, DataFrame>) {
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

    let n = crate::data::frames_across(&source, field).len();

    // Not a refusal. A hundred frames is a legal plot and Law 8 does not forbid
    // the ugly-but-legal — but the loop length is a default the caller did not
    // choose and cannot see, which is exactly §12's Assumption: it renders, and
    // the chosen default is said out loud so it can be confirmed.
    if n > FRAMES_WORTH_MENTIONING {
        let secs = n as f64 * def.frame_seconds();
        out.push(Diagnostic {
            kind: DiagnosticKind::Assumption,
            message: format!(
                "gog: `play({field})` cuts {n} frames, so the animation loops every \
                 {secs:.0} seconds. Run it faster with `play({field}, speed = 4)`, or \
                 bind a coarser column."
            ),
        });
    }
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
fn check_domain_split(out: &mut Vec<Diagnostic>, spec: &PlotSpec, layer: &Layer) {
    if !matches!(layer.mark, Mark::Line | Mark::Step | Mark::Area | Mark::Ribbon) {
        return;
    }
    // A violin reads along its slot, not along the plot's domain.
    if slot_density(spec, layer, None).is_some() {
        return;
    }
    let Some(x) = spec.position_for(layer, &Channel::X).map(|d| d.field.as_str()) else {
        return;
    };

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
             {noun} and supplies `x` — so every {one} holds a single `{split}`, and a \
             {m} needs at least two positions to read between. {direction}",
            m = mark_name(&layer.mark),
            one = noun.trim_end_matches('s'),
        ),
    });
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
fn check_free(
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
    if !matches!(channel, Channel::X | Channel::Y | Channel::Z) {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `{c}({field}, free = TRUE)` — a free scale is fitted per panel, \
                 and `{c}` is read from one key for the whole plot rather than from an \
                 axis in each panel. Free a position instead: `y(<column>, free = TRUE)`."
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
                "gog: `{c}({field}, limits = …, free = TRUE)` states the domain and then \
                 asks each panel to choose its own. Keep `limits` for one scale across \
                 every panel, or `free = TRUE` for one scale per panel."
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
                "gog: `{c}({field}, free = TRUE)` frees an axis, and a `nest()` packing \
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
                "gog: `{c}({field}, free = TRUE)` fits one scale per *panel*, and this \
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
            "gog: `{c}({field}, free = TRUE)` fits one scale per panel, and this plot \
             has one panel. Facet it — `plot | facet(<column>)` — or drop `free`."
        ),
    });
}

fn check_speed(
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
                "gog: `{c}({field}, speed = {s})` — `{c}` is drawn all at once, so it has \
                 no pace to set. Only `play` spends time. Put `speed` on the frames — \
                 `play(…, speed = {s})` — or leave it off."
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

// ---------------------------------------------------------------------------
// Scales
//
// A scale is a property of a binding, not an atom, so it is checked here beside
// the binding rather than in a table of its own. What can go wrong is different
// in kind from what goes wrong with a channel: a mapping is judged against the
// column's *type*, a scale against its *values*. `log` is the first thing in
// gog that a perfectly well-typed column can still fail.
// ---------------------------------------------------------------------------

/// Does this channel run along a scale at all?
///
/// Every channel that can carry a *continuous* column does, and for the same
/// reason: a scale answers "how far along?", which is a question only a
/// magnitude has. `shape` and `group` take a categorical column, where the
/// answer is "which one?" — there is no distance between circle and square for
/// a logarithm to compress.
fn reads_a_scale(channel: &Channel) -> bool {
    matches!(
        channel,
        Channel::X | Channel::Y | Channel::Z | Channel::Color | Channel::Size | Channel::Opacity
    )
}

/// Which axis a synthesizing transform writes to.
///
/// Orientation decides it: `bar * count + x(cat)` invents y, `bar * count +
/// y(cat)` invents x. True of every slot mark, not just `bar` — `interval *
/// bounds(lo, hi) + y(cat)` is the horizontal error bar whose extents are the
/// two named columns, so the axis it invents is x.
///
/// **In the cube the answer is `z`, and orientation never comes up.** A mark
/// that cuts both positions there has spent `x` and `y` on the two edges of its
/// footprint, so the axis left to write a tally to is the third one — which is
/// why there is no 3-D counterpart of the horizontal bar. The flat reading is
/// unchanged: which of `x`/`y` measures is a property of the pair, and
/// `slot_orient` reads it off the bindings.
///
/// `pub` since scale limits arrived (spec §10): a stated domain filters rows
/// *before* the statistics on the axis they group by and judges the computed
/// values on the axis they write, which is the same distinction `check_scale`
/// draws with `values_are_the_input`. Two copies of this `match` would be two
/// answers to one question, and this file records what that costs.
pub fn synth_axis(spec: &PlotSpec, layer: &Layer, df: Option<&DataFrame>) -> Channel {
    match (&layer.mark, df) {
        (m, _) if cuts_both_positions(m, space_of(spec)) && !has_no_measure_axis(m) => Channel::Z,
        (m, Some(df)) if is_slot_mark(m) => {
            let xt = spec.position_for(layer, &Channel::X).and_then(|c| actual_type(df, &c.field));
            let yt = spec.position_for(layer, &Channel::Y).and_then(|c| actual_type(df, &c.field));
            match slot_orient(xt, yt) {
                Orient::Horizontal => Channel::X,
                Orient::Vertical => Channel::Y,
            }
        }
        _ => Channel::Y,
    }
}

/// What a layer's stated domains keep, and what they cut (spec §10).
///
/// One authority for the question, because the renderer filters on it and
/// `check_limits` reports on it: a count that disagreed with the rows actually
/// dropped would be the silent-drop failure wearing a diagnostic.
pub struct LimitCut {
    /// One entry per row of the frame this was computed against.
    pub keep: Vec<bool>,
    /// `(channel, column, excluded)` for every stated domain that cut something.
    pub cuts: Vec<(Channel, String, usize)>,
    pub total: usize,
}

impl LimitCut {
    /// Nothing stated, or nothing outside it — the overwhelmingly common case.
    pub fn is_empty(&self) -> bool {
        self.cuts.is_empty()
    }
    /// Every row cut: an empty panel with fabricated axes unless someone refuses.
    pub fn takes_everything(&self) -> bool {
        !self.cuts.is_empty() && !self.keep.iter().any(|&k| k)
    }
}

/// Apply this layer's stated domains to a frame.
///
/// Only the bindings whose **values are the input** take part: on the axis a
/// transform writes there is no column yet to filter, and the limit judges the
/// computed values instead (§10's ordering rule, which a limit inherits from
/// being a scale property rather than restating).
pub fn limit_cut(spec: &PlotSpec, layer: &Layer, df: &DataFrame) -> LimitCut {
    let total = df.len();
    let mut keep = vec![true; total];
    let mut cuts = Vec::new();
    let synth = synth_axis(spec, layer, Some(df));

    for channel in ALL_CHANNELS {
        if !reads_a_scale(&channel) {
            continue;
        }
        // The axis a statistic writes: its values do not exist yet.
        if !layer.transforms.is_empty() && channel == synth {
            continue;
        }
        let Some(def) = binding_of(spec, layer, &channel) else { continue };
        if !crate::scale::has_limits(Some(def)) {
            continue;
        }
        let Some(col) = df.float_col(&def.field) else { continue };
        // Counted per binding rather than per row, so two domains cutting the
        // same row each say so — the reader needs to know which limit did it,
        // and the totals are reported separately from the rows kept.
        let mut cut = 0usize;
        for (i, &v) in col.iter().enumerate().take(keep.len()) {
            if !crate::scale::within_limits(Some(def), v) {
                keep[i] = false;
                cut += 1;
            }
        }
        if cut > 0 {
            cuts.push((channel, def.field.clone(), cut));
        }
    }
    LimitCut { keep, cuts, total }
}

/// The binding for a channel, wherever it lives — on the layer or the plot.
///
/// Positions resolve through `position_for` (the layer's own column, else the
/// axis's), so the two kinds of channel answer the same question the same way.
fn binding_of<'a>(spec: &'a PlotSpec, layer: &'a Layer, channel: &Channel) -> Option<&'a ChannelDef> {
    match channel {
        Channel::X | Channel::Y | Channel::Z => spec.position_for(layer, channel),
        other => layer.encodings.get(other),
    }
}

/// Check a scale override against the column it is applied to.
///
/// `values_are_the_input` is false on the axis a transform *writes*. The raw
/// column is not what gets scaled there, so its values say nothing about whether
/// the result can be placed: `bar * sum + y(profit, scale = "log")` is well
/// formed over negative profits that sum to a positive total, and refusing it on
/// the raw column would be a false alarm. What actually failed to place is
/// reported by the renderer, which is the only stage that can see it.
#[allow(clippy::too_many_arguments)]
fn check_scale(
    out: &mut Vec<Diagnostic>,
    def: &ChannelDef,
    c: &str,
    field: &str,
    df: &DataFrame,
    actual: VarType,
    scaled_channel: bool,
    values_are_the_input: bool,
) {
    // Nothing said about scaling — the overwhelmingly common case.
    if def.scale.is_none() && def.base.is_none() {
        return;
    }

    // A categorical channel has no "how far along?" for a scale to answer.
    if !scaled_channel {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `{c}({field}, scale = …)` — `{c}` distinguishes categories rather than \
                 measuring them, so there is no scale for it to run along. Remove the scale."
            ),
        });
        return;
    }

    // A base belongs to a logarithm and to nothing else. There is no base of a
    // linear scale, so `base = 2` without one is a request the engine cannot
    // honor in any way — silently ignoring it is the outcome forbidden here.
    if def.base.is_some() && def.scale != Some(ScaleType::Log) {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `{c}({field}, base = …)` has no scale to be the base of. A base belongs to \
                 a logarithm — add `scale = \"log\"`, or remove the base."
            ),
        });
        return;
    }

    match def.scale.as_ref().unwrap_or(&ScaleType::Linear) {
        // The default. Saying it out loud is allowed and means nothing extra —
        // except on a date column, where "linear" would mean *un*-dating the
        // axis into raw epoch seconds. Honoring that silently would draw an
        // axis labeled 1.7B; ignoring it silently is forbidden outright.
        ScaleType::Linear => {
            if df.time_unit(field).is_some() {
                out.push(Diagnostic {
                    kind: DiagnosticKind::Illegal,
                    message: format!(
                        "gog: `{c}({field}, scale = \"linear\")` — `{field}` is a date column, \
                         and a date always reads as a calendar. If you truly want raw epoch \
                         numbers, convert the column with `as.numeric()`."
                    ),
                });
            }
        }

        // The time scale comes from the column's type, like `category` does: a
        // date column is already temporal, so saying so is allowed and means
        // nothing extra. On anything else it is unanswerable — the engine
        // cannot know whether 20656 is a day, a second, or a year.
        ScaleType::Time => {
            if df.time_unit(field).is_none() {
                out.push(Diagnostic {
                    kind: DiagnosticKind::Illegal,
                    message: format!(
                        "gog: `{c}({field}, scale = \"time\")` — `{field}` is not a date column, \
                         and a number alone does not say what moment it is. Convert it with \
                         `as.Date()` (or `as.POSIXct()`); gog reads the calendar from the \
                         column's type."
                    ),
                });
            }
        }

        // The one scale whose refusal cannot be written once, because what
        // *removing it* leaves behind depends on the column underneath. On text
        // it leaves the categorical axis you already had, so "remove it" is the
        // whole answer. On a number it leaves the **continuous** axis the caller
        // was trying to escape, so the same sentence sends them in a circle —
        // which is what it did until 2026-07-28, having been written for the text
        // column and then handed to everyone. §12 asks a diagnostic to say what to
        // do, and a direction that does not lead anywhere fails that test as
        // squarely as no direction at all.
        //
        // **On a text column it is now allowed and means nothing extra** (ruled
        // 2026-07-28), which is the allowance `linear` has on a number and `time`
        // has on a date. It was a refusal until then, and that was a Law-2
        // exception hiding in plain sight: three scales chosen from the column's
        // type, two of them sayable out loud, one refused. The comment on the
        // `Time` arm above even names the parallel — *"the time scale comes from
        // the column's type, like `category` does"* — and the code then broke it
        // two arms later.
        //
        // **On anything else it is Illegal rather than Unsupported**, and that is
        // a ruling and not an inability (spec §18). A scale refines *how* a
        // measured column is placed; **which kind of axis you get is the column's
        // type**, and every other cell of this match already enforces exactly
        // that: `log` on text, `linear` on a date, `time` on a number. Category on
        // a number is the fourth cell of that table and was the only one filed as
        // "not built yet". Two further reasons, both in §18: a category position
        // is a *rank over the whole column*, which is transform-shaped rather than
        // scale-shaped (§10 opens by drawing that very line); and nothing is lost,
        // since one slot per distinct value is a text column and cutting a number
        // into ranges is `bin`.
        //
        // **Each direction names one edit that renders**, which is the lesson of
        // the 2026-07-28 surface refusal. Converting the column is only half the
        // edit — a reader who does exactly that and keeps `scale = "category"`
        // used to land in the text branch and be refused a second time, for a new
        // reason, having done what they were told. That trap is gone now that text
        // is allowed, but the directions still say it, because they should read
        // the same whichever way that ruling later goes.
        ScaleType::Category => {
            // The axis a transform writes does not carry this column's values, so
            // the column's type says nothing about what lands there — and advice
            // about converting it would be advice about the wrong numbers.
            if !values_are_the_input {
                out.push(Diagnostic {
                    kind: DiagnosticKind::Illegal,
                    message: format!(
                        "gog: `{c}({field}, scale = \"category\")` — `{c}` carries what the \
                         transform computed, not `{field}`, and a computed number has no \
                         distinct values to give slots to. Remove the scale."
                    ),
                });
            } else if df.time_unit(field).is_some() {
                // A date carries a distinct moment per row, so one slot per
                // distinct value is one slot per row. The column has to be
                // coarsened to the period actually meant before it is a category
                // at all — the sentence `facet` gives a date column.
                out.push(Diagnostic {
                    kind: DiagnosticKind::Illegal,
                    message: format!(
                        "gog: `{c}({field}, scale = \"category\")` — `{field}` is a date column, \
                         and one slot per distinct moment is one slot per row. Format it as the \
                         period you mean, the year or the month, and drop the scale: a text \
                         column gets a categorical axis from its type."
                    ),
                });
            } else if actual == VarType::Continuous {
                out.push(Diagnostic {
                    kind: DiagnosticKind::Illegal,
                    message: format!(
                        "gog: `{c}({field}, scale = \"category\")` — a scale says how a measured \
                         column is placed; whether an axis measures at all is the column's type. \
                         Removing the scale alone would leave the continuous axis you were trying \
                         to escape. Make `{field}` text — in R, `factor({field})` — and drop the \
                         scale. To cut the numbers into ranges instead, use `bin`."
                    ),
                });
            }
        }

        ScaleType::Log => {
            // log base 1 is a division by zero and a base at or below zero has
            // no logarithm at all. `scale::log_base` falls back to 10 so the
            // renderer cannot emit infinities, but falling back silently would
            // draw a picture nobody asked for.
            if let Some(b) = def.base {
                if !b.is_finite() || b <= 1.0 {
                    out.push(Diagnostic {
                        kind: DiagnosticKind::Illegal,
                        message: format!(
                            "gog: `{c}({field}, base = {b})` is not a base a logarithm can have — \
                             it must be greater than 1. Use 10 (the default), 2 for doublings, \
                             or `exp(1)` for e-foldings."
                        ),
                    });
                    return;
                }
            }
            if actual == VarType::Discrete {
                out.push(Diagnostic {
                    kind: DiagnosticKind::Illegal,
                    message: format!(
                        "gog: `{c}({field}, scale = \"log\")` needs a number to take the \
                         logarithm of, but `{field}` is text. A text column already gets a \
                         categorical axis — remove `scale = \"log\"`."
                    ),
                });
                return;
            }
            // A moment in time is a point on an interval scale: its zero —
            // the epoch — is an arbitrary convention, and a logarithm measured
            // from an arbitrary zero measures nothing.
            if df.time_unit(field).is_some() {
                out.push(Diagnostic {
                    kind: DiagnosticKind::Illegal,
                    message: format!(
                        "gog: `{c}({field}, scale = \"log\")` — `{field}` is a date column, and \
                         a moment in time has no logarithm: the calendar's zero is an arbitrary \
                         origin. Log the measured axis instead, or remove the scale."
                    ),
                });
                return;
            }
            if !values_are_the_input {
                return;
            }
            if let Some(u) = crate::scale::unplaceable(df, field) {
                out.push(Diagnostic {
                    kind: DiagnosticKind::Illegal,
                    message: format!(
                        "gog: `{c}({field}, scale = \"log\")` has no place for {} of {} rows — a \
                         logarithm is undefined at zero and below, and `{field}` reaches {}. \
                         Filter those rows before plotting, or use a linear scale.",
                        u.count, u.total, u.smallest,
                    ),
                });
            }
        }
    }
}

/// Check a stated domain against the channel and column it is stated on (§10).
///
/// The row-counting half is `check_limit_rows`, which needs the frame and the
/// whole layer; this half needs only the binding, and answers the three
/// questions that make a domain meaningless before any row is read.
/// **`tick_count` reaches a narrower set than `limits`, and the line between them
/// is the point** (spec §10). Both describe the scale, so both ride the binding —
/// but `limits` states a **domain**, which every magnitude channel has, while this
/// states how many ticks an **axis** gets, and only the three positions draw one.
///
/// A legend is not a short axis. It names three rows structurally, both ends and
/// the middle, and on a log scale that middle is the geometric mean √(min·max)
/// rather than the midpoint — so its rows are derived from the scale's shape, not
/// chosen. A denser color bar is a real and drawable thing; it is a property of
/// the *legend*, which is furniture, so it would arrive through `theme()` and not
/// here. Refused with that direction rather than accepted and ignored.
fn check_tick_count(
    out: &mut Vec<Diagnostic>,
    def: &ChannelDef,
    c: &str,
    field: &str,
    actual: VarType,
    channel: &Channel,
) {
    let Some(n) = def.tick_count else { return };

    if !matches!(channel, Channel::X | Channel::Y | Channel::Z) {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `{c}({field}, tick_count = {n})` — `{c}` is decoded by a legend rather \
                 than by an axis, and a legend names three rows: both ends and the middle. \
                 There is no count to choose. Put `tick_count` on a position — \
                 `x({field}, tick_count = {n})` — or leave it off."
            ),
        });
        return;
    }

    // A category axis has one slot per level: the levels *are* the ticks, so a
    // count would have to invent or hide one. `order()` is the atom for choosing
    // which appear in what sequence, and filtering is the atom for choosing which
    // appear at all — the same two directions `limits` gives on a category.
    if actual == VarType::Discrete {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `{c}({field}, tick_count = {n})` — `{field}` is text, and a categorical \
                 axis has one tick per category, so the count is the data's rather than \
                 yours. To change which categories appear, filter the table before plotting; \
                 to change their order, use `order({field})`."
            ),
        });
        return;
    }

    // Two is the fewest that describes an axis: one tick shows a position and no
    // direction, and zero is an axis with no scale on it. Reported rather than
    // clamped, because a `tick_count = 1` is a mistake about what a tick is and
    // silently drawing five would leave the caller believing otherwise.
    if n < 2 {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `{c}({field}, tick_count = {n})` — an axis needs at least two ticks to \
                 show a direction as well as a place. Ask for 2 or more, or leave \
                 `tick_count` off for the default of 5."
            ),
        });
    }
}

fn check_limits(
    out: &mut Vec<Diagnostic>,
    def: &ChannelDef,
    c: &str,
    field: &str,
    actual: VarType,
    scaled_channel: bool,
) {
    let Some([lo, hi]) = def.limits else { return };
    if lo.is_none() && hi.is_none() {
        // `limits = c(NA, NA)` says nothing, which is what saying nothing says.
        return;
    }

    // Same reason a scale is refused there: `shape` and `group` answer *which
    // one?*, and there is no distance between a circle and a square for a
    // domain to be an interval of.
    if !scaled_channel {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `{c}({field}, limits = …)` — `{c}` distinguishes categories rather than \
                 measuring them, so there is no range along it for limits to cut. Remove the \
                 limits."
            ),
        });
        return;
    }

    // ggplot2 gives discrete limits a second meaning — select and reorder the
    // categories — which is one word doing two jobs, told apart only by the
    // column's type. gog has an atom for that job already, so this points at it
    // rather than growing the word (spec §10, §13).
    if actual == VarType::Discrete {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `{c}({field}, limits = …)` — `{field}` is text, and a category has no \
                 range to lie inside. To choose which categories appear, filter the table \
                 before plotting; to change the order they appear in, use `order({field})`."
            ),
        });
        return;
    }

    if let (Some(l), Some(h)) = (lo, hi) {
        if !(l < h) {
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: format!(
                    "gog: `{c}({field}, limits = c({l}, {h}))` runs backwards or has no width — \
                     the first number is the low end. Write `c({}, {})`, or leave one end out \
                     with `NA` to let the data decide it.",
                    l.min(h), l.max(h),
                ),
            });
            return;
        }
    }

    // A logarithm is undefined at zero and below, so an end there is an end the
    // axis cannot reach. Refused for the same reason a base ≤ 1 is: falling back
    // silently would draw a range nobody asked for.
    if matches!(def.scale.as_ref(), Some(ScaleType::Log)) {
        for (which, end) in [("lower", lo), ("upper", hi)] {
            if let Some(v) = end {
                if v <= 0.0 {
                    out.push(Diagnostic {
                        kind: DiagnosticKind::Illegal,
                        message: format!(
                            "gog: `{c}({field}, scale = \"log\", limits = …)` puts its {which} end \
                             at {v}, and a logarithm is undefined at zero and below. Use a \
                             positive end — on a log axis the bottom is a small number, never 0."
                        ),
                    });
                    return;
                }
            }
        }
    }
}

/// What a layer's stated domains actually cut, said out loud (§10, §12).
///
/// The rule is not *never drop a row* but *never drop one in silence*, and a
/// limit is the one place where dropping is the instruction rather than a
/// surprise — which is why this is an Assumption where the same condition under
/// `scale = "log"` is a refusal. Taking **every** row is the exception: that is
/// an empty panel with fabricated axes, the failure §12 has been burned by three
/// times, so it is fatal and names the range the column actually has.
fn check_limit_rows(out: &mut Vec<Diagnostic>, spec: &PlotSpec, df: &DataFrame, layer: &Layer) {
    let cut = limit_cut(spec, layer, df);
    if cut.is_empty() {
        return;
    }
    let fatal = cut.takes_everything();
    for (channel, field, excluded) in &cut.cuts {
        let c = channel_name(channel);
        let def = binding_of(spec, layer, channel);
        // The guarded read: a malformed pair cut nothing, so it is not reported
        // on here — `check_limits` has already named it as the typo it is.
        let (lo, hi) = crate::scale::domain_of(def);
        // A temporal domain is quoted back as dates. The caller wrote
        // `as.Date("2024-03-01")`; answering with 1709251200 tells them nothing
        // they can act on, which is the whole of what §12 asks a message for.
        // `fmt_moment` is the same one the legends use, and says so.
        let unit = df.time_unit(field);
        let end = |v: Option<f64>| match (v, unit) {
            (Some(v), Some(u)) => crate::time::fmt_moment(v, u),
            (Some(v), None) => format!("{v}"),
            (None, _) => "…".to_string(),
        };
        let shown = format!("[{}, {}]", end(lo), end(hi));
        if fatal {
            let span = df.float_col(field).map(|col| {
                let mn = col.iter().copied().filter(|v| v.is_finite()).fold(f64::INFINITY, f64::min);
                let mx = col.iter().copied().filter(|v| v.is_finite()).fold(f64::NEG_INFINITY, f64::max);
                format!("{} to {}", end(Some(mn)), end(Some(mx)))
            });
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: format!(
                    "gog: `{c}({field}, limits = …)` leaves no rows at all — {} does not meet \
                     {shown}, so the panel would be empty with axes drawn over nothing. Widen \
                     the limits, or remove them.",
                    span.map(|s| format!("`{field}` runs {s}, which"))
                        .unwrap_or_else(|| format!("`{field}`")),
                ),
            });
        } else {
            out.push(Diagnostic {
                kind: DiagnosticKind::Assumption,
                message: format!(
                    "gog: `{c}({field}, limits = …)` excludes {excluded} of {} rows, which fall \
                     outside {shown}. Stating a domain is what removes them — widen the limits \
                     if they should be drawn.",
                    cut.total,
                ),
            });
        }
    }
}

/// Report where a plot-scoped channel lands, when it does not land everywhere.
///
/// Plot scope reaches only the marks that have the feature, which is what makes
/// `size(population) + line + point` usable at all. But a binding that quietly
/// applies to some layers and not others is exactly the kind of silence the
/// grammar refuses, so the skip is said out loud: an Assumption when it still
/// reaches something, and Illegal when it reaches nothing.
fn check_plot_scope(out: &mut Vec<Diagnostic>, spec: &PlotSpec) {
    for (channel, def) in &spec.channels {
        let c = channel_name(channel);
        let field = &def.field;

        let (mut taken, mut skipped) = (Vec::new(), Vec::new());
        for layer in &spec.layers {
            let m = mark_name(&layer.mark);
            if accepts_binding(&layer.mark, channel) {
                taken.push(m);
            } else {
                skipped.push(m);
            }
        }
        skipped.dedup();
        if skipped.is_empty() {
            continue;
        }

        if taken.is_empty() {
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: format!(
                    "gog: `{c}({field})` is written for the whole plot, but no mark here has \
                     a {c} feature ({}). Remove it, or add a mark that does.",
                    or_list(&skipped.iter().map(|s| format!("`{s}`")).collect::<Vec<_>>())
                ),
            });
            continue;
        }

        taken.dedup();
        out.push(Diagnostic {
            kind: DiagnosticKind::Assumption,
            message: format!(
                "gog: `{c}({field})` is written for the whole plot, so it applies to {} — \
                 {} ha{} no {c} feature and {} left unchanged. Move `{c}({field})` after a \
                 mark to bind it to that mark alone.",
                or_list(&taken.iter().map(|s| format!("`{s}`")).collect::<Vec<_>>()),
                or_list(&skipped.iter().map(|s| format!("`{s}`")).collect::<Vec<_>>()),
                if skipped.len() == 1 { "s" } else { "ve" },
                if skipped.len() == 1 { "is" } else { "are" },
            ),
        });
    }
}

/// Check one layer's constant settings against the same `(mark, channel)` table
/// the mapped channels use.
fn check_style(
    out: &mut Vec<Diagnostic>,
    mark: &Mark,
    style: &StyleSpec,
    bound: &[(Channel, &str)],
) {
    let m = mark_name(mark);

    for (channel, written) in style.set_features() {
        let c = channel_name(&channel);
        let r = rule_for(mark, &channel);

        // The mark has no such visual feature — setting it is as meaningless
        // as mapping it.
        if !r.settable {
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: format!(
                    "gog: `style({c} = {written})` — {} {m} has no {c} to set. \
                     Remove it, or use a mark that has one.",
                    article(m)
                ),
            });
            continue;
        }

        // Mapping and setting the same feature are contradictory instructions,
        // and honoring one means silently dropping the other.
        if let Some((_, field)) = bound.iter().find(|(ch, _)| *ch == channel) {
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: format!(
                    "gog: `{c}({field})` maps {c} and `style({c} = {written})` sets it — \
                     one layer cannot do both. Keep `{c}({field})` to show the data, \
                     or `style({c} = {written})` to fix every {m} at one value."
                ),
            });
            continue;
        }

        // The value itself is all there is to check.
        let bad = match channel {
            Channel::Color => {
                let v = style.color.as_deref().unwrap_or("");
                (!is_valid_color(v)).then(|| {
                    format!(
                        "gog: `style(color = {written})` is not a color.{}",
                        color_advice(v)
                    )
                })
            }
            Channel::Opacity => {
                let v = style.opacity.unwrap_or(1.0);
                (!(0.0..=1.0).contains(&v)).then(|| {
                    format!(
                        "gog: `style(opacity = {v})` is outside 0–1. Opacity is set \
                         literally, not rescaled from a data range: 0 is invisible, \
                         1 is solid."
                    )
                })
            }
            Channel::Size => {
                let v = style.size.unwrap_or(1.0);
                (v <= 0.0).then(|| {
                    format!(
                        "gog: `style(size = {v})` is not a visible size. Give a positive \
                         number of pixels — {}.",
                        if matches!(mark, Mark::Line) {
                            "the stroke width, default 2"
                        } else {
                            "the point radius, default 4.5"
                        }
                    )
                })
            }
            Channel::Shape => {
                let v = style.shape.as_deref().unwrap_or("");
                (!SHAPE_NAMES.contains(&v)).then(|| {
                    format!(
                        "gog: `style(shape = {written})` is not a glyph. Use one of: {}.",
                        SHAPE_NAMES.join(", ")
                    )
                })
            }
            _ => None,
        };

        if let Some(message) = bad {
            out.push(Diagnostic { kind: DiagnosticKind::Illegal, message });
        }
    }
}

/// The bar-outline setting — `style(border_color =, border_size =)`.
///
/// A border is a **setting, never a channel** (spec §5): a 0.5–1px rim has too
/// little area to decode a scale from, so it never earns a guide. It applies to a
/// *filled* mark, and today only `bar` — whose rim is a real outline that the
/// reader uses (the series-hue step of an overlaid histogram is exactly what a
/// caller reaches for `border_color` to recolor). The other marks refuse it with
/// direction rather than draw nothing: a `line`/`step` *is* a stroke (its
/// `style(color)`/`style(size)` are its outline), an `area`'s edge is a layer
/// (`area + line`), and a `point` border is designed but not built yet.
/// `caps` — the short crossbars at an interval whisker's ends — is an
/// interval-only setting, the way `border_*` is bar-only. On any other mark it
/// names nothing; refuse with direction rather than silently ignore it.
fn check_caps(out: &mut Vec<Diagnostic>, mark: &Mark, style: &StyleSpec) {
    if style.caps.is_none() || mark_takes_setting(mark, Setting::Caps) {
        return;
    }
    let m = mark_name(mark);
    out.push(Diagnostic {
        kind: DiagnosticKind::Illegal,
        message: format!(
            "gog: `style(caps = )` is an `interval` setting — the crossbars at a whisker's \
             ends — and {} `{m}` has none. Remove it, or use `interval`.",
            article(m)
        ),
    });
}

/// `center` — whether an interval draws its center dot (a confidence interval's
/// mean) — is an interval-only setting, `caps`'s twin. On any other mark it names
/// nothing; refuse with direction rather than silently ignore it.
fn check_center(out: &mut Vec<Diagnostic>, mark: &Mark, style: &StyleSpec) {
    if style.center.is_none() || mark_takes_setting(mark, Setting::Center) {
        return;
    }
    let m = mark_name(mark);
    out.push(Diagnostic {
        kind: DiagnosticKind::Illegal,
        message: format!(
            "gog: `style(center = )` is an `interval` setting — the center dot a \
             confidence interval draws — and {} `{m}` has none. Remove it, or use `interval`.",
            article(m)
        ),
    });
}

/// `nudge` moves a `text` label off its point so a superposed `point + text`
/// does not draw the label on the dot. Text-only (a nudge on a `point` or `bar`
/// would misplace the data itself), and one of four plain directions — anything
/// else is refused with direction rather than guessed.
fn check_nudge(out: &mut Vec<Diagnostic>, mark: &Mark, style: &StyleSpec) {
    let Some(dir) = style.nudge.as_deref() else { return };
    if !mark_takes_setting(mark, Setting::Nudge) {
        let m = mark_name(mark);
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `style(nudge = )` is a `text` setting — it moves a label off its \
                 point — and {} `{m}` has no label to move. Remove it, or use `text`.",
                article(m)
            ),
        });
        return;
    }
    if !NUDGES.contains(&dir) {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `style(nudge = \"{dir}\")` is not a direction. Use \"up\", \"down\", \
                 \"left\", or \"right\" — which way the label sits from its point."
            ),
        });
    }
}

/// The legal `style(arrow = )` values — which end of a path carries a head.
/// Three, not a boolean: `"both"` is an ordinary want (a measurement arrow), and
/// a flag would have needed a second setting to say it.
pub(crate) const ARROW_ENDS: [&str; 3] = ["end", "start", "both"];

/// `style(arrow = )` — a head on a `path`'s end, and a `path`'s alone.
///
/// The refusal it gives on `line` is the mark's argument in one message, so it is
/// worth stating rather than deriving: a head marks a *direction*, and a line has
/// none to mark. `line` sorts its vertices by x, so its last point is wherever
/// the domain ends, and a head there points at the sort. That is not a limitation
/// to apologize for — it is why `path` exists as a separate mark rather than as a
/// flag on `line`, and the direction says so.
fn check_arrow(out: &mut Vec<Diagnostic>, mark: &Mark, style: &StyleSpec) {
    let Some(end) = style.arrow.as_deref() else { return };
    if !mark_takes_setting(mark, Setting::Arrow) {
        let m = mark_name(mark);
        let why = match mark {
            Mark::Line | Mark::Step => "a line sorts its vertices by `x`, so its last point is \
                                        wherever the domain ends rather than where the data \
                                        stopped — a head there would point at the sort",
            Mark::Interval => "a whisker already decorates its ends: `style(caps = )`",
            _ => "an arrowhead marks the direction a stroke travels in, which this mark has none of",
        };
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `style(arrow = )` is a `path` setting, and {} `{m}` cannot carry it — \
                 {why}. Use `path`, which strokes the rows in the data's own order and so \
                 has an end the data chose.",
                article(m)
            ),
        });
        return;
    }
    if !ARROW_ENDS.contains(&end) {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `style(arrow = \"{end}\")` is not an end. Use \"end\" (a head at the \
                 last row), \"start\" (at the first), or \"both\"."
            ),
        });
    }
}

/// `style(pattern = )` — the **texture** of a mark's paint (spec §4, the settable
/// rule; Wilkinson's texture aesthetic). It is realized per geometry: on the **path
/// strokes** (`line`, `step`, `interval`) it is the dash pattern —
/// `"solid"`/`"dashed"`/`"dotted"`, paint not geometry (unlike `step`, it never moves
/// the path). On the **fills** (`bar`, `box`, `area`, `ribbon`) it is a hatch texture
/// — `"solid"`/`"hatch"`/`"crosshatch"`/`"grid"`/`"dots"` — drawn as a `<pattern>`
/// tile (`render::pattern`). `text` is refused outright (a string has no region to
/// texture; its form is its content), and a value from the wrong geometry — a dash on
/// a fill, a hatch on a stroke — is refused with direction.
/// Which realization of the texture aesthetic a mark's geometry carries — the
/// settable rule's two arms (spec §4), stated **once**.
///
/// This exists because stating them twice failed. `check_pattern` used to name
/// the stroke class inline and sweep everything else into a `_` arm, so `path`
/// shipped with `pattern` settable in `rule_for` (which the book's generated
/// Mark × Setting grid reads, and which `write_path` draws) while the checker
/// called a path "a glyph" and refused the dash. The grid promised what the
/// engine refused — the exact drift `mark_takes_setting` was introduced to stop,
/// one level down, in the *value* check rather than the mark check.
///
/// Total on purpose: a new mark is a compile error here, not a silent `_`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Texture {
    /// A stroke's dash — every mark drawn as a line along its own route.
    Dash,
    /// A fill's hatch tile — every mark drawn as a closed region.
    Hatch,
}

pub(crate) fn texture_of(mark: &Mark) -> Option<Texture> {
    match mark {
        Mark::Line | Mark::Step | Mark::Interval | Mark::Path | Mark::Rule => Some(Texture::Dash),
        Mark::Bar | Mark::Box | Mark::Area | Mark::Ribbon | Mark::Zone => Some(Texture::Hatch),
        // A glyph (`point`) or a string (`text`) is too small / not a region to
        // texture. A `surface` is a region and still refuses, on the one ground that
        // outranks the settable rule here: a hatch tile is a texture in **screen**
        // space, and every face of a projected mesh is foreshortened differently, so
        // one tile size would read as a different density on every face — a texture
        // that varies with the viewing angle instead of with the data. That is
        // precisely the light this mark's shading is defined not to be (spec §15).
        Mark::Point | Mark::Text | Mark::Surface => None,
    }
}

/// The marks carrying one realization of the texture aesthetic, as a message
/// fragment — **read off [`texture_of`] rather than typed out.**
///
/// Written for the reason `check_polar`'s fallback was: a hand-kept list beside a
/// generated one always loses. This one had already lost. The refusal a `point` or a
/// `text` gets named "the fills (`bar`/`box`/`area`/`ribbon`)" and **omitted `zone`**,
/// which has taken a hatch since it shipped — so a reader asking about a texture was
/// told, in the engine's own voice, that the mark they might want it on cannot have
/// it. Found by auditing every refusal the book prints against `gog-cli --rules`.
fn marks_with_texture(t: Texture) -> String {
    let names: Vec<String> = ALL_MARKS
        .iter()
        .filter(|m| texture_of(m) == Some(t))
        .map(|m| format!("`{}`", mark_name(m)))
        .collect();
    names.join("/")
}

/// The legal dash values for a stroke, the counterpart of [`FILL_TEXTURES`].
pub(crate) const STROKE_DASHES: [&str; 3] = ["solid", "dashed", "dotted"];

/// The directions `style(nudge = )` accepts. Named here beside the other closed
/// vocabularies rather than only inside its own refusal message, so
/// `setting_values` can hand it to the book like every other one.
pub(crate) const NUDGES: [&str; 4] = ["up", "down", "left", "right"];

fn check_pattern(out: &mut Vec<Diagnostic>, mark: &Mark, style: &StyleSpec) {
    let Some(p) = style.pattern.as_deref() else { return };
    let m = mark_name(mark);
    match texture_of(mark) {
        // The path strokes — the dash pattern, built.
        Some(Texture::Dash) => {
            if !STROKE_DASHES.contains(&p) {
                out.push(Diagnostic {
                    kind: DiagnosticKind::Illegal,
                    message: format!(
                        "gog: `style(pattern = \"{p}\")` is not a stroke pattern. Use \"solid\" (the \
                         default), \"dashed\", or \"dotted\"."
                    ),
                });
            }
        }
        // The fills — a hatch texture, the settable rule's fill arm (§4), now drawn
        // (`render::pattern`). `solid` (shared with the strokes) is the no-texture
        // default; the four hatchings texture. A stroke's dash value or a typo is
        // refused with direction: the dash is a stroke's; a fill takes a texture.
        Some(Texture::Hatch) => {
            if !FILL_TEXTURES.contains(&p) {
                let lead = if matches!(p, "dashed" | "dotted") {
                    format!("`\"{p}\"` is a stroke's dash, not a fill texture.")
                } else {
                    format!("`\"{p}\"` is not a fill texture.")
                };
                out.push(Diagnostic {
                    kind: DiagnosticKind::Illegal,
                    message: format!(
                        "gog: `style(pattern = )` on {} `{m}`: {lead} Use \"solid\" (the default), \
                         \"hatch\", \"crosshatch\", \"grid\", or \"dots\".",
                        article(m)
                    ),
                });
            }
        }
        // Three marks have no texture, and **they do not have the same reason**, which
        // is why this arm is a match rather than one sentence. It was one sentence
        // until 2026-07-26, and it told a reader that a `surface` "is a glyph" whose
        // form is set by `shape` — false twice over, since a surface is a mesh of
        // fills and refuses `shape` too. `texture_of` above had the right reason
        // written down all along; this restated it wrongly, which is the drift that
        // function's own comment says stating a rule twice produces.
        None => out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: match mark {
                // A face *is* a fill, so this is the one refusal here that is not
                // about the mark lacking a region to texture. A hatch tile is a
                // texture in **screen** space, and a projected mesh foreshortens
                // every face differently, so one tile would read as a different
                // density on every face — a texture varying with the viewing angle
                // rather than with the data, which is what this mark's slope shading
                // is defined not to be (spec §15).
                Mark::Surface => format!(
                    "gog: `style(pattern = )` textures a fill, and a `surface` is a mesh of fills \
                     — but a hatch tile is a texture in *screen* space, and a projected mesh \
                     foreshortens every face differently, so one tile would read as a different \
                     density on every face. That is a texture that changes with the viewing angle \
                     instead of with the data. A surface says its shape with slope shading; \
                     `style(border_color = )` draws its mesh lines, and `color` ramps it."
                ),
                // A glyph (`point`) or a string (`text`) is too small / not a region
                // to texture — a point's form is `shape`, a string's is its content.
                _ => format!(
                    "gog: `style(pattern = )` is a stroke's dash or a fill's texture, and {} `{m}` \
                     is a glyph, not either — a point's form is set by `shape`, a text's by its \
                     content. The strokes ({}) take a dash; the fills ({}) a texture.",
                    article(m),
                    marks_with_texture(Texture::Dash),
                    marks_with_texture(Texture::Hatch),
                ),
            },
        }),
    }
}

fn check_border(out: &mut Vec<Diagnostic>, mark: &Mark, style: &StyleSpec) {
    if style.border_color.is_none() && style.border_size.is_none() {
        return;
    }
    let m = mark_name(mark);
    // Which marks carry a border is `mark_takes_setting` (the closed-glyph fills —
    // spec §4, the settable rule), shared with the generated grid so the two agree.
    // On a mark that does, only the value needs checking: a valid color, a
    // non-negative width. `point`'s border draws on the fillable glyphs and no-ops
    // on a `cross` (no fill to rim); the renderer handles that, not this check.
    if mark_takes_setting(mark, Setting::BorderColor) {
        if let Some(c) = &style.border_color {
            if !is_valid_color(c) {
                out.push(Diagnostic {
                    kind: DiagnosticKind::Illegal,
                    message: format!(
                        "gog: `style(border_color = \"{c}\")` is not a color.{}",
                        color_advice(c)
                    ),
                });
            }
        }
        if let Some(w) = style.border_size {
            if w < 0.0 {
                out.push(Diagnostic {
                    kind: DiagnosticKind::Illegal,
                    message: format!(
                        "gog: `style(border_size = {w})` is negative. Give 0 or more pixels — \
                         `0` draws no border (just the fill), a positive number an outline of \
                         that width."
                    ),
                });
            }
        }
        return;
    }
    // The marks with no border of their own — each refused toward the right fix.
    match mark {
        Mark::Line | Mark::Step | Mark::Interval | Mark::Rule => out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: {} `{m}` is drawn with a stroke, not a filled shape — it has no separate \
                 border. `style(color = )` sets its color and `style(size = )` its width.",
                article(m)
            ),
        }),
        Mark::Area => out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: "gog: an `area` has no border of its own — layer a `line` for a visible \
                      edge (`area + line`).".to_string(),
        }),
        Mark::Ribbon => out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: "gog: a `ribbon` is a filled band with no border of its own — layer a `line` \
                      for a visible edge along one of its boundaries.".to_string(),
        }),
        Mark::Text => out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: "gog: a `text` mark draws glyphs, not a filled shape — it has no border. \
                      `style(color = )` sets the text color.".to_string(),
        }),
        Mark::Path => out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: "gog: a `path` is a stroke, and a stroke has no separate border — \
                      `style(color = )` sets the line itself, `style(size = )` its width. \
                      (The same answer `line` gives.)".to_string(),
        }),
        // All five carry a border and are handled above, so none reaches here — a
        // `surface`'s being its mesh lines (spec §15), a `zone`'s the frame round
        // each region it fills.
        Mark::Bar | Mark::Box | Mark::Point | Mark::Surface | Mark::Zone => {}
    }
}

// ---------------------------------------------------------------------------
// The palette vocabulary — which names exist, and what kind each one is
//
// Three kinds, because they say three different things to a reader: one color
// per category, a run from little to much, and a distance from a center in
// either direction. The kind decides which columns a name suits, which is why
// the vocabulary sits here rather than beside the hex values in
// `render::palette` — the colors are the renderer's business, but "may this
// name meet this column" is the grammar's, and this layer is where the grammar
// is enforced.
//
// Nothing in the type system ties these lists to `named_ramp`/`resolve_palette`
// downstream, so `plot.rs`'s `every_named_palette_resolves_to_its_own_colors`
// walks both directions. A name legal here and unknown there would fall through
// to the default and draw the wrong colors in silence — which is the exact
// defect `check_palette` exists to stop, one level up.
// ---------------------------------------------------------------------------

pub(crate) const CATEGORICAL_PALETTES: &[&str] = &["gog", "okabe", "soft"];
pub(crate) const SEQUENTIAL_RAMPS: &[&str] =
    &["blue", "viridis", "magma", "inferno", "plasma", "cividis", "gray"];
pub(crate) const DIVERGING_RAMPS: &[&str] = &["blue_red", "brown_teal"];

/// Names in backticks, ready for [`or_list`] — the one bit of formatting every
/// palette diagnostic shares, and now that there are three lists to quote it is
/// cheaper to name it than to repeat the closure.
fn tick(names: &[&str]) -> Vec<String> {
    names.iter().map(|s| format!("`{s}`")).collect()
}

/// Check the plot-level palette.
///
/// A palette *is* a set of constants, so it validates against the same color
/// rule `style(color = ...)` uses. Before this existed, `palette("red")` was
/// read as a palette *name*, failed to match one, and fell through to the
/// default — the plot rendered in the wrong colors with nothing said.
fn check_palette(out: &mut Vec<Diagnostic>, spec: &PlotSpec, data: &HashMap<String, DataFrame>) {
    const CATEGORICAL: &[&str] = CATEGORICAL_PALETTES;
    // Both ramp kinds want a number; they differ in what they do with it, which
    // is the reader's choice and not something to refuse over.
    let continuous: Vec<&str> =
        SEQUENTIAL_RAMPS.iter().chain(DIVERGING_RAMPS).copied().collect();
    let known: Vec<&str> = CATEGORICAL.iter().chain(&continuous).copied().collect();

    // What kind of column is bound to `color`? A palette is chosen for a
    // column, so asking for the wrong kind is a mistake worth naming rather
    // than silently resolving one way or the other.
    let bound_type = spec.layers.iter().find_map(|layer| {
        let cd = layer.encodings.get(&Channel::Color).or_else(|| spec.channels.get(&Channel::Color))?;
        let df = layer.data.as_ref().or(spec.data.as_ref()).and_then(|n| data.get(n))?;
        actual_type(df, &cd.field)
    });

    match &spec.palette {
        PaletteDef::Auto => {}
        PaletteDef::Named(name) => {
            let n = name.as_str();
            match (bound_type, CATEGORICAL.contains(&n), continuous.contains(&n)) {
                (Some(VarType::Continuous), true, _) => {
                    out.push(Diagnostic {
                        kind: DiagnosticKind::Illegal,
                        message: format!(
                            "gog: `palette(\"{n}\")` hands out one color per category, but \
                             `color` is bound to a numeric column. Use a ramp — {} run one \
                             way, {} diverge from a center — or give your own stops, e.g. \
                             `palette(c(\"white\", \"navy\"))`.",
                            or_list(&tick(SEQUENTIAL_RAMPS)),
                            or_list(&tick(DIVERGING_RAMPS)),
                        ),
                    });
                    return;
                }
                (Some(VarType::Discrete), _, true) => {
                    // A gray ramp on a text column is almost always a print
                    // figure, and the grammar's answer there is not a grayer
                    // palette — four grays is the most anyone can tell apart.
                    // It is `pattern`, which separates categories without
                    // spending color at all.
                    let print = if n == "gray" {
                        " For categories in a figure that has to print in black \
                         and white, reach for `pattern(<column>)` instead — it \
                         tells them apart without spending color."
                    } else {
                        ""
                    };
                    out.push(Diagnostic {
                        kind: DiagnosticKind::Illegal,
                        message: format!(
                            "gog: `palette(\"{n}\")` is a {} ramp for numbers, but \
                             `color` is bound to a text column. Use a categorical palette — \
                             {} — or list one color per category.{print}",
                            if DIVERGING_RAMPS.contains(&n) { "diverging" } else { "sequential" },
                            or_list(&tick(CATEGORICAL)),
                        ),
                    });
                    return;
                }
                _ => {}
            }
            if known.contains(&n) {
                return;
            }
            // `gray` is in the vocabulary and `grey` is not, which is the
            // American-English rule enforced at the door rather than merely
            // obeyed inside it — the same ruling that refuses `colour` and
            // `centre` by name. Without this arm the British spelling would
            // fall to the color hint below and be told it is not a palette,
            // which is now false of the word it was reaching for.
            let hint = if name.eq_ignore_ascii_case("grey") {
                " gog spells it `gray`, the American form it uses everywhere \
                 else in the vocabulary."
                    .to_string()
            } else if is_valid_color(name) {
                format!(
                    " `\"{name}\"` is a color, not a palette. To paint every mark one \
                     color use `style(color = \"{name}\")`; to give each category its own \
                     color pass a vector: `palette(c(\"{name}\", ...))`."
                )
            } else {
                String::new()
            };
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: format!(
                    "gog: `palette(\"{name}\")` is not a known palette. Named palettes are \
                     {} for categories; {} for numbers; {} for numbers that diverge from a \
                     center.{hint}",
                    CATEGORICAL.join(", "),
                    SEQUENTIAL_RAMPS.join(", "),
                    DIVERGING_RAMPS.join(", "),
                ),
            });
        }
        PaletteDef::Custom(colors) => {
            if colors.is_empty() {
                out.push(Diagnostic {
                    kind: DiagnosticKind::Illegal,
                    message: "gog: `palette()` was given no colors. Pass at least one, \
                              e.g. `palette(c(\"steelblue\", \"tomato\"))`."
                        .into(),
                });
                return;
            }
            for c in colors {
                if is_valid_color(c) {
                    continue;
                }
                out.push(Diagnostic {
                    kind: DiagnosticKind::Illegal,
                    message: format!(
                        "gog: `palette()` entry \"{c}\" is not a color.{}",
                        color_advice(c)
                    ),
                });
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{Layer, PlotSpec};

    fn data() -> HashMap<String, DataFrame> {
        let df = DataFrame::new()
            .with_float("gdp", vec![1.0, 2.0, 3.0])
            .with_float("life", vec![4.0, 5.0, 6.0])
            .with_str(
                "continent",
                vec!["Asia".into(), "Europe".into(), "Africa".into()],
            )
            // A *second* categorical column, added when the tile plot shipped: the
            // rule that a category bounds its axis cannot be stated with one, because
            // a zone on `x(continent) + y(continent)` is a diagonal degenerate case
            // rather than the plot the rule is about.
            .with_str(
                "region",
                vec!["North".into(), "South".into(), "North".into()],
            )
            // A third *numeric* column, added when the terraced sheet shipped: a
            // surface reduces a named column on `z` while both floor axes stay
            // numeric, so it is the first mark needing three distinct numbers.
            .with_float("value", vec![7.0, 8.0, 9.0]);
        let mut m = HashMap::new();
        m.insert("t".to_string(), df);
        m
    }

    fn base() -> PlotSpec {
        PlotSpec::new().data("t").x("gdp").y("life")
    }

    fn kinds(d: &[Diagnostic]) -> Vec<DiagnosticKind> {
        d.iter().map(|x| x.kind).collect()
    }

    /// The messages, for an assertion that fails readably.
    fn msgs(d: &[Diagnostic]) -> Vec<String> {
        d.iter().map(|x| x.message.clone()).collect()
    }

    // -----------------------------------------------------------------------
    // Coord — a space with no marks in it at all (spec §15)
    // -----------------------------------------------------------------------

    /// One minimal, otherwise-clean sentence per space, so the assertion below is
    /// about the space rather than about the plot being ill-formed. `Space` binds
    /// a `z` because that, not the atom, is what makes a plot three-dimensional.
    fn spec_in(space: SpaceKind) -> PlotSpec {
        let s = base().layer(Layer::new(Mark::Point));
        match space {
            SpaceKind::Flat => s,
            SpaceKind::Space => {
                s.z("value").coord(CoordSpace::Space(crate::ir::SpaceView::default()))
            }
            SpaceKind::Polar => s.coord(CoordSpace::Polar(crate::ir::PolarView::default())),
            SpaceKind::Nest => s.coord(CoordSpace::Nest),
            SpaceKind::Globe => s.coord(CoordSpace::Globe),
            SpaceKind::Map => s.coord(CoordSpace::Map),
        }
    }

    /// The whole `Spaces:` line in one test, read off `mark_draws_in_space` rather
    /// than typed out: a space no mark stands in must be refused, and a space some
    /// mark stands in must not be refused *here*, its own gate owning that.
    ///
    /// `map` and `globe` were accepted and **drawn flat** for the project's life,
    /// and no test could have caught it, because the assertion that would have and
    /// the function that would have were the same absence. This one flips by itself
    /// the day a space gains its first mark, which is how `map` will arrive.
    #[test]
    fn a_space_no_mark_stands_in_is_refused_rather_than_drawn_flat() {
        for space in ALL_SPACES {
            let mut out = Vec::new();
            check_coord(&mut out, &spec_in(space));
            let drawn = ALL_MARKS.iter().any(|m| mark_draws_in_space(m, space));
            assert_eq!(
                out.is_empty(),
                drawn,
                "`{}` disagrees with the Mark × Space grid: {:?}",
                space_name(space),
                msgs(&out)
            );
        }
    }

    /// The refusal a user meets, through the whole checker rather than the one
    /// function: **Unsupported**, because the grammar allows the sentence and the
    /// engine cannot draw it yet (§12) — and it says what to write instead, not
    /// only what went wrong.
    #[test]
    fn an_undrawn_space_says_what_to_write_instead() {
        for (coord, atom) in [(CoordSpace::Map, "map"), (CoordSpace::Globe, "globe")] {
            let spec = base().layer(Layer::new(Mark::Point)).coord(coord);
            let out = check(&spec, &data());
            assert!(
                out.iter().any(|d| d.kind == DiagnosticKind::Unsupported
                    && d.message.contains(&format!("`{atom}()`"))
                    && d.message.contains(&format!("Drop `{atom}()`"))),
                "`{atom}()` was accepted and drawn flat: {:?}",
                msgs(&out)
            );
        }
    }

    // -----------------------------------------------------------------------
    // Brush — the selection's refusals (spec §15)
    // -----------------------------------------------------------------------

    /// **Brushability derives; it is not a second table.** The five marks that
    /// refuse `group` are exactly the five whose rows are elements rather than
    /// vertices, which is the same question a per-row predicate asks. Written as
    /// a correspondence over all thirteen rather than a list, so a mark added
    /// later gets its verdict without anyone remembering this file exists.
    #[test]
    fn a_mark_can_be_brushed_exactly_when_one_row_is_one_element() {
        let brushable: Vec<&str> = ALL_MARKS.iter().filter(|m| mark_takes_selection(m))
            .map(mark_name).collect();
        assert_eq!(brushable, vec!["point", "bar", "text", "rule", "zone"],
            "the brushable set must stay the `group`-refusing set");
        for m in &ALL_MARKS {
            assert_eq!(mark_takes_selection(m),
                rule_for(m, &Channel::Group).obligation == Obligation::Cannot,
                "`{}` disagrees with its own `group` rule", mark_name(m));
        }
    }

    /// The scrub bar, refused where it is written. It selects frames rather than
    /// rows inside one, every frame is already drawn, and so it moves the clock —
    /// which belongs to the viewer and not to the sentence.
    #[test]
    fn brushing_the_played_column_is_the_scrub_bar_and_is_refused() {
        let spec = base()
            .layer(Layer::new(Mark::Point))
            .channel(Channel::Play, "continent")
            .brush(crate::ir::BrushDef::new("continent").levels(vec!["Asia".into()]));
        let out = check(&spec, &data());
        assert!(out.iter().any(|d| d.kind == DiagnosticKind::Illegal
            && d.message.contains("scrub bar")
            && d.message.contains("Brush a column")), "{:?}", msgs(&out));
    }

    /// A mark whose rows are vertices has no single row to select, and the
    /// refusal names both ways out: split it, or brush a mark that draws one
    /// shape per row.
    #[test]
    fn a_mark_whose_rows_are_vertices_cannot_be_brushed() {
        let spec = base()
            .layer(Layer::new(Mark::Line))
            .brush(crate::ir::BrushDef::new("gdp").at(1.0, 2.0));
        let out = check(&spec, &data());
        assert!(out.iter().any(|d| d.kind == DiagnosticKind::Illegal
            && d.message.contains("one shape through many rows")
            && d.message.contains("`group()`")), "{:?}", msgs(&out));
    }

    /// A summarized layer beside one that draws its rows: the plot draws, and the
    /// engine says which layer it left whole. An **Assumption**, not a refusal —
    /// the binding still does something, so refusing it would forbid a legal
    /// sentence (Law 8).
    #[test]
    fn a_summarized_layer_beside_a_brushable_one_is_reported_not_refused() {
        let spec = base()
            .layer(Layer::new(Mark::Point))
            .layer(Layer::new(Mark::Bar).transform(Transform::Count))
            .brush(crate::ir::BrushDef::new("gdp").at(1.0, 2.0));
        let out = check(&spec, &data());
        assert!(!out.iter().any(|d| d.is_fatal()), "the plot must still draw: {:?}", msgs(&out));
        assert!(out.iter().any(|d| d.kind == DiagnosticKind::Assumption
            && d.message.contains("drawn whole")), "{:?}", msgs(&out));
    }

    /// A range that does not run upward selects nothing, so it is a mistake
    /// rather than an empty selection, and the message says which way to write it.
    #[test]
    fn a_range_that_does_not_run_upward_is_refused() {
        let spec = base()
            .layer(Layer::new(Mark::Point))
            .brush(crate::ir::BrushDef::new("gdp").at(9.0, 1.0));
        let out = check(&spec, &data());
        assert!(out.iter().any(|d| d.kind == DiagnosticKind::Illegal
            && d.message.contains("smaller number first")), "{:?}", msgs(&out));
    }

    // -----------------------------------------------------------------------
    // Nest — the packed space's own refusals (spec §15)
    // -----------------------------------------------------------------------

    /// The sentence that must stay clean, so every refusal below is about the
    /// thing it names rather than about the space being unusable.
    fn nest_base() -> PlotSpec {
        PlotSpec::new().data("t").y("gdp").coord(CoordSpace::Nest)
    }

    #[test]
    fn a_packed_bar_with_a_measure_is_accepted() {
        let spec = nest_base()
            .layer(Layer::new(Mark::Bar).transform(Transform::Sum).encode(Channel::Color, "continent"));
        let out = check(&spec, &data());
        assert!(out.is_empty(), "a plain treemap was refused: {:?}", msgs(&out));
    }

    /// A label needs no `x` here — Law 7's third relaxation, and the one this
    /// space adds. Flat, `text` requires both positions; a packing places by
    /// region, so the one-level treemap's label is well formed with a measure and
    /// a string and nothing else.
    #[test]
    fn a_packed_label_needs_no_domain_axis() {
        let spec = nest_base()
            .layer(Layer::new(Mark::Text).encode(Channel::Label, "continent"));
        let out = check(&spec, &data());
        assert!(out.is_empty(), "a packed label was refused: {:?}", msgs(&out));
    }

    /// And it still needs the **measure**, which is what the relaxation does not
    /// touch: `y` is the only thing a space with no coordinates cannot pack
    /// without, so dropping it is refused rather than assumed.
    #[test]
    fn a_packed_label_still_needs_its_measure() {
        let spec = PlotSpec::new().data("t").coord(CoordSpace::Nest)
            .layer(Layer::new(Mark::Text).encode(Channel::Label, "continent"));
        let out = check(&spec, &data());
        assert!(out.iter().any(|d| d.kind == DiagnosticKind::Illegal && d.message.contains("`y()`")),
                "a packing with nothing to size its regions by was accepted: {:?}", msgs(&out));
    }

    /// `style(nudge = )` steps a label off the point it would cover, and a packed
    /// label covers no point. Refused rather than ignored — accepting a setting
    /// that changes nothing is the silent drop §12 forbids.
    #[test]
    fn a_nudge_is_refused_in_a_packed_panel() {
        let mut text = Layer::new(Mark::Text).encode(Channel::Label, "continent");
        text.style.nudge = Some("up".into());
        let spec = nest_base().layer(text);
        let out = check(&spec, &data());
        assert!(out.iter().any(|d| d.kind == DiagnosticKind::Illegal
                                && d.message.contains("nudge")),
                "a nudge with nothing to move away from was accepted: {:?}", msgs(&out));
    }

    /// The whole Mark × Space column in one test, read off `mark_draws_in_space`
    /// rather than typed out — the drift `check_polar`'s comment warns about. The
    /// name says "the grid" rather than "every mark but bar" because the column
    /// gained `text` on 2026-07-27 and a test named after its answer goes stale
    /// the moment the answer moves.
    #[test]
    fn only_the_marks_the_grid_lists_draw_in_a_packed_panel() {
        for m in ALL_MARKS.iter() {
            if !is_drawable(m) { continue; }
            let mut out = Vec::new();
            let spec = nest_base().layer(Layer::new(m.clone()));
            check_nest(&mut out, &spec, &data());
            let refused = out.iter().any(|d| d.message.contains("Drop `nest()`"));
            assert_eq!(
                !refused,
                mark_draws_in_space(m, SpaceKind::Nest),
                "{m:?} in nest: {:?}", msgs(&out)
            );
        }
    }

    /// A collision modifier presupposes places for marks to collide in, and a
    /// packing has none. This is where the build diverged from the design (§15),
    /// so it is pinned rather than left to the renderer to shrug off.
    #[test]
    fn a_collision_modifier_is_refused_in_a_packed_panel() {
        for t in [Transform::Stack, Transform::Dodge, Transform::Jitter] {
            let spec = nest_base().layer(
                Layer::new(Mark::Bar).transform(Transform::Sum).transform(t.clone())
                    .encode(Channel::Color, "continent"));
            let out = check(&spec, &data());
            let name = transform_name(&t);
            assert!(out.iter().any(|d| d.kind == DiagnosticKind::Illegal
                                    && d.message.contains(name)),
                    "{name} was accepted in a packed panel: {:?}", msgs(&out));
        }
    }

    #[test]
    fn naming_an_axis_of_a_packed_panel_is_refused() {
        for spec in [nest_base().x_label("Wealth"), nest_base().y_label("Wealth")] {
            let spec = spec.layer(Layer::new(Mark::Bar).transform(Transform::Sum));
            let out = check(&spec, &data());
            assert!(out.iter().any(|d| d.kind == DiagnosticKind::Illegal
                                    && d.message.contains("names an axis")),
                    "an axis label was accepted: {:?}", msgs(&out));
        }
    }

    /// The refusal that exists because the alternative is a *silent* one: a log
    /// scale here would be accepted and change nothing on the page.
    #[test]
    fn a_log_scale_on_a_packed_measure_is_refused() {
        let spec = PlotSpec::new().data("t").y_scaled("gdp", ScaleType::Log)
            .coord(CoordSpace::Nest)
            .layer(Layer::new(Mark::Bar).transform(Transform::Sum));
        let out = check(&spec, &data());
        assert!(out.iter().any(|d| d.message.contains("share of the total")),
                "a log scale was accepted in a packed panel: {:?}", msgs(&out));
    }

    #[test]
    fn a_negative_measure_has_no_area_and_is_refused() {
        let mut d = data();
        d.insert("t".into(), DataFrame::new()
            .with_float("gdp", vec![1.0, -2.0, 3.0])
            .with_str("continent", vec!["Asia".into(), "Europe".into(), "Africa".into()]));
        let spec = nest_base().layer(Layer::new(Mark::Bar).transform(Transform::Sum));
        let out = check(&spec, &d);
        assert!(out.iter().any(|x| x.kind == DiagnosticKind::Illegal
                                && x.message.contains("cannot be negative")),
                "a negative area was accepted: {:?}", msgs(&out));
    }

    #[test]
    fn a_packed_panel_with_nothing_to_measure_is_refused() {
        let spec = PlotSpec::new().data("t").coord(CoordSpace::Nest)
            .layer(Layer::new(Mark::Bar).encode(Channel::Color, "continent"));
        let out = check(&spec, &data());
        assert!(out.iter().any(|d| d.message.contains("has no measure")),
                "a packing with no measure was accepted: {:?}", msgs(&out));
    }

    /// `order()` sorts a categorical position axis, so a plot with none was
    /// accepting it and dropping it. Found via the packed space, which is where a
    /// sentence with no bound position is the *ordinary* one, and fixed for every
    /// space because the rule was always `order()`'s rather than the packing's.
    #[test]
    fn order_needs_a_categorical_axis_to_sort() {
        let ordered = |spec: PlotSpec| {
            let mut out = Vec::new();
            check_order(&mut out, &spec.order_desc("gdp"), &data());
            out
        };
        // No categorical position anywhere: refused, in the plane and packed alike.
        for spec in [
            PlotSpec::new().data("t").y("gdp"),
            PlotSpec::new().data("t").y("gdp").coord(CoordSpace::Nest),
            PlotSpec::new().data("t").x("gdp").y("life"),
        ] {
            let out = ordered(spec);
            assert!(out.iter().any(|d| d.message.contains("nothing for it to put in order")),
                    "order() with no categorical axis was accepted: {:?}", msgs(&out));
        }
        // A categorical axis is exactly what it wants, in either space.
        for spec in [
            PlotSpec::new().data("t").x("continent").y("gdp"),
            PlotSpec::new().data("t").x("continent").y("gdp").coord(CoordSpace::Nest),
        ] {
            assert!(ordered(spec).is_empty(), "order() was refused where it applies");
        }
    }

    #[test]
    fn a_packing_and_a_cube_are_two_spaces() {
        let spec = nest_base().z("life")
            .layer(Layer::new(Mark::Bar).transform(Transform::Sum));
        let out = check(&spec, &data());
        assert!(out.iter().any(|d| d.message.contains("asks for two")),
                "`nest()` with `z()` was accepted: {:?}", msgs(&out));
    }

    #[test]
    fn clean_scatter_has_no_diagnostics() {
        let spec = base().layer(Layer::new(Mark::Point));
        assert!(check(&spec, &data()).is_empty());
    }

    // -- the area mark ------------------------------------------------------

    #[test]
    fn a_plain_area_is_legal() {
        let spec = base().layer(Layer::new(Mark::Area));
        assert!(check(&spec, &data()).is_empty());
    }

    #[test]
    fn an_area_splits_by_a_category_the_way_a_line_does() {
        // Wilkinson 8.1.5: a categorical variable on an aesthetic splits the
        // graphic. Whatever `line` answers here, `area` must answer too.
        let spec = base().layer(Layer::new(Mark::Area).encode(Channel::Color, "continent"));
        // Legal — it draws. (It also warns that the regions overlap; that is
        // `a_split_area_says_the_regions_will_overlap`, and an Assumption is
        // guidance, not a refusal.)
        assert!(check(&spec, &data()).iter().all(|d| !d.is_fatal()));
        // A *category* splits both marks alike, which is the claim above.
        // They part on the other reading of `color`, and the split is stated in
        // `a_region_refuses_the_measured_color_a_stroke_takes`: a stroke has a
        // length for a measure to vary along, a region has an interior.
        for m in [Mark::Area, Mark::Line] {
            let r = rule_for(&m, &Channel::Color);
            assert!(r.renders.is_some(), "{m:?} draws a split");
            assert!(matches!(r.accepts, VarType::Discrete | VarType::Either),
                    "{m:?} takes a category on color, got {:?}", r.accepts);
        }
    }

    #[test]
    fn a_region_refuses_the_measured_color_a_stroke_takes() {
        // Where the stroke marks and the region marks part, and why. A stroke has
        // a *length*, so a measure can vary along it and `color` reads off the
        // piece you are looking at. A region has an *interior*: coloring it by a
        // measure is a gradient fill, a different visual and a much larger job
        // than segmenting a stroke. So this is a boundary with a reason behind
        // it, not the leftover of the old discrete-only rule.
        for m in [Mark::Area, Mark::Ribbon] {
            let spec = base().layer(Layer::new(m.clone()).encode(Channel::Color, "gdp"));
            assert!(
                kinds(&check(&spec, &data())).contains(&DiagnosticKind::Illegal),
                "{m:?} should refuse a measured color"
            );
        }
    }

    #[test]
    fn an_area_takes_opacity_as_a_setting_and_refuses_it_as_a_channel() {
        // One region, one fill. A row here is a vertex of the boundary, not a
        // region of its own, so there is nothing for a per-row opacity to vary
        // across — `line`'s reasoning, one dimension up.
        let mapped = base().layer(Layer::new(Mark::Area).encode(Channel::Opacity, "gdp"));
        assert!(kinds(&check(&mapped, &data())).contains(&DiagnosticKind::Illegal));

        let set = base().layer(Layer::new(Mark::Area).style_opacity(0.4));
        assert!(check(&set, &data()).is_empty(), "{:?}", check(&set, &data()));
    }

    #[test]
    fn an_area_has_no_size_to_set_where_a_line_does() {
        // The one place `area` and `line` diverge, and it is not an exception
        // but a consequence: a stroke's width is free, an area's extent is
        // pinned by x, y and the baseline. Wilkinson ch. 10 — "size for area is
        // a data attribute, not an arbitrary value we may change".
        assert!(rule_for(&Mark::Line, &Channel::Size).settable);
        assert!(!rule_for(&Mark::Area, &Channel::Size).settable);

        let spec = base().layer(Layer::new(Mark::Area).style_size(6.0));
        assert!(
            !check(&spec, &data()).is_empty(),
            "setting a size on an area should be refused"
        );
    }

    #[test]
    fn step_is_the_line_family_so_it_shares_lines_rules() {
        // `step` differs from `line` only in the renderer (right angles, not
        // slants), so every legality verdict must match — anything else would be
        // a per-mark exception the No Exceptions law forbids.
        for ch in [Channel::X, Channel::Y, Channel::Color, Channel::Group,
                   Channel::Size, Channel::Opacity, Channel::Shape, Channel::Pattern, Channel::Label] {
            let (line, step) = (rule_for(&Mark::Line, &ch), rule_for(&Mark::Step, &ch));
            assert_eq!(line.obligation, step.obligation, "obligation mismatch on {ch:?}");
            assert_eq!(line.settable, step.settable, "settable mismatch on {ch:?}");
            assert_eq!(line.renders.is_some(), step.renders.is_some(), "renders mismatch on {ch:?}");
            // `accepts` too, which this test used to leave unchecked — and it is
            // the field a widening moves. `color` going `Discrete` → `Either` on
            // `line` alone would have been exactly the per-mark exception the
            // test exists to catch, and it would have passed.
            assert_eq!(line.accepts, step.accepts, "accepts mismatch on {ch:?}");
        }
        // And it draws — never refused as unbuilt like `surface`.
        let spec = base().layer(Layer::new(Mark::Step));
        assert!(check(&spec, &data()).is_empty(), "a plain step should be legal: {:?}", check(&spec, &data()));
    }

    #[test]
    fn a_bar_takes_a_border_and_other_marks_refuse_it() {
        // `style(border_color =, border_size =)` is the outline of a filled mark.
        // A bar takes it; a stroke mark (line/step) and an area refuse with
        // direction rather than draw nothing; a bad color is Illegal.
        let ok = base().layer(Layer::new(Mark::Bar).style_border("white", 1.5));
        assert!(check(&ok, &data()).is_empty(), "a bar takes a border: {:?}", check(&ok, &data()));

        for m in [Mark::Line, Mark::Step, Mark::Area] {
            let spec = base().layer(Layer::new(m.clone()).style_border("black", 1.0));
            let d = check(&spec, &data());
            assert!(!d.is_empty(), "{m:?} must refuse a border with direction");
        }

        let bad = base().layer(Layer::new(Mark::Bar).style_border("borgundy", 1.0));
        assert_eq!(kinds(&check(&bad, &data())), vec![DiagnosticKind::Illegal],
            "a bad border color is Illegal");

        // `border_size = 0` means "no border" and is legal — a bar with a fill and
        // no outline; a negative width is not.
        let zero = base().layer(Layer::new(Mark::Bar).style_border("white", 0.0));
        assert!(check(&zero, &data()).is_empty(), "border_size = 0 is legal (no border): {:?}", check(&zero, &data()));
        let neg = base().layer(Layer::new(Mark::Bar).style_border("white", -1.0));
        assert_eq!(kinds(&check(&neg, &data())), vec![DiagnosticKind::Illegal],
            "a negative border width is refused");
    }

    #[test]
    fn a_split_area_says_the_regions_will_overlap() {
        // An Assumption, not a refusal: the picture is legal, but "overlay"
        // rather than "stack" is a choice the engine made, and §12 only lets a
        // default stay silent when there is one sensible value.
        let spec = base().layer(Layer::new(Mark::Area).encode(Channel::Color, "continent"));
        let d = check(&spec, &data());
        assert_eq!(kinds(&d), vec![DiagnosticKind::Assumption]);
        assert!(!d[0].is_fatal(), "an overlap must never block the render");
        assert!(d[0].message.contains("opacity"), "{}", d[0].message);
    }

    #[test]
    fn a_set_opacity_answers_the_overlap_question() {
        // The caller has already thought about it; saying so again is noise.
        let spec = base().layer(
            Layer::new(Mark::Area).encode(Channel::Color, "continent").style_opacity(0.5),
        );
        assert!(check(&spec, &data()).is_empty());
    }

    #[test]
    fn one_region_cannot_overlap_anything() {
        let spec = base().layer(Layer::new(Mark::Area));
        assert!(check(&spec, &data()).is_empty());
    }

    #[test]
    fn a_refusal_about_an_area_reads_as_english() {
        // Three templates hardcoded "a {mark}", which was invisible until a
        // mark began with a vowel. `area` is the first, and it made the
        // diagnostics read "a area has no size to set".
        let refusals = [
            check(
                &base().layer(Layer::new(Mark::Area).style_size(4.0)),
                &data(),
            ),
            check(
                &base().layer(Layer::new(Mark::Area).encode(Channel::Shape, "continent")),
                &data(),
            ),
            check(
                &PlotSpec::new().data("t").x("gdp").layer(Layer::new(Mark::Area)),
                &data(),
            ),
        ];
        for d in refusals.iter().flatten() {
            assert!(
                !d.message.contains("a area"),
                "ungrammatical refusal: {}", d.message
            );
        }
        // And the article really is being chosen, not just absent.
        let d = check(&base().layer(Layer::new(Mark::Area).style_size(4.0)), &data());
        assert!(d[0].message.contains("an area"), "{}", d[0].message);
    }

    // -- marks this engine cannot draw *here* --------------------------------

    #[test]
    fn a_mark_that_cannot_draw_is_refused_before_it_renders_nothing() {
        // The bug this closes: `area` passed the check and drew an empty panel
        // with exit 0 and no diagnostic. The per-channel `renders: None` arm
        // could not catch it — x and y are plot-scoped, so a layer carrying no
        // encodings of its own was never asked a single question.
        //
        // **The list of marks with no renderer is now empty**, `surface` having been
        // the last (2026-07-26), so it is asserted empty rather than looped over: a
        // green run has to mean *nothing in the kernel is unbuilt*, not *the loop
        // found nothing to check*, and a vacuous loop cannot tell the two apart.
        assert!(
            ALL_MARKS.iter().all(is_drawable),
            "a mark with no renderer is back — loop over it here and give it direction"
        );

        // The live form of the same failure is one space over rather than gone: a
        // `surface` draws in the cube and nowhere else, so a flat one would render an
        // empty panel exactly as `area` did. It is refused as **Illegal** and not
        // `Unsupported`, and the difference is the point — a flat sheet is not an
        // unbuilt feature, it is an incomplete syllable (Law 7), the same kind
        // `interval` gets with no range transform.
        let spec = base().layer(Layer::new(Mark::Surface));
        let diags = check(&spec, &data());
        assert_eq!(
            diags.len(), 1,
            "a flat surface should give exactly one diagnostic, got {diags:?}"
        );
        assert_eq!(diags[0].kind, DiagnosticKind::Illegal);
        assert!(
            diags[0].message.contains(mark_name(&Mark::Surface)),
            "the message must name the mark: {}", diags[0].message
        );
    }

    #[test]
    fn a_flat_surface_is_given_both_routes_into_the_cube_and_the_flat_alternative() {
        // Law 5. Every refusal says what to do instead — and this one has three things
        // to say, because "needs `z()`" would be true and useless to a reader who
        // wanted a sheet in the plane.
        let spec = base().layer(Layer::new(Mark::Surface));
        let msg = &check(&spec, &data())[0].message;
        assert!(msg.contains("z("), "bind the height: {msg}");
        assert!(msg.contains("space()"), "or synthesize it under space(): {msg}");
        assert!(msg.contains("`zone`"), "the field in the plane is a zone's: {msg}");
    }

    /// A grid table and the 3-D spec that draws it as a sheet — the fixture the
    /// surface checks need, since `data()`'s three rows are a line and not a mesh.
    fn grid_data(nx: usize, ny: usize) -> HashMap<String, DataFrame> {
        let (mut xs, mut ys, mut hs) = (Vec::new(), Vec::new(), Vec::new());
        for j in 0..ny {
            for i in 0..nx {
                xs.push(i as f64);
                ys.push(j as f64);
                hs.push((i * j) as f64);
            }
        }
        HashMap::from([(
            "t".to_string(),
            DataFrame::new().with_float("x", xs).with_float("y", ys).with_float("h", hs),
        )])
    }

    fn grid_spec() -> PlotSpec {
        PlotSpec::new().data("t").x("x").y("y").z("h")
    }

    #[test]
    fn surface_takes_the_two_floor_transforms_and_the_five_that_reduce_into_them() {
        // **A surface needs a floor whose cells tile without gaps, and two transforms
        // give it one** — `density` as nodes to interpolate between, `bin` as cells to
        // lay lids on. The five reductions ride `bin`, exactly as they ride it on a 3-D
        // `bar` (Law 2). Pinned across the whole row rather than sampled, so a later
        // transform cannot quietly join one.
        for t in USER_TRANSFORMS {
            let expect = match t {
                Transform::Density | Transform::Bin => TransformLegality::Combines,
                Transform::Sum | Transform::Mean | Transform::Median
                | Transform::Max | Transform::Min => TransformLegality::Combines,
                _ => TransformLegality::None,
            };
            assert_eq!(mark_takes_transform(&Mark::Surface, &t), expect,
                "the grid is wrong for surface * {}", transform_name(&t));
        }
        // And what is refused still says which sentence to write instead. `smooth` is
        // the one worth checking: it is refused for a reason a cell cannot fix, so the
        // message has to name the two floors that do tile rather than apologize.
        let spec = grid_spec().layer(Layer::new(Mark::Surface).transform(Transform::Smooth));
        let msg = msgs(&check(&spec, &grid_data(4, 4))).join(" ");
        assert!(msg.contains("surface * bin"), "toward the terraced sheet: {msg}");
        assert!(msg.contains("surface * density"), "or the estimated field: {msg}");
    }

    #[test]
    fn a_cut_floor_is_a_surface_floor_and_a_slotted_one_is_still_not() {
        // The ruling this feature turned on (spec §15). `bin` cuts adjacent cells, so
        // the lids tile into a stepped sheet and the sentence draws; a category owns a
        // *slot* that leaves air, so lids would float apart and there is no sheet.
        let cut = grid_spec()
            .layer(Layer::new(Mark::Surface).transform(Transform::Bin).transform(Transform::Mean));
        assert!(check(&cut, &grid_data(4, 4)).is_empty(),
            "a cut floor tiles: {:?}", msgs(&check(&cut, &grid_data(4, 4))));

        // A category on either position, still refused: the bare `surface * mean` this
        // rule always turned down, for the reason it always gave.
        let df = HashMap::from([(
            "t".to_string(),
            DataFrame::new()
                .with_str("g", vec!["a".into(), "a".into(), "b".into(), "b".into()])
                .with_float("y", vec![0.0, 1.0, 0.0, 1.0])
                .with_float("h", vec![1.0, 2.0, 3.0, 4.0]),
        )]);
        let slotted = PlotSpec::new().data("t").x("g").y("y").z("h")
            .layer(Layer::new(Mark::Surface).transform(Transform::Mean));
        let msg = msgs(&check(&slotted, &df)).join(" ");
        assert!(!msg.is_empty(), "a slotted floor is not a sheet");
    }

    #[test]
    fn a_scatter_cannot_be_a_surface_and_is_told_which_two_sentences_can() {
        // The empty panel this project refuses above all others, arriving as geometry:
        // *n* points in general position describe an *n*×*n* lattice with *n* nodes in
        // it and not one complete block of four. Fatal on that condition exactly —
        // there is no fraction to tune.
        let scatter = HashMap::from([(
            "t".to_string(),
            DataFrame::new()
                .with_float("x", vec![0.11, 0.42, 0.77, 0.93, 0.28])
                .with_float("y", vec![0.51, 0.13, 0.88, 0.34, 0.67])
                .with_float("h", vec![1.0, 2.0, 3.0, 4.0, 5.0]),
        )]);
        let spec = grid_spec().layer(Layer::new(Mark::Surface));
        let d = check(&spec, &scatter);
        assert_eq!(kinds(&d), vec![DiagnosticKind::Illegal], "{:?}", msgs(&d));
        assert!(d[0].message.contains("`point`"), "for a cloud: {}", d[0].message);
        assert!(d[0].message.contains("surface * density"), "to estimate: {}", d[0].message);

        // The same scatter *with* the transform is the sentence the refusal advises,
        // so it must be clean — the check asks about the reader's mesh, never about one
        // a transform is going to make. Getting this wrong refused `surface * density`
        // as a scatter, quoting the sentence that had just been written.
        let estimated = PlotSpec::new().data("t").x("x").y("y")
            .coord(CoordSpace::Space(crate::ir::SpaceView::default()))
            .layer(Layer::new(Mark::Surface).transform(Transform::Density));
        assert!(check(&estimated, &scatter).is_empty(), "{:?}", msgs(&check(&estimated, &scatter)));
    }

    #[test]
    fn a_grid_with_a_gap_draws_and_counts_the_crossings_it_is_missing() {
        // Legitimate and reported: a response surface can be missing a cell, so
        // refusing it would be taste enforced as legality (Law 8) — but a mesh silently
        // full of holes is how a lattice recovered from rounded coordinates would look
        // like one with a designed opening.
        let full = grid_data(4, 4);
        let df = full.get("t").unwrap();
        let (xs, ys, hs) = (
            df.float_col("x").unwrap().to_vec(),
            df.float_col("y").unwrap().to_vec(),
            df.float_col("h").unwrap().to_vec(),
        );
        let keep: Vec<usize> = (0..xs.len()).filter(|&i| i != 5).collect();
        let holed = HashMap::from([(
            "t".to_string(),
            DataFrame::new()
                .with_float("x", keep.iter().map(|&i| xs[i]).collect())
                .with_float("y", keep.iter().map(|&i| ys[i]).collect())
                .with_float("h", keep.iter().map(|&i| hs[i]).collect()),
        )]);
        let d = check(&grid_spec().layer(Layer::new(Mark::Surface)), &holed);
        assert_eq!(kinds(&d), vec![DiagnosticKind::Assumption], "{:?}", msgs(&d));
        assert!(d[0].message.contains("15 of 16"), "count both: {}", d[0].message);

        // And a whole grid says nothing at all, or the report would be noise.
        assert!(check(&grid_spec().layer(Layer::new(Mark::Surface)), &full).is_empty());
    }

    #[test]
    fn a_surface_refuses_a_category_on_the_floor_and_names_the_bar_that_takes_one() {
        // Where the sheet parts from the 3-D bar: a face spans the gap between two
        // samples, and between two categories there is nothing to span. The refusal
        // gives the mark whose floor *may* be slotted, which is the non-obvious half.
        let mut d3 = grid_data(4, 4);
        let df = d3.remove("t").unwrap();
        let n = df.float_col("y").unwrap().len();
        d3.insert(
            "t".to_string(),
            df.with_str("cat", (0..n).map(|i| format!("g{}", i % 2)).collect()),
        );
        let spec = PlotSpec::new().data("t").x("cat").y("y").z("h")
            .layer(Layer::new(Mark::Surface));
        let d = check(&spec, &d3);
        assert_eq!(kinds(&d), vec![DiagnosticKind::Illegal], "{:?}", msgs(&d));
        assert!(d[0].message.contains("bar * count"), "toward the 3-D bar: {}", d[0].message);

        // **And the sentence it names has to be one the reader can write.** This
        // assertion used to read `bar * bin`, pinning a direction that was *itself
        // refused* — `bin` cuts a continuous axis, so anyone who followed it hit a
        // second wall naming the same category (fixed 2026-07-28). String-matching a
        // message proves only that the words are there; building the recommended
        // spec and checking it is what proves the advice.
        let recommended = PlotSpec::new().data("t").x("cat").y("cat2")
            .layer(Layer::new(Mark::Bar).transform(Transform::Count))
            .coord(CoordSpace::Space(crate::ir::SpaceView { turn: 30.0, tilt: 25.0 }));
        let mut d4 = grid_data(4, 4);
        let df4 = d4.remove("t").unwrap();
        let n4 = df4.float_col("y").unwrap().len();
        d4.insert("t".to_string(), df4
            .with_str("cat", (0..n4).map(|i| format!("g{}", i % 2)).collect())
            .with_str("cat2", (0..n4).map(|i| format!("h{}", i % 3)).collect()));
        assert!(check(&recommended, &d4).iter().all(|x| !x.is_fatal()),
            "the refusal's own direction must draw: {:?}", msgs(&check(&recommended, &d4)));
    }

    #[test]
    fn levels_are_refused_on_a_surface_toward_the_two_marks_that_band_a_field() {
        // A level set is a region *in the plane*, and a surface has already spent the
        // third axis on the measurement — so a band on a sheet could only be a color,
        // which is `zone`'s reading of the same request. Refused with direction rather
        // than accepted and ignored, which is what `zone * density(levels = )` itself
        // did for a while.
        let spec = PlotSpec::new().data("t").x("x").y("y")
            .coord(CoordSpace::Space(crate::ir::SpaceView::default()))
            .layer({
                let mut l = Layer::new(Mark::Surface).transform(Transform::Density);
                l.density = Some(crate::ir::DensitySpec {
                    adjust: None, bandwidth: None, levels: Some(5), compare: None, reach: None,
                });
                l
            });
        let d = check(&spec, &grid_data(6, 6));
        assert_eq!(kinds(&d), vec![DiagnosticKind::Illegal], "{:?}", msgs(&d));
        assert!(d[0].message.contains("zone * density"), "{}", d[0].message);
        assert!(d[0].message.contains("path * density"), "{}", d[0].message);
    }

    #[test]
    fn path_takes_only_the_field_transform_and_the_refusal_names_line() {
        // `density` is the **one** column of the grid a path takes, and every other
        // is `None` — pinned across the whole row rather than a sample, so a later
        // transform cannot quietly join one. The reason differs by class even where
        // the answer does not, which is why the row is worth stating in full.
        for t in USER_TRANSFORMS {
            let expect = if t == Transform::Density {
                TransformLegality::Combines
            } else {
                TransformLegality::None
            };
            assert_eq!(mark_takes_transform(&Mark::Path, &t), expect,
                "the grid is wrong for path * {}", transform_name(&t));
            if expect == TransformLegality::None {
                let spec = base().layer(Layer::new(Mark::Path).transform(t.clone()));
                let d = check(&spec, &data());
                assert!(d.iter().any(|x| x.is_fatal()),
                    "path * {} should be refused: {:?}", transform_name(&t), msgs(&d));
            }
        }
        // Law 5: the refusal names the mark to use instead.
        let spec = base().layer(Layer::new(Mark::Path).transform(Transform::Mean));
        assert!(check(&spec, &data())[0].message.contains("`line"),
            "path * mean should point at line: {:?}", msgs(&check(&spec, &data())));
        // And the one it takes is *not* refused — the contour, which needs both
        // positions and gets them from `base()`.
        let ok = base().layer(Layer::new(Mark::Path).transform(Transform::Density));
        assert!(!check(&ok, &data()).iter().any(|x| x.is_fatal()),
            "path * density is the contour: {:?}", msgs(&check(&ok, &data())));
    }

    #[test]
    fn a_paths_positions_are_the_glyph_marks_row_not_the_line_familys() {
        // §6's role test, asserted against `point` rather than restated, so the
        // two cannot drift: a path's two axes are both *positions* (neither is a
        // domain, because nothing is sorted), which is exactly a glyph's case.
        // Compared with `line` in the same breath so the parting is explicit.
        for ch in [Channel::X, Channel::Y] {
            let p = rule_for(&Mark::Point, &ch);
            let path = rule_for(&Mark::Path, &ch);
            assert_eq!((path.obligation, path.accepts, path.renders),
                       (p.obligation, p.accepts, p.renders),
                       "path's {ch:?} must match point's");
        }
        // And it is a real difference, not a coincidence: `line`'s y is narrower.
        assert_eq!(rule_for(&Mark::Line, &Channel::Y).accepts, VarType::Continuous);
        assert_eq!(rule_for(&Mark::Path, &Channel::Y).accepts, VarType::Either);
    }

    #[test]
    fn arrow_is_a_path_setting_and_every_other_mark_refuses_it_with_direction() {
        for m in &ALL_MARKS {
            if !is_drawable(m) { continue }
            let style = StyleSpec { arrow: Some("end".into()), ..Default::default() };
            let mut layer = Layer::new(m.clone());
            layer.style = style;
            // `interval`/`ribbon` need a range transform, `text` a label — give
            // them their minimum syllable so the only thing under test is `arrow`.
            let layer = match m {
                Mark::Interval | Mark::Ribbon => layer.transform(Transform::Range),
                Mark::Text => { layer.encode(Channel::Label, "continent") }
                _ => layer,
            };
            let d = check(&base().layer(layer), &data());
            let refused = d.iter().any(|x| x.message.contains("`style(arrow = )`"));
            assert_eq!(refused, *m != Mark::Path,
                "{m:?} arrow: refused={refused}, but only path should take it: {:?}", msgs(&d));
            if refused {
                assert!(d.iter().any(|x| x.message.contains("Use `path`")),
                    "{m:?}: the refusal must give direction: {:?}", msgs(&d));
            }
        }
    }

    #[test]
    fn an_arrow_end_that_is_not_an_end_is_refused_with_the_three_that_are() {
        let mut layer = Layer::new(Mark::Path);
        layer.style = StyleSpec { arrow: Some("head".into()), ..Default::default() };
        let d = check(&base().layer(layer), &data());
        assert!(d.iter().any(|x| x.message.contains("is not an end")
                                 && x.message.contains("\"both\"")),
            "an unknown end must be refused with the legal ones: {:?}", msgs(&d));
        // And each legal one passes.
        for end in ARROW_ENDS {
            let mut layer = Layer::new(Mark::Path);
            layer.style = StyleSpec { arrow: Some(end.into()), ..Default::default() };
            assert!(check(&base().layer(layer), &data()).is_empty(), "{end} should be legal");
        }
    }

    #[test]
    fn a_path_has_no_border_because_a_stroke_has_no_border() {
        // The settable rule: `border_*` spans the closed-glyph fills, and a path
        // is a stroke — the same answer `line` gives, which is the point.
        let mut layer = Layer::new(Mark::Path);
        layer.style = StyleSpec { border_color: Some("black".into()), ..Default::default() };
        let d = check(&base().layer(layer), &data());
        assert!(d.iter().any(|x| x.message.contains("has no separate border")),
            "path should refuse a border: {:?}", msgs(&d));
    }

    #[test]
    fn a_path_draws_and_is_no_longer_refused() {
        // The other half of the change above: `path` used to be refused with a
        // direction toward `line`, and that refusal is now the mark itself.
        let spec = base().layer(Layer::new(Mark::Path));
        assert!(check(&spec, &data()).is_empty(), "{:?}", check(&spec, &data()));
    }

    // -- scales -----------------------------------------------------------

    #[test]
    fn a_log_scale_over_positive_numbers_is_fine() {
        let spec = PlotSpec::new().data("t")
            .x_scaled("gdp", ScaleType::Log).y("life")
            .layer(Layer::new(Mark::Point));
        assert!(check(&spec, &data()).is_empty());
    }

    #[test]
    fn a_logarithm_of_text_is_illegal() {
        let spec = PlotSpec::new().data("t")
            .x_scaled("continent", ScaleType::Log).y("life")
            .layer(Layer::new(Mark::Point));
        let d = check(&spec, &data());
        assert_eq!(kinds(&d), vec![DiagnosticKind::Illegal]);
        assert!(d[0].message.contains("is text"), "{}", d[0].message);
    }

    #[test]
    fn a_log_scale_names_the_rows_it_cannot_place() {
        let df = DataFrame::new()
            .with_float("v", vec![1.0, 0.0, -4.0, 100.0])
            .with_float("y", vec![1.0, 2.0, 3.0, 4.0]);
        let mut m = HashMap::new();
        m.insert("t".to_string(), df);

        let spec = PlotSpec::new().data("t")
            .x_scaled("v", ScaleType::Log).y("y")
            .layer(Layer::new(Mark::Point));
        let d = check(&spec, &m);
        assert_eq!(kinds(&d), vec![DiagnosticKind::Illegal]);
        // Directional: says how many, how bad, and what to do about it.
        assert!(d[0].message.contains("2 of 4"), "{}", d[0].message);
        assert!(d[0].message.contains("-4"), "{}", d[0].message);
    }

    #[test]
    fn a_transforms_output_axis_is_not_judged_on_its_raw_column() {
        // `bar * sum` over profits that are individually negative but sum to a
        // positive total is well formed. Refusing it on the raw column would be
        // a false alarm — the values that get scaled do not exist yet.
        let df = DataFrame::new()
            .with_str("cat", vec!["a".into(), "a".into()])
            .with_float("profit", vec![-50.0, 150.0]);
        let mut m = HashMap::new();
        m.insert("t".to_string(), df);

        let spec = PlotSpec::new().data("t")
            .x("cat").y_scaled("profit", ScaleType::Log)
            .layer(Layer::new(Mark::Bar).transform(Transform::Sum));
        assert!(check(&spec, &m).is_empty());
    }

    #[test]
    fn a_transforms_key_axis_is_judged_on_its_raw_column() {
        // The other side of the same rule: `bin` groups by x *before* the scale
        // is irrelevant — the raw column is exactly what gets logged, so a
        // non-positive value there is knowable in advance.
        let df = DataFrame::new().with_float("v", vec![1.0, -1.0, 10.0]);
        let mut m = HashMap::new();
        m.insert("t".to_string(), df);

        let spec = PlotSpec::new().data("t")
            .x_scaled("v", ScaleType::Log)
            .layer(Layer::new(Mark::Bar).transform(Transform::Bin));
        let d = check(&spec, &m);
        assert_eq!(kinds(&d), vec![DiagnosticKind::Illegal]);
    }

    /// The third of the three scales the **column's type** chooses, and now the
    /// third that may be said out loud for nothing. `linear` on a number and
    /// `time` on a date have had this allowance and a test apiece since they
    /// shipped; `category` on text was refused until 2026-07-28, which made it the
    /// Law-2 exception the other two tests were quietly measuring against.
    #[test]
    fn a_text_column_needs_no_category_scale_said_out_loud() {
        let spec = PlotSpec::new().data("t")
            .x_scaled("continent", ScaleType::Category).y("life")
            .layer(Layer::new(Mark::Point));
        assert!(check(&spec, &data()).is_empty());
    }

    /// **Not Unsupported.** Until 2026-07-28 this said "not yet", promising a
    /// feature §18 has now ruled out: a scale says *how* a measured column is
    /// placed, and whether an axis measures at all is the column's type — the rule
    /// `log`-on-text, `linear`-on-a-date and `time`-on-a-number already enforce.
    /// One direction per column type, each asserted on what it tells the caller to
    /// **do** (§12), because the kind is the half a `kinds()` assertion can see and
    /// the direction is the half it cannot.
    #[test]
    fn the_category_refusal_gives_each_column_type_its_own_direction() {
        let refusal = |field: &str, frames| {
            let spec = PlotSpec::new().data("t")
                .x_scaled(field, ScaleType::Category).y("life")
                .layer(Layer::new(Mark::Point));
            let d = check(&spec, frames);
            assert_eq!(kinds(&d), vec![DiagnosticKind::Illegal], "{field}");
            d[0].message.clone()
        };

        let frames = data();

        // Number: removing it hands back the continuous axis the caller was
        // escaping, so the message must not say to — it must say `factor`.
        let number = refusal("gdp", &frames);
        assert!(number.contains("factor(gdp)"), "{number}");
        assert!(
            !number.contains("a text column already gets"),
            "the number branch must not repeat the text column's advice: {number}"
        );

        // A date is not a category yet either, and "make it a factor" is not the
        // sentence — one slot per distinct moment is one slot per row.
        let mut dated_life = dated();
        let df = dated_life.remove("t").unwrap().with_float("life", vec![4.0, 5.0, 6.0]);
        dated_life.insert("t".to_string(), df);
        let date = refusal("day", &dated_life);
        assert!(date.contains("one slot per row"), "{date}");
        assert!(!date.contains("factor(day)"), "{date}");

        // Both directions end in "drop the scale", so **follow one exactly** and
        // the plot must draw. String-matching a direction proves the words are
        // present and nothing about whether they are advice — the upgrade the
        // surface refusal's entry asked every `*_with_direction` test to make.
        for m in [&number, &date] {
            assert!(m.contains("drop the scale"), "{m}");
        }
        let converted = DataFrame::new()
            .with_str("gdp", vec!["1".into(), "2".into(), "3".into()])
            .with_float("life", vec![4.0, 5.0, 6.0]);
        let mut after = HashMap::new();
        after.insert("t".to_string(), converted);
        let followed = PlotSpec::new().data("t")
            .x("gdp").y("life")
            .layer(Layer::new(Mark::Point));
        assert!(
            check(&followed, &after).is_empty(),
            "doing what the refusal says must render: {:?}",
            check(&followed, &after)
        );

        // The axis a transform **writes** gets a fourth sentence, and needs one:
        // the column's type is not what lands there, so "make `life` text" would
        // be advice about the wrong numbers — and following it would break the
        // summary that produced them. This is the same distinction `check_scale`
        // draws for `log`, one arm over.
        let synth = PlotSpec::new().data("t")
            .x("continent").y_scaled("life", ScaleType::Category)
            .layer(Layer::new(Mark::Bar).transform(Transform::Mean));
        let d = check(&synth, &frames);
        assert_eq!(kinds(&d), vec![DiagnosticKind::Illegal]);
        assert!(d[0].message.contains("the transform computed"), "{}", d[0].message);
        assert!(!d[0].message.contains("factor(life)"), "{}", d[0].message);
    }

    // -- time -------------------------------------------------------------

    fn dated() -> HashMap<String, DataFrame> {
        let df = DataFrame::new()
            .with_time("day", vec![0.0, 86_400.0, 172_800.0], crate::time::TimeUnit::Day)
            .with_float("sales", vec![3.0, 5.0, 4.0]);
        let mut m = HashMap::new();
        m.insert("t".to_string(), df);
        m
    }

    #[test]
    fn a_date_column_needs_no_scale_said_out_loud() {
        // The time scale comes from the column's type, like `category` does.
        let spec = PlotSpec::new().data("t")
            .x("day").y("sales")
            .layer(Layer::new(Mark::Line));
        assert!(check(&spec, &dated()).is_empty());
    }

    #[test]
    fn saying_time_on_a_date_column_costs_nothing() {
        // Same allowance `linear` gets on a number: true, and means nothing extra.
        let spec = PlotSpec::new().data("t")
            .x_scaled("day", ScaleType::Time).y("sales")
            .layer(Layer::new(Mark::Line));
        assert!(check(&spec, &dated()).is_empty());
    }

    #[test]
    fn a_time_scale_on_a_plain_number_is_illegal_with_direction() {
        // The engine cannot know whether 20656 is a day, a second, or a year —
        // the calendar comes from the column's type, and the message says how
        // to give it one.
        let spec = PlotSpec::new().data("t")
            .x_scaled("sales", ScaleType::Time).y("sales")
            .layer(Layer::new(Mark::Point));
        let d = check(&spec, &dated());
        assert_eq!(kinds(&d), vec![DiagnosticKind::Illegal]);
        assert!(d[0].message.contains("as.Date"), "{}", d[0].message);
    }

    #[test]
    fn a_moment_in_time_has_no_logarithm() {
        let spec = PlotSpec::new().data("t")
            .x_scaled("day", ScaleType::Log).y("sales")
            .layer(Layer::new(Mark::Line));
        let d = check(&spec, &dated());
        assert_eq!(kinds(&d), vec![DiagnosticKind::Illegal]);
        assert!(d[0].message.contains("date"), "{}", d[0].message);
    }

    #[test]
    fn linear_cannot_quietly_undate_an_axis() {
        // `scale = "linear"` on a date would mean raw epoch seconds. Honoring
        // it silently draws an axis labeled 1.7B; refusing says what to do.
        let spec = PlotSpec::new().data("t")
            .x_scaled("day", ScaleType::Linear).y("sales")
            .layer(Layer::new(Mark::Point));
        let d = check(&spec, &dated());
        assert_eq!(kinds(&d), vec![DiagnosticKind::Illegal]);
        assert!(d[0].message.contains("as.numeric"), "{}", d[0].message);
    }

    #[test]
    fn a_bar_cannot_measure_a_date() {
        // A bar's length is an amount; a moment is a point. Both orientations.
        let vertical = PlotSpec::new().data("t")
            .x("sales").y("day")
            .layer(Layer::new(Mark::Bar));
        let d = check(&vertical, &dated());
        assert_eq!(kinds(&d), vec![DiagnosticKind::Illegal]);
        assert!(d[0].message.contains("amount"), "{}", d[0].message);
    }

    #[test]
    fn bars_may_sit_on_dates_and_measure_numbers() {
        // The position axis is exactly where a date belongs.
        let spec = PlotSpec::new().data("t")
            .x("day").y("sales")
            .layer(Layer::new(Mark::Bar));
        assert!(check(&spec, &dated()).is_empty());
    }

    #[test]
    fn saying_linear_out_loud_costs_nothing() {
        let spec = PlotSpec::new().data("t")
            .x_scaled("gdp", ScaleType::Linear).y("life")
            .layer(Layer::new(Mark::Point));
        assert!(check(&spec, &data()).is_empty());
    }

    #[test]
    fn every_channel_that_measures_can_take_a_scale() {
        // Orthogonality: a scale answers "how far along?", so it belongs to
        // every channel that carries a magnitude, not only the axes.
        for ch in [Channel::Color, Channel::Size, Channel::Opacity] {
            let spec = base().layer(
                Layer::new(Mark::Point).encode_scaled(ch.clone(), "gdp", ScaleType::Log),
            );
            assert!(check(&spec, &data()).is_empty(), "{ch:?} should accept a log scale");
        }
    }

    #[test]
    fn a_channel_that_only_distinguishes_cannot_take_a_scale() {
        // `shape` answers "which one?", and there is no distance between circle
        // and square for a logarithm to compress.
        let spec = base().layer(
            Layer::new(Mark::Point)
                .encode_scaled(Channel::Shape, "continent", ScaleType::Log),
        );
        let d = check(&spec, &data());
        assert_eq!(kinds(&d), vec![DiagnosticKind::Illegal]);
        assert!(d[0].message.contains("distinguishes categories"), "{}", d[0].message);
    }

    #[test]
    fn a_log_color_still_needs_a_numeric_column() {
        let spec = base().layer(
            Layer::new(Mark::Point)
                .encode_scaled(Channel::Color, "continent", ScaleType::Log),
        );
        let d = check(&spec, &data());
        assert_eq!(kinds(&d), vec![DiagnosticKind::Illegal]);
        assert!(d[0].message.contains("is text"), "{}", d[0].message);
    }

    #[test]
    fn size_on_categorical_is_illegal() {
        let spec = base().layer(Layer::new(Mark::Point).encode(Channel::Size, "continent"));
        let d = check(&spec, &data());
        assert_eq!(kinds(&d), vec![DiagnosticKind::Illegal]);
        assert!(d[0].message.contains("needs a continuous"));
        // The message must be directional, not merely a complaint.
        assert!(d[0].message.contains("Use `color`, `shape`, or `pattern`"));
    }

    #[test]
    fn size_on_bar_is_illegal() {
        let spec = base().layer(Layer::new(Mark::Bar).encode(Channel::Size, "gdp"));
        assert_eq!(kinds(&check(&spec, &data())), vec![DiagnosticKind::Illegal]);
    }

    #[test]
    fn opacity_renders_on_point_and_bar() {
        for mark in [Mark::Point, Mark::Bar] {
            let spec = base().layer(Layer::new(mark.clone()).encode(Channel::Opacity, "gdp"));
            assert!(
                check(&spec, &data()).is_empty(),
                "opacity should render on {:?}",
                mark
            );
        }
    }

    #[test]
    fn opacity_on_line_is_illegal_for_the_same_reason_as_size() {
        // A polyline is drawn with a single stroke, so a per-row continuous
        // channel has nothing to vary along. `size` and `opacity` must give the
        // same answer here — differing would be a per-channel special case.
        let size = check(
            &base().layer(Layer::new(Mark::Line).encode(Channel::Size, "gdp")),
            &data(),
        );
        let opacity = check(
            &base().layer(Layer::new(Mark::Line).encode(Channel::Opacity, "gdp")),
            &data(),
        );
        assert_eq!(kinds(&size), vec![DiagnosticKind::Illegal]);
        assert_eq!(kinds(&opacity), vec![DiagnosticKind::Illegal]);
    }

    #[test]
    fn opacity_on_a_categorical_column_is_illegal() {
        let spec = base().layer(Layer::new(Mark::Point).encode(Channel::Opacity, "continent"));
        let d = check(&spec, &data());
        assert_eq!(kinds(&d), vec![DiagnosticKind::Illegal]);
        assert!(d[0].message.contains("needs a continuous"));
    }

    #[test]
    fn categorical_x_works_for_both_point_and_bar() {
        // A categorical x is a strip plot for `point` and a category axis for
        // `bar`. Both must be accepted — gating it to one mark was a per-mark
        // special case, i.e. a Law of No Exceptions violation.
        for mark in [Mark::Point, Mark::Bar] {
            let spec = PlotSpec::new()
                .data("t")
                .x("continent")
                .y("life")
                .layer(Layer::new(mark.clone()));
            assert!(
                check(&spec, &data()).is_empty(),
                "categorical x should be legal for {:?}",
                mark
            );
        }
    }

    #[test]
    fn categorical_y_works_for_both_point_and_bar() {
        // The mirror of the x case: a categorical y is a horizontal strip plot
        // for `point` and a horizontal category axis for `bar`. `point` refusing
        // it while `bar` accepted it was the same per-mark asymmetry categorical
        // x had — Law 2 (No Exceptions). `line` still refuses (its own test).
        for mark in [Mark::Point, Mark::Bar] {
            let spec = PlotSpec::new()
                .data("t")
                .x("life")
                .y("continent")
                .layer(Layer::new(mark.clone()));
            assert!(
                check(&spec, &data()).is_empty(),
                "categorical y should be legal for {:?}",
                mark
            );
        }
    }

    #[test]
    fn synthesized_y_names_an_output_column_not_an_input() {
        // `bar * bin + x(height) + y(count)` — `count` is the name the transform
        // writes its output to, so it must not be checked against input columns.
        let spec = PlotSpec::new()
            .data("t")
            .x("gdp")
            .y("count")
            .layer(Layer::new(Mark::Bar).transform(Transform::Bin));
        assert!(check(&spec, &data()).is_empty());
    }

    /// A **composed** `proportion` synthesizes nothing, so its `y` is an input name
    /// and a misspelling of it must still be caught.
    ///
    /// Found by a reader looking at a plot, which is twice in three sessions for
    /// this corner. `bar * sum * proportion + x(continent) + y(pop)` — `pop` having
    /// been renamed `population` in the book's own data — drew an **empty panel on
    /// fabricated 0..1 axes**, exactly the failure `synthesizes_measure`'s doc
    /// comment warns about, because `proportion` was still on its list of
    /// inventors. The pair is what makes it obvious: dropping `proportion` from the
    /// sentence refused correctly, and a word that measures nothing cannot turn a
    /// misspelling into a legal name.
    #[test]
    fn a_composed_proportion_still_checks_the_column_it_rescales() {
        let missing = |ts: Vec<Transform>| {
            let mut layer = Layer::new(Mark::Bar);
            for t in ts { layer = layer.transform(t); }
            check(&PlotSpec::new().data("t").x("continent").y("nosuchcolumn").layer(layer), &data())
        };
        for ts in [
            vec![Transform::Sum],
            vec![Transform::Sum, Transform::Proportion],
            vec![Transform::Mean, Transform::Proportion],
        ] {
            let d = missing(ts.clone());
            assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
                        && x.message.contains("not in the data")),
                "`bar * {}` + y(nosuchcolumn) must refuse: {:?}",
                ts.iter().map(transform_name).collect::<Vec<_>>().join(" * "), msgs(&d));
        }
        // …and a *bare* `proportion` still names its own output, which is the
        // reading that put it on the inventors' list in the first place.
        assert!(missing(vec![Transform::Proportion]).is_empty(),
            "`bar * proportion + y(<name>)` names the column it writes");
    }

    #[test]
    fn missing_y_is_illegal_unless_a_transform_synthesizes_it() {
        let spec = PlotSpec::new()
            .data("t")
            .x("gdp")
            .layer(Layer::new(Mark::Point));
        assert_eq!(kinds(&check(&spec, &data())), vec![DiagnosticKind::Illegal]);

        let spec = PlotSpec::new()
            .data("t")
            .x("gdp")
            .layer(Layer::new(Mark::Bar).transform(Transform::Count));
        assert!(check(&spec, &data()).is_empty());
    }

    #[test]
    fn a_color_passed_to_the_color_channel_points_at_style() {
        // `color("red")` is the natural first guess for setting a constant.
        // The quotes survive deparsing, so gog can tell a value from a column
        // name and say so, instead of sending the reader hunting for a typo.
        let spec = base().layer(Layer::new(Mark::Point).encode(Channel::Color, "\"red\""));
        let d = check(&spec, &data());
        assert_eq!(kinds(&d), vec![DiagnosticKind::Illegal]);
        assert!(d[0].message.contains(r#"style(color = "red")"#), "got: {}", d[0].message);
        assert!(!d[0].message.contains("spelling"), "got: {}", d[0].message);
    }

    #[test]
    fn a_quoted_column_name_is_told_to_drop_the_quotes() {
        // Law 4: columns are bare names. Quoting one is a habit from other
        // R packages, not a typo, so the message names the real fix.
        let spec = PlotSpec::new()
            .data("t")
            .x("\"gdp\"")
            .y("life")
            .layer(Layer::new(Mark::Point));
        let d = check(&spec, &data());
        assert_eq!(kinds(&d), vec![DiagnosticKind::Illegal]);
        assert!(d[0].message.contains("x(gdp)"), "got: {}", d[0].message);
    }

    #[test]
    fn unknown_column_is_illegal() {
        let spec = PlotSpec::new()
            .data("t")
            .x("gdp")
            .y("lifeExp") // real column is `life`
            .layer(Layer::new(Mark::Point));
        let d = check(&spec, &data());
        assert_eq!(kinds(&d), vec![DiagnosticKind::Illegal]);
        assert!(d[0].message.contains("not in the data"));
    }

    #[test]
    fn color_on_point_takes_either_column_type() {
        // Was the last `accepts`/`renders` gap: `color` on `point` was declared
        // to take either type and drew only categories. A numeric column now
        // takes the sequential ramp.
        for field in ["gdp", "continent"] {
            let spec = base().layer(Layer::new(Mark::Point).encode(Channel::Color, field));
            assert!(
                check(&spec, &data()).is_empty(),
                "color({field}) should render: {:?}",
                check(&spec, &data())
            );
        }
    }

    #[test]
    fn a_stroke_takes_a_measured_color_but_still_refuses_a_measured_width() {
        // The two halves of one distinction, asserted together so neither can
        // drift into the other. A stroke's color may be read off the *piece* of
        // it you are looking at, so a measure varies along it; a stroke's width
        // and opacity belong to the whole element, so they cannot. That is why
        // `color` widened to `Either` on the stroke marks while `size` and
        // `opacity` stayed set-only — not a channel-by-channel judgment call.
        for m in [Mark::Line, Mark::Step, Mark::Path] {
            let ramped = check(&base().layer(Layer::new(m.clone()).encode(Channel::Color, "gdp")), &data());
            assert!(
                ramped.iter().all(|d| !d.is_fatal()),
                "{m:?} should take a measured color: {:?}", msgs(&ramped)
            );
            for ch in [Channel::Size, Channel::Opacity] {
                let d = check(&base().layer(Layer::new(m.clone()).encode(ch.clone(), "gdp")), &data());
                assert!(
                    kinds(&d).contains(&DiagnosticKind::Illegal),
                    "{m:?} should still refuse a mapped {ch:?}: {:?}", msgs(&d)
                );
            }
        }
    }

    #[test]
    fn a_palette_must_match_the_kind_of_column_it_colors() {
        // A palette is chosen for a column. Asking for the wrong kind is a
        // mistake worth naming, not something to silently resolve either way.
        let mut spec = base().layer(Layer::new(Mark::Point).encode(Channel::Color, "gdp"));
        spec.palette = PaletteDef::Named("okabe".into());
        let d = check(&spec, &data());
        assert_eq!(kinds(&d), vec![DiagnosticKind::Illegal]);
        assert!(d[0].message.contains("one color per category"), "got: {}", d[0].message);
        assert!(d[0].message.contains("viridis"), "should name the ramps: {}", d[0].message);

        let mut spec = base().layer(Layer::new(Mark::Point).encode(Channel::Color, "continent"));
        spec.palette = PaletteDef::Named("viridis".into());
        let d = check(&spec, &data());
        assert_eq!(kinds(&d), vec![DiagnosticKind::Illegal]);
        assert!(d[0].message.contains("sequential ramp"), "got: {}", d[0].message);
    }

    #[test]
    fn the_ramps_are_accepted_for_the_column_they_suit() {
        for (field, pal) in [("gdp", "blue"), ("gdp", "viridis"), ("continent", "gog")] {
            let mut spec = base().layer(Layer::new(Mark::Point).encode(Channel::Color, field));
            spec.palette = PaletteDef::Named(pal.into());
            assert!(check(&spec, &data()).is_empty(), "{pal} should suit {field}");
        }
    }

    #[test]
    fn an_unset_palette_suits_either_column_type() {
        // `Auto` exists so "said nothing" and "asked for the categorical
        // palette" can differ: the first picks a ramp, the second is an error.
        for field in ["gdp", "continent"] {
            let spec = base().layer(Layer::new(Mark::Point).encode(Channel::Color, field));
            assert!(matches!(spec.palette, PaletteDef::Auto));
            assert!(check(&spec, &data()).is_empty(), "auto should suit {field}");
        }
    }

    // -- orientation -----------------------------------------------------

    #[test]
    fn orientation_is_read_off_the_bound_types() {
        use Orient::*;
        use VarType::{Continuous, Discrete};
        let cases = [
            // (x, y, expected, why)
            (Some(Discrete), Some(Continuous), Vertical, "categories on x"),
            (Some(Continuous), Some(Discrete), Horizontal, "categories on y"),
            (Some(Continuous), Some(Continuous), Vertical, "neither: keep the default"),
            (Some(Discrete), Some(Discrete), Vertical, "illegal; reported separately"),
            // A synthesizing transform fills the axis with no column of its own,
            // and what it writes is what the bars measure.
            (Some(Discrete), None, Vertical, "`bar * count + x(cat)`"),
            (None, Some(Discrete), Horizontal, "`bar * count + y(cat)`"),
            (Some(Continuous), None, Vertical, "`bar * bin + x(height)`"),
            (None, Some(Continuous), Horizontal, "`bar * bin + y(height)`"),
            (None, None, Vertical, "nothing bound"),
        ];
        for (x, y, want, why) in cases {
            assert_eq!(slot_orient(x, y), want, "{why}: ({x:?}, {y:?})");
        }
    }

    #[test]
    fn two_categorical_axes_leave_nothing_to_measure() {
        let spec = PlotSpec::new()
            .data("t")
            .x("continent")
            .y("continent")
            .layer(Layer::new(Mark::Bar));
        let d = check(&spec, &data());
        assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal));
        assert!(
            d[0].message.contains("nothing for it to measure"),
            "got: {}", d[0].message
        );
        // Directional: names the way to get a number out of two categories.
        assert!(d[0].message.contains("bar * count"));
    }

    /// The relational rule is the *family's*, not one mark's. Every slot mark
    /// relaxes `y` to Either (so a category there is the horizontal form), which
    /// means every one of them also has to refuse the pair that measures nothing —
    /// otherwise the relaxation opens a hole in exactly the marks it widened.
    /// Written against the family list so a fourth slot mark cannot skip it.
    #[test]
    fn every_slot_mark_refuses_two_categorical_axes() {
        for mark in [Mark::Bar, Mark::Box, Mark::Interval] {
            let mut layer = Layer::new(mark.clone());
            // `interval`'s minimum syllable includes a pair transform; without one
            // its own refusal would fire and mask the one under test.
            if mark == Mark::Interval {
                layer = layer.transform(Transform::Range);
            }
            let spec = PlotSpec::new().data("t").x("continent").y("continent").layer(layer);
            let d = check(&spec, &data());
            let m = mark_name(&mark);
            assert!(
                d.iter().any(|x| x.kind == DiagnosticKind::Illegal
                    && x.message.contains("categorical columns on both axes")
                    && x.message.contains(m)),
                "`{m}` accepted two categorical axes: {:?}",
                d.iter().map(|x| &x.message).collect::<Vec<_>>()
            );
        }
    }

    /// The Law-2 parity the horizontal box/interval work closed: a slot mark's two
    /// axes have the **same role** — one holds the slots, the other the measure —
    /// so any one of them accepting a category on `x` but refusing it on `y` is a
    /// gap, not a design. (A *path* is exempt and deliberately absent here: its `x`
    /// is permanently the domain and its `y` the measure, different roles, spec §6.)
    #[test]
    fn a_slot_marks_two_position_axes_carry_the_same_rule() {
        for mark in [Mark::Bar, Mark::Box, Mark::Interval] {
            let (x, y) = (rule_for(&mark, &Channel::X), rule_for(&mark, &Channel::Y));
            let m = mark_name(&mark);
            assert_eq!(x.obligation, y.obligation, "`{m}`: x and y disagree on obligation");
            assert_eq!(x.accepts, y.accepts, "`{m}`: x and y disagree on accepted type");
            assert_eq!(x.renders, y.renders, "`{m}`: x and y disagree on what renders");
        }
    }

    /// A horizontal box/whisker end to end: a category on `y`, the measure on `x`,
    /// and no diagnostic at all. The mirror of the upright form, which is the whole
    /// claim — `slot_orient` reads the pair, and there is no `flip` to write.
    #[test]
    fn a_category_on_y_is_the_horizontal_box_and_whisker() {
        let cases = [
            (Mark::Box, None),
            (Mark::Interval, Some(Transform::Range)),
            (Mark::Interval, Some(Transform::Confidence)),
        ];
        for (mark, t) in cases {
            let mut layer = Layer::new(mark.clone());
            if let Some(t) = t { layer = layer.transform(t); }
            let spec = PlotSpec::new().data("t").x("gdp").y("continent").layer(layer);
            let d = check(&spec, &data());
            assert!(
                d.is_empty(),
                "`{}` refused a category on y: {:?}",
                mark_name(&mark),
                d.iter().map(|x| &x.message).collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn a_horizontal_bar_may_omit_the_axis_its_transform_writes() {
        // `bar * count + y(cat)` invents x, exactly as `+ x(cat)` invents y.
        // Requiring `x()` here would be a vertical-only assumption.
        let spec = PlotSpec::new()
            .data("t")
            .y("continent")
            .layer(Layer::new(Mark::Bar).transform(Transform::Count));
        assert!(check(&spec, &data()).is_empty(), "{:?}", check(&spec, &data()));

        let spec = PlotSpec::new()
            .data("t")
            .x("continent")
            .layer(Layer::new(Mark::Bar).transform(Transform::Count));
        assert!(check(&spec, &data()).is_empty());
    }

    #[test]
    fn a_categorical_y_is_legal_on_bar_but_still_refused_on_line() {
        // Relaxing `rule_for(Bar, Y)` to Either must not leak into other marks:
        // a line has no notion of sitting on a category.
        assert!(check(
            &PlotSpec::new().data("t").x("gdp").y("continent").layer(Layer::new(Mark::Bar)),
            &data()
        ).is_empty());

        let d = check(
            &PlotSpec::new().data("t").x("gdp").y("continent").layer(Layer::new(Mark::Line)),
            &data(),
        );
        assert_eq!(kinds(&d), vec![DiagnosticKind::Illegal]);
    }

    // -- channel scope ---------------------------------------------------

    #[test]
    fn a_plot_scoped_channel_reaches_every_layer_that_accepts_it() {
        let spec = base()
            .channel(Channel::Color, "continent")
            .layer(Layer::new(Mark::Line))
            .layer(Layer::new(Mark::Point));
        let r = resolve_scopes(&spec);
        for layer in &r.layers {
            assert_eq!(
                layer.encodings.get(&Channel::Color).map(|c| c.field.as_str()),
                Some("continent"),
                "{:?} should have picked up the plot-scoped color",
                layer.mark
            );
        }
        assert!(check(&spec, &data()).is_empty());
    }

    #[test]
    fn a_layer_binding_beats_the_plot_scoped_one() {
        // Nearest wins — the same rule that governs `data()`.
        let spec = base()
            .channel(Channel::Color, "continent")
            .layer(Layer::new(Mark::Point).encode(Channel::Color, "country"));
        let r = resolve_scopes(&spec);
        assert_eq!(
            r.layers[0].encodings[&Channel::Color].field,
            "country"
        );
    }

    #[test]
    fn a_plot_scoped_channel_skips_marks_without_the_feature() {
        // This is the case the old backward broadcast could not express: `size`
        // belongs to the points, and the line beside them simply has no size.
        let spec = base()
            .channel(Channel::Size, "gdp")
            .layer(Layer::new(Mark::Line))
            .layer(Layer::new(Mark::Point));
        let r = resolve_scopes(&spec);
        assert!(!r.layers[0].encodings.contains_key(&Channel::Size), "line must be skipped");
        assert!(r.layers[1].encodings.contains_key(&Channel::Size), "point must take it");

        // Skipping is reported, never silent — but it still renders.
        let d = check(&spec, &data());
        assert_eq!(kinds(&d), vec![DiagnosticKind::Assumption]);
        assert!(!d[0].is_fatal());
        assert!(d[0].message.contains("`point`") && d[0].message.contains("`line`"));
    }

    #[test]
    fn a_plot_scoped_channel_no_mark_accepts_is_illegal() {
        let spec = base()
            .channel(Channel::Size, "gdp")
            .layer(Layer::new(Mark::Line));
        let d = check(&spec, &data());
        assert_eq!(kinds(&d), vec![DiagnosticKind::Illegal]);
        assert!(d[0].message.contains("no mark here has a size feature"));
    }

    #[test]
    fn order_no_longer_changes_meaning() {
        // The defect this replaced: `set_channel` reached backwards, so the same
        // atoms in a different order produced different plots. Binding forward
        // only, a layer written after the channel is unaffected by it.
        let a = resolve_scopes(
            &base()
                .layer(Layer::new(Mark::Point).encode(Channel::Color, "continent"))
                .layer(Layer::new(Mark::Line)),
        );
        let b = resolve_scopes(
            &base()
                .layer(Layer::new(Mark::Line))
                .layer(Layer::new(Mark::Point).encode(Channel::Color, "continent")),
        );
        let colored = |s: &PlotSpec| {
            s.layers
                .iter()
                .filter(|l| l.encodings.contains_key(&Channel::Color))
                .map(|l| format!("{:?}", l.mark))
                .collect::<Vec<_>>()
        };
        assert_eq!(colored(&a), vec!["Point"]);
        assert_eq!(colored(&b), vec!["Point"]);
    }

    #[test]
    fn resolving_twice_changes_nothing() {
        // `check` and the renderer both resolve on entry; that must be safe.
        let spec = base()
            .channel(Channel::Color, "continent")
            .layer(Layer::new(Mark::Point));
        let once = resolve_scopes(&spec);
        let twice = resolve_scopes(&once);
        assert_eq!(
            format!("{:?}", once.layers[0].encodings),
            format!("{:?}", twice.layers[0].encodings)
        );
    }

    // -- set vs map ------------------------------------------------------

    #[test]
    fn a_constant_is_settable_exactly_where_a_column_is_not_mappable() {
        // The whole point of the distinction. A polyline is one stroke, so
        // `opacity`/`size` cannot vary per row — yet that same one stroke has
        // a width and an opacity, so setting them is fine.
        for ch in [Channel::Opacity, Channel::Size] {
            let mapped = check(
                &base().layer(Layer::new(Mark::Line).encode(ch.clone(), "gdp")),
                &data(),
            );
            assert_eq!(
                kinds(&mapped),
                vec![DiagnosticKind::Illegal],
                "{ch:?} must not be mappable on line"
            );
        }
        let set = check(
            &base().layer(Layer::new(Mark::Line).style_opacity(0.4).style_size(6.0)),
            &data(),
        );
        assert!(set.is_empty(), "style must be accepted on line: {set:?}");
    }

    #[test]
    fn setting_a_feature_the_mark_does_not_have_is_illegal() {
        // `shape` on bar is Cannot for a different reason than `opacity` on
        // line: a bar has no glyph at all, so there is nothing to set either.
        let d = check(
            &base().layer(Layer::new(Mark::Bar).style_shape("square")),
            &data(),
        );
        assert_eq!(kinds(&d), vec![DiagnosticKind::Illegal]);
        assert!(d[0].message.contains("has no shape to set"));
    }

    #[test]
    fn mapping_and_setting_the_same_feature_is_illegal() {
        // Honoring one would mean silently dropping the other.
        let d = check(
            &base().layer(
                Layer::new(Mark::Point)
                    .encode(Channel::Color, "continent")
                    .style_color("red"),
            ),
            &data(),
        );
        assert_eq!(kinds(&d), vec![DiagnosticKind::Illegal]);
        assert!(d[0].message.contains("one layer cannot do both"));
    }

    #[test]
    fn a_set_value_is_checked_because_no_data_can_check_it() {
        let cases: Vec<(Layer, &str)> = vec![
            (Layer::new(Mark::Point).style_color("stelblue"), "steelblue"),
            (Layer::new(Mark::Point).style_opacity(3.0), "outside 0–1"),
            (Layer::new(Mark::Point).style_size(0.0), "positive"),
            (Layer::new(Mark::Point).style_shape("star"), "circle, square"),
        ];
        for (layer, expect) in cases {
            let d = check(&base().layer(layer), &data());
            assert_eq!(kinds(&d), vec![DiagnosticKind::Illegal]);
            assert!(
                d[0].message.contains(expect),
                "message should contain {expect:?}, got: {}",
                d[0].message
            );
        }
    }


    #[test]
    fn an_r_color_name_says_which_vocabulary_is_in_force() {
        // `gray80` is an R name, not a CSS one, and it is the first thing an R
        // user reaches for. Suggesting plain "gray" would be a different shade,
        // so the message has to explain the vocabulary instead.
        let d = check(
            &base().layer(Layer::new(Mark::Point).style_color("gray80")),
            &data(),
        );
        assert_eq!(kinds(&d), vec![DiagnosticKind::Illegal]);
        assert!(d[0].message.contains("R color name"), "got: {}", d[0].message);
        assert!(d[0].message.contains("lightgray"), "got: {}", d[0].message);
        assert!(!d[0].message.contains("Did you mean"), "got: {}", d[0].message);

        assert_eq!(numbered_shade("gray50"), Some("gray"));
        assert_eq!(numbered_shade("gray100"), Some("gray"));
        // gog writes American spellings, but CSS defines both, so both are
        // still *accepted* from a user and the advice follows the spelling
        // they typed rather than correcting it to ours.
        assert_eq!(numbered_shade("grey80"), Some("grey"));
        // Not every trailing digit is a shade number.
        assert_eq!(numbered_shade("steelblue"), None);
        assert_eq!(numbered_shade("12345"), None);
    }

    #[test]
    fn a_suggested_color_is_always_a_real_one() {
        // CSS has `lightsteelblue` but no `darksteelblue`. A suggestion that
        // fails when the reader retries it is worse than offering none.
        for bad in ["gray80", "gray50", "steelblue3", "orchid4", "tomato2"] {
            let d = check(
                &base().layer(Layer::new(Mark::Point).style_color(bad)),
                &data(),
            );
            assert_eq!(kinds(&d), vec![DiagnosticKind::Illegal]);
            for quoted in d[0].message.split('"').skip(1).step_by(2) {
                // The message echoes the offending value back; skip that and
                // the hex example, and check what remains — the suggestions.
                if quoted == bad || quoted.starts_with('#') {
                    continue;
                }
                assert!(
                    is_valid_color(quoted),
                    "suggested {quoted:?} for {bad:?}, which is not a color: {}",
                    d[0].message
                );
            }
        }
    }

    #[test]
    fn suggestion_lists_read_as_a_sentence() {
        let s = |v: &[&str]| or_list(&v.iter().map(|x| x.to_string()).collect::<Vec<_>>());
        assert_eq!(s(&["a"]), "a");
        assert_eq!(s(&["a", "b"]), "a or b");
        assert_eq!(s(&["a", "b", "c"]), "a, b, or c");
    }


    // -- palette ---------------------------------------------------------

    #[test]
    fn a_color_name_as_a_palette_is_refused_not_silently_ignored() {
        // Was: `palette("red")` fell through to the default palette and the
        // plot rendered in the wrong colors with nothing said.
        let spec = base()
            .layer(Layer::new(Mark::Point).encode(Channel::Color, "continent"));
        let mut spec = spec;
        spec.palette = PaletteDef::Named("red".into());
        let d = check(&spec, &data());
        assert_eq!(kinds(&d), vec![DiagnosticKind::Illegal]);
        // Directional: names *every* real palette and points at `style()`.
        // Driven off the list rather than spelled out, so a name added to the
        // vocabulary and left out of the message fails here — a refusal that
        // recites two thirds of the options is how a reader concludes the third
        // does not exist.
        for name in CATEGORICAL_PALETTES {
            assert!(
                d[0].message.contains(name),
                "the refusal omits `{name}`: {}",
                d[0].message
            );
        }
        assert!(d[0].message.contains("style(color = \"red\")"));
    }

    #[test]
    fn a_misspelt_palette_color_is_refused() {
        let mut spec = base().layer(Layer::new(Mark::Point));
        spec.palette = PaletteDef::Custom(vec!["red".into(), "stelblue".into()]);
        let d = check(&spec, &data());
        assert_eq!(kinds(&d), vec![DiagnosticKind::Illegal]);
        assert!(d[0].message.contains("steelblue"));
    }

    #[test]
    fn the_named_palettes_and_css_color_lists_still_pass() {
        for name in ["gog", "okabe"] {
            let mut spec = base().layer(Layer::new(Mark::Point));
            spec.palette = PaletteDef::Named(name.into());
            assert!(check(&spec, &data()).is_empty(), "{name} should be a palette");
        }
        let mut spec = base().layer(Layer::new(Mark::Point));
        spec.palette = PaletteDef::Custom(vec!["red".into(), "#4e79a7".into()]);
        assert!(check(&spec, &data()).is_empty());
    }

    #[test]
    fn what_renders_can_also_be_set() {
        // Regularity: if the engine can draw a feature *varying* per row, it can
        // certainly draw it fixed. Only the five style-eligible features are in
        // scope — `x`/`y`/`z` are position, and `group` partitions rather than
        // decorates, so neither is style.
        let marks = [Mark::Point, Mark::Line, Mark::Area, Mark::Bar];
        let features = [Channel::Color, Channel::Size, Channel::Shape, Channel::Opacity, Channel::Pattern];
        for m in &marks {
            for c in &features {
                let r = rule_for(m, c);
                if r.renders.is_some() {
                    assert!(r.settable, "{m:?}/{c:?} renders when mapped but is not settable");
                }
            }
        }
        for c in [Channel::X, Channel::Y, Channel::Z, Channel::Group, Channel::Label, Channel::Play] {
            for m in &marks {
                assert!(!rule_for(m, &c).settable, "{m:?}/{c:?} must not be settable");
            }
        }
    }

    // --- facets ------------------------------------------------------------

    #[test]
    fn facet_by_a_category_column_is_legal() {
        let spec = base().layer(Layer::new(Mark::Point)).facet_col("continent");
        assert!(check(&spec, &data()).is_empty());
    }

    #[test]
    fn facet_by_a_number_column_is_illegal_with_direction() {
        let spec = base().layer(Layer::new(Mark::Point)).facet_col("gdp");
        let d = check(&spec, &data());
        assert_eq!(kinds(&d), [DiagnosticKind::Illegal]);
        assert!(d[0].message.contains("factor(gdp)"), "{}", d[0].message);
    }

    #[test]
    fn facet_by_a_date_column_is_illegal_with_direction() {
        let mut m = data();
        let df = m.get_mut("t").unwrap();
        *df = df.clone().with_time("when", vec![0.0, 86400.0, 172800.0], crate::time::TimeUnit::Day);
        let spec = base().layer(Layer::new(Mark::Point)).facet_col("when");
        let d = check(&spec, &m);
        assert_eq!(kinds(&d), [DiagnosticKind::Illegal]);
        assert!(d[0].message.contains("period"), "{}", d[0].message);
    }

    #[test]
    fn facet_by_a_missing_column_is_illegal() {
        let spec = base().layer(Layer::new(Mark::Point)).facet_row("contnent");
        let d = check(&spec, &data());
        assert_eq!(kinds(&d), [DiagnosticKind::Illegal]);
        assert!(d[0].message.contains("spelling"), "{}", d[0].message);
    }

    #[test]
    fn the_same_column_on_both_facet_axes_is_illegal() {
        let spec = base()
            .layer(Layer::new(Mark::Point))
            .facet_col("continent")
            .facet_row("continent");
        let d = check(&spec, &data());
        assert_eq!(kinds(&d), [DiagnosticKind::Illegal]);
    }

    /// `wrap` shapes a *line* of panels, so it needs a line. A crossing has
    /// already fixed the rectangle with two columns, and a count beside it could
    /// only be a second, contradicting statement of the same shape.
    #[test]
    fn wrapping_a_crossed_facet_is_illegal_with_direction() {
        let spec = base()
            .layer(Layer::new(Mark::Point))
            .facet_col("continent")
            .facet_row("country")
            .facet_wrap(3);
        let d = check(&spec, &data());
        assert_eq!(kinds(&d), [DiagnosticKind::Illegal]);
        assert!(d[0].message.contains("Drop `wrap`"), "{}", d[0].message);
    }

    #[test]
    fn wrapping_nothing_is_illegal_with_direction() {
        let spec = base().layer(Layer::new(Mark::Point)).facet_wrap(3);
        let d = check(&spec, &data());
        assert_eq!(kinds(&d), [DiagnosticKind::Illegal]);
        assert!(d[0].message.contains("no \nfacet") || d[0].message.contains("no facet"),
                "{}", d[0].message);
    }

    #[test]
    fn wrapping_after_no_panels_is_illegal_with_direction() {
        let spec = base()
            .layer(Layer::new(Mark::Point))
            .facet_col("continent")
            .facet_wrap(0);
        let d = check(&spec, &data());
        assert_eq!(kinds(&d), [DiagnosticKind::Illegal]);
        assert!(d[0].message.contains("wrap = 4"), "{}", d[0].message);
    }

    // --- free scales -------------------------------------------------------

    /// The three positions draw an axis and nothing else does. `limits` reaches
    /// all six magnitude channels because each has a domain; a legend is one key
    /// for the whole plot, so a per-panel color scale would decode nothing.
    #[test]
    fn freeing_a_channel_that_is_not_a_position_is_illegal_with_direction() {
        let mut spec = base().layer(Layer::new(Mark::Point)).facet_col("continent");
        spec.channels.insert(Channel::Color,
                             ChannelDef::field("continent").with_free());
        let d = check(&spec, &data());
        assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
                             && x.message.contains("Free a position")),
                "{d:?}");
    }

    #[test]
    fn freeing_a_scale_with_no_panels_is_illegal_with_direction() {
        let mut spec = base().layer(Layer::new(Mark::Point));
        spec.y = spec.y.map(|c| c.with_free());
        let d = check(&spec, &data());
        assert!(d.iter().any(|x| x.message.contains("has one panel")), "{d:?}");
    }

    /// The argument this one has to make is longer than "you cannot", because
    /// the caller is imagining panels and has frames. §16: a frame replaces the
    /// one before it, so a scale refitted per frame moves the axis under the data.
    #[test]
    fn freeing_a_scale_over_frames_rather_than_panels_is_illegal_with_direction() {
        let mut spec = base().layer(
            Layer::new(Mark::Point).encode(Channel::Play, "continent"));
        spec.y = spec.y.map(|c| c.with_free());
        let d = check(&spec, &data());
        assert!(d.iter().any(|x| x.message.contains("motion would be the scale's")),
                "{d:?}");
    }

    /// A stated domain *is* a fixed scale, so the two together ask the axis to be
    /// two things.
    #[test]
    fn freeing_a_scale_that_states_its_domain_is_illegal_with_direction() {
        let mut spec = base().layer(Layer::new(Mark::Point)).facet_col("continent");
        spec.y = spec.y.map(|c| ChannelDef { limits: Some([Some(0.0), Some(9e9)]), ..c }
                            .with_free());
        let d = check(&spec, &data());
        assert!(d.iter().any(|x| x.message.contains("one scale per panel")), "{d:?}");
    }

    /// A count larger than the level list is not an error — it is a line that
    /// never reaches its turn, which is the unwrapped picture.
    #[test]
    fn wrapping_wider_than_the_ribbon_is_legal() {
        let spec = base()
            .layer(Layer::new(Mark::Point))
            .facet_col("continent")
            .facet_wrap(99);
        assert!(check(&spec, &data()).is_empty());
    }

    // --- play --------------------------------------------------------------

    /// The whole channel, in one assertion: legal on every mark, either column
    /// type, and drawn. `play` is the only row of the table that is identical
    /// across all thirteen marks, because a frame is a subset of the rows and
    /// there is no geometry that cannot draw a subset — so a mark missing from
    /// this loop would be the Law-1 exception, not a special case.
    #[test]
    fn play_is_legal_and_drawn_on_every_mark_without_exception() {
        for m in &ALL_MARKS {
            let r = rule_for(m, &Channel::Play);
            assert_eq!(r.obligation, Obligation::Can, "{m:?} refuses play");
            assert_eq!(r.accepts, VarType::Either, "{m:?} narrows what play accepts");
            assert_eq!(r.renders, Some(VarType::Either), "{m:?} does not draw play");
            assert!(!r.settable, "{m:?}: a moment is not something to set");
        }
    }

    /// A number is welcome where `facet` refuses one, and the reason is the cost
    /// function rather than the type: panels compete for page area, frames for
    /// time. This is the pair of `facet_by_a_number_column_is_illegal`.
    #[test]
    fn play_by_a_number_column_is_legal_where_facet_refuses_one() {
        let spec = base()
            .layer(Layer::new(Mark::Point))
            .channel(Channel::Play, "gdp");
        assert!(check(&spec, &data()).is_empty());
    }

    #[test]
    fn play_by_a_category_column_is_legal_too() {
        let spec = base()
            .layer(Layer::new(Mark::Point))
            .channel(Channel::Play, "continent");
        assert!(check(&spec, &data()).is_empty());
    }

    /// One column names the panels or names the moments, never both — drawn, it
    /// would leave every panel but one empty in every frame.
    #[test]
    fn one_column_cannot_name_both_the_panels_and_the_moments() {
        let spec = base()
            .layer(Layer::new(Mark::Point))
            .channel(Channel::Play, "continent")
            .facet_col("continent");
        let d = check(&spec, &data());
        assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
            && x.message.contains("name the frames")), "{d:?}");
    }

    /// A long sequence is legal — Law 8 does not forbid the ugly — but the loop
    /// length is a default the caller did not choose, so §12 says it out loud.
    #[test]
    fn a_long_sequence_says_how_long_it_will_loop_for() {
        let years: Vec<f64> = (0..40).map(|i| 1950.0 + i as f64).collect();
        let n = years.len();
        let df = DataFrame::new()
            .with_float("gdp", vec![1.0; n])
            .with_float("life", vec![2.0; n])
            .with_float("year", years);
        let mut m = HashMap::new();
        m.insert("t".to_string(), df);
        let spec = base().layer(Layer::new(Mark::Point)).channel(Channel::Play, "year");
        let d = check(&spec, &m);
        assert_eq!(kinds(&d), [DiagnosticKind::Assumption], "it draws, and it remarks");
        assert!(d[0].message.contains("40 frames"), "{}", d[0].message);
        assert!(d[0].message.contains("speed"), "and gives the direction: {}", d[0].message);
    }

    /// A short one says nothing: §12's rule is that an unambiguous default is
    /// used silently, and a decade at the normal pace is what anyone writing
    /// `play(year)` already expects.
    #[test]
    fn a_short_sequence_is_drawn_without_remark() {
        let spec = base().layer(Layer::new(Mark::Point)).channel(Channel::Play, "gdp");
        assert!(check(&spec, &data()).is_empty());
    }

    /// `speed` is the narrowest binding parameter: `limits` needs a domain,
    /// `tick_count` an axis, and this a duration — which only `play` has.
    #[test]
    fn speed_on_a_channel_that_does_not_spend_time_is_illegal() {
        let spec = base().layer(
            Layer::new(Mark::Point)
                .encode_def(Channel::Color, ChannelDef::field("continent").with_speed(2.0)),
        );
        let d = check(&spec, &data());
        assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
            && x.message.contains("no pace to set")), "{d:?}");
    }

    #[test]
    fn a_speed_of_zero_or_less_is_not_a_pace() {
        for bad in [0.0, -2.0] {
            let spec = base().layer(
                Layer::new(Mark::Point)
                    .encode_def(Channel::Play, ChannelDef::field("gdp").with_speed(bad)),
            );
            let d = check(&spec, &data());
            assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal),
                "speed = {bad} should be refused: {d:?}");
        }
    }

    #[test]
    fn a_positive_speed_on_play_is_accepted() {
        let spec = base().layer(
            Layer::new(Mark::Point)
                .encode_def(Channel::Play, ChannelDef::field("gdp").with_speed(2.0)),
        );
        assert!(check(&spec, &data()).is_empty());
    }

    /// The reader-found defect, and the one this whole section nearly shipped:
    /// `line + x(year) + play(year)` drew axes, a legend and a frame strip
    /// around an **empty panel**, exit 0. Cut the plot by the column that
    /// supplies `x` and every frame holds one position, so a function has
    /// nothing to read between.
    #[test]
    fn a_domain_reading_mark_cannot_be_cut_by_the_column_that_supplies_its_x() {
        for mark in [Mark::Line, Mark::Step, Mark::Area, Mark::Ribbon] {
            let spec = PlotSpec::new().data("t").x("gdp").y("life")
                .layer(Layer::new(mark.clone()).encode(Channel::Play, "gdp"));
            let d = check(&spec, &data());
            assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
                && x.message.contains("reads a function along `x`")),
                "{mark:?} should refuse: {d:?}");
        }
    }

    /// The same defect through the older door. `facet` has had it since
    /// faceting shipped, and catching only the door `play` opened would be the
    /// per-feature exception Law 1 exists to catch — the two are one partition
    /// asked twice (§11).
    #[test]
    fn the_same_is_true_of_a_facet_on_the_column_that_supplies_x() {
        for spec in [
            base().layer(Layer::new(Mark::Line)).facet_col("gdp"),
            base().layer(Layer::new(Mark::Line)).facet_row("gdp"),
        ] {
            let d = check(&spec, &data());
            assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
                && x.message.contains("reads a function along `x`")), "{d:?}");
        }
    }

    /// A mark that places each row on its own has no domain to lose, so the same
    /// sentence is legal — Law 8 forbids nothing that can be drawn. `path` is
    /// here too: it connects in the *table's* order and never promised a domain.
    #[test]
    fn a_mark_that_needs_no_domain_may_be_cut_by_its_own_x() {
        for mark in [Mark::Point, Mark::Bar, Mark::Path] {
            let spec = PlotSpec::new().data("t").x("gdp").y("life")
                .layer(Layer::new(mark.clone()).encode(Channel::Play, "gdp"));
            let d = check(&spec, &data());
            assert!(!d.iter().any(|x| x.message.contains("reads a function along `x`")),
                "{mark:?} has no domain to lose: {d:?}");
        }
    }

    /// And the ordinary sentence stays legal: it is the *same column* on both
    /// that is refused, not `play` beside a position.
    #[test]
    fn play_on_a_different_column_from_x_is_untouched() {
        let spec = base()
            .layer(Layer::new(Mark::Line))
            .channel(Channel::Play, "continent");
        assert!(check(&spec, &data()).is_empty());
    }

    /// `play` measures nothing, so it runs along no scale — the same answer
    /// `shape` and `group` get, and it needed no new code to be true.
    #[test]
    fn play_reads_no_scale() {
        assert!(!reads_a_scale(&Channel::Play));
        let spec = base().layer(
            Layer::new(Mark::Point)
                .encode_def(Channel::Play, ChannelDef::field("gdp").with_scale(ScaleType::Log)),
        );
        let d = check(&spec, &data());
        assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal), "{d:?}");
    }

    #[test]
    fn a_layer_whose_table_lacks_the_facet_column_is_an_assumption() {
        // A second table with no `continent` column: its layer is drawn in
        // every panel, and that chosen default is said out loud, non-fatally.
        let mut m = data();
        m.insert(
            "ref".to_string(),
            DataFrame::new()
                .with_float("gdp", vec![1.0, 2.0])
                .with_float("life", vec![5.0, 5.0]),
        );
        let mut line = Layer::new(Mark::Line);
        line.data = Some("ref".to_string());
        let spec = base()
            .layer(Layer::new(Mark::Point))
            .layer(line)
            .facet_col("continent");
        let d = check(&spec, &m);
        assert_eq!(kinds(&d), [DiagnosticKind::Assumption]);
        assert!(d[0].message.contains("every panel"), "{}", d[0].message);
    }

    /// Every atom a refusal **names** must be one a reader can actually write.
    ///
    /// `book/check_refusals.R` proves a documented refusal still refuses. Nothing
    /// proved anything about the sentence it points *at* — and a refusal's direction
    /// is the half of §12 that does the teaching. A direction naming a word the
    /// kernel does not have is the `total`-transform prose bug
    /// (`book/check_vocabulary.R`) one layer down: in the engine's own text, where
    /// no book check can reach it, because a message is a Rust string and not a
    /// chunk.
    ///
    /// **What this cannot do, stated so nobody reads it as more than it is.** It
    /// checks a direction's *vocabulary*, never its *quality*. Whether the plot a
    /// message names is better than the plot it declines is a judgment no test
    /// makes: `surface * bin` spent months directing readers to `bar * bin +
    /// space()`, whose walls occlude the relief the sheet exists to show, with every
    /// check in this file green (spec §15). That gap is open by construction.
    #[test]
    fn every_atom_named_in_a_refusal_is_one_the_kernel_has() {
        // The kernel's vocabulary, generated from the four lists the book's grids
        // and `rules_matrix` are generated from — so a new atom joins this check by
        // *existing* rather than by being remembered here.
        let mut known: std::collections::HashSet<String> = ALL_MARKS.iter()
            .map(|m| mark_name(m).to_string())
            .chain(ALL_CHANNELS.iter().map(|c| channel_name(c).to_string()))
            .chain(USER_TRANSFORMS.iter().map(|t| transform_name(t).to_string()))
            .chain(ALL_SETTINGS.iter().map(|s| setting_name(*s).to_string()))
            .collect();
        // The plot-level words. These are the **binding** surface rather than the
        // kernel — coordinate spaces, the table, the guides, the page — and the
        // engine has no list of them to generate from, since it receives them as
        // fields on `PlotSpec` rather than as atoms. Hand-written on purpose and
        // deliberately short: if it grows past a glance it wants generating, which
        // is `CONTRIBUTING.md`'s rule about a hand-written list beside a generated
        // one, and this is the list that would lose.
        known.extend(["space", "polar", "nest", "flat", "globe", "map",
                      "data", "facet", "style", "palette", "theme", "title",
                      "x_label", "y_label", "z_label", "render_svg"]
                     .iter().map(|s| s.to_string()));

        // Every word used as an **atom** in a fragment: at paren depth 0, so the
        // arguments are skipped. A word inside `(...)` is a column name, a value or
        // a parameter (`x(life)`, `bin(width = 5)`, `style(color = "red")`), none of
        // which this check has any business ruling on.
        let atoms_in = |frag: &str| -> Vec<String> {
            let (b, mut out, mut depth, mut i) = (frag.as_bytes(), Vec::new(), 0i32, 0usize);
            while i < b.len() {
                match b[i] {
                    b'(' => { depth += 1; i += 1 }
                    b')' => { depth -= 1; i += 1 }
                    c if (c.is_ascii_lowercase() || c == b'_') && depth == 0 => {
                        let s = i;
                        while i < b.len()
                            && (b[i].is_ascii_lowercase() || b[i] == b'_' || b[i].is_ascii_digit())
                        { i += 1 }
                        out.push(frag[s..i].to_string());
                    }
                    _ => i += 1,
                }
            }
            out
        };

        // A *direction* is a sentence, and a sentence has an operator in it. That
        // filter is what separates `bar * bin + x(<a>)` from the bare references a
        // message also backticks (`count`, `hex`, `GOG_STRICT=0`), which name a
        // thing rather than recommend a plot.
        let sentences = |msg: &str| -> Vec<String> {
            msg.split('`').skip(1).step_by(2)
                .filter(|f| f.contains(" + ") || f.contains(" * "))
                .map(|f| f.to_string())
                .collect()
        };

        let d = data();
        let mut checked = 0usize;
        let mut bad: Vec<String> = Vec::new();
        let note = |msg: &str, ctx: &str, bad: &mut Vec<String>, checked: &mut usize| {
            for frag in sentences(msg) {
                *checked += 1;
                for word in atoms_in(&frag) {
                    if !known.contains(&word) {
                        bad.push(format!("{ctx}: `{frag}` names `{word}`, which is not an atom"));
                    }
                }
            }
        };

        // The two enumerable surfaces: every mark against every channel, and every
        // mark against every transform. Both numeric and categorical columns, since
        // which one a message refuses on decides which message it writes.
        for m in &ALL_MARKS {
            for c in &ALL_CHANNELS {
                for col in ["gdp", "continent"] {
                    let spec = PlotSpec::new().data("t").x("gdp").y("life")
                        .layer(Layer::new(m.clone()).encode(c.clone(), col));
                    let ctx = format!("{m:?}/{c:?}/{col}");
                    for diag in check(&spec, &d) {
                        note(&diag.message, &ctx, &mut bad, &mut checked);
                    }
                }
            }
            for t in USER_TRANSFORMS {
                let spec = PlotSpec::new().data("t").x("gdp").y("life")
                    .layer(Layer::new(m.clone()).transform(t.clone()));
                let ctx = format!("{m:?}*{}", transform_name(&t));
                for diag in check(&spec, &d) {
                    note(&diag.message, &ctx, &mut bad, &mut checked);
                }
            }
        }

        // 526 directed sentences at the time of writing, from ~450 refused pairs.
        // The floor is a scan-broke guard, not a target.
        assert!(checked > 400, "only {checked} directed sentences reached — the scan broke");
        assert!(bad.is_empty(), "{} bad direction(s) out of {checked}:\n{}",
                bad.len(), bad.join("\n"));
    }

    #[test]
    fn every_mark_channel_pair_has_a_rule() {
        // Guards the Law of No Exceptions: no (mark, channel) pair may be
        // undefined. If a mark or channel is added, this forces a decision.
        // One shared list with the generated grid and `rules_matrix`, so the
        // book's grid, this guard, and the JSON dump cannot disagree.
        for m in &ALL_MARKS {
            for c in &ALL_CHANNELS {
                let r = rule_for(m, c);
                // A rule may not promise to render a type it does not accept.
                if let Some(renders) = r.renders {
                    assert!(
                        r.accepts.accepts(renders) || r.accepts == VarType::Either,
                        "{:?}/{:?}: renders {:?} but only accepts {:?}",
                        m, c, renders, r.accepts
                    );
                }
            }
        }
    }

    #[test]
    fn rules_matrix_is_the_whole_table_and_invents_nothing() {
        // The `gog-cli --rules` dump the book's grid is generated from. It must
        // be `rule_for` iterated in full — every mark × every channel, no more,
        // no less — and speak only the wire vocabulary the book maps to glyphs.
        let m = rules_matrix();
        assert_eq!(m.marks.len(), ALL_MARKS.len());
        assert_eq!(m.channels.len(), ALL_CHANNELS.len());
        assert_eq!(m.cells.len(), ALL_MARKS.len() * ALL_CHANNELS.len());
        for cell in &m.cells {
            assert!(
                matches!(cell.obligation, "must" | "can" | "cannot"),
                "unknown obligation {:?}", cell.obligation
            );
            assert!(matches!(cell.accepts, "continuous" | "discrete" | "either"));
            if let Some(r) = cell.renders {
                assert!(matches!(r, "continuous" | "discrete" | "either"));
            }
        }
        // The four corners the book's five-state glyph mapping is pinned to:
        // a required position, a set-only stroke width, a reserved (grammar-legal
        // but undrawn) cell, and an absent feature. If any of these flips, the
        // grid's legend would mislabel a column of cells.
        let cell = |mk: &str, ch: &str| {
            m.cells.iter().find(|c| c.mark == mk && c.channel == ch).unwrap()
        };
        assert_eq!(cell("point", "x").obligation, "must"); // ● required
        let sz = cell("line", "size");
        assert_eq!(sz.obligation, "cannot"); // cannot map…
        assert!(sz.settable); // …but ○ set-only
        // The reserved cell was `bar`/`z` until the 3-D histogram drew it (spec
        // §5/§15), which is this guard working rather than a fixture going stale:
        // the glyph mapping had to be re-pinned to a cell that is still undrawn.
        // `line`/`z` is the one with the sharpest reason behind it — a line reads a
        // domain left to right and the cube has no left to right, so the space curve
        // is `path`'s — which makes it the right example for "grammar-legal, not
        // drawn" rather than merely the next one along.
        assert_eq!(cell("line", "z").obligation, "can"); // grammar-legal…
        assert!(cell("line", "z").renders.is_none()); // …but ◌ not drawn yet
        // And the cell that moved is pinned from the other side, so a regression
        // that quietly un-draws it fails here too.
        assert!(cell("bar", "z").renders.is_some(), "bar should draw in the cube");
        let pat = cell("point", "pattern");
        assert_eq!(pat.obligation, "cannot"); // — absent
        assert!(!pat.settable);
    }

    #[test]
    fn settings_grid_agrees_with_the_style_checks() {
        // `mark_takes_setting` is the one source the generated Mark × Setting grid
        // reads. For the five style-only settings (the channel-backed five come
        // straight from `rule_for(_).settable`, which `check_style` gates on
        // identically) it is written separately from the `check_*` functions that
        // enforce them, so the two are pinned together here: the grid can never
        // promise a setting the engine refuses, nor hide one it allows.
        let border = || StyleSpec { border_color: Some("black".into()), border_size: Some(1.0), ..Default::default() };
        let caps   = || StyleSpec { caps: Some(true), ..Default::default() };
        let center = || StyleSpec { center: Some(true), ..Default::default() };
        let nudge  = || StyleSpec { nudge: Some("up".into()), ..Default::default() };
        type Check = fn(&mut Vec<Diagnostic>, &Mark, &StyleSpec);
        let cases: [(Setting, fn() -> StyleSpec, Check); 5] = [
            (Setting::BorderColor, border, check_border),
            (Setting::BorderSize,  border, check_border),
            (Setting::Caps,        caps,   check_caps),
            (Setting::Center,      center, check_center),
            (Setting::Nudge,       nudge,  check_nudge),
        ];
        for m in &ALL_MARKS {
            if !is_drawable(m) { continue; } // path/surface are refused before these run
            for &(setting, style, check) in &cases {
                let mut out = Vec::new();
                check(&mut out, m, &style());
                let engine_allows = out.is_empty();
                assert_eq!(
                    mark_takes_setting(m, setting), engine_allows,
                    "{:?}/{}: table says {}, engine allows {}",
                    m, setting_name(setting), mark_takes_setting(m, setting), engine_allows
                );
            }
        }
    }

    #[test]
    fn settings_matrix_covers_every_setting_and_mark() {
        // The `settings` block of the dump the Setting grid is generated from.
        let mtx = rules_matrix();
        assert_eq!(mtx.settings.len(), ALL_SETTINGS.len());
        assert_eq!(mtx.setting_cells.len(), ALL_SETTINGS.len() * ALL_MARKS.len());
        let sc = |s: &str, m: &str| {
            mtx.setting_cells.iter().find(|c| c.setting == s && c.mark == m).unwrap().settable
        };
        // Corners: color on every mark; size set-only on a line but pinned on a bar;
        // border on the three fills only; caps on interval alone; nudge on text alone.
        assert!(sc("color", "point") && sc("color", "ribbon"));
        assert!(sc("size", "line") && !sc("size", "bar"));
        assert!(sc("border_color", "bar") && sc("border_color", "box") && sc("border_color", "point"));
        assert!(!sc("border_color", "line") && !sc("border_color", "area"));
        assert!(sc("caps", "interval") && !sc("caps", "point"));
        assert!(sc("nudge", "text") && !sc("nudge", "line"));
    }

    #[test]
    fn dodge_needs_a_width_bearing_mark_and_refuses_the_rest_with_direction() {
        // A collision modifier combines with the marks whose geometry it was
        // defined for (Law 1 read correctly). `dodge` subdivides a *width*, so a
        // widthless or continuous mark is refused — with the offset that *does*
        // fit named in the message.
        let split = |m: Mark, x: &str| {
            PlotSpec::new().data("t").x(x).y("life")
                .layer(Layer::new(m).transform(Transform::Dodge).encode(Channel::Color, "continent"))
        };

        // A point has no width — the refusal points at jitter.
        let d = check(&split(Mark::Point, "continent"), &data());
        assert!(
            d.iter().any(|x| x.kind == DiagnosticKind::Illegal && x.message.contains("jitter")),
            "point * dodge should be refused toward jitter: {:?}",
            d.iter().map(|x| &x.message).collect::<Vec<_>>()
        );

        // A connected path is offset by accumulating — the refusal points at stack.
        for m in [Mark::Line, Mark::Area] {
            let d = check(&split(m.clone(), "gdp"), &data());
            assert!(
                d.iter().any(|x| x.kind == DiagnosticKind::Illegal && x.message.contains("stack")),
                "{m:?} * dodge should be refused toward stack: {:?}",
                d.iter().map(|x| &x.message).collect::<Vec<_>>()
            );
        }

        // A filled band has no width to subdivide, and no baseline to stack — its
        // honest overlap answer is transparency, so the refusal points at opacity.
        let d = check(&split(Mark::Ribbon, "gdp"), &data());
        assert!(
            d.iter().any(|x| x.kind == DiagnosticKind::Illegal && x.message.contains("opacity")),
            "ribbon * dodge should be refused toward opacity: {:?}",
            d.iter().map(|x| &x.message).collect::<Vec<_>>()
        );
    }

    /// **Two reductions in one layer are refused, on every mark and for every pair.**
    ///
    /// Filed 2026-07-30 as a silent drop and fixed the same day. `bar * sum * mean`
    /// used to render byte-identical to `bar * sum`, and `bar * mean * sum`
    /// byte-identical to `bar * mean`: `transform::reduces_column` is a `find_map`, so
    /// whichever was written first won and the other vanished with no warning and no
    /// refusal. That is the silent drop §12 forbids, and it also made false the rule
    /// the book states in three places — that the order of transforms never matters.
    ///
    /// Every pair on every mark is named rather than a sample, because the defect was
    /// uniform across all of them and a sample is what missed it the first time: the
    /// sweep that reported "no counterexamples" had used a *continuous* `x`, where
    /// nothing reduces per category and both orders therefore agreed. The fixture
    /// here is categorical for that reason.
    #[test]
    fn two_reductions_in_one_layer_are_refused() {
        let family = [Transform::Sum, Transform::Mean, Transform::Median,
                      Transform::Max, Transform::Min];
        let marks = [Mark::Bar, Mark::Point, Mark::Line, Mark::Area];
        let cat = || PlotSpec::new().data("t").x("continent").y("life");

        let mut pairs = 0;
        for m in &marks {
            for (i, a) in family.iter().enumerate() {
                for b in family.iter().skip(i + 1) {
                    for (first, second) in [(a, b), (b, a)] {
                        let d = check(
                            &cat().layer(Layer::new(m.clone())
                                .transform(first.clone())
                                .transform(second.clone())),
                            &data(),
                        );
                        assert!(
                            d.iter().any(|x| x.kind == DiagnosticKind::Illegal
                                && x.message.contains("measures each cell twice")),
                            "{m:?} * {first:?} * {second:?} must refuse, not silently keep one: {:?}",
                            d.iter().map(|x| &x.message).collect::<Vec<_>>()
                        );
                        pairs += 1;
                    }
                }
            }
        }
        assert_eq!(pairs, 80, "10 pairs, both orders, four marks");

        // And one reduction on its own is untouched — the fix must not widen into the
        // ordinary summary, which is most of what the aggregation family is for.
        for m in &marks {
            for t in &family {
                let d = check(&cat().layer(Layer::new(m.clone()).transform(t.clone())), &data());
                assert!(
                    !d.iter().any(|x| x.message.contains("measures each cell twice")),
                    "{m:?} * {t:?} alone must stay legal: {:?}",
                    d.iter().map(|x| &x.message).collect::<Vec<_>>()
                );
            }
        }
    }

    #[test]
    fn dodge_needs_a_split_but_then_composes_cleanly() {
        let cat = || PlotSpec::new().data("t").x("continent").y("life");

        // No color/group: nothing to set beside anything — refused toward adding it.
        let d = check(&cat().layer(Layer::new(Mark::Bar).transform(Transform::Dodge)), &data());
        assert!(
            d.iter().any(|x| x.kind == DiagnosticKind::Illegal && x.message.contains("color")),
            "bar * dodge with no split should ask for color(): {:?}",
            d.iter().map(|x| &x.message).collect::<Vec<_>>()
        );

        // With a split, dodge composes with a box's *own* summary and stays silent —
        // it is a position modifier, orthogonal to the statistic `box` carries.
        let d = check(
            &cat().layer(Layer::new(Mark::Box).transform(Transform::Dodge).encode(Channel::Color, "continent")),
            &data(),
        );
        assert!(d.is_empty(), "box * dodge + color should be legal: {:?}",
            d.iter().map(|x| &x.message).collect::<Vec<_>>());

        // dodge does not satisfy interval's range requirement — a separate law.
        let d = check(
            &cat().layer(Layer::new(Mark::Interval).transform(Transform::Dodge).encode(Channel::Color, "continent")),
            &data(),
        );
        assert!(
            d.iter().any(|x| x.kind == DiagnosticKind::Illegal && x.message.contains("range")),
            "interval * dodge without a range transform still needs range"
        );

        // interval * range * dodge + color is the whole, legal expression.
        let d = check(
            &cat().layer(Layer::new(Mark::Interval).transform(Transform::Range).transform(Transform::Dodge).encode(Channel::Color, "continent")),
            &data(),
        );
        assert!(d.is_empty(), "interval * range * dodge + color should be legal: {:?}",
            d.iter().map(|x| &x.message).collect::<Vec<_>>());
    }

    #[test]
    fn stack_needs_an_accumulating_mark_and_refuses_the_rest_with_direction() {
        // `stack`'s geometry is accumulation, so it is legal on the marks that can
        // accumulate — bar/area by length, `point` by piling dots — and every other
        // mark is refused with the offset that *does* fit named, the mirror of the
        // dodge division.
        let split = |m: Mark, x: &str, extra: &[Transform]| {
            let mut layer = Layer::new(m);
            for t in extra { layer = layer.transform(t.clone()); }
            layer = layer.transform(Transform::Stack).encode(Channel::Color, "continent");
            PlotSpec::new().data("t").x(x).y("life").layer(layer)
        };
        let msgs = |d: &[Diagnostic]| d.iter().map(|x| x.message.clone()).collect::<Vec<_>>();

        // An unfilled path has nothing to fill and pile — points at area.
        for m in [Mark::Line, Mark::Step] {
            let d = check(&split(m.clone(), "gdp", &[]), &data());
            assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal && x.message.contains("area")),
                "{m:?} * stack should be refused toward area: {:?}", msgs(&d));
        }

        // A width-bearing mark subdivides its slot instead — points at dodge.
        let d = check(&split(Mark::Box, "continent", &[]), &data());
        assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal && x.message.contains("dodge")),
            "box * stack should be refused toward dodge: {:?}", msgs(&d));
        let d = check(&split(Mark::Interval, "continent", &[Transform::Range]), &data());
        assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal && x.message.contains("dodge")),
            "interval * range * stack should be refused toward dodge: {:?}", msgs(&d));

        // A ribbon already spans a low to a high — no baseline height to pile —
        // so it is refused toward transparency, not toward another offset.
        let d = check(&split(Mark::Ribbon, "gdp", &[Transform::Range]), &data());
        assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal && x.message.contains("opacity")),
            "ribbon * range * stack should be refused toward opacity: {:?}", msgs(&d));
    }

    #[test]
    fn the_dot_plot_needs_a_tally_to_pile_and_needs_no_split() {
        // `point * stack` is the dot plot (spec §5): one dot per observation, piled
        // along the measure axis. A point has no height, so what it accumulates is
        // rows — which makes its second condition a *counting transform* where
        // bar/area want a `color` split, and it needs no split at all.
        let msgs = |d: &[Diagnostic]| d.iter().map(|x| x.message.clone()).collect::<Vec<_>>();
        let dot = |extra: &[Transform]| {
            let mut layer = Layer::new(Mark::Point);
            for t in extra { layer = layer.transform(t.clone()); }
            PlotSpec::new().data("t").x("gdp").layer(layer.transform(Transform::Stack))
        };

        // The two sentences that draw one: a continuous axis cut into intervals, and
        // a categorical one tallied. Neither names a color.
        let d = check(&dot(&[Transform::Bin]), &data());
        assert!(d.is_empty(), "`point * bin * stack` is the dot plot and needs no split: {:?}", msgs(&d));
        let cat = PlotSpec::new().data("t").x("continent")
            .layer(Layer::new(Mark::Point).transform(Transform::Count).transform(Transform::Stack));
        assert!(check(&cat, &data()).is_empty(),
            "`point * count * stack` piles dots per category: {:?}", msgs(&check(&cat, &data())));

        // Nothing counts the rows: refused toward the transform that would, naming
        // both readings rather than guessing which axis the user meant.
        let d = check(&PlotSpec::new().data("t").x("gdp").y("life")
            .layer(Layer::new(Mark::Point).transform(Transform::Stack)), &data());
        assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
            && x.message.contains("bin") && x.message.contains("count")),
            "`point * stack` alone should name bin and count: {:?}", msgs(&d));

        // A statistic that is *not* a tally is refused for the same reason: a pile of
        // 3.7 dots means nothing, and rounding it would invent data.
        for t in [Transform::Mean, Transform::Proportion, Transform::Density] {
            let d = check(&dot(&[t.clone()]), &data());
            assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal && x.message.contains("counts")),
                "`point * {t:?} * stack` should be refused toward a counting transform: {:?}", msgs(&d));
        }
    }

    #[test]
    fn stack_needs_a_split_and_retires_the_area_overlap_assumption() {
        let cat = || PlotSpec::new().data("t");
        let msgs = |d: &[Diagnostic]| d.iter().map(|x| x.message.clone()).collect::<Vec<_>>();

        // No color/group: nothing to pile — refused toward adding the split.
        let d = check(&cat().x("continent").y("life")
            .layer(Layer::new(Mark::Bar).transform(Transform::Stack)), &data());
        assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal && x.message.contains("color")),
            "bar * stack with no split should ask for color(): {:?}", msgs(&d));

        // bar * stack + color is the whole, legal expression.
        let d = check(&cat().x("continent").y("life")
            .layer(Layer::new(Mark::Bar).transform(Transform::Stack).encode(Channel::Color, "continent")), &data());
        assert!(d.is_empty(), "bar * stack + color should be legal: {:?}", msgs(&d));

        // A split area overlaps, and un-stacked that is an Assumption...
        let plain = check(&cat().x("gdp").y("life")
            .layer(Layer::new(Mark::Area).encode(Channel::Color, "continent")), &data());
        assert_eq!(kinds(&plain), [DiagnosticKind::Assumption], "a split area still warns when not stacked");

        // ...but `stack` resolves the overlap outright, so the warning is gone —
        // the forcing case this build was written against (spec §5).
        let stacked = check(&cat().x("gdp").y("life")
            .layer(Layer::new(Mark::Area).transform(Transform::Stack).encode(Channel::Color, "continent")), &data());
        assert!(stacked.is_empty(), "area * stack + color resolves the overlap and should be silent: {:?}", msgs(&stacked));
    }

    #[test]
    fn a_pile_whose_members_disagree_in_sign_is_refused_and_one_that_agrees_is_not() {
        // `stack`'s third condition, and the only one that reads the numbers. All four
        // cases are asserted together because the refusal is easy to get *too wide*:
        // "no negatives" would forbid three plots that are perfectly well formed, which
        // is Law 8 (never forbid the ugly-but-legal), and a `sum` that comes out
        // positive from mixed rows is the one a raw-column check gets wrong.
        let pile = |ts: &[Transform], key: Vec<String>, g: Vec<String>, v: Vec<f64>| {
            let mut data = HashMap::new();
            data.insert("p".to_string(), DataFrame::new()
                .with_str("q", key).with_str("kind", g).with_float("v", v));
            let mut layer = Layer::new(Mark::Bar).encode(Channel::Color, "kind");
            for t in ts { layer = layer.transform(t.clone()); }
            check(&PlotSpec::new().data("p").x("q").y("v").layer(layer), &data)
        };
        let s = |xs: &[&str]| xs.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        let msgs = |d: &[Diagnostic]| d.iter().map(|x| x.message.clone()).collect::<Vec<_>>();
        let st = [Transform::Stack];

        // Mixed at one position: refused, and the message has to carry the position and
        // both numbers, since "some values are negative" is not something a reader can
        // act on without re-reading the whole table.
        let d = pile(&st, s(&["Q1", "Q1"]), s(&["sales", "returns"]), vec![5.0, -3.0]);
        let m = d.iter().find(|x| x.kind == DiagnosticKind::Illegal)
            .map(|x| x.message.clone()).unwrap_or_default();
        assert!(m.contains("disagree in sign"), "a mixed pile is refused: {:?}", msgs(&d));
        assert!(m.contains("Q1") && m.contains("sales") && m.contains("returns")
                && m.contains('5') && m.contains("-3"),
            "the refusal names the position, both groups and both numbers: {m}");
        assert!(m.contains("dodge"), "and points a bar at the offset that keeps a baseline: {m}");

        // All-positive and all-negative are both piles that agree, and both stay legal:
        // one grows up from zero, the other down, and the arithmetic is the same.
        for v in [vec![5.0, 3.0], vec![-5.0, -3.0]] {
            let d = pile(&st, s(&["Q1", "Q1"]), s(&["a", "b"]), v.clone());
            assert!(d.is_empty(), "a pile that agrees is legal ({v:?}): {:?}", msgs(&d));
        }

        // Piles at *different* positions may point different ways — each is read on its
        // own. Refusing the column rather than the pile would lose this one.
        let d = pile(&st, s(&["Q1", "Q1", "Q2", "Q2"]), s(&["a", "b", "a", "b"]),
                     vec![5.0, 3.0, -5.0, -3.0]);
        assert!(d.is_empty(), "two piles may point opposite ways: {:?}", msgs(&d));

        // And the case that decides *where* the question is asked: raw rows of mixed
        // sign whose `sum` comes out positive in every cell. The pile is made of the
        // sums, so this is legal — a check that read the bound column instead of running
        // the transforms would refuse it, which is why `check_stack_signs` runs them.
        let d = pile(&[Transform::Sum, Transform::Stack],
                     s(&["Q1", "Q1", "Q1", "Q1"]), s(&["a", "a", "b", "b"]),
                     vec![10.0, -2.0, 8.0, -1.0]);
        assert!(d.is_empty(), "mixed rows summing to positive cells are legal: {:?}", msgs(&d));
    }

    #[test]
    fn jitter_is_point_only_and_refuses_the_rest_with_direction() {
        // The third offset's mark is `point` alone; every other mark is refused
        // toward the offset its geometry wants — the width marks at `dodge`.
        let msgs = |d: &[Diagnostic]| d.iter().map(|x| x.message.clone()).collect::<Vec<_>>();
        let jit = |m: Mark, x: &str, extra: &[Transform]| {
            let mut layer = Layer::new(m);
            for t in extra { layer = layer.transform(t.clone()); }
            PlotSpec::new().data("t").x(x).y("life").layer(layer.transform(Transform::Jitter))
        };

        // A width-bearing mark subdivides its slot — the refusal names dodge.
        for m in [Mark::Bar, Mark::Box] {
            let d = check(&jit(m.clone(), "continent", &[]), &data());
            assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal && x.message.contains("dodge")),
                "{m:?} * jitter should be refused toward dodge: {:?}", msgs(&d));
        }

        // A connected path or filled region is one shape, not a cloud of points —
        // refused, not toward an offset (none fits) but with that reason stated.
        for m in [Mark::Line, Mark::Area, Mark::Step, Mark::Ribbon] {
            let d = check(&jit(m.clone(), "gdp", &[]), &data());
            assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal && x.message.contains("not a")),
                "{m:?} * jitter should be refused as a connected shape: {:?}", msgs(&d));
        }
    }

    #[test]
    fn jitter_needs_a_categorical_band_but_no_split() {
        let msgs = |d: &[Diagnostic]| d.iter().map(|x| x.message.clone()).collect::<Vec<_>>();

        // The documented strip plot: categorical x, continuous y — legal, and note
        // it needs *no* color split, unlike dodge/stack (it spreads coincident
        // points, which needs no grouping).
        let d = check(&PlotSpec::new().data("t").x("continent").y("life")
            .layer(Layer::new(Mark::Point).transform(Transform::Jitter)), &data());
        assert!(d.is_empty(), "point * jitter + x(cat) + y(val) should be legal with no split: {:?}", msgs(&d));

        // The horizontal strip — the category on y — is equally legal.
        let d = check(&PlotSpec::new().data("t").x("life").y("continent")
            .layer(Layer::new(Mark::Point).transform(Transform::Jitter)), &data());
        assert!(d.is_empty(), "point * jitter + x(val) + y(cat) should be legal: {:?}", msgs(&d));

        // Both axes continuous: no band to spread within, and jitter must not move a
        // measured value — refused toward opacity, the honest tool.
        let d = check(&PlotSpec::new().data("t").x("gdp").y("life")
            .layer(Layer::new(Mark::Point).transform(Transform::Jitter)), &data());
        assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal && x.message.contains("opacity")),
            "point * jitter on two continuous axes should be refused toward opacity: {:?}", msgs(&d));
    }

    /// The path/region family's position rule, asserted across all four marks at
    /// once so it cannot become a per-mark answer: **the domain takes either type,
    /// the measure takes only numbers.**
    ///
    /// `x` is the axis the path is read along, and a category is a place to read a
    /// value at — the profile plot, and the radar in polar. `y` is the quantity the
    /// path traces and the region closes on; a mean of category names is not one.
    /// This is why the family's two positions do not share a row where `point`'s and
    /// `text`'s do: a glyph's axes are the same kind of thing, a path's are not.
    #[test]
    fn the_path_family_reads_a_category_on_the_domain_but_never_on_the_measure() {
        let msgs = |d: &[Diagnostic]| d.iter().map(|x| x.message.clone()).collect::<Vec<_>>();
        // `ribbon` cannot be bare — its minimum syllable includes a pair transform
        // — and `area` refuses one (it draws a value per x, not a span), so each
        // mark is asked in its own smallest legal form.
        let layer = |m: Mark| match m {
            Mark::Ribbon => Layer::new(Mark::Ribbon).transform(Transform::Range),
            other => Layer::new(other),
        };

        for m in [Mark::Line, Mark::Step, Mark::Area, Mark::Ribbon] {
            assert_eq!(rule_for(&m, &Channel::X).accepts, VarType::Either,
                "{m:?} must take a category on the domain");
            assert_eq!(rule_for(&m, &Channel::Y).accepts, VarType::Continuous,
                "{m:?} must keep its measure numeric");

            // The domain: legal, and legal for the same reason on every one of them.
            let d = check(&PlotSpec::new().data("t").x("continent").y("life")
                .layer(layer(m.clone())), &data());
            assert!(d.is_empty(), "{m:?} + x(category) should be legal: {:?}", msgs(&d));

            // The measure: refused, naming the binding and what it needs.
            let d = check(&PlotSpec::new().data("t").x("gdp").y("continent")
                .layer(layer(m.clone())), &data());
            assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
                    && x.message.contains("y(continent)")
                    && x.message.contains("continuous")),
                "{m:?} + y(category) should be refused with direction: {:?}", msgs(&d));
        }

        // And the refusals that make the relaxation safe: a transform that needs a
        // number line still says so on the domain, now that the rule table no
        // longer says it for them (`check_distribution_axis`, spec §12).
        //
        // **`density` is asked with the measure unbound, and that is the point rather
        // than a convenience.** With both positions bound it is the *violin* (spec
        // §5) — the category says which rows each estimate is made from and `y` is
        // what it spreads along — so the sentence this loop used to refuse is now a
        // plot. What is still refused is the category with nothing beside it, which
        // is the case the refusal was always really about: no number line anywhere.
        for (t, atom, measure) in [(Transform::Bin, "count", true),
                                   (Transform::Density, "proportion", false),
                                   (Transform::Smooth, "mean", true)] {
            let spec = PlotSpec::new().data("t").x("continent");
            let spec = if measure { spec.y("life") } else { spec };
            let d = check(&spec.layer(Layer::new(Mark::Line).transform(t.clone())), &data());
            assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal && x.message.contains(atom)),
                "line * {t:?} + x(category) should still be refused toward {atom}: {:?}", msgs(&d));
        }
    }

    #[test]
    fn a_ribbon_needs_a_range_transform_and_otherwise_mirrors_area() {
        let msgs = |d: &[Diagnostic]| d.iter().map(|x| x.message.clone()).collect::<Vec<_>>();

        // Its channel row *is* `area`'s — color splits, opacity is set-only, size
        // cannot be set, shape/label are refused. Asserted against `area` directly
        // so the "mirrors area" claim cannot drift as either row changes.
        for c in [Channel::X, Channel::Y, Channel::Z, Channel::Color, Channel::Size,
                  Channel::Shape, Channel::Pattern, Channel::Opacity, Channel::Group, Channel::Label, Channel::Play] {
            assert_eq!(rule_for(&Mark::Ribbon, &c).renders, rule_for(&Mark::Area, &c).renders,
                "ribbon and area must agree on {c:?}.renders");
            assert_eq!(rule_for(&Mark::Ribbon, &c).settable, rule_for(&Mark::Area, &c).settable,
                "ribbon and area must agree on {c:?}.settable");
        }

        // But where `area` stands alone (it closes on the baseline 0), a `ribbon`
        // floats between two extents like `interval`, so it needs a range transform —
        // refused with direction without one (its minimum syllable, §6/§7).
        let d = check(&base().layer(Layer::new(Mark::Ribbon)), &data());
        assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal && x.message.contains("range")),
            "a bare ribbon should be refused toward a range transform: {:?}", msgs(&d));

        // With `range` or `confidence` it is the whole legal expression.
        for t in [Transform::Range, Transform::Confidence] {
            let d = check(&base().layer(Layer::new(Mark::Ribbon).transform(t.clone())), &data());
            assert!(d.is_empty(), "ribbon * {t:?} + x + y should be legal: {:?}", msgs(&d));
        }

        // And like `area`, a categorical x is *accepted*: a band across categories
        // is the spread profile, the filled counterpart to one `interval` whisker
        // per category. This used to be refused, on the reasoning that a band has
        // no gap to fill between categories — which was true of `area` in exactly
        // the same way, so once `area` took the domain the refusal was `interval`
        // is the tidier chart, enforced as legality. Law 8 puts that judgment back
        // where it belongs, with the reader.
        let cat_x = PlotSpec::new().data("t").x("continent").y("life")
            .layer(Layer::new(Mark::Ribbon).transform(Transform::Range));
        assert!(check(&cat_x, &data()).is_empty(),
            "a ribbon on a categorical x should be legal, like an area: {:?}",
            msgs(&check(&cat_x, &data())));
    }

    #[test]
    fn bounds_supplies_a_precomputed_pair_for_the_span_marks() {
        let mut data = HashMap::new();
        data.insert("t".to_string(), DataFrame::new()
            .with_float("x",  vec![1.0, 2.0, 3.0])
            .with_float("lo", vec![1.0, 2.0, 3.0])
            .with_float("hi", vec![4.0, 5.0, 6.0]));
        let msgs = |d: &[Diagnostic]| d.iter().map(|x| x.message.clone()).collect::<Vec<_>>();
        let spec = |m: Mark| PlotSpec::new().data("t").x("x").layer(Layer::new(m).bounds("lo", "hi"));

        // `bounds` is legal on all four band marks (ribbon fills the pair, interval
        // whiskers it, line/step trace its two boundaries) with no `y()` — it
        // synthesizes the extents from the two named columns (like `count`).
        for m in [Mark::Ribbon, Mark::Interval, Mark::Line, Mark::Step] {
            let d = check(&spec(m.clone()), &data);
            assert!(d.is_empty(), "{m:?} * bounds + x should be legal: {:?}", msgs(&d));
        }

        // Any other mark draws no low/high pair, so `bounds` is refused there —
        // toward the two marks that do.
        let d = check(&spec(Mark::Bar), &data);
        assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
            && x.message.contains("interval") && x.message.contains("ribbon")),
            "bar * bounds should be refused toward the span marks: {:?}", msgs(&d));

        // It reshapes, never computes, so a named column that is not in the data is
        // refused with direction rather than silently drawing nothing.
        let bad = PlotSpec::new().data("t").x("x")
            .layer(Layer::new(Mark::Ribbon).bounds("lo", "nope"));
        assert!(check(&bad, &data).iter().any(|x| x.kind == DiagnosticKind::Illegal
            && x.message.contains("nope")),
            "a missing bounds column should be refused: {:?}", msgs(&check(&bad, &data)));
    }

    /// `bin`/`density`/`smooth` describe a spread *along* an axis, so the axis they
    /// read must carry a number — and the refusal must be **fatal**, which is the
    /// whole point of this test. The three used to warn from `transform.rs` and then
    /// hand the renderer something anyway: `bin`/`density` an empty frame (an empty
    /// panel with fabricated 0..1 axes), `smooth` its input unchanged (the raw
    /// scatter drawn as if it were the fit). `writing.qmd` documented the `bin` case
    /// as a refusal for several sessions while it quietly drew that empty panel.
    #[test]
    fn a_distributional_transform_needs_a_numeric_axis_and_refuses_fatally() {
        let msgs = |d: &[Diagnostic]| d.iter().map(|x| x.message.clone()).collect::<Vec<_>>();

        // Each of the three is refused on a categorical key axis, fatally, and points
        // at the atom that *does* answer the categorical question.
        for (t, mark, toward) in [
            (Transform::Bin, Mark::Bar, "count"),
            (Transform::Density, Mark::Bar, "proportion"),
            (Transform::Smooth, Mark::Point, "mean"),
        ] {
            let spec = PlotSpec::new().data("t").x("continent").y("life")
                .layer(Layer::new(mark.clone()).transform(t.clone()));
            let d = check(&spec, &data());
            assert!(
                d.iter().any(|x| x.kind == DiagnosticKind::Illegal
                    && x.message.contains("continent")
                    && x.message.contains(toward)),
                "{mark:?} * {t:?} on a categorical x must be Illegal and point at `{toward}`: {:?}",
                msgs(&d)
            );
        }

        // The same three on a numeric axis stay legal — the refusal must not have
        // swallowed the histogram, the density curve or the LOESS fit.
        for (t, mark) in [
            (Transform::Bin, Mark::Bar),
            (Transform::Density, Mark::Area),
            (Transform::Smooth, Mark::Point),
        ] {
            let d = check(&base().layer(Layer::new(mark.clone()).transform(t.clone())), &data());
            assert!(d.is_empty(), "{mark:?} * {t:?} on numeric axes must stay legal: {:?}", msgs(&d));
        }

        // Which axis is read is *relational*, not fixed to x: a horizontal bar cuts
        // `y`, so a categorical y is refused there — and a categorical *x* is not,
        // because on a horizontal bar x is the measure the transform writes.
        let horiz = |axis_cat: bool| {
            let s = PlotSpec::new().data("t");
            let s = if axis_cat { s.y("continent") } else { s.y("life") };
            s.layer(Layer::new(Mark::Bar).transform(Transform::Bin))
        };
        let d = check(&horiz(true), &data());
        assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal && x.message.contains("y(continent)")),
            "a horizontal `bar * bin` must refuse a categorical y, naming it: {:?}", msgs(&d));
        let d = check(&horiz(false), &data());
        assert!(d.is_empty(), "`bar * bin + y(life)` is the histogram on its side: {:?}", msgs(&d));

        // `smooth` fits y against x, so it needs *both* axes numeric — a categorical
        // y is refused even when x is fine. This is what makes it not just "check x".
        let d = check(&PlotSpec::new().data("t").x("gdp").y("continent")
            .layer(Layer::new(Mark::Point).transform(Transform::Smooth)), &data());
        assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal && x.message.contains("y(continent)")),
            "`smooth` must refuse a categorical y as well as a categorical x: {:?}", msgs(&d));
    }

    // The Mark × Transform grid (spec §5): `mark_takes_transform` is the one source
    // the generated grid reads. These pin it to the engine so the grid can never
    // promise a combination the engine refuses, nor hide one it allows — the
    // transform analog of `settings_grid_agrees_with_the_style_checks`.

    #[test]
    fn transform_grid_agrees_with_the_dedicated_checks() {
        // The four transforms with a dedicated mark-check now read `mark_takes_transform`
        // for the mark half, so they agree by construction; calling each in isolation —
        // with its *secondary* condition satisfied (a split for dodge/stack, a
        // categorical axis for jitter, existing columns for bounds) — proves the
        // consultation is wired and stays wired.
        let df = DataFrame::new()
            .with_float("gdp", vec![1.0, 2.0, 3.0])
            .with_float("life", vec![4.0, 5.0, 6.0])
            .with_float("lo", vec![1.0, 2.0, 3.0])
            .with_float("hi", vec![4.0, 5.0, 6.0])
            .with_str("continent", vec!["Asia".into(), "Europe".into(), "Africa".into()]);
        let cat_x = PlotSpec::new().data("t").x("continent").y("life");

        for m in &ALL_MARKS {
            if !is_drawable(m) {
                continue; // path/surface are refused before any transform check runs
            }
            let split = |t: Transform| {
                Layer::new(m.clone()).transform(t).encode(Channel::Color, "continent")
            };
            let expect = |t: Transform| mark_takes_transform(m, &t) != TransformLegality::None;

            let mut out = Vec::new();
            check_dodge(&mut out, &split(Transform::Dodge));
            assert_eq!(out.is_empty(), expect(Transform::Dodge), "{m:?} dodge: {out:?}");

            // `stack`'s secondary condition is not the same sentence for every mark,
            // because what a mark piles differs: bar/area pile a split's measured
            // heights, `point` piles the rows a tally counted (spec §5). Each gets
            // its own satisfied form, or the point row would fail here for the
            // secondary condition while the grid is asking about the mark.
            let mut out = Vec::new();
            let stack_layer = match m {
                Mark::Point => split(Transform::Stack).transform(Transform::Count),
                _ => split(Transform::Stack),
            };
            check_stack(&mut out, &stack_layer);
            assert_eq!(out.is_empty(), expect(Transform::Stack), "{m:?} stack: {out:?}");

            let mut out = Vec::new();
            let jl = Layer::new(m.clone()).transform(Transform::Jitter);
            check_jitter(&mut out, &cat_x, &df, &jl);
            assert_eq!(out.is_empty(), expect(Transform::Jitter), "{m:?} jitter: {out:?}");

            let mut out = Vec::new();
            check_bounds(&mut out, &df, &Layer::new(m.clone()).bounds("lo", "hi"));
            assert_eq!(out.is_empty(), expect(Transform::Bounds), "{m:?} bounds: {out:?}");
        }
    }

    #[test]
    fn transform_statistics_and_pairs_match_the_engine_off_their_marks() {
        // The value statistics and `range`/`confidence` are *not* routed through the
        // table (their refusals are emergent — `check_span_needs_range`, `check_box`,
        // the `label` requirement — plus the new gap-closing `check_pair_transform_marks`).
        // So a representative full `check()` validates the grid against the engine
        // non-circularly: a continuous x/y that every locus mark accepts, one value
        // statistic and one pair transform, across every drawable mark.
        let fatal = |d: &[Diagnostic]| d.iter().any(|x| x.is_fatal());
        for m in &ALL_MARKS {
            if !is_drawable(m) {
                continue;
            }
            for t in [Transform::Mean, Transform::Range] {
                // Each mark gets a spec it could actually draw, the way the `stack`
                // row above gives `point` its own satisfied form — otherwise this
                // asks about the *positions* while the grid is answering about the
                // mark. A `zone` measures by color over a pair of categorical
                // slots, so that is what a `zone * mean` has to be handed (spec §5);
                // every other mark reads a continuous x/y.
                let spec = match m {
                    Mark::Zone => PlotSpec::new().data("t").x("continent").y("region")
                        .layer(Layer::new(m.clone()).transform(t.clone())
                            .encode(Channel::Color, "life")),
                    // A surface has no flat form *and* no single-transform form: it
                    // measures with `z`, so a reduction needs cells to reduce into, and
                    // only `bin` cuts a surface any (spec §15). So the spec it "could
                    // actually draw" is the terraced sheet — the same accommodation the
                    // `zone` above gets for needing two categorical slots. `bin` rides
                    // along for `range` too, where it changes nothing: that refusal is
                    // the pair transform's, and the grid still says `None`.
                    Mark::Surface => PlotSpec::new().data("t").x("gdp").y("life").z("value")
                        .layer(Layer::new(m.clone())
                            .transform(Transform::Bin).transform(t.clone())),
                    _ => PlotSpec::new().data("t").x("gdp").y("life")
                        .layer(Layer::new(m.clone()).transform(t.clone())),
                };
                let refused = fatal(&check(&spec, &data()));
                let table_none = mark_takes_transform(m, &t) == TransformLegality::None;
                assert_eq!(
                    refused, table_none,
                    "{m:?} * {}: engine refuses={refused}, grid says none={table_none}: {:?}",
                    transform_name(&t),
                    check(&spec, &data()).iter().map(|x| x.message.clone()).collect::<Vec<_>>(),
                );
            }
        }
    }

    #[test]
    fn a_pair_transform_is_refused_on_a_locus_mark_toward_the_span_mark() {
        // The Law-1 gap the grid surfaced: `range`/`confidence` draw a span, so on a
        // mark that draws one value per x they are refused — `point`/`bar` toward the
        // whisker/band marks, `area` toward `ribbon` (its filled twin). Previously
        // these rendered nonsense (two bars per group) with no word.
        let msgs = |d: &[Diagnostic]| d.iter().map(|x| x.message.clone()).collect::<Vec<_>>();
        for m in [Mark::Point, Mark::Bar] {
            let d = check(&PlotSpec::new().data("t").x("gdp").y("life")
                .layer(Layer::new(m.clone()).transform(Transform::Range)), &data());
            assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
                && (x.message.contains("interval") || x.message.contains("ribbon"))),
                "{m:?} * range should point at the span marks: {:?}", msgs(&d));
        }
        let d = check(&PlotSpec::new().data("t").x("gdp").y("life")
            .layer(Layer::new(Mark::Area).transform(Transform::Confidence)), &data());
        assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal && x.message.contains("ribbon")),
            "area * confidence should point at ribbon: {:?}", msgs(&d));
    }

    /// **The published chain grid says `—` for exactly the pairs the engine refuses.**
    ///
    /// The book generates its Transform × Transform grid from `chain_cells`, and a
    /// generated grid is only trustworthy while what it is generated *from* is the
    /// same thing the caller meets. It was not, for a few hours on 2026-07-31: the
    /// cells came from `job_conflict` alone, so the grid marked `proportion * smooth`
    /// legal while `check_share_composition` refused it — the same over-promise a
    /// hand-written legend once put on the Mark × Space column, arriving through the
    /// generator instead.
    ///
    /// Two rules reach a pair, so the grid has to read both. This walks every ordered
    /// pair against `check` itself rather than against either predicate, which is what
    /// makes it catch a *third* rule if one is ever added in a third place.
    #[test]
    fn the_published_chain_grid_matches_what_the_engine_refuses() {
        let mtx = rules_matrix();
        // A mark that takes as much as possible, so a pair is judged on the pair
        // rather than on the mark: `bar` takes 12 of the 17, `line` the pair
        // transforms `bar` does not. Between them they cover every transform that
        // composes at all.
        for (mark, x, y) in [(Mark::Bar, "cat", "life"), (Mark::Line, "gdp", "life")] {
            for cell in &mtx.chain_cells {
                let (Some(a), Some(b)) = (
                    USER_TRANSFORMS.iter().find(|t| transform_name(t) == cell.a),
                    USER_TRANSFORMS.iter().find(|t| transform_name(t) == cell.b),
                ) else { continue };
                // Only pairs this mark actually takes can say anything about the pair.
                if mark_takes_transform(&mark, a) == TransformLegality::None
                    || mark_takes_transform(&mark, b) == TransformLegality::None { continue }
                let layer = Layer::new(mark.clone())
                    .transform(a.clone()).transform(b.clone()).bounds("lo", "hi");
                let d = check(&PlotSpec::new().data("t").x(x).y(y).layer(layer), &data());
                let refused = d.iter().any(|x| x.kind == DiagnosticKind::Illegal);
                if !cell.legal {
                    assert!(refused,
                        "the grid says `{} * {}` is refused, and the engine drew it",
                        cell.a, cell.b);
                }
            }
        }
        // And the reasons the grid can give are the two rules that exist.
        for cell in &mtx.chain_cells {
            if let Some(j) = cell.job {
                assert!(["extent", "measure", "scale", "position", "share"].contains(&j),
                    "`{} * {}` reports an unknown reason `{j}`", cell.a, cell.b);
            }
        }
    }

    #[test]
    fn transforms_matrix_covers_every_transform_and_mark() {
        // The `transforms` block of the dump the grid is generated from.
        let mtx = rules_matrix();
        assert_eq!(mtx.transforms.len(), USER_TRANSFORMS.len());
        assert_eq!(mtx.transform_cells.len(), USER_TRANSFORMS.len() * ALL_MARKS.len());
        let tc = |t: &str, m: &str| {
            mtx.transform_cells.iter().find(|c| c.transform == t && c.mark == m).unwrap().state
        };
        let cls = |t: &str| mtx.transforms.iter().find(|i| i.name == t).unwrap().class;
        // The three classes are labeled so the book can group the columns.
        assert_eq!(cls("bin"), "statistic");
        assert_eq!(cls("range"), "pair");
        assert_eq!(cls("dodge"), "collision");
        // Corners: a value statistic on a locus mark vs a span mark; the pair
        // transform required on interval/ribbon, optional on line, and the closed gap
        // on bar; and the collision trio's partition — which divides by geometry *and
        // axis*, so `point` appears under two of the three (`stack` for the dot
        // plot's measure axis, `jitter` for the strip plot's categorical one).
        assert_eq!(tc("bin", "bar"), "combines");
        assert_eq!(tc("bin", "interval"), "none");
        assert_eq!(tc("range", "interval"), "required");
        assert_eq!(tc("range", "ribbon"), "required");
        assert_eq!(tc("range", "line"), "combines");
        assert_eq!(tc("range", "bar"), "none");
        assert_eq!(tc("dodge", "bar"), "combines");
        assert_eq!(tc("dodge", "point"), "none");
        assert_eq!(tc("stack", "area"), "combines");
        assert_eq!(tc("stack", "point"), "combines");
        assert_eq!(tc("stack", "box"), "none");
        assert_eq!(tc("jitter", "point"), "combines");
        assert_eq!(tc("jitter", "bar"), "none");
    }

    // The Mark × Space grid — the fourth face of the orthogonality matrix. Same
    // two guards its three siblings have: the dump covers every pair, and the
    // table the grid prints is the one the refusals enforce.

    #[test]
    fn spaces_matrix_covers_every_space_and_mark() {
        let mtx = rules_matrix();
        assert_eq!(mtx.spaces.len(), ALL_SPACES.len());
        assert_eq!(mtx.space_cells.len(), ALL_SPACES.len() * ALL_MARKS.len());
        let sc = |s: &str, m: &str| {
            mtx.space_cells.iter().find(|c| c.space == s && c.mark == m).unwrap().drawn
        };
        // Corners the book's glyph mapping relies on: every drawable mark in the
        // plane; only the scatter in space; the two designed-but-unbuilt spaces
        // empty throughout.
        assert!(sc("flat", "bar") && sc("flat", "box") && sc("flat", "ribbon"));
        assert!(!sc("flat", "surface"), "a sheet has no reading in the plane");
        assert!(sc("space", "point"));
        assert!(!sc("space", "line"));
        // `path` bends with the rest — its segments are strokes between placed
        // vertices, which is `line`'s geometry, so the spiral needs no arc work.
        assert!(sc("flat", "path") && sc("polar", "path"));
        // **Polar's column has exactly one blank, and it is not a polar gap.**
        // Every mark that draws flat draws bent, since 2026-07-26; `surface` is
        // absent from both for one reason, its minimum syllable including the cube.
        // Written as the whole column rather than as a list of corners, because a
        // list is what went stale twice while `path` and then `rule` learned to
        // bend — the same hand-written-list drift `check_polar`'s message fixed.
        for m in &ALL_MARKS {
            assert_eq!(
                sc("polar", mark_name(m)),
                sc("flat", mark_name(m)),
                "{m:?}: the plane and the circle must agree about which marks draw"
            );
        }
        for m in &ALL_MARKS {
            assert!(!sc("globe", mark_name(m)) && !sc("map", mark_name(m)),
                "{m:?}: globe/map have no renderer, so no cell may claim one");
        }
    }

    /// **A blank `z` cell is one of three different things**, and until 2026-07-26
    /// every one of them said "not drawn yet" — including the four the spec had
    /// argued *against* since M8a. A message that promises a decided refusal is
    /// coming is the same defect as a book chunk claiming a refusal that stopped
    /// happening, one layer down.
    #[test]
    fn a_blank_z_cell_says_which_of_three_things_it_is() {
        // Decided: the path/region family reads a domain, and a cube has no left to
        // right. `Illegal`, and the direction is `path` — `line` with the sort removed.
        for m in [Mark::Line, Mark::Step, Mark::Area, Mark::Ribbon] {
            assert_eq!(z_refusal_kind(&m), DiagnosticKind::Illegal, "{m:?} is a ruling, not a gap");
            let msg = z_refusal(&m, "h");
            assert!(msg.contains("no left to right"), "{m:?}: {msg}");
            assert!(msg.contains("`path`"), "{m:?} must point at the mark that does draw: {msg}");
            assert!(!msg.contains("not drawn yet") && !msg.contains("does not draw it yet"),
                "{m:?} is refused by the grammar, so it must not promise a renderer: {msg}");
        }
        // Blocked on occlusion: both are marks whose 3-D form has no footprint, so
        // painter's order cannot place them. `Unsupported` — owed, not refused.
        for m in [Mark::Rule, Mark::Zone] {
            assert_eq!(z_refusal_kind(&m), DiagnosticKind::Unsupported);
            let msg = z_refusal(&m, "h");
            assert!(msg.contains("footprint"), "{m:?} must name why it cannot be sorted: {msg}");
            // The blocker is named in plain words rather than as "occlusion", which is
            // rendering jargon this audience does not have (book law 8), and it no longer
            // cites a milestone id a reader cannot look up. Assert the meaning, not the term.
            assert!(msg.contains("what hides what"), "{m:?} must name the blocker: {msg}");
            assert!(!msg.contains("occlusion") && !msg.contains("M8a"),
                "{m:?}: a refusal a user reads must not use rendering jargon or an internal \
                 milestone id: {msg}");
        }
        // And the marks that draw take no message at all.
        for m in [Mark::Point, Mark::Bar, Mark::Path, Mark::Surface, Mark::Box, Mark::Interval] {
            assert!(rule_for(&m, &Channel::Z).renders.is_some(), "{m:?} should draw in the cube");
        }
    }

    /// **The three slot marks stand or fall together**, which is what `is_slot_mark`
    /// asserts and what `slot_orient` has said in prose since orientation was
    /// decided: a bar's length, a whisker's span and a box's summary are the same
    /// question asked of the same pair of axes. A cube that took one and not the
    /// others would be the Law-1 gap the family exists to prevent.
    #[test]
    fn the_three_slot_marks_all_stand_in_the_cube() {
        for m in ALL_MARKS.iter().filter(|m| is_slot_mark(m)) {
            assert!(mark_draws_in_space(m, SpaceKind::Space),
                "{m:?} is a slot mark and must stand on the cube's floor");
            // And the dimensionality rule must agree, or `check_slot_shape` would
            // still demand a measured axis on a floor of two categories.
            assert!(cuts_both_positions(m, SpaceKind::Space),
                "{m:?} must cut the floor in the cube");
            assert_eq!(measure_channel(m, SpaceKind::Space), Some(Channel::Z),
                "{m:?} measures along `z` in the cube");
        }
    }

    /// **A pair is a reduction too.** `reads_two_dimensions` asked only about the
    /// five single-value statistics, so a whisker in the cube fell through to the
    /// one-key branch and drew one per *row* rather than one per cell.
    #[test]
    fn a_pair_transform_is_a_two_dimensional_reading_in_the_cube() {
        for t in [Transform::Range, Transform::Confidence, Transform::Box] {
            assert!(crate::transform::pairs_a_column(&[t.clone()]), "{t:?} pairs a column");
            assert!(reads_two_dimensions(&Mark::Interval, &[t.clone()], SpaceKind::Space),
                "{t:?} on a whisker in the cube must group by the floor");
            // Flat, the same sequence is a *one*-dimensional reading — the whole
            // point of the subtraction is that the space decides, not the transform.
            assert!(!reads_two_dimensions(&Mark::Interval, &[t.clone()], SpaceKind::Flat),
                "{t:?} on a flat whisker groups by its slot, not by a pair");
        }
        // `bounds` names columns that already hold the pair, so there is nothing for
        // a cell to reduce and it is not one of them.
        assert!(!crate::transform::pairs_a_column(&[Transform::Bounds]));
    }

    /// **A hexagonal mesh has no polar reading** — `bin(tiling = )`'s third
    /// refusal, and the one that arrived with the space rather than with the
    /// tiling. The first two are a one-dimensional bin and a categorical axis;
    /// this is the third way a plane can fail to be there. `rect` must survive,
    /// because a rectangle bent is the sector the mark draws.
    #[test]
    fn a_hexagonal_mesh_has_no_polar_reading_and_a_rectangular_one_does() {
        let polar_bin = |tiling: &str| {
            let mut out = Vec::new();
            let mut layer = Layer::new(Mark::Zone).transform(Transform::Bin);
            layer.bin = Some(crate::ir::BinSpec { tiling: Some(tiling.into()), ..Default::default() });
            let spec = PlotSpec::new().data("t").x("a").y("b")
                .coord(CoordSpace::Polar(crate::ir::PolarView::default()))
                .layer(layer);
            check_polar(&mut out, &spec);
            out
        };
        let hex = polar_bin("hex");
        assert_eq!(hex.len(), 1, "a hex mesh in polar must be refused exactly once");
        assert_eq!(hex[0].kind, DiagnosticKind::Illegal);
        // Direction, never a bare refusal (§12): it must name the tiling that does
        // bend and the other way out.
        assert!(hex[0].message.contains("rect"), "no direction: {}", hex[0].message);
        assert!(hex[0].message.contains("drop `polar()`"), "no second way out: {}", hex[0].message);
        assert!(polar_bin("rect").is_empty(), "a rectangle bent is a sector, which draws");
    }

    // -- the violin: the slot reading of `density` (spec §5) -------------------

    /// `x(continent) + y(life)` — a category and a number, which is the whole
    /// condition. Both marks that draw the reading, upright and lying down.
    #[test]
    fn a_violin_is_legal_on_both_marks_and_both_ways_round() {
        for mark in [Mark::Ribbon, Mark::Area] {
            let upright = PlotSpec::new().data("t").x("continent").y("life")
                .layer(Layer::new(mark.clone()).transform(Transform::Density));
            let d = check(&upright, &data());
            assert!(d.is_empty(), "{mark:?} * density + x(cat) + y(num): {:?}", msgs(&d));

            // Sideways, with the category on `y` — the form with room for long
            // names. The static rule table says `y` on these marks is the measure,
            // and it is right in every other reading; the violin reads its
            // orientation off the bindings the way `bar`/`box`/`interval` do (§6),
            // which is why there is still no `flip` atom.
            let sideways = PlotSpec::new().data("t").x("life").y("continent")
                .layer(Layer::new(mark.clone()).transform(Transform::Density));
            let d = check(&sideways, &data());
            assert!(d.is_empty(), "{mark:?} * density + x(num) + y(cat): {:?}", msgs(&d));
        }
    }

    /// The reading is selected by the *types*, so the curve must still be refused on
    /// a `ribbon` — with direction toward the binding that would make it a violin,
    /// not toward `range`.
    #[test]
    fn a_ribbon_density_curve_is_refused_toward_the_violin() {
        let curve = PlotSpec::new().data("t").x("life")
            .layer(Layer::new(Mark::Ribbon).transform(Transform::Density));
        let d = check(&curve, &data());
        assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
                    && x.message.contains("violin")),
            "a ribbon density curve should point at the violin: {:?}", msgs(&d));
    }

    /// A category with **no measure** is not the slot reading, and must keep the
    /// refusal it has always had — the estimate has nothing to spread along.
    #[test]
    fn a_density_over_a_bare_category_is_still_refused() {
        let d = check(&PlotSpec::new().data("t").x("continent")
            .layer(Layer::new(Mark::Area).transform(Transform::Density)), &data());
        assert!(!d.is_empty(), "`area * density + x(cat)` must still be refused");
    }

    /// `compare` belongs to exactly one reading, as `bandwidth` and `levels` do —
    /// refused on the curve and the field, and its value checked by name so a typo
    /// cannot fall through to the default and answer the other question.
    #[test]
    fn compare_is_refused_in_the_readings_it_cannot_mean() {
        let with = |spec: PlotSpec, how: &str| {
            let mut s = spec;
            for l in &mut s.layers {
                l.density = Some(crate::ir::DensitySpec {
                    adjust: None, bandwidth: None, levels: None, compare: Some(how.into()), reach: None,
                });
            }
            check(&s, &data())
        };
        // The curve — one estimate with a whole axis to itself, and no slots.
        let d = with(base().layer(Layer::new(Mark::Line).transform(Transform::Density)), "count");
        assert!(d.iter().any(|x| x.message.contains("no slots")),
            "compare on a curve should be refused: {:?}", msgs(&d));
        // The field — measured by color, with no width in it at all.
        let d = with(base().layer(Layer::new(Mark::Path).transform(Transform::Density)), "count");
        assert!(d.iter().any(|x| x.message.contains("no slots")),
            "compare on a field should be refused: {:?}", msgs(&d));
        // On the violin, both words are legal and a third is not.
        let violin = || PlotSpec::new().data("t").x("continent").y("life")
            .layer(Layer::new(Mark::Ribbon).transform(Transform::Density));
        for how in [crate::ir::COMPARE_SHAPE, crate::ir::COMPARE_COUNT] {
            let d = with(violin(), how);
            assert!(d.is_empty(), "compare = {how:?} should be legal: {:?}", msgs(&d));
        }
        let d = with(violin(), "area");
        assert!(d.iter().any(|x| x.message.contains("not a reading this engine has")),
            "an unknown compare should be refused by name: {:?}", msgs(&d));
    }

    /// The two stroke marks joined the slot reading to close a **silent misdraw**,
    /// not only to add the ridgeline's edge: `line * density + x(<number>) +
    /// y(<category>)` was legal before and drew the *pooled* curve, because a bound
    /// `y` on a synthesizing transform names the output column — so the category was
    /// swallowed as a name and the axis came out reading "Continent" over density
    /// values. Pinned as a *plot* now rather than as the absence of a diagnostic,
    /// since the old behavior raised none either.
    #[test]
    fn the_stroke_marks_trace_the_slot_reading_rather_than_pooling_it() {
        for mark in [Mark::Line, Mark::Step] {
            let spec = PlotSpec::new().data("t").x("life").y("continent")
                .layer(Layer::new(mark.clone()).transform(Transform::Density));
            let d = check(&spec, &data());
            assert!(d.is_empty(), "{mark:?} * density + x(num) + y(cat): {:?}", msgs(&d));
            assert_eq!(slot_density(&spec, &spec.layers[0], data().get("t")),
                       Some(Orient::Horizontal),
                       "{mark:?} must read as the slot reading, not the pooled curve");
        }
        // The curve is still the curve when there is no category to group by.
        let curve = base().layer(Layer::new(Mark::Line).transform(Transform::Density));
        assert!(slot_density(&curve, &curve.layers[0], data().get("t")).is_none());
    }

    /// `reach` is the fourth knob to belong to one reading, and the slot's second.
    #[test]
    fn reach_is_refused_where_there_are_no_slots_to_measure_it_in() {
        let with = |spec: PlotSpec, r: f64| {
            let mut s = spec;
            for l in &mut s.layers {
                l.density = Some(crate::ir::DensitySpec {
                    adjust: None, bandwidth: None, levels: None, compare: None, reach: Some(r),
                });
            }
            check(&s, &data())
        };
        let d = with(base().layer(Layer::new(Mark::Line).transform(Transform::Density)), 2.5);
        assert!(d.iter().any(|x| x.message.contains("no slots")),
            "reach on a curve should be refused: {:?}", msgs(&d));
        let violin = || PlotSpec::new().data("t").x("continent").y("life")
            .layer(Layer::new(Mark::Area).transform(Transform::Density));
        assert!(with(violin(), 2.5).is_empty(), "an overlapping ridge is a plot, not a mistake");
        let d = with(violin(), -1.0);
        assert!(d.iter().any(|x| x.message.contains("positive number of slots")),
            "a negative reach should be refused: {:?}", msgs(&d));
    }

    /// A split violin's regions stand in **separate slots**, so the warning that a
    /// split `area` hides itself is false there — it fired on every colored
    /// ridgeline until this was scoped.
    #[test]
    fn a_split_violin_raises_no_overlap_warning() {
        let spec = PlotSpec::new().data("t").x("life").y("continent")
            .layer(Layer::new(Mark::Area).transform(Transform::Density)
                .encode(Channel::Color, "continent"));
        let d = check(&spec, &data());
        assert!(d.is_empty(), "a colored ridgeline warns about nothing: {:?}", msgs(&d));
        // …while an ordinary split area, whose regions do share a domain, still does.
        let plain = base().layer(Layer::new(Mark::Area).encode(Channel::Color, "continent"));
        assert!(!check(&plain, &data()).is_empty(),
            "the overlap warning must survive for the case it was written for");
    }

    /// The grid the book generates must agree that a ribbon takes `density`.
    /// `book/combinations.qmd` is built from this dump on every render, so a cell
    /// left saying "refused" would be the book documenting a refusal that stopped
    /// happening — the failure `check_refusals.R` exists to catch one level up.
    #[test]
    fn the_grid_records_that_a_ribbon_takes_density() {
        assert_eq!(mark_takes_transform(&Mark::Ribbon, &Transform::Density),
                   TransformLegality::Required,
                   "the violin is a fourth way to satisfy a ribbon's minimum syllable");
        assert_eq!(mark_takes_transform(&Mark::Area, &Transform::Density),
                   TransformLegality::Combines);
        // And no other span mark gained it: a whisker has no width to spread across.
        assert_eq!(mark_takes_transform(&Mark::Interval, &Transform::Density),
                   TransformLegality::None);
    }

    /// `surface` is refused in polar by `check_surface` and **not** a second time
    /// by `check_polar`. Pins the one exclusion in the agreement below, so that
    /// removing it shows up here rather than as a doubled diagnostic in a plot.
    #[test]
    fn a_surface_in_polar_is_refused_once_and_names_the_cube() {
        let mut out = Vec::new();
        let spec = PlotSpec::new().data("t").x("a").y("b")
            .coord(CoordSpace::Polar(crate::ir::PolarView::default()))
            .layer(Layer::new(Mark::Surface));
        check_polar(&mut out, &spec);
        assert!(out.is_empty(), "check_polar must not speak for a surface: {out:?}");
    }

    #[test]
    fn space_grid_agrees_with_the_polar_refusals() {
        // The grid says which marks draw in polar; `check_polar` refuses the rest.
        // If the two ever disagree the book would promise a plot the engine will
        // not draw — the drift the generated grids exist to make impossible.
        for m in &ALL_MARKS {
            if !is_drawable(m) {
                continue; // refused earlier, by `check_mark`, with its own direction
            }
            // `surface` is the one mark that does not bend and is deliberately
            // *not* refused here: a sheet needs the cube, which is the sentence
            // `check_surface` already gives with both routes into it named, and
            // saying it twice helps nobody. So it is excluded from the agreement
            // rather than silently making it fail.
            if matches!(m, Mark::Surface) {
                continue;
            }
            let mut out = Vec::new();
            let spec = PlotSpec::new().data("t").x("gdp").y("life")
                .coord(CoordSpace::Polar(crate::ir::PolarView::default()))
                .layer(Layer::new(m.clone()));
            check_polar(&mut out, &spec);
            assert_eq!(
                out.is_empty(),
                mark_draws_in_space(m, SpaceKind::Polar),
                "{m:?} in polar: refusals={:?}",
                out.iter().map(|d| d.message.clone()).collect::<Vec<_>>()
            );
            // A refusal must give direction, never a bare "not supported" (§12).
            for d in &out {
                assert_eq!(d.kind, DiagnosticKind::Unsupported);
                assert!(d.message.contains("Drop `polar()`"), "no direction: {}", d.message);
            }
        }
    }

    #[test]
    fn a_bound_z_projects_without_space_and_a_synthesized_one_needs_it() {
        // The two ways to have a third dimension, and the asymmetry between them
        // (§15). Binding `z` is the original trigger and needs no `space()` — that
        // only sets the angle — while a *synthesized* `z` needs the coordinate to say
        // where the invented measure goes, or every flat histogram would project.
        //
        // This is a regression pin, not a restatement. `space_of` used to require
        // `CoordSpace::Space` for both, so it disagreed with the renderer's own copy
        // of the test about every `z`-bound plot drawn since M8a; making the renderer
        // read `space_of` then silently flattened the glider route — a 50 KB plot
        // came out 7 KB, and it was byte-comparison against the previous build that
        // caught it rather than any test here.
        let bare_z = PlotSpec::new().data("t").x("gdp").y("life").z("pop")
            .layer(Layer::new(Mark::Point));
        assert_eq!(space_of(&bare_z), SpaceKind::Space,
            "a bound `z` projects on its own — `space()` sets the angle, not the dimension");

        let flat_hist = PlotSpec::new().data("t").x("life")
            .layer(Layer::new(Mark::Bar).transform(Transform::Bin));
        assert_eq!(space_of(&flat_hist), SpaceKind::Flat,
            "a flat histogram invents a measure too — it must not project");

        let hist_3d = PlotSpec::new().data("t").x("gdp").y("life")
            .coord(CoordSpace::Space(crate::ir::SpaceView::default()))
            .layer(Layer::new(Mark::Bar).transform(Transform::Bin));
        assert_eq!(space_of(&hist_3d), SpaceKind::Space,
            "`space()` plus a transform that invents a measure is the 3-D histogram");
    }

    /// **A facet crossed with the cube is clean, and both routes into the cube
    /// agree** — which is the part that was broken rather than merely missing.
    ///
    /// `check_space` refused a *bound* `z` beside a facet as "not drawn yet", and
    /// never reached that line for a *synthesized* `z`, because it early-returns
    /// on `axis_def(Z)` first. So the 3-D histogram faceted drew five cubes under
    /// `GOG_STRICT=1` while the scatter beside it was refused: one partition, two
    /// answers, and a Law 2 break that shipped. Deleting the refusal is what makes
    /// these two lines assert the same thing.
    #[test]
    fn a_facet_crossed_with_the_cube_is_clean() {
        let d = data();
        let bound = PlotSpec::new().data("t").x("gdp").y("life").z("value")
            .layer(Layer::new(Mark::Point))
            .facet_col("continent");
        assert!(msgs(&check(&bound, &d)).is_empty(),
            "a faceted cube is drawn: {:?}", msgs(&check(&bound, &d)));

        let synthesized = PlotSpec::new().data("t").x("continent").y("region")
            .coord(CoordSpace::Space(crate::ir::SpaceView::default()))
            .layer(Layer::new(Mark::Bar).transform(Transform::Count))
            .facet_col("continent");
        assert!(msgs(&check(&synthesized, &d)).is_empty(),
            "and so is the faceted 3-D histogram, which always was: {:?}",
            msgs(&check(&synthesized, &d)));
    }

    /// `free` on `z` needed no rule of its own, and this is the assertion that
    /// says so. `check_free` reads the *positions* as one family of three (Law 1),
    /// so the third one has always been accepted with a facet and refused without
    /// one. It was simply unreachable while the cube could not be faceted — spec
    /// §11 recorded it as "accepts it and cannot yet reach it", predicting it
    /// would start working with nothing added. It did.
    #[test]
    fn free_on_z_needs_no_second_rule() {
        let d = data();
        let free_z = |spec: PlotSpec| {
            let mut s = spec;
            s.z = s.z.map(|def| def.with_free());
            s
        };
        let with_facet = free_z(PlotSpec::new().data("t").x("gdp").y("life").z("value")
            .layer(Layer::new(Mark::Point))
            .facet_col("continent"));
        assert!(msgs(&check(&with_facet, &d)).is_empty(),
            "a freed z is legal beside a facet: {:?}", msgs(&check(&with_facet, &d)));

        let no_facet = free_z(PlotSpec::new().data("t").x("gdp").y("life").z("value")
            .layer(Layer::new(Mark::Point)));
        assert!(msgs(&check(&no_facet, &d)).iter().any(|m| m.contains("free")),
            "a freed axis with no panels to free it across is refused: {:?}",
            msgs(&check(&no_facet, &d)));
    }

    #[test]
    fn a_reduction_groups_by_the_positions_the_mark_does_not_measure_with() {
        // Spec §5's subtraction, read as the **two-dimensional group-by**: a value
        // statistic groups by every position the mark does not measure with, and
        // reduces the column named on the one it does. A flat `bar` measures with `y`,
        // so it groups by `x`; a `bar` in `space` measures with `z`, so it groups by
        // the pair; a `zone` measures with `color`, so it groups by the pair too.
        //
        // This replaces the refusal that stood here. That refusal was right about the
        // bug — left to run, `bar * mean + x + y + z + space()` grouped by `x` and
        // wrote to `y`, one column per *row* piled at each slot with a height that was
        // whichever row painted last, every part legal and nothing said — and wrong
        // about the reason, which it gave as "there is no channel left to name the
        // column". `z` names it, exactly as `y` does flat.
        let cube = |t: Transform| PlotSpec::new().data("t").x("continent").y("region").z("life")
            .coord(CoordSpace::Space(crate::ir::SpaceView::default()))
            .layer(Layer::new(Mark::Bar).transform(t));
        let tile = |t: Transform| PlotSpec::new().data("t").x("continent").y("region")
            .layer(Layer::new(Mark::Zone).transform(t).encode(Channel::Color, "life"));
        for t in [Transform::Mean, Transform::Sum, Transform::Median,
                  Transform::Max, Transform::Min] {
            for (what, spec) in [("in the cube", cube(t.clone())), ("on a zone", tile(t.clone()))] {
                let d = check(&spec, &data());
                assert!(!d.iter().any(|x| x.is_fatal()),
                    "`{} * {}` {what} must draw: {:?}",
                    if what.contains("zone") { "zone" } else { "bar" }, transform_name(&t),
                    d.iter().map(|x| x.message.clone()).collect::<Vec<_>>());
            }
        }

        // `smooth` is the one that stays refused, and for its own reason rather than
        // theirs: it *fits* along a domain instead of reducing, and a floor has no
        // left to right — the same thing that keeps `line` and `area` out of the cube.
        let d = check(&cube(Transform::Smooth), &data());
        let hit = d.iter().find(|x| x.message.contains("no left to right"));
        let Some(hit) = hit else {
            panic!("`bar * smooth` in space was not refused: {:?}",
                   d.iter().map(|x| x.message.clone()).collect::<Vec<_>>());
        };
        assert_eq!(hit.kind, DiagnosticKind::Illegal);
        // §12: a refusal names what to do instead. Both routes, since which one the
        // reader wants depends on whether they meant to fit or to summarize.
        assert!(hit.message.contains("line * smooth"), "no flat-fit direction: {}", hit.message);
        assert!(hit.message.contains("mean"), "no reduction direction: {}", hit.message);

        // And the flat reading is untouched — the pair reading is a *widening*, so
        // one key must keep meaning what it always did.
        let flat = PlotSpec::new().data("t").x("continent").y("life")
            .layer(Layer::new(Mark::Bar).transform(Transform::Mean));
        assert!(!check(&flat, &data()).iter().any(|x| x.is_fatal()),
            "the flat `bar * mean` must keep drawing");
    }

    #[test]
    fn a_pair_reduction_needs_its_measure_channel_named_and_its_cells_slotted() {
        // What the pair reading has to ask that the one-key reading got for free:
        // flat, `y` is required by the mark, so a value statistic never had to check
        // that its column was named. Here the measurement rides an **optional**
        // channel — `color` on a zone, `z` on a bar — so all three questions are live,
        // and each is fatal rather than warn-then-draw (§12).
        let msgs = |d: &[Diagnostic]| d.iter().map(|x| x.message.clone()).collect::<Vec<_>>();

        // 1. No column to reduce, on the mark where that is reachable. A zone's
        //    `color` is optional, so `zone * mean` with nothing bound is a plot
        //    someone can write; a bar's third axis is what *puts it in the cube* at
        //    all, so `bar * mean + space()` with no `z` is a flat plot with two
        //    categorical axes and `check_slot_shape` owns it — asserted below.
        let d = check(&PlotSpec::new().data("t").x("continent").y("region")
            .layer(Layer::new(Mark::Zone).transform(Transform::Mean)), &data());
        let hit = d.iter().find(|x| x.message.contains("nothing says which column"));
        let Some(hit) = hit else { panic!("an unnamed column was accepted: {:?}", msgs(&d)) };
        assert_eq!(hit.kind, DiagnosticKind::Illegal);
        assert!(hit.message.contains("measures by `color`"),
            "the wrong channel was named: {}", hit.message);
        assert!(hit.message.contains("count"), "no count direction: {}", hit.message);

        // The bar's version of the same mistake, refused by the checks that already
        // owned it — and between them they still say to bind `z`, which is §12's
        // whole requirement of a refusal.
        let d = check(&PlotSpec::new().data("t").x("continent").y("region")
            .coord(CoordSpace::Space(crate::ir::SpaceView::default()))
            .layer(Layer::new(Mark::Bar).transform(Transform::Mean)), &data());
        assert!(d.iter().any(|x| x.is_fatal()), "a bar measuring nothing was accepted: {:?}", msgs(&d));
        assert!(d.iter().any(|x| x.message.contains("z(<column>)")),
            "nothing pointed at the third axis: {:?}", msgs(&d));

        // 1b. Two transforms cannot both be the cell's measurement. `bin` invents one
        //     and `mean` reduces a named one, and a cell holds a single number — so
        //     composing them is refused rather than one of the two quietly winning.
        let d = check(&PlotSpec::new().data("t").x("continent").y("region")
            .layer(Layer::new(Mark::Zone).transform(Transform::Count)
                .transform(Transform::Mean).encode(Channel::Color, "life")), &data());
        let hit = d.iter().find(|x| x.message.contains("measures each cell twice"));
        let Some(hit) = hit else { panic!("a doubly-measured cell was accepted: {:?}", msgs(&d)) };
        assert_eq!(hit.kind, DiagnosticKind::Illegal);
        assert!(hit.message.contains("zone * count") && hit.message.contains("color(<column>)"),
            "both ways out must be named: {}", hit.message);

        // 1c. `bin` is the exception, and it is the whole composition ruling: it
        //     supplies an *extent* rather than a measurement, so it composes. The
        //     two questions above it still apply — the column must be named and must
        //     be a number — and only *both positions categorical* is answered
        //     differently, because a cut axis owns cells too.
        let d = check(&PlotSpec::new().data("t").x("gdp").y("life")
            .layer(Layer::new(Mark::Zone).transform(Transform::Bin)
                .transform(Transform::Mean).encode(Channel::Color, "life")), &data());
        assert!(!d.iter().any(Diagnostic::is_fatal),
            "a cut cell reduced by a named statistic is the summary heatmap: {:?}", msgs(&d));

        // 2. A column that is not a number. There is no mean of a category, and
        //    answering it in the transform would be the warn-then-draw §12 forbids.
        let d = check(&PlotSpec::new().data("t").x("continent").y("region")
            .layer(Layer::new(Mark::Zone).transform(Transform::Mean)
                .encode(Channel::Color, "region")), &data());
        assert!(d.iter().any(|x| x.is_fatal() && x.message.contains("not a numeric column")),
            "a categorical column was reduced: {:?}", msgs(&d));

        // 3. A continuous position. These five **measure without cutting**, so their
        //    cells can only be the slots categories own — a number is a point. The
        //    refusal says so per axis and points at binning where the data lives.
        let d = check(&PlotSpec::new().data("t").x("gdp").y("region")
            .layer(Layer::new(Mark::Zone).transform(Transform::Mean)
                .encode(Channel::Color, "life")), &data());
        let hit = d.iter().find(|x| x.message.contains("owns no cell to summarize into"));
        let Some(hit) = hit else { panic!("a continuous key was accepted: {:?}", msgs(&d)) };
        assert!(hit.message.contains("x(gdp)") && !hit.message.contains("y(region)"),
            "the refusal must name the axis that is loose, and only it: {}", hit.message);
        // And exactly one refusal for the one mistake: `check_zone_extent` defers to
        // this one, the way it already does for a tally.
        assert_eq!(d.iter().filter(|x| x.is_fatal()).count(), 1,
            "one mistake, one refusal: {:?}", msgs(&d));
    }

    #[test]
    fn only_bin_gives_its_measurement_up_when_two_transforms_compose() {
        // **Which transform owns the measurement when two are composed** (spec §5).
        // The answer is *the one that was handed a column*, so a value statistic
        // always keeps it and the synthesizing transform keeps whatever else it
        // supplies — an extent for `bin`, nothing for the other three.
        //
        // Asked on a one-key mark deliberately. Until 2026-07-26 this refusal lived
        // inside the *two-dimensional* group-by, so the identical mistake was fatal
        // on a `zone` and silent on a `bar`: every one-key composition ran, dropped
        // the statistic, and relabeled the axis to the column nobody had read.
        let compose = |t: Transform| {
            check(&PlotSpec::new().data("t").x("gdp").y("life")
                .layer(Layer::new(Mark::Bar).transform(t.clone()).transform(Transform::Mean)), &data())
        };

        // `bin` composes: it cuts, and a cut is an extent description.
        let d = compose(Transform::Bin);
        assert!(!d.iter().any(Diagnostic::is_fatal),
            "`bar * bin * mean` is the binned mean profile: {:?}", msgs(&d));

        // `proportion` composes too, and for a different reason from `bin`'s: it
        // does not give a measurement up, it **rescales the one it finds**. This
        // was refused for a day, which made `bar * sum * proportion` — each slot's
        // summed column as a share of the total — a sentence gog could not say.
        let d = compose(Transform::Proportion);
        assert!(!d.iter().any(Diagnostic::is_fatal),
            "`bar * proportion * mean` reads the mean as a share: {:?}", msgs(&d));

        // The other two do not, and each is refused for **its own** reason rather
        // than a shared one — the defect the 2026-07-26 refusal audit went looking
        // for is a message restating a rule that belongs somewhere else.
        for (t, reason) in [
            (Transform::Count,      "supplies only a measurement"),
            (Transform::Density,    "not a bucket holding rows"),
        ] {
            let d = compose(t.clone());
            let hit = d.iter().find(|x| x.message.contains("measures each cell twice"));
            let Some(hit) = hit else {
                panic!("`bar * {} * mean` was accepted: {:?}", transform_name(&t), msgs(&d))
            };
            assert_eq!(hit.kind, DiagnosticKind::Illegal);
            assert!(hit.message.contains(reason),
                "`{}` must be refused for its own reason: {}", transform_name(&t), hit.message);
            // Every refusal names the way *through* as well as the two ways out —
            // §12's whole requirement, and here it is the feature this ruling built.
            assert!(hit.message.contains("bin"),
                "the refusal must point at the transform that does cut: {}", hit.message);
        }

        // Two **synthesizing** transforms are the same contradiction with neither side
        // handed a column, so there is nothing to give way.
        //
        // `proportion` was on this list for one day, and the pair that put it there —
        // `bin * proportion` — is the relative-frequency histogram, now legal and
        // asserted below. The plot that condemned it (twelve equal bars at 1/12, under
        // an axis reading `Count`) was a *sequencing* defect: run in order, `bin`
        // tallied and `proportion` then read the binned frame as its population, where
        // every cell appears exactly once. Refusing the sentence for what the
        // implementation did to it is the mistake this line records.
        for (a, b) in [
            (Transform::Bin,   Transform::Density),
            (Transform::Bin,   Transform::Count),
            (Transform::Count, Transform::Density),
        ] {
            let d = check(&PlotSpec::new().data("t").x("gdp").y("life")
                .layer(Layer::new(Mark::Bar).transform(a.clone()).transform(b.clone())), &data());
            let hit = d.iter().find(|x| x.message.contains("measures each cell twice"));
            let Some(hit) = hit else {
                panic!("`bar * {} * {}` was accepted: {:?}",
                    transform_name(&a), transform_name(&b), msgs(&d))
            };
            assert_eq!(hit.kind, DiagnosticKind::Illegal);
            assert!(hit.message.contains("neither was handed a column"),
                "two invented measurements are refused for *that* reason: {}", hit.message);
        }

        // `smooth` is refused against all four, `bin` included, and for a reason none
        // of them share: it fits a curve and already averages locally as it goes, so
        // cutting the domain first buys it nothing it was not doing.
        for t in [Transform::Bin, Transform::Count, Transform::Density] {
            let d = check(&PlotSpec::new().data("t").x("gdp").y("life")
                .layer(Layer::new(Mark::Line).transform(t.clone()).transform(Transform::Smooth)), &data());
            let hit = d.iter().find(|x| x.message.contains("asks one question twice"));
            let Some(hit) = hit else {
                panic!("`line * {} * smooth` was accepted: {:?}", transform_name(&t), msgs(&d))
            };
            assert_eq!(hit.kind, DiagnosticKind::Illegal);
            assert!(hit.message.contains("bin * mean"),
                "…and points at the statistic that does summarize a cell: {}", hit.message);
        }

        // **A normalizer composes with everything that leaves one number per cell.**
        // The three sentences the day-old refusal had made unsayable, and the reason
        // `proportion` had to leave the synthesizing class rather than gain an
        // exception inside it (Law 2).
        for ts in [
            vec![Transform::Bin, Transform::Proportion],       // relative-frequency histogram
            vec![Transform::Count, Transform::Proportion],     // the tally, spelled out
            vec![Transform::Sum, Transform::Proportion],       // each slot's share of a total
        ] {
            let mut layer = Layer::new(Mark::Bar);
            for t in &ts { layer = layer.transform(t.clone()); }
            let d = check(&PlotSpec::new().data("t").x("gdp").y("life").layer(layer), &data());
            assert!(!d.iter().any(Diagnostic::is_fatal),
                "`bar * {}` must draw: {:?}",
                ts.iter().map(transform_name).collect::<Vec<_>>().join(" * "), msgs(&d));
        }

        // …and refuses exactly what it cannot rescale: no total to divide by
        // (`density`, `smooth`) or two numbers per cell instead of one (the pairs).
        // Each says so in its own words rather than sharing a message.
        for (t, reason) in [
            (Transform::Density,    "already integrates to 1"),
            (Transform::Smooth,     "no total to divide by"),
            (Transform::Range,      "share of a span"),
            (Transform::Confidence, "share of a span"),
        ] {
            let mark = if matches!(t, Transform::Range | Transform::Confidence) {
                Mark::Interval
            } else {
                Mark::Line
            };
            let d = check(&PlotSpec::new().data("t").x("gdp").y("life")
                .layer(Layer::new(mark).transform(t.clone()).transform(Transform::Proportion)),
                &data());
            let hit = d.iter().find(|x| x.message.contains(reason));
            assert!(hit.is_some_and(|h| h.kind == DiagnosticKind::Illegal),
                "`{} * proportion` must be refused for its own reason: {:?}",
                transform_name(&t), msgs(&d));
        }
    }

    #[test]
    fn a_plot_is_drawn_in_one_space_not_two() {
        // `polar()` bends the plane the plot already has; `z` adds a dimension to
        // it. Asking for both is asking for a cylinder, which is not built — so it
        // is refused with direction rather than one of the two quietly winning.
        let spec = PlotSpec::new().data("t").x("gdp").y("life").z("pop")
            .coord(CoordSpace::Polar(crate::ir::PolarView::default()))
            .layer(Layer::new(Mark::Point));
        let d = check(&spec, &data());
        assert!(
            d.iter().any(|x| x.kind == DiagnosticKind::Unsupported
                && x.message.contains("one coordinate space")),
            "polar + z was not refused: {:?}",
            d.iter().map(|x| x.message.clone()).collect::<Vec<_>>()
        );
    }

    // The one relaxation of Law 7's minimum syllable (spec §15): a `bar` whose
    // split is its segmentation has one slot and needs no position axis. These pin
    // both halves — that it is allowed where it means something, and refused where
    // it does not, which is what keeps it a rule rather than an exception.

    #[test]
    fn a_bar_may_drop_its_position_when_a_split_divides_the_one_slot() {
        let fatal = |d: &[Diagnostic]| d.iter().any(|x| x.is_fatal());
        let bar = |split: bool| {
            let mut l = Layer::new(Mark::Bar).transform(Transform::Count).transform(Transform::Stack);
            if split { l = l.encode(Channel::Color, "continent"); }
            PlotSpec::new().data("t").layer(l)
        };
        assert!(!fatal(&check(&bar(true), &data())), "a split bar with no x must be legal: {:?}",
            check(&bar(true), &data()).iter().map(|d| d.message.clone()).collect::<Vec<_>>());

        // Without the split there is nothing to divide the slot, so every row would
        // pile into one place with nothing to tell them apart. Still refused, and
        // the refusal still names the missing position.
        let d = check(&bar(false), &data());
        assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal && x.message.contains("needs `x()`")),
            "an unsplit bar with no x must still be refused: {:?}",
            d.iter().map(|x| x.message.clone()).collect::<Vec<_>>());
    }

    #[test]
    fn exactly_three_marks_may_drop_a_position() {
        // Law 7's floor has not moved; the point of this test is that the list of
        // marks standing off it is closed and each member has a reason stated once
        // in the engine. `bar` drops `x` when a split divides its one slot
        // (`bar_divides_one_slot`). `rule` is *placed* by one position and spans
        // the other (`rule_axis`). `zone` is *bounded* on one axis and spans the
        // other (`check_bounds`) — the same relaxation as `rule`'s, one dimension
        // up, which is why the two arrived together rather than the second being
        // an exception the first made room for. Every other drawable mark keeps
        // both positions.
        let fatal = |d: &[Diagnostic]| d.iter().any(|x| x.is_fatal());
        for m in &ALL_MARKS {
            if matches!(m, Mark::Bar | Mark::Rule | Mark::Zone) || !is_drawable(m) {
                continue;
            }
            let spec = PlotSpec::new().data("t").y("life")
                .layer(Layer::new(m.clone()).encode(Channel::Color, "continent"));
            assert!(fatal(&check(&spec, &data())), "{m:?} drew with no x");
        }
        // The relaxed three really are relaxed — asserted here rather than only in
        // their own tests, so this stays a statement about the whole set.
        let mut d2 = data();
        d2.insert("z".to_string(),
            DataFrame::new().with_float("lo", vec![1.0]).with_float("hi", vec![2.0]));
        for spec in [
            PlotSpec::new().data("t").y("life").layer(
                Layer::new(Mark::Bar).transform(Transform::Count).encode(Channel::Color, "continent")),
            PlotSpec::new().data("t").y("life").layer(Layer::new(Mark::Rule)),
            PlotSpec::new().data("t").y("life").layer(
                Layer::new(Mark::Zone).data("z").bounds("lo", "hi")),
        ] {
            assert!(!fatal(&check(&spec, &d2)), "a relaxed mark was refused: {:?}",
                msgs(&check(&spec, &d2)));
        }
    }

    // -----------------------------------------------------------------------
    // Per-layer positions — one axis, its own column (spec §8)
    // -----------------------------------------------------------------------

    /// A second table naming its own position columns. This is the sentence spec
    /// §8 asserted for several sessions while it could not run: `x(at)` resolved
    /// against the *plot's* table and was refused as a missing column, so the
    /// section's own worked example had to reuse the base table's names.
    #[test]
    fn a_second_table_may_name_its_own_position_columns() {
        let mut d = data();
        d.insert(
            "notes".to_string(),
            DataFrame::new()
                .with_float("at", vec![2.0])
                .with_float("val", vec![5.0])
                .with_str("what", vec!["here".into()]),
        );
        let spec = base().layer(Layer::new(Mark::Point)).layer(
            Layer::new(Mark::Text)
                .data("notes")
                .encode(Channel::X, "at")
                .encode(Channel::Y, "val")
                .encode(Channel::Label, "what"),
        );
        let diags = check(&spec, &d);
        assert!(
            diags.iter().all(|x| !x.is_fatal()),
            "the note table's own columns were refused: {:?}",
            msgs(&diags)
        );
    }

    /// The plot's column is *not* looked for in a layer that named its own —
    /// which is the precise failure the defect was. A note table holds `at`, not
    /// `gdp`, and asking it for `gdp` is what produced "column not in the data".
    #[test]
    fn the_plots_column_is_not_demanded_of_a_layer_that_named_its_own() {
        let mut d = data();
        d.insert(
            "notes".to_string(),
            DataFrame::new().with_float("at", vec![2.0]).with_float("val", vec![5.0]),
        );
        let spec = base().layer(Layer::new(Mark::Point)).layer(
            Layer::new(Mark::Point)
                .data("notes")
                .encode(Channel::X, "at")
                .encode(Channel::Y, "val"),
        );
        let diags = check(&spec, &d);
        assert!(
            !msgs(&diags).iter().any(|m| m.contains("gdp")),
            "the plot's column was demanded of the note table: {:?}",
            msgs(&diags)
        );
    }

    /// A layer's own position is still checked — against *its own* table. The
    /// feature moves which table answers the question, never whether it is asked.
    #[test]
    fn a_layer_position_naming_a_missing_column_is_still_refused() {
        let mut d = data();
        d.insert("notes".to_string(), DataFrame::new().with_float("at", vec![2.0]));
        let spec = base().layer(
            Layer::new(Mark::Point).data("notes").encode(Channel::X, "nope"),
        );
        let diags = check(&spec, &d);
        assert!(
            diags.iter().any(|x| x.is_fatal()),
            "a missing column on a layer position must still be fatal"
        );
        assert!(msgs(&diags).iter().any(|m| m.contains("nope")));
    }

    /// One axis, its own column — **never its own scale**. The line between the
    /// thing §8 owes and the thing §18 refuses is exactly one field wide, so it
    /// gets its own test.
    #[test]
    fn a_layer_may_not_bring_its_own_scale() {
        let mut d = data();
        d.insert("notes".to_string(), DataFrame::new().with_float("at", vec![2.0]));
        let spec = base().layer(
            Layer::new(Mark::Point)
                .data("notes")
                .encode_scaled(Channel::X, "at", ScaleType::Log),
        );
        let diags = check(&spec, &d);
        assert!(
            diags.iter().any(|x| x.kind == DiagnosticKind::Illegal
                && x.message.contains("share one x axis")),
            "a layer's own scale is a second axis and must be refused: {:?}",
            msgs(&diags)
        );
    }

    /// The same column with the same scale as the axis is not a second scale, so
    /// it is not refused. Guards the check against firing on a plot that merely
    /// repeats itself — Law 8, never forbid the ugly-but-legal.
    #[test]
    fn a_layer_repeating_the_axis_scale_is_legal() {
        let mut spec = PlotSpec::new().data("t").y("life").layer(
            Layer::new(Mark::Point).encode_scaled(Channel::X, "gdp", ScaleType::Log),
        );
        spec.x = Some(ChannelDef::field("gdp").with_scale(ScaleType::Log));
        let diags = check(&spec, &data());
        assert!(
            !msgs(&diags).iter().any(|m| m.contains("share one x axis")),
            "repeating the axis's own scale is not a second scale: {:?}",
            msgs(&diags)
        );
    }

    /// A layer that states **nothing** inherits the axis, and inheriting is not
    /// disagreeing. Until 2026-07-27 the check compared `Option` against `Option`,
    /// so any plot-scoped `limits`/`scale`/`tick_count` refused *every* layer that
    /// named its own position column — the sentence the diagnostic itself calls
    /// fine ("`y(v)` on its own"). Found while drawing a sunburst, where the
    /// radial domain is what puts the hole in the middle and a second layer is
    /// what labels the rings, so the two could not be said together.
    #[test]
    fn a_layer_naming_its_column_inherits_the_axis_scale_properties() {
        for axis in [
            ChannelDef::field("gdp").with_limits(Some(0.0), Some(10.0)),
            ChannelDef::field("gdp").with_scale(ScaleType::Log),
            ChannelDef::field("gdp").with_tick_count(8),
        ] {
            let mut spec = PlotSpec::new().data("t").y("life").layer(
                // The layer names the column and nothing else — no scale, no
                // limits, no tick count.
                Layer::new(Mark::Point).encode(Channel::X, "gdp"),
            );
            spec.x = Some(axis.clone());
            let diags = check(&spec, &data());
            assert!(
                !msgs(&diags).iter().any(|m| m.contains("share one x axis")),
                "a layer stating no scale property of its own inherits the axis's, \
                 and must not be refused for one it never asked for: axis {axis:?} \
                 gave {:?}",
                msgs(&diags)
            );
        }
    }

    // -----------------------------------------------------------------------
    // `rule` — one position from the data, the other extent from the panel
    // -----------------------------------------------------------------------

    /// The payoff §18 predicted when it filed the ambiguous case as "the sharpest
    /// live cost of the parked per-layer position bindings": a thresholds table
    /// holding *both* of the plot's columns was refused for being unreadable, and
    /// a layer that says which axis it means now answers it outright.
    #[test]
    fn a_rule_can_say_which_axis_it_means_instead_of_being_refused() {
        let mut d = data();
        d.insert(
            "both".to_string(),
            DataFrame::new().with_float("gdp", vec![2.5]).with_float("life", vec![5.5]),
        );
        let ambiguous = base().layer(Layer::new(Mark::Point))
            .layer(Layer::new(Mark::Rule).data("both"));
        assert!(
            check(&ambiguous, &d).iter().any(|x| x.is_fatal()),
            "a table answering both axes still has nothing to say which is meant"
        );

        let said = base().layer(Layer::new(Mark::Point)).layer(
            Layer::new(Mark::Rule).data("both").encode(Channel::X, "gdp"),
        );
        let diags = check(&said, &d);
        assert!(
            diags.iter().all(|x| !x.is_fatal()),
            "`rule + x(gdp)` names its axis and must be accepted: {:?}",
            msgs(&diags)
        );
        assert_eq!(rule_axis(&said, &d["both"], &said.layers[1]), Some(Channel::X));
    }

    #[test]
    fn a_rule_is_placed_by_whichever_axis_its_own_table_answers() {
        // The mark's whole claim, as one differential assertion over both axes:
        // the *same* layer, against the *same* plot, lands on x or on y purely
        // according to which position column the rule's table holds. Written as
        // one test over both so it cannot pass by describing each separately.
        let cuts = |col: &str| {
            let mut m = data();
            m.insert("cuts".to_string(), DataFrame::new().with_float(col, vec![2.5]));
            m
        };
        let spec = base().layer(Layer::new(Mark::Point))
            .layer(Layer::new(Mark::Rule).data("cuts"));

        for (col, want) in [("gdp", Channel::X), ("life", Channel::Y)] {
            let d = check(&spec, &cuts(col));
            assert!(d.iter().all(|x| !x.is_fatal()), "a rule over `{col}` was refused: {:?}", msgs(&d));
            let df = &cuts(col)["cuts"];
            let rule_layer = &spec.layers[1];
            assert_eq!(rule_axis(&spec, df, rule_layer), Some(want.clone()),
                "a table holding only `{col}` must place the rule on {want:?}");
        }
    }

    #[test]
    fn a_rule_refuses_the_two_shapes_that_do_not_say_which_axis() {
        // Neither column, and both columns. The second is the one that matters:
        // it is the rug written straight over the plot's own table, it is
        // genuinely ambiguous, and guessing an axis there would teach a rule that
        // is not real — `check_slot_shape`'s refusal of a two-categorical bar,
        // one mark over.
        let spec = base().layer(Layer::new(Mark::Point)).layer(Layer::new(Mark::Rule).data("r"));
        let with = |df: DataFrame| {
            let mut m = data();
            m.insert("r".to_string(), df);
            check(&spec, &m)
        };

        let neither = with(DataFrame::new().with_float("other", vec![1.0]));
        assert!(neither.iter().any(|x| x.kind == DiagnosticKind::Illegal
                && x.message.contains("needs a position")),
            "a table answering neither axis must be refused: {:?}", msgs(&neither));

        let both = with(DataFrame::new().with_float("gdp", vec![1.0]).with_float("life", vec![2.0]));
        assert!(both.iter().any(|x| x.kind == DiagnosticKind::Illegal
                && x.message.contains("both")),
            "a table answering both axes must be refused: {:?}", msgs(&both));
        // And the refusal must direct, not just decline: the fix is a smaller
        // table, so the message has to name it.
        assert!(both.iter().any(|x| x.message.contains("its own table")),
            "the ambiguity refusal must say how to break the tie: {:?}", msgs(&both));
    }

    #[test]
    fn the_axis_a_rule_spans_is_not_reported_as_a_missing_column() {
        // A thresholds table holding *only* the threshold is the designed form
        // (§18), so the plot's other position column is *expected* to be absent
        // from it. Reporting that as a missing column would make the ordinary
        // sentence fail — which is the bug this suppression exists to prevent.
        let mut m = data();
        m.insert("cuts".to_string(), DataFrame::new().with_float("life", vec![5.0]));
        let d = check(
            &base().layer(Layer::new(Mark::Point)).layer(Layer::new(Mark::Rule).data("cuts")),
            &m,
        );
        assert!(!d.iter().any(|x| x.message.contains("not in the data")),
            "the spanned axis must not be reported missing: {:?}", msgs(&d));
    }

    #[test]
    fn reach_is_rule_only_and_takes_one_of_two_values() {
        let styled = |m: Mark, v: &str| {
            let mut layer = Layer::new(m);
            layer.style.reach = Some(v.to_string());
            base().layer(layer)
        };
        // The mark half, read off `mark_takes_setting` so the grid agrees.
        let d = check(&styled(Mark::Point, "panel"), &data());
        assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
                && x.message.contains("has no such axis")),
            "reach on a two-position mark must be refused: {:?}", msgs(&d));

        // The value half, read off `REACHES`.
        for v in REACHES {
            let d = check(&styled(Mark::Rule, v), &data());
            assert!(!d.iter().any(|x| x.message.contains("is not a reach")), "`{v}` must be legal");
        }
        let d = check(&styled(Mark::Rule, "middle"), &data());
        assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
                && x.message.contains("is not a reach")),
            "an invented reach must be refused: {:?}", msgs(&d));
    }

    #[test]
    fn every_none_cell_of_the_transform_grid_actually_refuses() {
        // The grid is generated from `mark_takes_transform`, so a `—` cell is a
        // promise the book prints: this combination does not exist. Nothing made
        // the engine keep it. `rule * mean` was a `—` that *rendered* — two
        // warnings about a missing `x` and an empty panel — which is warn-then-
        // draw, the silent drop §12 forbids, and the same blind spot as an
        // `#| error: true` chunk that stops erroring.
        //
        // What is required is that the layer is **refused**, not that the message
        // names the transform. `interval * bin` is caught by the missing-range
        // refusal instead, and that message is the more useful one — it names
        // what the mark actually needs. The wording of each refusal is pinned by
        // the mark's own test; this one pins that a `—` cell never draws.
        let mut gaps: Vec<String> = Vec::new();
        for m in &ALL_MARKS {
            if !is_drawable(m) {
                continue;
            }
            for t in USER_TRANSFORMS {
                if mark_takes_transform(m, &t) != TransformLegality::None {
                    continue;
                }
                let mut layer = Layer::new(m.clone()).transform(t.clone());
                // Give each mark the rest of its minimum syllable, so what is
                // left to complain about is the transform alone.
                if *m == Mark::Text { layer = layer.encode(Channel::Label, "continent"); }
                if is_collision_modifier(&t) { layer = layer.encode(Channel::Color, "continent"); }
                let d = check(&base().layer(layer), &data());
                if !d.iter().any(|x| x.is_fatal()) {
                    gaps.push(format!("{} * {}", mark_name(m), transform_name(&t)));
                }
            }
        }
        assert!(gaps.is_empty(), "the grid says these do not exist, but nothing refuses them: {gaps:?}");
    }

    #[test]
    fn a_zone_takes_the_six_transforms_that_give_it_a_cell() {
        // The mark's Mark × Transform row, pinned from both sides, and the line it
        // draws is *what each transform supplies*. `bounds` names the sides;
        // `bin`/`density` cut them and measure inside; `count`/`proportion` measure
        // into the cells the categories already are. Those five read `Required`
        // because any one satisfies the mark — the same "● means one of these" the
        // three pair transforms on `interval` already mean.
        //
        // **The five reductions `Combine` rather than bound**, which is the
        // two-dimensional group-by (spec §5). They measure without cutting, so they
        // answer *what is in this cell* and never *where are my cells* — the
        // categorical positions answer that. Their column is named by the channel the
        // mark measures with, which for a zone is `color`: the claim they were once
        // refused under (no channel left to name it) was false on its own terms.
        //
        // **`partition` is the sixth** (2026-07-27), and it belongs to the first
        // group without qualification: it names the sides the way `bounds` does,
        // and it measures inside them the way `bin` does, being the only one of the
        // six that does both. What it adds to the row is not a new kind of cell but
        // a new *source* of the rectangular one — cut from a tree instead of from a
        // mesh or handed over column by column.
        for t in USER_TRANSFORMS {
            let expect = if matches!(t, Transform::Bounds | Transform::Bin | Transform::Density
                                        | Transform::Count | Transform::Proportion
                                        | Transform::Partition) {
                TransformLegality::Required
            } else if crate::transform::reduces_column(std::slice::from_ref(&t)).is_some() {
                TransformLegality::Combines
            } else {
                TransformLegality::None
            };
            assert_eq!(mark_takes_transform(&Mark::Zone, &t), expect,
                "`zone * {}` is wrong in the grid", transform_name(&t));
        }
        // A zone on two *continuous* positions with no transform has nothing to
        // bound it, and the refusal offers every route out of that.
        let d = check(&PlotSpec::new().data("t").x("gdp").y("life")
            .layer(Layer::new(Mark::Zone)), &data());
        assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
            && x.message.contains("bounds") && x.message.contains("bin")
            && x.message.contains("categorical position")),
            "a bare zone is told every way to give it sides: {:?}", msgs(&d));
    }

    // -- partition ---------------------------------------------------------

    /// A hierarchy table, laid out the way the book's is: two branches, one of
    /// which reaches a third level and one of which does not.
    fn tree_data() -> HashMap<String, DataFrame> {
        let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        HashMap::from([(
            "t".to_string(),
            DataFrame::new()
                .with_str("group", s(&["A", "A", "B"]))
                .with_str("item", s(&["p", "q", "r"]))
                .with_str("detail", s(&["deep", "", ""]))
                .with_float("amount", vec![1.0, 2.0, 3.0])
                .with_float("gdp", vec![1.0, 2.0, 3.0]),
        )])
    }
    fn part(mark: Mark) -> Layer {
        Layer::new(mark).partition(&["group", "item", "detail"])
    }

    /// **Two readers, which is what makes it a transform.** A transform legal on
    /// exactly one mark is that mark's business wearing a transform's name (§5's
    /// growth test read from the other end), so the row is pinned from both sides:
    /// `zone` takes the four edges and `text` the center, and every other mark is
    /// refused toward them.
    #[test]
    fn a_partition_is_read_by_two_marks_and_refused_by_the_rest() {
        for m in &ALL_MARKS {
            let expect = match m {
                Mark::Zone => TransformLegality::Required,
                Mark::Text => TransformLegality::Combines,
                _ => TransformLegality::None,
            };
            assert_eq!(mark_takes_transform(m, &Transform::Partition), expect,
                "`{} * partition` is wrong in the grid", mark_name(m));
        }
        let d = check(&PlotSpec::new().data("t").x("amount")
            .layer(part(Mark::Bar)), &tree_data());
        assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
            && x.message.contains("region")
            && x.message.contains("zone") && x.message.contains("text")),
            "a mark with no region reading is sent to the two that have one: {:?}", msgs(&d));
    }

    /// A partition places its own nodes, so neither position has to be named —
    /// Law 7's relaxation in `rule`'s and `zone`'s *constitutive* family rather
    /// than the bar's conditional one.
    #[test]
    fn a_partition_needs_no_position_bound() {
        let d = check(&PlotSpec::new().data("t").layer(part(Mark::Zone)), &tree_data());
        assert!(!d.iter().any(|x| x.is_fatal()),
            "a partition supplies both positions: {:?}", msgs(&d));
        // And `text` reads it too, naming each node where the same computation
        // put it — the second reader, drawn rather than merely allowed.
        let d = check(&PlotSpec::new().data("t")
            .layer(part(Mark::Text).encode(Channel::Label, "name")), &tree_data());
        assert!(!d.iter().any(|x| x.is_fatal()),
            "`text * partition + label(name)` names an output column: {:?}", msgs(&d));
    }

    /// The one genuine ambiguity, refused rather than defaulted — Law 5, and the
    /// ruling `bin(30, width = 5)` set. plotly picks a side here
    /// (`branchvalues="remainder"`); gog will not, because the arithmetic is the
    /// accounts' and not the grammar's.
    #[test]
    fn an_interior_node_with_its_own_value_is_refused_with_direction() {
        let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        let d = HashMap::from([(
            "t".to_string(),
            DataFrame::new()
                .with_str("group", s(&["A", "A"]))
                .with_str("item", s(&["", "p"]))
                .with_float("amount", vec![5.0, 5.0]),
        )]);
        let out = check(&PlotSpec::new().data("t").x("amount")
            .layer(Layer::new(Mark::Zone).partition(&["group", "item"])), &d);
        assert!(out.iter().any(|x| x.kind == DiagnosticKind::Illegal
            && x.message.contains("value of its own")
            && x.message.contains("leaf")),
            "an interior value is refused and told where numbers belong: {:?}", msgs(&out));
    }

    /// A level is a **name** a branch shares; a number is what the branch is
    /// weighed by. Refused here rather than by `check_distribution_axis`, whose
    /// message would send the reader looking for a bin.
    #[test]
    fn a_numeric_level_is_refused_toward_the_measure() {
        let out = check(&PlotSpec::new().data("t")
            .layer(Layer::new(Mark::Zone).partition(&["group", "gdp"])), &tree_data());
        assert!(out.iter().any(|x| x.kind == DiagnosticKind::Illegal
            && x.message.contains("holds numbers") && x.message.contains("x(gdp)")),
            "a numeric level is refused toward `x()`: {:?}", msgs(&out));
    }

    #[test]
    fn a_categorical_position_bounds_a_zone_and_a_continuous_one_does_not() {
        // Spec §5's fourth extent description, which is the whole of the tile plot:
        // **a category owns a slot, a number is a point.** Nothing else changes —
        // no transform, no channel, no new vocabulary — so the test is that one
        // sentence read off `check`, one axis at a time.
        let bare = |x: &str, y: &str| {
            check(&PlotSpec::new().data("t").x(x).y(y).layer(Layer::new(Mark::Zone)), &data())
                .iter().any(|d| d.kind == DiagnosticKind::Illegal
                    && d.message.contains("nothing here says where its sides are"))
        };
        assert!(bare("gdp", "life"), "two numbers bound nothing: a point has no width");
        assert!(!bare("continent", "region"), "two categories are a mesh — the tile plot");
        // And one of each still draws: bounded on the categorical axis, spanning the
        // panel on the other. That is `rule`'s relaxation arriving a third time, and
        // it falls out of the same sentence rather than being a case of its own.
        assert!(!bare("continent", "life"), "a category bounds its own axis");
        assert!(!bare("gdp", "region"), "and so does the other one");
    }

    // -----------------------------------------------------------------------
    // The two-dimensional readings — `path * density` (the contour) and
    // `zone * density` (the estimated heatmap)
    //
    // The dimensionality is read off the mark, so these tests are about the *rule*
    // rather than about two features: what makes both positions compulsory, what
    // makes the measurement unbindable, and which knob means nothing in which
    // reading. Written against `has_no_measure_axis` so a third such mark inherits
    // them rather than needing its own copy.
    // -----------------------------------------------------------------------

    #[test]
    fn a_field_needs_both_axes_because_it_cuts_both() {
        // A `bar * density` invents its measure and needs one axis. A field has no
        // axis to invent onto, so a missing position is refused rather than drawn as
        // an empty panel — the silent drop §12 forbids.
        for mark in [Mark::Path, Mark::Zone] {
            for spec in [
                PlotSpec::new().data("t").x("gdp"),
                PlotSpec::new().data("t").y("life"),
            ] {
                let s = spec.layer(Layer::new(mark.clone()).transform(Transform::Density));
                let d = check(&s, &data());
                assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
                    && x.message.contains("both axes")),
                    "{mark:?} * density with one axis must be refused: {:?}", msgs(&d));
            }
        }
        // With both, it draws.
        for mark in [Mark::Path, Mark::Zone] {
            let s = base().layer(Layer::new(mark.clone()).transform(Transform::Density));
            assert!(!check(&s, &data()).iter().any(|x| x.is_fatal()),
                "{mark:?} * density with both axes is legal: {:?}", msgs(&check(&s, &data())));
        }
    }

    #[test]
    fn a_field_refuses_a_color_it_did_not_compute_and_names_the_one_it_did() {
        // Both positions are spoken for, so the measurement goes to `color` — and any
        // *other* column is a request for rows the transform replaced. The refusal
        // names the binding that does mean something (Law 5), which differs by
        // reading: a ring has a level, a cell has a density.
        for (mark, synthesized) in [
            (Mark::Path, crate::transform::FIELD_LEVEL),
            (Mark::Zone, crate::transform::FIELD_DENSITY),
        ] {
            let bad = base().layer(Layer::new(mark.clone())
                .transform(Transform::Density)
                .encode(Channel::Color, "continent"));
            let d = check(&bad, &data());
            assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
                && x.message.contains(synthesized)),
                "{mark:?} * density must refuse color(continent) toward `{synthesized}`: {:?}",
                msgs(&d));

            // Naming it out loud is the same plot, and must not be refused as a
            // missing column — the column exists only downstream of the transform.
            let good = base().layer(Layer::new(mark.clone())
                .transform(Transform::Density)
                .encode(Channel::Color, synthesized));
            assert!(!check(&good, &data()).iter().any(|x| x.is_fatal()),
                "color({synthesized}) says the default out loud: {:?}",
                msgs(&check(&good, &data())));
        }
    }

    #[test]
    fn a_field_refuses_a_categorical_axis_toward_the_curve_per_group() {
        // `bin`'s refusal one transform over: an estimate needs an interval to spread
        // along, and a category is one slot. The direction is the plot the user
        // probably wanted — one density curve per category.
        let s = PlotSpec::new().data("t").x("continent").y("life")
            .layer(Layer::new(Mark::Path).transform(Transform::Density));
        let d = check(&s, &data());
        assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
            && x.message.contains("line * density")),
            "a categorical axis on a contour points at the per-group curve: {:?}", msgs(&d));
    }

    #[test]
    fn each_density_knob_is_refused_in_the_reading_it_cannot_mean() {
        // The mirror of `bin(tiling = )`'s refusal, both halves.
        let estimated = |mark: Mark, levels: Option<usize>, bandwidth: Option<f64>| {
            let mut l = Layer::new(mark).transform(Transform::Density);
            l.density = Some(crate::ir::DensitySpec { adjust: None, bandwidth, levels , compare: None, reach: None });
            base().layer(l)
        };

        // `levels` counts iso-lines, and a curve is one line.
        let d = check(&estimated(Mark::Line, Some(8), None), &data());
        assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
            && x.message.contains("path * density(levels")),
            "levels on a curve is refused toward the contour: {:?}", msgs(&d));

        // A `zone` **is** legal with `levels`, and the history is worth keeping because
        // the bug and the feature were the same discovery. `zone * density(levels = 4)`
        // was first accepted and ignored — byte-identical output, the silent drop §12
        // forbids. It was then briefly *refused*, which closed the drop by deleting the
        // request. What it actually means is the third reading: cut the field into
        // levels and **fill** the bands, whose edges are the very curves a `path`
        // strokes. So the parameter is answered rather than removed.
        let banded = estimated(Mark::Zone, Some(4), None);
        assert!(!check(&banded, &data()).iter().any(|x| x.is_fatal()),
            "levels on a zone fills the bands: {:?}", msgs(&check(&banded, &data())));

        // `bandwidth` is a width in one column's units, and a field has two columns
        // measuring different things — so it is refused toward the dimensionless knob.
        let d = check(&estimated(Mark::Path, None, Some(5.0)), &data());
        assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
            && x.message.contains("adjust")),
            "bandwidth on a field is refused toward adjust: {:?}", msgs(&d));

        // And each is legal in its own reading.
        for (mark, levels, bandwidth) in [
            (Mark::Line, None, Some(5.0)),
            (Mark::Path, Some(8), None),
            (Mark::Zone, Some(8), None),
        ] {
            let s = estimated(mark.clone(), levels, bandwidth);
            assert!(!check(&s, &data()).iter().any(|x| x.is_fatal()),
                "{mark:?} with its own knob is legal: {:?}", msgs(&check(&s, &data())));
        }

        // `adjust` is dimensionless, so it reaches every reading — the painted field
        // included, which is what makes the refusal above a narrowing of one knob
        // rather than of the mark.
        let mut z = Layer::new(Mark::Zone).transform(Transform::Density);
        z.density = Some(crate::ir::DensitySpec { adjust: Some(1.6), bandwidth: None, levels: None , compare: None, reach: None });
        let s = base().layer(z);
        assert!(!check(&s, &data()).iter().any(|x| x.is_fatal()),
            "adjust is legal on a painted field: {:?}", msgs(&check(&s, &data())));
    }

    #[test]
    fn a_field_does_not_synthesize_a_measure_so_a_mistyped_axis_is_still_caught() {
        // The hole `synthesizes_measure` would have had without the mark: `density`
        // is on the synthesizing list, so asked of the transforms alone it would
        // exempt a field's `y` from the column check and draw nothing at all.
        assert!(!synthesizes_measure(&Mark::Path, &[Transform::Density]),
            "a contour binds both positions and invents neither");
        assert!(synthesizes_measure(&Mark::Line, &[Transform::Density]),
            "a curve still invents its measure");
        let s = PlotSpec::new().data("t").x("gdp").y("nosuch")
            .layer(Layer::new(Mark::Path).transform(Transform::Density));
        let d = check(&s, &data());
        assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
            && x.message.contains("nosuch")),
            "a mistyped y on a contour is reported, not skipped: {:?}", msgs(&d));
    }

    #[test]
    fn a_zone_may_not_have_its_sides_named_and_cut_at_once() {
        // Two extent sources disagree the moment the bin layout moves, so this is
        // refused rather than resolved by letting one quietly win.
        let d = check(&PlotSpec::new().data("t").x("gdp").y("life")
            .layer(Layer::new(Mark::Zone).transform(Transform::Bin).bounds("lo", "hi")), &data());
        assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
            && x.message.contains("twice")),
            "`zone * bounds * bin` says where the sides are twice: {:?}", msgs(&d));
    }

    #[test]
    fn a_binned_zone_needs_both_axes_and_colors_by_the_count() {
        // Both positions. A *bounded* zone may name neither axis and still draw —
        // it spans the panel where it is not given a pair — so this requirement is
        // the binned reading's alone, and its absence used to be an empty panel.
        for spec in [
            PlotSpec::new().data("t").x("gdp"),
            PlotSpec::new().data("t").y("life"),
            PlotSpec::new().data("t"),
        ] {
            let d = check(&spec.layer(Layer::new(Mark::Zone).transform(Transform::Bin)), &data());
            assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
                && x.message.contains("cuts both axes")),
                "a binned zone missing an axis must be refused: {:?}", msgs(&d));
        }

        // The count *is* the color, so any other field names a column the
        // transform has already replaced. Refused, naming the one binding that
        // still means something.
        let d = check(&PlotSpec::new().data("t").x("gdp").y("life")
            .layer(Layer::new(Mark::Zone).transform(Transform::Bin)
                .encode(Channel::Color, "continent")), &data());
        assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
            && x.message.contains("color(count)")),
            "a foreign color on a binned zone is refused toward `count`: {:?}", msgs(&d));

        // And naming it out loud is legal — the long form of what the short form
        // already does (Law 5), so it must not be refused as a missing column.
        let d = check(&PlotSpec::new().data("t").x("gdp").y("life")
            .layer(Layer::new(Mark::Zone).transform(Transform::Bin)
                .encode(Channel::Color, "count")), &data());
        assert!(!d.iter().any(|x| x.is_fatal()),
            "`color(count)` names the synthesized column: {:?}", msgs(&d));
    }

    #[test]
    fn bin_cuts_and_count_tallies_on_a_zone_as_they_do_on_a_bar() {
        // One rule with two polarities, and the pair is what makes it a rule rather
        // than two refusals: a **cut** needs room to cut, so `bin` refuses a category;
        // a **tally** needs cells that already exist, so `count` refuses a number.
        // The same division `bar * bin` and `bar * count` have had all along, with
        // both axes in it — which is why each refusal names the *other* transform.
        //
        // Read one axis at a time, which is what the mixed mesh below turns on: a
        // `bin` refuses only where **neither** axis can be cut.
        let d = check(&PlotSpec::new().data("t").x("continent").y("region")
            .layer(Layer::new(Mark::Zone).transform(Transform::Bin)), &data());
        assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
            && x.message.contains("no width to cut") && x.message.contains("zone * count")),
            "two categorical axes leave a cut nothing to cut: {:?}", msgs(&d));
        assert!(!d.iter().any(|x| x.message.contains("bar * count")),
            "and not toward a bar chart, which is a different plot: {:?}", msgs(&d));

        let d = check(&PlotSpec::new().data("t").x("gdp").y("region")
            .layer(Layer::new(Mark::Zone).transform(Transform::Count)), &data());
        assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
            && x.message.contains("owns no cell") && x.message.contains("`bin`")),
            "a continuous axis on a tile plot is refused toward the cut: {:?}", msgs(&d));

        // Both categorical is the tile plot, and nothing about it is fatal.
        let d = check(&PlotSpec::new().data("t").x("continent").y("region")
            .layer(Layer::new(Mark::Zone).transform(Transform::Count)), &data());
        assert!(!d.iter().any(|x| x.is_fatal()), "the tile plot draws: {:?}", msgs(&d));
    }

    #[test]
    fn one_categorical_axis_is_the_mixed_mesh_and_two_is_nothing_to_cut() {
        // The rule above read one axis at a time. A `bin` needs *something to cut*,
        // not two continuous axes — so exactly one category is legal, and it is the
        // mixed mesh: the continuous axis cut into cells, one row of them per
        // category. Both orientations, because a mesh that worked one way round and
        // not the other would be the Law-2 exception the pair exists to refuse.
        for (x, y) in [("gdp", "region"), ("region", "gdp")] {
            let d = check(&PlotSpec::new().data("t").x(x).y(y)
                .layer(Layer::new(Mark::Zone).transform(Transform::Bin)), &data());
            assert!(!d.iter().any(|x| x.is_fatal()),
                "`zone * bin + x({x}) + y({y})` is the mixed mesh: {:?}", msgs(&d));
        }

        // A **field** is not the same case and keeps needing two continuous axes: a
        // density per category is a *conditional* estimate, normalized within each
        // slot, so its cells would not be comparable across slots while the ramp
        // said they were. That is the which-margin-normalizes question, not this one.
        let d = check(&PlotSpec::new().data("t").x("gdp").y("region")
            .layer(Layer::new(Mark::Zone).transform(Transform::Density)), &data());
        assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
            && x.message.contains("Both axes must be continuous")),
            "a field still needs a plane to spread over: {:?}", msgs(&d));
    }

    #[test]
    fn a_mixed_mesh_has_two_axes_and_still_no_plane_to_tile() {
        // A tiling partitions a *plane*, and `hex` interleaves two lattices by
        // weighting a step in y against a step in x — so it needs a distance on both
        // axes. A category's slots are an order, not a metric. `"rect"` is what the
        // mixed mesh already cuts, so only a non-rectangular tiling is refused.
        let tiled = |t: &str| {
            let mut l = Layer::new(Mark::Zone).transform(Transform::Bin);
            l.bin = Some(crate::ir::BinSpec {
                bins: None, width: None, tiling: Some(t.to_string()),
            });
            check(&PlotSpec::new().data("t").x("gdp").y("region").layer(l), &data())
        };
        let d = tiled("hex");
        assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
            && x.message.contains("an order, not a distance")),
            "a hexagon has nothing to be regular against here: {:?}", msgs(&d));
        assert!(!tiled("rect").iter().any(|x| x.is_fatal()),
            "and rectangles are what it cuts anyway: {:?}", msgs(&tiled("rect")));
    }

    #[test]
    fn a_tiling_is_validated_and_belongs_to_a_mark_with_a_plane() {
        let binned = |mark: Mark, tiling: &str| {
            let mut l = Layer::new(mark).transform(Transform::Bin);
            l.bin = Some(crate::ir::BinSpec {
                bins: None, width: None, tiling: Some(tiling.to_string()),
            });
            PlotSpec::new().data("t").x("gdp").y("life").layer(l)
        };

        // Every declared tiling is legal on the mark that has a plane to tile.
        for t in TILINGS {
            let d = check(&binned(Mark::Zone, t), &data());
            assert!(!d.iter().any(|x| x.is_fatal()),
                "`zone * bin(tiling = \"{t}\")` must be legal: {:?}", msgs(&d));
        }

        // An invented one is a typo, answered with the list.
        let d = check(&binned(Mark::Zone, "octagon"), &data());
        assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
            && x.message.contains("is not a tiling")),
            "an invented tiling is refused: {:?}", msgs(&d));

        // A tiling on a one-dimensional bin is not a typo, it is a reasonable
        // guess — so the refusal explains that an interval has no shape and names
        // the mark that does tile a plane.
        let d = check(&binned(Mark::Bar, "hex"), &data());
        assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
            && x.message.contains("interval has no shape")
            && x.message.contains("zone")),
            "a tiling on a histogram is refused toward `zone`: {:?}", msgs(&d));
    }

    #[test]
    fn a_rule_takes_no_transform_and_no_second_position() {
        // Two facts that are the mark's definition seen from other axes of the
        // grid, both pinned so a later change cannot quietly grant them.
        for t in USER_TRANSFORMS {
            assert_eq!(mark_takes_transform(&Mark::Rule, &t), TransformLegality::None,
                "`rule * {}` must not combine", transform_name(&t));
        }
        // A rule has nothing to connect, so `group` is refused the way it is on
        // `point`; and it has no glyph, so `shape` is too.
        for c in [Channel::Group, Channel::Shape, Channel::Label] {
            assert_eq!(rule_for(&Mark::Rule, &c).obligation, Obligation::Cannot,
                "`{}` must be refused on a rule", channel_name(&c));
        }
    }

    #[test]
    fn a_positionless_bar_refuses_the_statistics_that_need_an_axis() {
        // `count`/`sum` answer "one value for these rows", which means something
        // with nothing to group by. `bin`/`density`/`smooth` describe how values are
        // spread *along* an axis, and there is no axis — refused with direction
        // rather than quietly returning the input unchanged (§12).
        let fatal = |d: &[Diagnostic]| d.iter().any(|x| x.is_fatal());
        let with = |t: Transform| PlotSpec::new().data("t").y("life").layer(
            Layer::new(Mark::Bar).transform(t).encode(Channel::Color, "continent"));

        for t in [Transform::Count, Transform::Sum, Transform::Mean, Transform::Proportion] {
            assert!(!fatal(&check(&with(t.clone()), &data())), "{t:?} should be legal with no x");
        }
        for t in [Transform::Bin, Transform::Density, Transform::Smooth] {
            let d = check(&with(t.clone()), &data());
            assert!(
                d.iter().any(|x| x.kind == DiagnosticKind::Illegal && x.message.contains("Add `x(<column>)`")),
                "{t:?} with no x should be refused with direction: {:?}",
                d.iter().map(|x| x.message.clone()).collect::<Vec<_>>()
            );
        }
    }

    // The settable rule (spec §4): a setting spans its geometry class with no
    // per-mark gap, and is refused off it. These test the class membership directly
    // through the check functions — the settable analog of
    // `every_mark_channel_pair_has_a_rule`.

    #[test]
    fn border_spans_the_closed_glyph_fills() {
        // A border rims a filled glyph — available on the closed-glyph fills (bar,
        // box, point, and since 2026-07-27 `zone`, the region mark the mosaic could
        // not be read without), refused on the curve fills (edge is a layered line),
        // the strokes, and text. None answers `Unsupported`: the class is built, not
        // "designed but not drawn."
        //
        // `zone` was the gap this test could not see: it sat in *neither* list, so
        // moving it from one side of the class to the other left the assertion green
        // either way. Naming every mark is what makes a class membership testable —
        // the same lesson `pattern_spans_*` below records one paragraph down.
        let style = StyleSpec { border_color: Some("black".into()), border_size: Some(1.0), ..Default::default() };
        for m in [Mark::Bar, Mark::Box, Mark::Point, Mark::Zone] {
            let mut out = Vec::new();
            check_border(&mut out, &m, &style);
            assert!(out.is_empty(), "{m:?} accepts a border with no complaint: {out:?}");
        }
        for m in [Mark::Area, Mark::Ribbon, Mark::Line, Mark::Step, Mark::Interval, Mark::Text] {
            let mut out = Vec::new();
            check_border(&mut out, &m, &style);
            assert!(out.iter().any(|d| d.kind == DiagnosticKind::Illegal),
                "{m:?} refuses a border with direction: {out:?}");
        }

        // And the drift guard the two lists above cannot be: walk **every** mark and
        // require the class's two statements to agree. `mark_takes_setting` is what
        // `check_style` gates on and what the book's generated grid prints;
        // `check_border` is what the caller actually meets. A mark named in neither
        // hand-written list is invisible to the assertions above, which is exactly
        // how `zone` and `surface` both sat outside this test while being inside the
        // class it describes.
        for m in &ALL_MARKS {
            let mut out = Vec::new();
            check_border(&mut out, m, &style);
            let refused = out.iter().any(|d| d.kind == DiagnosticKind::Illegal);
            assert_eq!(refused, !mark_takes_setting(m, Setting::BorderColor),
                "{m:?}: `check_border` and `mark_takes_setting` disagree about the \
                 closed-glyph-fill class");
        }
    }

    #[test]
    fn pattern_spans_the_path_strokes_and_the_fills() {
        // `pattern` is a general texture aesthetic (spec §4), realized per geometry:
        // the dash on the path strokes, a hatch on the fills — *both built*, so no
        // member of either class answers `Unsupported` any more. Each geometry takes
        // its own values; the glyphs (`point`/`text`) have no region to texture.
        let pat = |v: &str| StyleSpec { pattern: Some(v.into()), ..Default::default() };

        // The drift guard, and the one that was missing: walk **every** mark and
        // require the two statements of the class to agree. `rule_for(_).settable`
        // is what the book's generated grid prints and what `check_style` gates
        // on; `texture_of` is what the value check reads. `path` shipped with
        // those two disagreeing — the grid promised a dash the checker refused —
        // because both this test and the checker enumerated marks by hand.
        for m in &ALL_MARKS {
            assert_eq!(
                texture_of(m).is_some(),
                rule_for(m, &Channel::Pattern).settable,
                "{m:?}: the grid and the value check disagree about `pattern`"
            );
            // And whichever class it is in, its own values are accepted and the
            // other class's are refused with direction — no mark is in a class
            // whose values it then rejects.
            let Some(kind) = texture_of(m) else { continue };
            let (mine, theirs): (&[&str], &[&str]) = match kind {
                Texture::Dash => (&STROKE_DASHES, &["hatch"]),
                Texture::Hatch => (&FILL_TEXTURES, &["dashed"]),
            };
            for v in mine {
                let mut out = Vec::new();
                check_pattern(&mut out, m, &pat(v));
                assert!(out.is_empty(), "{m:?} must accept its own {v:?}: {out:?}");
            }
            for v in theirs {
                let mut out = Vec::new();
                check_pattern(&mut out, m, &pat(v));
                assert!(out.iter().any(|d| d.kind == DiagnosticKind::Illegal),
                    "{m:?} must refuse the other class's {v:?}: {out:?}");
            }
        }

        // **The refusal a glyph gets must name every mark that does take a texture.**
        // It listed them by hand and had gone stale: the fills read
        // "`bar`/`box`/`area`/`ribbon`", omitting `zone`, which has taken a hatch since
        // it shipped — so the engine told a reader in its own voice that the mark they
        // might want a texture on cannot have one. Asserted against `texture_of`
        // rather than against a second typed list, which is the only version of this
        // test that cannot go stale the same way (`check_polar`'s lesson).
        for glyph in [Mark::Point, Mark::Text] {
            let mut out = Vec::new();
            check_pattern(&mut out, &glyph, &pat("hatch"));
            let msg = &out[0].message;
            for m in &ALL_MARKS {
                let Some(kind) = texture_of(m) else { continue };
                assert!(
                    msg.contains(&format!("`{}`", mark_name(m))),
                    "the {glyph:?} refusal omits `{}`, which takes a {kind:?}: {msg}",
                    mark_name(m)
                );
            }
        }

        // Strokes take the dash values; a fill texture on a stroke is refused.
        for m in [Mark::Line, Mark::Step, Mark::Interval, Mark::Path, Mark::Rule] {
            for v in ["solid", "dashed", "dotted"] {
                let mut out = Vec::new();
                check_pattern(&mut out, &m, &pat(v));
                assert!(out.is_empty(), "{m:?} accepts the stroke pattern {v:?}: {out:?}");
            }
            let mut out = Vec::new();
            check_pattern(&mut out, &m, &pat("hatch"));
            assert!(out.iter().any(|d| d.message.contains("dotted")),
                "{m:?} refuses a fill texture on a stroke, pointing at the dashes: {out:?}");
        }

        // Fills take the hatch values — none `Unsupported`, the class is closed.
        for m in [Mark::Bar, Mark::Box, Mark::Area, Mark::Ribbon] {
            for v in FILL_TEXTURES {
                let mut out = Vec::new();
                check_pattern(&mut out, &m, &pat(v));
                assert!(out.is_empty(), "{m:?} accepts the fill texture {v:?}: {out:?}");
            }
            // A stroke's dash on a fill is refused with direction — never reserved.
            let mut out = Vec::new();
            check_pattern(&mut out, &m, &pat("dashed"));
            assert!(out.iter().any(|d| d.kind == DiagnosticKind::Illegal && d.message.contains("hatch")),
                "{m:?} refuses a stroke dash on a fill, pointing at the textures: {out:?}");
            assert!(!out.iter().any(|d| d.kind == DiagnosticKind::Unsupported),
                "{m:?} no longer reserves the fill texture: {out:?}");
        }

        // The glyphs refuse any texture.
        for m in [Mark::Point, Mark::Text] {
            let mut out = Vec::new();
            check_pattern(&mut out, &m, &pat("hatch"));
            assert!(out.iter().any(|d| d.kind == DiagnosticKind::Illegal), "{m:?} refuses a pattern: {out:?}");
        }

        // An unknown value is refused on each geometry, each pointing at its own set.
        let mut out = Vec::new();
        check_pattern(&mut out, &Mark::Line, &pat("wiggly"));
        assert!(out.iter().any(|d| d.message.contains("dotted")), "unknown stroke pattern → the dash set: {out:?}");
        let mut out = Vec::new();
        check_pattern(&mut out, &Mark::Bar, &pat("wiggly"));
        assert!(out.iter().any(|d| d.message.contains("crosshatch")), "unknown fill pattern → the texture set: {out:?}");
    }

    #[test]
    fn pattern_channel_maps_on_strokes_and_fills_and_refused_on_glyphs() {
        // The mapped `pattern()` channel (spec §5) — `shape`'s twin, dispatching by
        // geometry: a dash per series on the strokes, a hatch per series on the fills.
        // It renders on both classes; the glyph marks refuse it.
        for m in [Mark::Line, Mark::Step, Mark::Interval, Mark::Bar, Mark::Box, Mark::Area, Mark::Ribbon] {
            assert!(rule_for(&m, &Channel::Pattern).renders.is_some(), "{m:?} draws a mapped pattern");
        }
        for m in [Mark::Point, Mark::Text] {
            assert!(rule_for(&m, &Channel::Pattern).renders.is_none(), "{m:?} refuses a mapped pattern");
        }

        // A category maps; a numeric column is refused with direction (discrete, like shape).
        let ok = base().layer(Layer::new(Mark::Line).encode(Channel::Pattern, "continent"));
        assert!(check(&ok, &data()).is_empty(), "a line mapped by a category is legal: {:?}", check(&ok, &data()));
        let bad = base().layer(Layer::new(Mark::Line).encode(Channel::Pattern, "gdp"));
        assert!(check(&bad, &data()).iter().any(|x| x.message.contains("pattern")),
            "a numeric pattern column is refused: {:?}", check(&bad, &data()));

        // Mapping and setting the texture at once is contradictory (like color/shape).
        let both = PlotSpec::new().data("t").x("continent").y("gdp")
            .layer(Layer::new(Mark::Bar).encode(Channel::Pattern, "continent").style_pattern("hatch"));
        assert!(check(&both, &data()).iter().any(|x| x.message.contains("cannot do both")),
            "map + set pattern is refused: {:?}", check(&both, &data()));
    }

    // -- limits: what a stated domain refuses and what it reports (spec §10) --

    /// `x(<field>, limits = …)` on the plot's x axis.
    fn limited(field: &str, lo: Option<f64>, hi: Option<f64>) -> PlotSpec {
        let mut s = base();
        s.x = Some(ChannelDef::field(field).with_limits(lo, hi));
        s
    }

    #[test]
    fn a_stated_domain_that_keeps_everything_says_nothing() {
        // gdp runs 1..3. A domain around it excludes no row, so there is nothing
        // to report — the extension direction is silent, which is what makes it
        // usable for a periodic axis.
        let spec = limited("gdp", Some(0.0), Some(10.0)).layer(Layer::new(Mark::Point));
        assert!(check(&spec, &data()).is_empty(), "{:?}", msgs(&check(&spec, &data())));
    }

    #[test]
    fn excluded_rows_are_counted_aloud_but_still_draw() {
        // The rule is not *never drop a row* but *never drop one in silence*, and
        // here the dropping is the instruction — so it is an Assumption where the
        // same condition under `scale = "log"` is a refusal.
        let spec = limited("gdp", Some(2.0), None).layer(Layer::new(Mark::Point));
        let d = check(&spec, &data());
        assert_eq!(kinds(&d), vec![DiagnosticKind::Assumption], "{:?}", msgs(&d));
        assert!(d[0].message.contains("excludes 1 of 3 rows"), "{}", d[0].message);
    }

    #[test]
    fn a_domain_that_keeps_no_row_is_fatal() {
        // An empty panel with fabricated axes is the failure §12 has been burned
        // by three times, so this one end of the rule is a refusal.
        let spec = limited("gdp", Some(100.0), Some(200.0)).layer(Layer::new(Mark::Point));
        let d = check(&spec, &data());
        assert_eq!(kinds(&d), vec![DiagnosticKind::Illegal], "{:?}", msgs(&d));
        assert!(d[0].message.contains("leaves no rows at all"), "{}", d[0].message);
        assert!(d[0].message.contains("runs 1 to 3"), "it names the range the column has: {}", d[0].message);
    }

    #[test]
    fn a_backwards_domain_is_refused_once_and_not_blamed_on_the_data() {
        // Both a malformed pair and an emptied frame would fire here. Only the
        // first is the caller's actual mistake, and §12 wants one diagnostic per
        // undrawable thing rather than the true-but-useless second.
        let spec = limited("gdp", Some(20.0), Some(5.0)).layer(Layer::new(Mark::Point));
        let d = check(&spec, &data());
        assert_eq!(kinds(&d), vec![DiagnosticKind::Illegal], "{:?}", msgs(&d));
        assert!(d[0].message.contains("runs backwards"), "{}", d[0].message);
        assert!(d[0].message.contains("c(5, 20)"), "it shows the fix: {}", d[0].message);
    }

    #[test]
    fn a_category_has_no_range_to_lie_inside_and_the_refusal_names_order() {
        // ggplot2 gives discrete limits a second meaning — select and reorder.
        // gog has `order()` for that job, so the word keeps one meaning (§13).
        let mut spec = PlotSpec::new().data("t").y("gdp");
        spec.x = Some(ChannelDef::field("continent").with_limits(Some(0.0), Some(5.0)));
        let spec = spec.layer(Layer::new(Mark::Bar));
        let d = check(&spec, &data());
        assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
            && x.message.contains("order(continent)")), "{:?}", msgs(&d));
    }

    #[test]
    fn limits_span_exactly_the_channels_that_read_a_scale() {
        // Law 1: a domain is a scale property, so it reaches every channel that
        // carries a magnitude and no channel that answers *which one?*.
        for c in ALL_CHANNELS {
            let takes = reads_a_scale(&c);
            let field = if takes { "gdp" } else { "continent" };
            let ftype = if takes { VarType::Continuous } else { VarType::Discrete };
            // A mark that actually *has* this channel, so the loop reaches the
            // limits check rather than refusing the binding one step earlier.
            let Some(mark) = ALL_MARKS.iter().find(|m| {
                let r = rule_for(m, &c);
                r.obligation != Obligation::Cannot && r.accepts.accepts(ftype)
            }) else { continue };
            let spec = base().layer(
                Layer::new(mark.clone()).encode_def(
                    c.clone(), ChannelDef::field(field).with_limits(Some(0.0), Some(100.0))));
            let refused = check(&spec, &data()).iter().any(|x|
                x.message.contains("no range along it for limits to cut"));
            assert_eq!(refused, !takes,
                "{c:?} on {mark:?}: limits should be {} here",
                if takes { "accepted" } else { "refused" });
        }
    }

    #[test]
    fn a_tick_count_spans_the_channels_with_an_axis_and_no_others() {
        // The line between `tick_count` and `limits`, stated as a test because it
        // is the one thing about this parameter that is easy to get wrong. Both
        // describe the scale, so both ride the binding — but `limits` states a
        // **domain**, which all six magnitude channels have, while this states how
        // many ticks an **axis** gets, and only the three positions draw one. A
        // legend names three rows structurally, so there is no count to choose.
        for c in ALL_CHANNELS {
            let position = matches!(c, Channel::X | Channel::Y | Channel::Z);
            let Some(mark) = ALL_MARKS.iter().find(|m| {
                let r = rule_for(m, &c);
                r.obligation != Obligation::Cannot && r.accepts.accepts(VarType::Continuous)
            }) else { continue };
            let spec = base().layer(
                Layer::new(mark.clone()).encode_def(
                    c.clone(), ChannelDef::field("gdp").with_tick_count(8)));
            let refused = check(&spec, &data()).iter().any(|x|
                x.message.contains("decoded by a legend rather than by an axis"));
            assert_eq!(refused, !position,
                "{c:?} on {mark:?}: tick_count should be {} here",
                if position { "accepted" } else { "refused" });
        }
    }

    #[test]
    fn a_tick_count_under_two_is_refused_rather_than_clamped() {
        // One tick shows a place and no direction; zero is an axis with no scale
        // on it. Reported rather than quietly replaced by the default, because
        // `tick_count = 1` is a mistake about what a tick is, and drawing five
        // would leave the caller believing otherwise.
        for n in [0usize, 1] {
            let mut spec = base();
            spec.x = Some(ChannelDef::field("gdp").with_tick_count(n));
            let spec = spec.layer(Layer::new(Mark::Point));
            let d = check(&spec, &data());
            assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
                && x.message.contains("at least two ticks")),
                "tick_count = {n} was accepted: {:?}", msgs(&d));
        }
    }

    #[test]
    fn a_categorical_axis_takes_its_tick_count_from_the_data() {
        // The levels *are* the ticks, so a count would have to invent or hide
        // one. Directed at the two atoms that do the jobs a caller might be
        // reaching for, exactly as a categorical `limits` is.
        let mut spec = base();
        spec.x = Some(ChannelDef::field("continent").with_tick_count(3));
        let spec = spec.layer(Layer::new(Mark::Point));
        let d = check(&spec, &data());
        assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
            && x.message.contains("one tick per category")
            && x.message.contains("order(continent)")), "{:?}", msgs(&d));
    }

    #[test]
    fn a_layer_may_not_state_its_own_tick_count() {
        // Third parameter through the plot-scoped-scale door, and the mildest:
        // two layers asking for different counts is one axis asked to carry two
        // sets of ticks, and whichever came first would win silently.
        let mut spec = base();
        spec.x = Some(ChannelDef::field("gdp").with_tick_count(4));
        let spec = spec.layer(
            Layer::new(Mark::Point).encode_def(
                Channel::X, ChannelDef::field("gdp").with_tick_count(9)));
        let d = check(&spec, &data());
        assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
            && x.message.contains("its own tick count")), "{:?}", msgs(&d));
    }

    #[test]
    fn a_layer_may_not_state_its_own_domain() {
        // One axis, one domain — a layer stating its own is the same two
        // coordinate spaces in one panel that a per-layer *scale* would be.
        let spec = limited("gdp", Some(0.0), Some(10.0)).layer(
            Layer::new(Mark::Point).encode_def(
                Channel::X, ChannelDef::field("gdp").with_limits(Some(0.0), Some(2.0))));
        let d = check(&spec, &data());
        assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
            && x.message.contains("its own limits")), "{:?}", msgs(&d));
    }

    #[test]
    fn a_log_domain_cannot_reach_zero() {
        let mut spec = base();
        spec.x = Some(ChannelDef::field("gdp").with_scale(ScaleType::Log)
            .with_limits(Some(0.0), Some(100.0)));
        let spec = spec.layer(Layer::new(Mark::Point));
        let d = check(&spec, &data());
        assert!(d.iter().any(|x| x.kind == DiagnosticKind::Illegal
            && x.message.contains("undefined at zero and below")), "{:?}", msgs(&d));
    }
}
