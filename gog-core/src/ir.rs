/// Grammar of Graphics — Intermediate Representation
///
/// These typed, serializable structs are the single source of truth for
/// what a plot *is*. Language front-ends (R, Python, Julia) build a value
/// of `PlotSpec` and hand it to a renderer.
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Custom deserializer: accepts {} (normal Rust output) or [] (jsonlite output
// for empty R lists) for HashMap<Channel, ChannelDef>.
// ---------------------------------------------------------------------------

fn deserialize_encodings<'de, D>(
    deserializer: D,
) -> Result<HashMap<Channel, ChannelDef>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::{MapAccess, SeqAccess, Visitor};
    use std::fmt;

    struct MapOrEmpty;

    impl<'de> Visitor<'de> for MapOrEmpty {
        type Value = HashMap<Channel, ChannelDef>;

        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            write!(f, "a channel-encoding map or empty array")
        }

        fn visit_map<M: MapAccess<'de>>(self, mut access: M) -> Result<Self::Value, M::Error> {
            let mut map = HashMap::new();
            while let Some((k, v)) = access.next_entry()? {
                map.insert(k, v);
            }
            Ok(map)
        }

        fn visit_seq<S: SeqAccess<'de>>(self, _: S) -> Result<Self::Value, S::Error> {
            Ok(HashMap::new()) // [] from R → empty map
        }
    }

    deserializer.deserialize_any(MapOrEmpty)
}

// ---------------------------------------------------------------------------
// Marks — the "consonants": visible geometric forms
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mark {
    Point,
    Line,
    Area,
    Bar,
    /// A line that holds each value until it changes, then jumps — a staircase.
    /// The `line` family's sibling: same channels, right-angle interpolation
    /// instead of straight. Drawn as the histogram silhouette under `bin`
    /// (`step * bin`), and as a hold-until-change step otherwise (a CDF, a
    /// survival curve, a rate that steps on the day it changed).
    Step,
    /// A mark that spans *from* a low value *to* a high one at each x — the
    /// error-bar / range whisker. Unlike `bar` (baseline→value), an interval
    /// floats between two extents, so it needs a range-producing transform to
    /// supply them: its minimum syllable is `interval * range` (spec §6). The
    /// extents are transform-synthesized, never new channels — the decision that
    /// keeps `ymin`/`ymax` out of the kernel.
    Interval,
    /// The box-and-whisker glyph — the answer to "how is this distributed, per
    /// group?" (spec §6). Draws the five-number summary at each x: a box from the
    /// lower quartile to the upper, a line at the median, and whiskers out to the
    /// minimum and maximum. Wilkinson's *schema*.
    ///
    /// Unlike `interval` (pure geometry — it spans whatever a range transform
    /// hands it), `box` carries its summary *intrinsically*: a five-number summary
    /// drawn as anything but a box is not a thing, so the statistic is constitutive
    /// of the mark, not an orthogonal transform the user composes (Laws 1–2). Its
    /// minimum syllable is therefore `box + x + y` — no `* range` — and the core
    /// injects [`Transform::Box`] onto the layer (`legality::resolve_scopes`); a
    /// user-typed `box * <transform>` is refused with direction.
    Box,
    /// A filled band spanning *from* a low boundary *to* a high one across a
    /// continuous x — the confidence/spread band. Geometrically it is [`Area`]
    /// (one filled region, no stroke, `opacity` a setting, `color` splits it into
    /// regions), but where an area fills from the data down to a baseline at 0, a
    /// ribbon fills between *two* data-driven boundaries. Those two boundaries are
    /// the low/high pair a range transform synthesizes — the same pair [`Interval`]
    /// spans — so a ribbon **requires** a range transform (`ribbon * range`,
    /// `ribbon * confidence`), refused with direction without one, exactly as
    /// `interval` is. The area geometry is why x is continuous (a band has no
    /// "between" to fill across categories); the interval machinery is why the
    /// extents ride transform-synthesized rows and never new channels.
    Ribbon,
    Text,
    /// A stroke through the rows **in the data's own order** — [`Line`]'s twin,
    /// parting from it on one question: which order the vertices are visited in.
    /// A `line` sorts by `x`, because it draws a *function* (one `y` per `x`,
    /// read along a domain); a `path` visits row 1, then row 2, and may double
    /// back, cross itself, or return where it started. That makes its two axes
    /// the *same* kind of thing — two positions, neither a domain — so its row
    /// is `point`'s, not `line`'s (spec §6's role test).
    ///
    /// Two consequences the mark is built on. **It takes no transform:** every
    /// value statistic replaces the rows with one summary per key, and the row
    /// order a path *is* does not survive that, so `path * mean` is refused with
    /// direction toward `line`. And **it is the mark an arrow can be drawn on**,
    /// because only a path has a direction to point in — `line` throws direction
    /// away when it sorts (`style(arrow = "end"|"start"|"both")`).
    Path,
    /// A mark placed by **one** position, whose other extent the panel supplies —
    /// the reference line at a threshold, and the rug tick at each observation.
    /// Wilkinson's `form.line()` annotation guide, built as a mark because gog
    /// draws data, and its position is a *column* like every other (§18).
    ///
    /// It is one mark rather than the three ggplot2 spells (`vline`/`hline`/
    /// `abline`), because a vertical and a horizontal reference line differ only
    /// in which axis carries the position — read off the bindings, exactly as
    /// `slot_orient` reads a bar's orientation, which is also why there is no
    /// `flip` atom (§6). And it is one mark rather than two (`rule` + `rug`),
    /// because a rug tick is the same geometry reaching a shorter way:
    /// `style(reach = "edge")`, a parameter, not a second mark.
    ///
    /// Needing only one position makes it **Law 7's second relaxation**, and it
    /// clears the same bar the pie's did (spec §4): stated once
    /// (`legality::rule_axis`), relational, and identical in every space — in
    /// `polar` a rule on the angular axis is a spoke and one on the radial axis
    /// is a ring, because *spanning the other axis whole* is what it always
    /// meant.
    Rule,
    /// A filled **rectangle** in data space — the highlighted area. [`Rule`]'s
    /// sibling one dimension up: where a rule takes one *position* from the data
    /// and spans the other axis, a zone takes a *pair* on an axis and spans the
    /// other, and given pairs on both it is a box.
    ///
    /// That "spans the other" is the whole reason it exists as a mark. A
    /// rectangle bounded on *both* axes already draws today — `ribbon *
    /// bounds(lo, hi)` over a two-row table — but a ribbon is bounded by its data,
    /// so it stops at the numbers given and cannot be padded outward without
    /// widening the axis itself, which changes the plot in order to decorate it.
    ///
    /// Its sides ride `bounds` rather than four new channels, on the ruling that
    /// kept `ymin`/`ymax` out of the kernel (spec §6): `bounds(lower, upper)` is
    /// the measure pair every band mark already reads, and `start`/`end` the
    /// domain pair only a rectangle has. **One row is one rectangle**, so one
    /// table draws several — `rule`'s payoff for taking a column rather than a
    /// number, inherited whole.
    ///
    /// It is also the mark a **heatmap cell** will be, which is why it is not a
    /// second bar-like mark (§3's standing objection): a bar's identity is a
    /// length from a baseline, and a heatmap cell measures nothing by length —
    /// its measure is `color` and its extent is its slot on each axis. So a 2-D
    /// `bin` feeds this mark rather than needing one of its own.
    Zone,
    Surface,
}

// ---------------------------------------------------------------------------
// Channels — the "vowels": dimensions that animate a mark
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Channel {
    X,
    Y,
    Z,
    Color,
    Size,
    Shape,
    /// The **texture** a mark's paint carries, mapped from a category — `shape`'s
    /// twin one geometry class over. Realized per geometry (spec §4/§5): on the
    /// fills (`bar`/`box`/`area`/`ribbon`) each category draws as a hatch
    /// (`solid`/`hatch`/`crosshatch`/`grid`/`dots`); on the path strokes
    /// (`line`/`step`/`interval`) as a dash (`solid`/`dashed`/`dotted`). Discrete
    /// like `shape` — there is no distance for a scale to run along — and, like
    /// `color`, it splits a mark into one series per category. `point`/`text`
    /// refuse it (a point's form is `shape`, a string's its content). The settable
    /// counterpart is `style(pattern = )`.
    Pattern,
    Opacity,
    Group,
    /// The string a `text` mark draws — its *content*, supplied by a column
    /// (`label(name)`) the way `x`/`y` supply its position. A content channel,
    /// distinct in kind from `shape`: `shape` selects one glyph from a closed set
    /// of five, whereas a label *is* the datum itself — unbounded, one per row.
    /// `text`'s minimum syllable (§7) requires it; other marks refuse it, so a
    /// labeled scatter is the superposition `point + text`. The first channel
    /// added since `group`.
    Label,
    Play,
}

// ---------------------------------------------------------------------------
// Scales — auto-selected from column type, overridable
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ScaleType {
    Linear,
    Log,
    Time,
    Category,
}

// ---------------------------------------------------------------------------
// Coordinate space — flat (2-D), space (3-D), polar, globe, map
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CoordSpace {
    Flat,
    /// 3-D. Carries its own viewing angle — a *view parameter of the space*, not
    /// a channel or an atom of its own (spec §15). `{"space":{turn,tilt}}` on the
    /// wire; a bare `"space"` is not a legal form because the angle is part of the
    /// space, so R always sends the object.
    Space(SpaceView),
    /// The plane bent into a circle: `x` becomes the angle, `y` the radius
    /// (Wilkinson §9.1.6 — the first dimension is the domain and takes θ). Carries
    /// where the circle starts, the way `Space` carries its viewing angle;
    /// `{"polar":{"start":0}}` on the wire, and a bare `"polar"` is not a legal
    /// form for the same reason a bare `"space"` is not.
    Polar(PolarView),
    Globe,
    /// The sphere flattened onto the page — cartography (spec §15). Carries what
    /// the flattening is asked to preserve, the way `Space` carries its viewing
    /// angle; `{"map":{"preserve":"area"}}` on the wire, and a bare `"map"` is not
    /// a legal form for the same reason a bare `"space"` is not.
    ///
    /// **The cheapest of the four spaces**, and the reason is worth keeping: this
    /// one is an ordinary coordinate transform. Longitude and latitude go in,
    /// projected positions come out, and everything after that is the flat
    /// renderer unchanged. `Polar` bends the normalized plane and `Space`
    /// projects a cube, so both have to be understood by the code that draws;
    /// no mark learns anything about this one (`render/geo.rs`).
    Map(MapView),
    /// The panel packed with nested regions: every row's measure becomes an
    /// **area**, and the areas partition the panel (spec §15). Wilkinson's ch. 13
    /// §13.3.4, "Mapping Nested Space to Euclidean" — the same section family as
    /// the sphere mapping that `Globe`/`Map` will be.
    ///
    /// **The one space that is not a map of the plane**, and the difference shows
    /// up right here in the type: `Space` and `Polar` carry a *view* — an angle you
    /// could turn the same picture through — because there is a picture underneath
    /// to view from somewhere. A packing has no underneath. There is no camera and
    /// no origin, and the two directions are not axes, so `Nest` is a unit variant
    /// and the wire form is the bare string `"nest"`, exactly as `Globe` and `Map`
    /// are. If a knob ever arrives it will be about the *packing* (which
    /// rectangle-fitting rule), never about where the reader stands.
    Nest,
}

impl Default for CoordSpace {
    fn default() -> Self {
        CoordSpace::Flat
    }
}

/// The angle a 3-D scene is viewed from — `space` needs one the way `polar`
/// needs a start angle (spec §15). Two degrees, plainly named: `turn` swings the
/// scene around its upright axis (which side you view it from), and `tilt` lifts
/// the eye above the floor (how steeply you look down). The default is a
/// three-quarter view that shows all three axes at once; `space(turn =, tilt =)`
/// overrides it. `CoordSpace` cannot derive `Eq`/`Hash` once this rides on it —
/// nothing needs it to (it is matched, never keyed or compared).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SpaceView {
    #[serde(default = "default_turn")]
    pub turn: f64,
    #[serde(default = "default_tilt")]
    pub tilt: f64,
}

fn default_turn() -> f64 {
    30.0
}
fn default_tilt() -> f64 {
    25.0
}

impl Default for SpaceView {
    fn default() -> Self {
        Self { turn: default_turn(), tilt: default_tilt() }
    }
}

/// Where a polar plot's circle begins — the one view parameter of `polar`, the
/// counterpart to `space`'s two angles (spec §15). Degrees clockwise from the top,
/// so `0` starts the first category at twelve o'clock and `90` at three. The
/// travel is clockwise because that is the direction a pie is cut and a compass
/// is read; the mathematician's counter-clockwise-from-east is the convention
/// this deliberately does not inherit (spec §9).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PolarView {
    #[serde(default)]
    pub start: f64,
}

impl Default for PolarView {
    fn default() -> Self {
        Self { start: 0.0 }
    }
}

/// What a flattened sphere is asked to get right — the one view parameter of
/// `map` (spec §15).
///
/// A sphere cannot be laid flat without giving something up, and Tissot's theorem
/// says which: **area and angle cannot both survive**. So this names the choice a
/// reader actually makes rather than the cartographer whose name the formula
/// carries, which is also what lets the parameter exist at all. A family behind
/// one knob is refused when its members are different things wearing one name
/// (`smooth(method = "lm" | "loess")`, §18) and allowed when each member is one
/// orthogonal meaning (`scale = "log"`, §10). "Preserves area" and "preserves
/// angle" are the second kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Preserve {
    /// Equal-area — every region gets ink in proportion to its true size.
    ///
    /// The default, because a **choropleth is read by area**: a reader compares
    /// how much ink a region has, so a projection that inflates Greenland tells
    /// them something false about the number inside it.
    #[default]
    Area,
    /// Conformal — every small shape keeps its true form, and area is what pays.
    ///
    /// Kept because Law 8 never forbids the ugly-but-legal. A choropleth drawn
    /// this way is a bad plot and a legal sentence, so it earns an **Assumption**
    /// rather than a refusal.
    Angle,
}

/// The one view parameter of `map`, carried the way `space` carries its angles
/// and `polar` its start (spec §15). `{"map":{"preserve":"area"}}` on the wire,
/// and a bare `"map"` is **not** a legal form, for the same reason a bare
/// `"space"` is not: the choice is part of the space.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MapView {
    #[serde(default)]
    pub preserve: Preserve,
}

// ---------------------------------------------------------------------------
// Transforms — derived chart types ("add-a-stroke" derivations)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Transform {
    Bin,
    Smooth,
    Count,
    Density,
    Sum,
    Mean,
    Median,
    Max,
    Min,
    Proportion,
    /// Per-group minimum and maximum, emitted as a *low, high* pair — the first
    /// transform to synthesize two output columns instead of one. Read by the
    /// `interval` mark (`interval * range`). A reading transform: it needs `y()`.
    Range,
    /// Per-group confidence interval of the mean — a *(low, high)* pair plus a
    /// `center` (the mean), so `interval * confidence` draws a whisker with a
    /// center dot (a pointrange). Uses the t-interval, mean ± t·se; the level
    /// (0.95 default) rides on the layer as [`ConfidenceSpec`]. Reading: needs `y()`.
    Confidence,
    /// The five-number summary per group — minimum, lower quartile, median, upper
    /// quartile, maximum — read by the [`Mark::Box`] mark. Like `range` it emits
    /// the two *extents* (min, max) as a low/high pair of rows in the y column, so
    /// they ride the ordinary axis-domain machinery; the three interior values
    /// travel as `lower`/`middle`/`upper` columns (the way `confidence` carries
    /// `center`). Reading: needs `y()`.
    ///
    /// It has **no user-facing atom** — the `box` mark injects it (see
    /// [`Mark::Box`]). A statistic that draws as exactly one mark is constitutive
    /// of that mark, not an orthogonal transform to compose, so it is never typed.
    Box,
    /// A **reshaping** transform, not a statistic — it *computes nothing*. Where
    /// `range`/`confidence` reduce raw `y` into a low/high pair, `bounds` takes two
    /// columns you already computed (`bounds(lower, upper)`) and emits them as that
    /// same pair of rows, so the span marks (`interval`, `ribbon`) draw a
    /// pre-computed band or error bar with no new plumbing. This is the answer for
    /// the common case a summary transform cannot serve: a confidence band whose
    /// bounds come from a model's SE, a psychometric CSEM, or a bootstrap — computed
    /// upstream, never from replicates in the plot. It synthesizes `y` (like `count`
    /// — the user binds no `y()`; the two named columns *are* the extents), and its
    /// column names ride on the layer as [`BoundsSpec`]. Legal on `interval`/`ribbon`
    /// (refused elsewhere); the pre-computed counterpart to `range`.
    Bounds,
    /// A **collision modifier**, not a statistic — the first *offset* (spec §5).
    /// Where a `color`/`group` split stacks several marks at one shared position,
    /// `dodge` sets them side by side within that position's slot: `G` groups tile
    /// the bar-thickness, each drawn at `1/G` of it. It synthesizes **no rows**
    /// (`transform::apply` treats it as identity) — the offset lives in the
    /// renderer, which reads it off the split field. Legal on the width-bearing
    /// marks whose geometry it subdivides — `bar`, `box`, `interval` — and refused
    /// with direction elsewhere (`point` → `jitter`, `line`/`area` → `stack`), the
    /// division §5 sets out. Takes no parameter: the width is derived from the slot.
    Dodge,
    /// A **collision modifier**, `dodge`'s sibling — the offset for marks that
    /// *sum* (spec §5). Where a `color`/`group` split draws several marks at one
    /// shared position, `stack` piles them along the **measure** axis: each group
    /// sits on the cumulative height of the groups below it. Legal on the marks
    /// whose geometry accumulates — `bar`, `area` — and refused with direction
    /// elsewhere (`point` → `jitter`, `line`/`step` → `area`, `box`/`interval` →
    /// `dodge`). No parameter.
    ///
    /// It diverges from `dodge` in *where* the offset lives, and the divergence is
    /// principled. `dodge` offsets along the *position* axis, which maps a category
    /// to a pixel slot that exists only at render time — so `dodge` synthesizes no
    /// rows and does its work in the renderer. `stack` offsets along the *measure*
    /// axis, and that offset **changes the scale domain** (the tallest stack, not
    /// the tallest single value, sets the axis top). So `stack` is a real
    /// data-space rewrite: it runs in `transform::apply` after the groups recombine
    /// — it is inherently *cross-group* (group *b*'s baseline is group *a*'s height
    /// at the same position), so unlike every statistic it cannot run *within* a
    /// group — rewriting the measure column to each element's cumulative **top** and
    /// emitting a `stack_base` column with its cumulative **bottom**. The measure
    /// axis then extends to the stacked total with no scale special-casing, and the
    /// `bar`/`area` writers read `stack_base` for each element's foot. First group
    /// in category order sits at the bottom.
    Stack,
    /// A **collision modifier**, the third and last offset (spec §5) — the one for
    /// the mark with *no width* to subdivide and no *measure* to pile: `point`.
    /// Where many points share a categorical position (a strip plot), they land on
    /// one line and heavy overlap hides the density; `jitter` spreads them apart.
    ///
    /// It offsets **only along a categorical position axis, never along an axis that
    /// carries a measured value** — in `point * jitter + x(cat) + y(val)` it nudges
    /// `x` (the category, which has no magnitude, so its pixel position means
    /// nothing exact) and leaves `y` untouched (the measurement, which it must not
    /// falsify). This is a deliberate divergence from ggplot2, whose `geom_jitter`
    /// moves both axes and can misread a value. With both axes continuous there is
    /// no band to spread within and nothing jitter may honestly move, so it is
    /// refused toward `style(opacity = )` (`legality::check_jitter`).
    ///
    /// Like `dodge` it synthesizes **no rows** (`transform::apply` treats it as
    /// identity) and does its work in the renderer, bounded to the categorical slot
    /// — so, unlike `stack`, it never changes the scale domain: a jittered point
    /// stays inside its category's band and the axis still reads correctly. The
    /// spread is **deterministic** — a pure function of each row's identity and
    /// data, the way gradient ids hash their stops, never a clock or a global RNG —
    /// so one spec is always one picture. Legal on `point` alone; every other mark
    /// is refused toward the offset its geometry wants (`dodge` for the width marks).
    ///
    /// The spread is derived from the slot by default, but — unlike `dodge`, whose
    /// width is *determined* by the group count — the jitter amount is a free
    /// legibility choice with no single correct value, so it takes an optional knob:
    /// `jitter(amount)` scales the default band ([`JitterSpec`], on the layer like
    /// `bin`/`density`). This is the same reason `density`/`bin`/`smooth` are
    /// parameterized while `count`/`sum`/`dodge` are not: a free parameter earns a
    /// knob; a determined value does not.
    Jitter,
    /// A whole apportioned among **nested parts** — one ring per level of a
    /// hierarchy, each arc as wide as its share. `zone * partition(a, b, c)` flat
    /// is the **icicle**; the same sentence `+ polar()` is the **sunburst**, and
    /// that one-atom difference is the whole of the derivation (Law 6), exactly as
    /// it is for a bar chart and a rose, or a stacked bar and a pie.
    ///
    /// **It is an extent description, which is why it is a transform** (spec §5).
    /// `zone` already reads four of them, and a partition publishes the *first*
    /// one's columns — a cell's four edges plus its center — computed from a tree
    /// instead of from a mesh. That is `bin`'s output shape verbatim, which is what
    /// lets one computation feed a rectangle and a label from the same layer, and
    /// it is why the coordinate space learns nothing: the sunburst needed no new
    /// polar reading, only these columns.
    ///
    /// **The hierarchy is *columns*, not an edge list.** `partition(group, item,
    /// detail)` names the levels outermost first, and each row of the table is a
    /// leaf whose path those columns spell. A `NULL` at a level ends that branch
    /// early, which is what gives a real hierarchy its ragged rim. The
    /// parent/child *edge list* (plotly's `parents=`) is deliberately not this
    /// atom's input: an arbitrary-depth tree from an edge table is the `link`
    /// family §19.1 declines, and a chart's popularity is not an argument for
    /// crossing that line by accident (spec §15, the treemap entry's scope
    /// boundary). Column-shaped nesting is the side of it that was always in scope.
    ///
    /// **The measure rides on `x`**, never inside the atom: `zone * partition(a, b)
    /// + x(amount)` apportions `amount`, on the precedent that `nest()` reads its
    /// measure from a bound position and `bar * bin + x(gdp)` consumes the bound
    /// column. Bind nothing and every leaf weighs 1, which is the tally
    /// `proportion` already does when nothing else measured. Composition then
    /// works without a knob: `* proportion` turns the axis into shares, where a
    /// `share =` parameter would have been the enumeration §5 refuses.
    ///
    /// **Values belong to the leaves.** An interior node carrying a number of its
    /// own is genuinely ambiguous — plotly spells the two readings
    /// `branchvalues="total"` and `"remainder"` — so it is refused with direction
    /// rather than guessed, which is Law 5 and the ruling `bin(30, width = 5)` set.
    Partition,
}

/// Parameters for the `bin` transform, carried on the layer.
///
/// A layer bins at most once, so these ride on the `Layer` rather than inside the
/// `Transform` enum — which keeps `Transform` a clean string-tagged "kind" and
/// avoids a struct variant (an `f64` width would cost the enum its `Eq`). All
/// `None` (the common case) means Sturges' rule chooses the count. `bins` and
/// `width` are mutually exclusive; the R binding refuses both, and the engine
/// prefers `bins` if both somehow arrive.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BinSpec {
    /// Number of bins, given explicitly (`bin(30)`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bins: Option<usize>,
    /// Bin width in the data's own units (`bin(width = 5)`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<f64>,
    /// How the plane is partitioned — `"rect"` (the default) or `"hex"`.
    ///
    /// A *mesh*, not a decoration: a different tiling puts different rows in
    /// different cells, so it changes the counts. That is why it rides on the
    /// transform rather than on the mark (spec §5's tiling ruling), and why it
    /// means nothing to a one-dimensional bin, where the cells are intervals and
    /// an interval has no shape. Validated against `legality::TILINGS`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tiling: Option<String>,
}

/// Parameters for the `density` transform, carried on the layer.
///
/// Mirrors [`BinSpec`]: a layer runs `density` at most once, so its knobs ride on
/// the `Layer` rather than inside the `Transform` enum, keeping `Transform` a
/// clean string-tagged "kind". All `None` (the common case) means Silverman's
/// rule chooses the bandwidth. `adjust` scales that automatic bandwidth
/// (`density(2)` → twice as smooth — the dimensionless everyday knob, like
/// `bin`'s count); `bandwidth` sets it absolutely in the data's units
/// (`density(bandwidth = 5)` — said out loud, like `bin`'s width). The two are
/// mutually exclusive; the R binding refuses both, and the engine prefers
/// `bandwidth` if both somehow arrive.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DensitySpec {
    /// Multiplier on the automatic bandwidth (`density(2)`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adjust: Option<f64>,
    /// Absolute bandwidth in the data's own units (`density(bandwidth = 5)`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bandwidth: Option<f64>,
    /// How many iso-lines the **two-dimensional** reading traces
    /// (`path * density(levels = 8)`). `None` → [`DEFAULT_LEVELS`].
    ///
    /// Exactly parallel to `bin`'s `tiling`, and refused the same way: it is
    /// meaningful only where the transform reads in two dimensions, and
    /// `legality::check_levels` refuses it on a one-dimensional `density` with
    /// direction — a curve is one line, and one line has no iso-lines to count.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub levels: Option<usize>,
    /// What the width means from slot to slot in the **violin** reading
    /// (`density(compare = "count" | "shape")`). `None` → [`DEFAULT_COMPARE`].
    ///
    /// The third knob, and the third to be meaningful in exactly one reading —
    /// `bandwidth` belongs to the curve, `levels` to the field, `compare` to the
    /// slot, and `legality::check_density_params` refuses each in the readings it
    /// cannot mean. It exists because a per-slot density is **conditional**: every
    /// violin's estimate integrates to 1 on its own, so drawn naively the widths
    /// say nothing about how many rows each group has. `compare` is that question
    /// answered out loud rather than left to a default nobody stated (spec §5).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compare: Option<String>,
    /// How far each shape reaches from its slot's line, **in slots**, in the
    /// violin reading (`density(reach = 2.5)`). `None` → [`DEFAULT_REACH`].
    ///
    /// The fourth knob, and the fourth to belong to exactly one reading. Past 0.5
    /// a shape leaves its own slot and runs into its neighbor's, which is not an
    /// accident to be guarded against but the **ridgeline plot** being asked for:
    /// overlapping ridges are that chart's whole look, and refusing them would be
    /// Law 8's ugly-but-legal (`style(opacity = )` and drawing order already say
    /// what a reader needs to tell them apart).
    ///
    /// Measured from the slot's line to the shape's furthest point rather than as
    /// a total width, so the number means the same thing to both marks: a
    /// `ribbon` reaches it **each way** and an `area` **one way**, which keeps the
    /// half violin exactly half of the violin at any value.
    ///
    /// **Why it is a transform knob and not `style(reach = )`.** `reach` on a
    /// `rule` is the same question (*how far across the extent you do not measure
    /// on?*) and the word means the same thing here — but `mark_takes_setting` is
    /// keyed on (mark, setting) and cannot see a transform, so putting it there
    /// would make the book's generated settings table promise `reach` on every
    /// `ribbon`, including the four-fifths of them that are bands with no slot.
    /// A grid that over-promises is the failure the `◌` legend taught,
    /// and the slot is the transform's doing, so its knobs ride where `compare`
    /// already does and `check_density_params` refuses them together.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reach: Option<f64>,
}

/// How far a violin reaches from its slot line when `reach` is not given.
///
/// Four tenths of a slot, so two violins face to face fill four fifths of the
/// space between their categories and the remaining fifth is air — which is
/// exactly a categorical **bar**'s rule (`marks::bar_thickness_svg`), and for the
/// same reason: that gap is what says the categories are separate rather than a
/// divided continuum. One number quoted by the engine, the four bindings and the
/// book; see [`DensitySpec::reach`] for why it is measured one way.
pub const DEFAULT_REACH: f64 = 0.4;

/// Each violin's area proportional to how many rows its group has.
pub const COMPARE_COUNT: &str = "count";
/// Every violin drawn to the same area, whatever its group's size.
pub const COMPARE_SHAPE: &str = "shape";

/// What a violin's width compares when `compare` is not given: the **count**.
///
/// Not the convention — every mature library defaults to equal areas — and chosen
/// against it deliberately, on the argument `legality::check_distribution_axis`
/// had already written down when it refused this reading outright: *a density per
/// category is conditional, each slot's estimate integrating to 1 on its own, so
/// the slots are not comparable and the picture would say they were.* Equal areas
/// are that non-comparability drawn: a group of two gets exactly as much ink as a
/// group of fifty-two, and on real data the two-row group is usually the one with
/// the spike, so it takes the whole panel and squashes every honest violin into a
/// sliver. Weighting by the count makes the widths mean one thing across the
/// panel, which is what lets the eye compare them at all.
///
/// Equal areas remain a real reading and are one word away — comparing the shapes
/// of unequal groups is a genuine question, and `compare = "shape"` asks it. Law 5
/// puts it that way round: the reading that needs saying out loud is the one whose
/// widths cannot be compared between slots.
///
/// Named here so the engine, the four bindings and the book quote one word.
pub const DEFAULT_COMPARE: &str = COMPARE_COUNT;

/// How many iso-lines `path * density` traces when `levels` is not given.
///
/// Six nested rings read as a shape without the innermost ones collapsing into a
/// blot, which is what a larger default does on a single mode. It lives here
/// rather than in the transform so the R binding, the engine and the book quote
/// one number.
pub const DEFAULT_LEVELS: usize = 6;

/// Parameters for the `jitter` collision modifier, carried on the layer like
/// [`BinSpec`]/[`DensitySpec`]. `amount` scales the automatic spread — the
/// slot-derived default band — exactly as `density`'s `adjust` scales the
/// automatic bandwidth: `jitter(0.5)` is half the spread, `jitter(2)` twice,
/// and a bare `jitter` (`None`) is `jitter(1)`. It is a dimensionless multiplier
/// on purpose: jitter only ever applies to a categorical axis, whose slot is the
/// natural unit, so there is no absolute-units knob to pair with it (unlike
/// `bin`/`density`, whose second knob measures in the data's own units).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct JitterSpec {
    /// Multiplier on the automatic (slot-derived) spread (`jitter(0.5)`). `None`
    /// → 1.0, the default band. `0.0` collapses the spread (points un-jittered).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub amount: Option<f64>,
}

/// The hierarchy columns the `partition` transform apportions, carried on the
/// layer like [`BinSpec`] and for the same reason: a layer partitions at most
/// once, so this rides on the `Layer` and leaves `Transform` a clean
/// string-tagged kind.
///
/// Unlike every other spec here it holds **columns rather than knobs**, which
/// puts it beside [`BoundsSpec`] instead: both name the columns the mark's extent
/// is read from, and neither is a parameter the caller tunes. That is also why
/// there is no `parents` field. The two hierarchy shapes are not two spellings of
/// one input — an edge list is a graph, and gog declines graphs (§19.1) — so
/// accepting one here later would be a decision recorded in the spec, not a field
/// quietly added.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PartitionSpec {
    /// The hierarchy's columns, **outermost first**: `partition(group, item,
    /// detail)` puts `group` on the innermost ring and `detail` on the rim.
    ///
    /// Reading order is the same one the sentence writes, which is the same one a
    /// path is spoken in (`Housing / Utilities / Energy`), so nothing has to be
    /// reversed anywhere between the binding and the renderer.
    #[serde(default)]
    pub levels: Vec<String>,
    /// Do the levels **cross** the plane rather than nest down one axis of it?
    ///
    /// The exception to this struct's own rule — a knob, not a column — and it is
    /// here rather than in [`Transform`] for the reason every other knob is, that a
    /// layer partitions at most once. Nested (the default) is the icicle flat and
    /// the sunburst bent; crossed is the **mosaic**, the first level dividing the
    /// width and the second the height within each column, which is Wilkinson's
    /// crossed frame (ch. 11 §11.3.5.5).
    ///
    /// A boolean rather than a named layout, on `stack(share = )`'s precedent: there
    /// are exactly two ways for a level to meet the one above it, along or across,
    /// and a string would invite a third the way §18's `tri` refusal warns about.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub cross: bool,
}

/// Parameters for the `stack` collision modifier, carried on the layer like
/// [`JitterSpec`].
///
/// `share` fills every pile to 1: each element's height is divided by the total of
/// the pile it sits in, so a slot reads as its split's **composition** rather than
/// its size (spec §5). It is a parameter on `stack` and not a second normalizer
/// beside `proportion`, because the two answer different questions and neither can
/// say the other's: `proportion` divides by the **whole frame's** total, so its bars
/// still say how big each slot is, while this divides by the **slot's own** total
/// and deliberately throws that away. A position adjustment — it changes where the
/// marks sit and what the scale reads, never what was counted — which is why it
/// composes with any measurement, `bar * sum * stack(share = true)` as readily as
/// `bar * count * stack(share = true)`. `proportion` could never express the first,
/// having no column to sum.
/// `baseline` says **where the pile hangs**, which is the other free choice a pile
/// has once its heights are fixed. Three answers, and they are one question rather
/// than a family: the reading never changes — a band's *thickness* is the data in all
/// three — so unlike `smooth(method = )` (§18) this is a parameter and not several
/// statistics wearing one name. `"zero"` stands every pile on the axis, which is the
/// plain stacked bar and the default. `"center"` hangs each pile so its midpoint is at
/// zero, which is the ThemeRiver. `"wiggle"` chooses the foot that makes the bands as
/// flat as it can, which is the streamgraph, and it exists because the other two have
/// a *measured* defect: in a floor-anchored stack only the bottom band has a readable
/// shape, since every band above it rides the sum of the ones below.
///
/// Displacing the pile spends the measure axis's origin, so a displaced stack draws no
/// ticks and no axis name (`render::svg`, the rule the sunburst's ring index already
/// runs on): no value on that axis corresponds to any measurement once the foot has
/// moved, and a number a reader can look up and be wrong about is worse than no number.
/// Validated against `legality::BASELINES`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StackSpec {
    /// Fill each pile to 1 (`stack(share = true)`). `None`/`false` piles the values
    /// themselves, which is the plain stacked bar.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub share: Option<bool>,
    /// Where each pile's foot sits — `"zero"` (the default), `"center"`, `"wiggle"`.
    /// Orthogonal to `share`, which scales the heights rather than placing them, so
    /// `stack(share = true, baseline = "center")` is every composition centered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub baseline: Option<String>,
}

/// Parameters for the `confidence` transform, carried on the layer like
/// [`BinSpec`]/[`DensitySpec`]. `level` is the confidence level in (0, 1);
/// `None` means 0.95. Parameterized the same way as `bin`/`density`:
/// `interval * confidence(0.99)` for a wider interval.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConfidenceSpec {
    /// Confidence level in (0, 1) — `confidence(0.99)`. `None` → 0.95.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<f64>,
}

/// Parameters for the `box` mark, carried on the layer like [`ConfidenceSpec`].
/// The one knob is the **whisker rule**: `"tukey"` (the default) runs the whiskers
/// to the most extreme point within 1.5·IQR of the box and draws the points beyond
/// as individual outliers — the standard box plot, Wilkinson's schema; `"range"`
/// runs them to the true minimum and maximum with no outliers (the plain
/// five-number summary). `None` means `"tukey"`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BoxSpec {
    /// `"tukey"` (default) or `"range"` — `box(whiskers = "range")`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub whiskers: Option<String>,
}

/// The pre-computed column names for the `bounds` transform, carried on the
/// layer — `bounds(lower, upper)`, and for [`Mark::Zone`] optionally
/// `bounds(lower, upper, start, end)`. Unlike every other transform spec (a
/// scalar knob), these name *columns*: `bounds` reshapes rather than computes, so
/// it must be told which columns hold the extents.
///
/// **Two pairs, because a rectangle has two.** `lower`/`upper` bound the
/// **measure** axis — the span every band mark already draws — and `start`/`end`
/// bound the **domain** axis, which only a `zone` has an extent along. The names
/// are deliberately *not* screen directions: "left"/"right" would bake in
/// horizontality, and this grammar reads orientation off the bindings and bends
/// into polar, where the domain axis is an angle and "left" means nothing.
///
/// All four are optional so one struct serves both duties, and **which pairs are
/// required is a per-mark question answered in `legality::check_bounds`**: the
/// band marks require `lower`/`upper` and refuse `start`/`end` (they have no
/// domain extent to bound); a `zone` requires at least one complete pair and
/// takes the axis it is *not* given from the panel.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BoundsSpec {
    /// Column holding the lower boundary on the measure axis.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lower: Option<String>,
    /// Column holding the upper boundary on the measure axis.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upper: Option<String>,
    /// Column holding where the rectangle begins along the **domain** axis.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<String>,
    /// Column holding where it ends along the domain axis.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<String>,
}

impl BoundsSpec {
    /// The measure pair, when both halves are named.
    pub fn measure(&self) -> Option<(&str, &str)> {
        Some((self.lower.as_deref()?, self.upper.as_deref()?))
    }
    /// The domain pair, when both halves are named.
    pub fn domain(&self) -> Option<(&str, &str)> {
        Some((self.start.as_deref()?, self.end.as_deref()?))
    }
}

// ---------------------------------------------------------------------------
// OrderSpec — the order of the categorical axis
//
// Applies to the plot-level categorical axis, whichever axis that is: `x` on a
// vertical bar chart, `y` on a horizontal one. The key may be any column in the
// data — the category column itself (alphabetical) or a numeric column (by
// value).
//
// The atom is `order(field)`, not `sort_by(field)`. Every atom that takes a
// column is a noun naming a property, and its argument is the column driving
// it: `color(species)`, `size(population)`, `order(gold)`. `sort_by` was the
// lone survivor of an older `_by` convention that `color_by` → `color` retired;
// the suffix marked the argument as a key, which is redundant when every atom
// in this group takes a key. `sort` would have dropped the suffix but is a verb,
// so `sort(gold)` reads as a command whose object is gold rather than a property
// keyed by it.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderSpec {
    /// Column whose values determine the order.
    pub field: String,
    /// `true` = descending (largest first / Z→A); `false` = ascending.
    #[serde(default)]
    pub descending: bool,
}

// ---------------------------------------------------------------------------
// Channel definition — a field binding + optional scale override
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelDef {
    /// Column name in the bound data table.
    pub field: String,
    /// Explicit scale; `None` means auto-detect from column type.
    #[serde(default)]
    pub scale: Option<ScaleType>,
    /// Base for a `log` scale; `None` means 10.
    ///
    /// Carried beside `scale` rather than inside `ScaleType::Log { base }` for
    /// one practical reason: `ScaleType` goes over the wire as a bare lowercase
    /// string, `"log"`, and a struct variant would change that shape for every
    /// binding whether or not it names a base. It is meaningless without a log
    /// scale, and `legality` says so rather than ignoring it.
    #[serde(default)]
    pub base: Option<f64>,
    /// The domain this channel runs over, when the data is not the authority
    /// (spec §10). `None` derives it from the data, which is the default and the
    /// overwhelmingly common case.
    ///
    /// Two ends, either of which may be `None` on its own: `[Some(0), None]`
    /// pins the baseline and lets the top follow the data. Written this way
    /// rather than as a `(f64, f64)` because a half-stated domain is a real
    /// request, and because the wire shape is then literally the pair every
    /// binding writes — `[0, 24]`, `[0, null]`.
    ///
    /// The values are in the **data's own units**, log scale or not: a log axis
    /// is `limits = [100, 100000]`, never `[2, 5]`. The ticks read in data units
    /// too, so the two agree.
    #[serde(default)]
    pub limits: Option<[Option<f64>; 2]>,
    /// How many ticks this channel's axis should aim for (spec §10). `None` takes
    /// the engine's default of 5. A target rather than a promise: the count picks
    /// a *step*, and the step is then rounded to a human number, so asking for 8
    /// on a 0..100 axis gets a step of 10 and nine ticks.
    ///
    /// **It lives here rather than on `AxisSpec` because it describes the scale,
    /// not the furniture** (§7 declined it for `theme()` on exactly that ground).
    /// Everything that answers *how does this channel measure* rides the binding —
    /// `scale`, `base`, `limits` and now this — and `AxisSpec` is left holding the
    /// axis's **name**, which is the one piece of it that is furniture. It sat on
    /// `AxisSpec` from the founding commit and no binding could reach it, which is
    /// how a property can be real in the IR, read by the renderer, and absent from
    /// the grammar for the whole life of the project.
    ///
    /// Only `x`, `y` and `z` accept it, and that is a narrower set than `limits`
    /// for a stated reason: every magnitude channel has a **domain**, so `limits`
    /// reaches all six, but only the three positions draw an **axis**. A legend
    /// names three rows structurally — both ends and the middle — so there is no
    /// count to ask for. `legality` refuses it elsewhere with that direction.
    #[serde(default)]
    pub tick_count: Option<usize>,
    /// How fast a `play` sequence runs, as a multiplier on the default frame
    /// duration. `None` takes the default; `2.0` is twice as fast.
    ///
    /// **On the binding rather than as a `play_speed()` atom**, which is where §13
    /// used to record it. Everything that answers *how does this channel behave* is
    /// already here — `scale`, `base`, `limits`, `tick_count` — and the test each of
    /// them passed is the one this passes too: it belongs to exactly one channel,
    /// and nothing else can combine with it. A fourteenth top-level atom whose only
    /// legal companion is `play` is the enumeration §5's growth policy refuses,
    /// and it would have been the first atom in the kernel that could not stand in
    /// a sentence by itself.
    ///
    /// Only `play` accepts it, for the reason `tick_count` reaches only `x`/`y`/`z`:
    /// no other channel has a duration to scale. `legality::check_speed` refuses it
    /// elsewhere with that direction.
    #[serde(default)]
    pub speed: Option<f64>,
    /// Fit this axis from each panel's own rows instead of from all of them —
    /// `y(life, free = TRUE)` (spec §11).
    ///
    /// A shared scale is what makes small multiples comparable, and it is also
    /// what makes some of them unreadable: facet a quantity spanning three
    /// orders of magnitude and the axis is spent on the largest panel while the
    /// rest lie flat. This trades the between-panel comparison for the
    /// within-panel one, and because that is a real loss it is asked for rather
    /// than derived.
    ///
    /// **Which axis is freed is not stated here**: it is whichever channel the
    /// request was written on, the same way `wrap`'s direction is whichever
    /// operator carried the facet. That is why there is no `free_x`/`free_y`
    /// vocabulary to enumerate.
    ///
    /// Two consequences travel with it and neither is separately requestable:
    /// the axis's **bin cut** is resolved per panel (a panel with its own scale
    /// wants its own edges), and **every panel draws its own ticks** (one set of
    /// labels is enough only because one scale is).
    ///
    /// Only `x`, `y` and `z` accept it, and only on a *faceted* plot. Refused on
    /// a `play` sequence with no facet: a frame replaces the one before it, so a
    /// per-frame scale would move the axis under the data (§16).
    #[serde(default, deserialize_with = "null_is_false",
            skip_serializing_if = "std::ops::Not::not")]
    pub free: bool,
}

/// Read `null` as `false` for a flag on the wire.
///
/// A binding builds one channel record and fills in whichever settings the
/// caller wrote, so the ones they did not write arrive as JSON `null` rather
/// than absent — that is what every `Option` field on [`ChannelDef`] already
/// tolerates for free. A `bool` does not, and refusing the whole spec because
/// an *unasked-for* flag came across as null would make the wire stricter than
/// the grammar. Absent, null and `false` are the same statement: nobody asked.
fn null_is_false<'de, D>(d: D) -> Result<bool, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Option::<bool>::deserialize(d)?.unwrap_or(false))
}

impl ChannelDef {
    pub fn field(name: impl Into<String>) -> Self {
        Self {
            field: name.into(),
            scale: None,
            base: None,
            limits: None,
            tick_count: None,
            speed: None,
            free: false,
        }
    }

    /// Fit this axis from each panel's own rows — `y(life, free = TRUE)`.
    pub fn with_free(mut self) -> Self {
        self.free = true;
        self
    }

    pub fn with_scale(mut self, scale: ScaleType) -> Self {
        self.scale = Some(scale);
        self
    }

    /// Set the base of a log scale — 2 for doublings, `E` for e-foldings.
    pub fn with_base(mut self, base: f64) -> Self {
        self.base = Some(base);
        self
    }

    /// State the domain rather than deriving it from the data (spec §10).
    pub fn with_limits(mut self, lo: Option<f64>, hi: Option<f64>) -> Self {
        self.limits = Some([lo, hi]);
        self
    }

    /// Aim for `n` ticks on this channel's axis rather than the default 5 (§10).
    pub fn with_tick_count(mut self, n: usize) -> Self {
        self.tick_count = Some(n);
        self
    }

    /// Run a `play` sequence at `n` times the default pace (§15).
    pub fn with_speed(mut self, n: f64) -> Self {
        self.speed = Some(n);
        self
    }

    /// How long one frame of this binding holds, in seconds.
    ///
    /// `None` and a nonsense speed both fall back to [`FRAME_SECONDS`]; a speed
    /// that made no sense has already been refused by `legality::check_speed`, and
    /// this is the permissive path (`GOG_STRICT=0`) drawing anyway rather than
    /// dividing by zero.
    pub fn frame_seconds(&self) -> f64 {
        match self.speed {
            Some(s) if s.is_finite() && s > 0.0 => FRAME_SECONDS / s,
            _ => FRAME_SECONDS,
        }
    }
}

/// How long one frame of a `play` sequence holds at `speed = 1`, in seconds.
///
/// Lives here rather than in the renderer because it is what `speed` is a
/// multiplier *on*: the number is half of a two-part contract whose other half is
/// in the IR, and splitting them would let the binding's meaning and the drawing's
/// behavior drift apart. `legality` reads it to say how long a long sequence will
/// loop for, and `render::svg` reads it to write the SMIL — the two callers that
/// CONTRIBUTING's rule 4 says a shared value must sit below.
///
/// Slow enough to read a redrawn panel, fast enough that a dozen frames are one
/// glance rather than an errand.
pub const FRAME_SECONDS: f64 = 0.8;

// ---------------------------------------------------------------------------
// StyleSpec — constant visual settings: values that are *set*, not *mapped*
//
// The distinction is not cosmetic. A channel maps a data column to a visual
// feature, so it consumes a scale and earns a guide: the reader needs a legend
// to decode it. A style sets that feature to one value for the whole layer, so
// it maps nothing, needs no scale, and earns no guide — there is nothing to
// decode. `color(species)` answers "which species?"; `style(color = "red")`
// answers nothing, it just makes the mark red.
//
// This is why constants are not channels, by the same reasoning that already
// rules rotation out of the channel set: a channel maps a data column to a
// visual feature, and a constant maps nothing.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StyleSpec {
    /// CSS color — a named color (`"red"`) or hex (`"#4e79a7"`).
    #[serde(default)]
    pub color: Option<String>,
    /// Opacity in `0.0..=1.0`. Unlike the channel, this is the literal value —
    /// there is no data range to rescale from.
    #[serde(default)]
    pub opacity: Option<f64>,
    /// How big, in pixels: point radius, or line stroke width.
    #[serde(default)]
    pub size: Option<f64>,
    /// Glyph name — `circle`, `square`, `triangle`, `diamond`, `cross`.
    #[serde(default)]
    pub shape: Option<String>,
    /// The outline color of a filled mark (a bar's rim). A **setting, never a
    /// channel** (spec §5): a 0.5–1px rim has too little area to decode a scale
    /// from, so it fails "a mapping earns a guide". Distinct from `color`, which
    /// is the fill. `None` leaves the engine's derived edge (series hue when
    /// overlaid, the panel-color hairline on a histogram, a faint self-color on
    /// a categorical bar).
    #[serde(default)]
    pub border_color: Option<String>,
    /// The outline width in pixels. `None` keeps the derived default for the bar
    /// kind. Named `border_size`, reusing `size` — the one word the grammar uses
    /// for every stroke width — rather than inventing `thickness`.
    #[serde(default)]
    pub border_size: Option<f64>,
    /// Whether an `interval` draws its end caps (the short crossbars). `None` or
    /// `true` = capped (an error bar); `false` = a bare linerange. Interval-only,
    /// the way `border_*` is bar-only — a setting for a geometry only one mark has.
    #[serde(default)]
    pub caps: Option<bool>,
    /// Whether an `interval` draws its center dot when the statistic supplies one
    /// (a CI's mean). `None` or `true` = draw it (a pointrange); `false` = suppress
    /// it (a bare whisker even from `confidence`). Interval-only, `caps`'s twin.
    /// The setting only *hides* — `range` has no center, so this bites only on
    /// `confidence`; no setting can conjure a center the statistic does not supply.
    #[serde(default)]
    pub center: Option<bool>,
    /// Which way a `text` label is nudged off its point — `"up"`, `"down"`,
    /// `"left"`, or `"right"` — so a superposed `point + text` does not draw the
    /// label on top of the dot. A **constant** displacement (it derives nothing
    /// from the data), so it is a setting, not a `*` collision modifier; the
    /// distance is derived from the font size, not a parameter. Text-only.
    #[serde(default)]
    pub nudge: Option<String>,
    /// The **texture** of a mark's paint (Wilkinson's texture aesthetic) — a general
    /// setting realized per geometry, the way `color` is a fill on a bar and a stroke
    /// on a line. On the **path strokes** (`line`, `step`, `interval`) it is the dash
    /// pattern — `"solid"` (the default), `"dashed"`, `"dotted"` — paint, not
    /// geometry (it never moves the path, so it is a setting, not a mark like `step`).
    /// On the **fills** (`bar`, `box`, `area`, `ribbon`) it will be a fill texture
    /// (hatching, stippling) — a colorblind-safe / grayscale-print aesthetic —
    /// **reserved but not yet built** (`Unsupported` there for now). One name, one
    /// value per mark-class.
    #[serde(default)]
    pub pattern: Option<String>,
    /// Which end of a `path` carries an arrowhead — `"end"`, `"start"`, or
    /// `"both"`. `None` draws a bare stroke.
    ///
    /// **Path-only, and for a reason that is about geometry rather than taste**
    /// (the settable rule, spec §4): an arrowhead marks a *direction*, and a
    /// path is the only mark that has one. A `line` sorts its vertices by `x`,
    /// so its "last" point is wherever the domain happens to end, and a head
    /// there would point at an artifact of the sort rather than at anything the
    /// data says. `interval` already decorates its ends with `caps`.
    ///
    /// A *value* (`"end"`), not a flag, following `pattern` rather than `caps`:
    /// a double-headed arrow (`"both"`) is an ordinary want, so a boolean would
    /// have had to grow a second setting to express it.
    #[serde(default)]
    pub arrow: Option<String>,
    /// How far a `rule` reaches across the axis it does not name — `"panel"`
    /// (the default: all the way, a reference line) or `"edge"` (a short tick at
    /// the start of that axis, a rug).
    ///
    /// **Rule-only**, and not an exception to the settable rule (spec §4): the
    /// geometry class is "a mark whose extent the panel supplies", and `rule` is
    /// its only member — every other mark's extent comes from the data, so there
    /// is nothing for a reach to mean. `caps`/`center` (interval), `nudge`
    /// (text) and `arrow` (path) are the precedent.
    ///
    /// A *value* rather than a boolean, on `arrow`'s reasoning rather than
    /// `caps`': the two readings have names ("a reference line", "a rug"), and a
    /// third reach is a plausible want, where a flag would have to be replaced
    /// rather than extended. There is no distance parameter — the tick length is
    /// derived from the panel, the way `nudge`'s is derived from the font size.
    #[serde(default)]
    pub reach: Option<String>,
}

impl StyleSpec {
    pub fn is_empty(&self) -> bool {
        self.color.is_none()
            && self.opacity.is_none()
            && self.size.is_none()
            && self.shape.is_none()
            && self.border_color.is_none()
            && self.border_size.is_none()
            && self.caps.is_none()
            && self.center.is_none()
            && self.nudge.is_none()
            && self.pattern.is_none()
            && self.arrow.is_none()
            && self.reach.is_none()
    }

    /// Which visual features this style sets, paired with the value as written.
    ///
    /// Style properties are named after channels on purpose: they address the
    /// same visual features, so legality can consult one table for both rather
    /// than growing a second one that could drift out of step.
    pub fn set_features(&self) -> Vec<(Channel, String)> {
        let mut out = Vec::new();
        if let Some(v) = &self.color {
            out.push((Channel::Color, format!("\"{v}\"")));
        }
        if let Some(v) = self.opacity {
            out.push((Channel::Opacity, v.to_string()));
        }
        if let Some(v) = self.size {
            out.push((Channel::Size, v.to_string()));
        }
        if let Some(v) = &self.shape {
            out.push((Channel::Shape, format!("\"{v}\"")));
        }
        out
    }
}

// ---------------------------------------------------------------------------
// Layer — one mark with its channel encodings and optional transforms
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Layer {
    pub mark: Mark,
    #[serde(default, deserialize_with = "deserialize_encodings")]
    pub encodings: HashMap<Channel, ChannelDef>,
    #[serde(default)]
    pub transforms: Vec<Transform>,
    /// Parameters for the `bin` transform, when the layer carries one. `None`
    /// means Sturges' rule chooses the bin count. Absent from the wire when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bin: Option<BinSpec>,
    /// Parameters for the `density` transform, when the layer carries one. `None`
    /// means Silverman's rule chooses the bandwidth. Absent from the wire when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub density: Option<DensitySpec>,
    /// Parameters for the `confidence` transform, when the layer carries one.
    /// `None` means the default 0.95 level. Absent from the wire when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<ConfidenceSpec>,
    /// Parameters for the `box` mark's summary. `None` means the default (Tukey)
    /// whisker rule. Absent from the wire when unset. Rides on the `Layer` like the
    /// transform specs above, even though `box` carries its summary intrinsically —
    /// the mark's one knob still lives where a layer's other statistic knobs do.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#box: Option<BoxSpec>,
    /// Parameters for the `jitter` collision modifier, when the layer carries one.
    /// `None` means the default slot-derived spread. Absent from the wire when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jitter: Option<JitterSpec>,
    /// The hierarchy columns for the `partition` transform (`partition(group,
    /// item, detail)`). Present iff the layer carries [`Transform::Partition`],
    /// which is why `check_partition` can treat its absence as the caller having
    /// named no levels rather than as a default. Absent from the wire when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub partition: Option<PartitionSpec>,
    /// Parameters for the `stack` collision modifier, when the layer carries one.
    /// `None` means the piles read in the measurement's own units. Absent from the
    /// wire when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stack: Option<StackSpec>,
    /// The two column names for the `bounds` transform (`bounds(lower, upper)`).
    /// Present iff the layer carries `Transform::Bounds`. Absent from the wire when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bounds: Option<BoundsSpec>,
    /// Layer-local data override; falls back to `PlotSpec::data` if `None`.
    #[serde(default)]
    pub data: Option<String>,
    /// Constant visual settings. Absent from the wire format when empty.
    #[serde(default)]
    pub style: StyleSpec,
}

impl Layer {
    pub fn new(mark: Mark) -> Self {
        Self {
            mark,
            encodings: HashMap::new(),
            transforms: Vec::new(),
            bin: None,
            density: None,
            confidence: None,
            r#box: None,
            jitter: None,
            partition: None,
            stack: None,
            bounds: None,
            data: None,
            style: StyleSpec::default(),
        }
    }

    /// Attach a `bounds` transform naming its two pre-computed columns
    /// (`interval * bounds(lo, hi)`). Sets both the transform and its spec, the
    /// way `bins`/`jitter_amount` pair a transform with its parameter.
    pub fn bounds(mut self, lower: &str, upper: &str) -> Self {
        self.transforms.push(Transform::Bounds);
        self.bounds = Some(BoundsSpec {
            lower: Some(lower.to_string()), upper: Some(upper.to_string()),
            ..Default::default()
        });
        self
    }

    /// Attach a `partition` transform naming the hierarchy's columns, outermost
    /// first (`zone * partition(group, item, detail)`). Pairs the transform with
    /// its spec exactly as [`Layer::bounds`] does, and for the same reason: both
    /// name columns the mark's extent is read from rather than knobs.
    pub fn partition(mut self, levels: &[&str]) -> Self {
        self.transforms.push(Transform::Partition);
        self.partition = Some(PartitionSpec {
            levels: levels.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        });
        self
    }

    /// The same, **crossed** — the levels alternate axes instead of nesting down
    /// one, which is the mosaic ([`PartitionSpec::cross`]).
    pub fn partition_crossed(mut self, levels: &[&str]) -> Self {
        self = self.partition(levels);
        if let Some(p) = self.partition.as_mut() {
            p.cross = true;
        }
        self
    }

    /// The domain pair a [`Mark::Zone`] takes along the axis a band mark has no
    /// extent on (`zone * bounds(start, end)`). Composes with [`Layer::bounds`]:
    /// call both and the zone is a box, call one and it spans the panel on the
    /// other axis.
    pub fn span(mut self, start: &str, end: &str) -> Self {
        if !self.transforms.contains(&Transform::Bounds) {
            self.transforms.push(Transform::Bounds);
        }
        let mut b = self.bounds.take().unwrap_or_default();
        b.start = Some(start.to_string());
        b.end = Some(end.to_string());
        self.bounds = Some(b);
        self
    }

    /// Scale a `jitter`'s spread by `amount` (`point * jitter(0.5)`). A
    /// dimensionless multiplier of the slot-derived default; `None`/absent is 1.0.
    pub fn jitter_amount(mut self, amount: f64) -> Self {
        self.jitter.get_or_insert_with(JitterSpec::default).amount = Some(amount);
        self
    }

    /// Set an explicit bin count for a `bin` transform (`bar * bin(30)`).
    pub fn bins(mut self, n: usize) -> Self {
        self.bin.get_or_insert_with(BinSpec::default).bins = Some(n);
        self
    }

    /// Set an explicit bin width for a `bin` transform (`bar * bin(width = 5)`).
    pub fn bin_width(mut self, w: f64) -> Self {
        self.bin.get_or_insert_with(BinSpec::default).width = Some(w);
        self
    }

    /// Set a constant color for the whole layer (no scale, no legend).
    pub fn style_color(mut self, color: impl Into<String>) -> Self {
        self.style.color = Some(color.into());
        self
    }

    /// Set a constant opacity in `0.0..=1.0` for the whole layer.
    pub fn style_opacity(mut self, opacity: f64) -> Self {
        self.style.opacity = Some(opacity);
        self
    }

    /// Set a constant size in pixels — point radius, or line stroke width.
    pub fn style_size(mut self, size: f64) -> Self {
        self.style.size = Some(size);
        self
    }

    /// Set a constant glyph for the whole layer.
    pub fn style_shape(mut self, shape: impl Into<String>) -> Self {
        self.style.shape = Some(shape.into());
        self
    }

    /// Set one texture for the whole layer — a stroke's dash or a fill's hatch.
    pub fn style_pattern(mut self, pattern: impl Into<String>) -> Self {
        self.style.pattern = Some(pattern.into());
        self
    }

    /// Set the outline color and width of a filled mark's rim (a bar's border).
    pub fn style_border(mut self, color: impl Into<String>, size: f64) -> Self {
        self.style.border_color = Some(color.into());
        self.style.border_size = Some(size);
        self
    }

    /// Bind a channel to a column name.
    pub fn encode(mut self, channel: Channel, field: impl Into<String>) -> Self {
        self.encodings.insert(channel, ChannelDef::field(field));
        self
    }

    /// Bind a channel to a binding built elsewhere — the general form the
    /// narrower `encode_*` helpers are shorthands for.
    pub fn encode_def(mut self, channel: Channel, def: ChannelDef) -> Self {
        self.encodings.insert(channel, def);
        self
    }

    /// Bind a channel with an explicit scale type.
    pub fn encode_scaled(
        mut self,
        channel: Channel,
        field: impl Into<String>,
        scale: ScaleType,
    ) -> Self {
        self.encodings
            .insert(channel, ChannelDef::field(field).with_scale(scale));
        self
    }

    pub fn transform(mut self, t: Transform) -> Self {
        self.transforms.push(t);
        self
    }

    pub fn data(mut self, name: impl Into<String>) -> Self {
        self.data = Some(name.into());
        self
    }
}

// ---------------------------------------------------------------------------
// AxisSpec — the axis's *furniture*, which is now only its name
//
// It used to carry `tick_count` as well, and that was the wrong home: how many
// ticks an axis gets is a property of the **scale** (§7 declined the same field
// for `theme()` on exactly that ground, and §10 has owned it since). Everything
// answering *how does this channel measure* lives on `ChannelDef` beside `scale`,
// `base` and `limits`, where a binding can write it. Moved 2026-07-26, which is
// also when it first became reachable from any binding at all.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AxisSpec {
    /// Override the auto-derived label. `None` = capitalize the channel's field name.
    pub label: Option<String>,
}

// ---------------------------------------------------------------------------
// PaletteDef — named palette or custom color list
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PaletteDef {
    /// No palette named — the renderer picks one from the color column's type:
    /// the categorical default for text, the sequential default for numbers.
    ///
    /// This has to be its own variant rather than defaulting to `Named("gog")`,
    /// because "the user said nothing" and "the user asked for the categorical
    /// palette" want different answers once a continuous column can be colored:
    /// the first should quietly pick a ramp, the second is a mistake worth
    /// reporting.
    Auto,
    Named(String),
    Custom(Vec<String>),
}

impl Default for PaletteDef {
    fn default() -> Self { PaletteDef::Auto }
}

// ---------------------------------------------------------------------------
// ThemeSpec — the page, not the ink
// ---------------------------------------------------------------------------

/// Plot furniture: everything on the page that is not the data.
///
/// Spec §7 is the ruling. Each of these maps no column, so each is a *setting* —
/// but none of them belongs to a mark either, which is why they are not
/// `style()`. A layer has no gridlines and a plot has no fill, so the two
/// property sets are disjoint; telling them apart by where they were written
/// would make a sub-expression mean different things in different places, which
/// is Law 6. Position decides *who* a thing applies to, never *what it is*.
///
/// `None` everywhere means "the caller said nothing" and the renderer's own
/// defaults stand — the same distinction [`PaletteDef::Auto`] draws, and for the
/// same reason: said-nothing and asked-for-the-default want different answers the
/// moment a preset can supply one.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ThemeSpec {
    /// A named preset, resolved *under* whatever else was set explicitly.
    ///
    /// It travels to the engine as a name rather than being expanded by the
    /// binding, for the usual reason: a rule implemented in a binding is a rule
    /// the other three get wrong (§14). Four bindings expanding one preset is
    /// four chances for them to disagree about what "minimal" means.
    #[serde(default)]
    pub preset: Option<String>,
    /// Which gridlines are drawn: `both` (default), `x`, `y`, `none`.
    ///
    /// `x` means *the gridlines belonging to the x axis*, which run vertically.
    /// Naming them by their axis rather than by their direction is the same
    /// choice `x_label` makes, and it survives `polar()`, where the x axis's
    /// gridlines are spokes and nothing runs vertically at all.
    #[serde(default)]
    pub grid: Option<String>,
    /// The panel's width ÷ height. `Some(1.0)` is a square panel.
    ///
    /// Applied per *panel*, not to the image: a faceted row of circles wants
    /// every circle round, which is a statement about panels. The image keeps
    /// the size it was given and the panels are shrunk and centered inside their
    /// cells, so a ratio never changes what the plot costs to place on a page.
    #[serde(default)]
    pub ratio: Option<f64>,
    /// Degrees to rotate the x tick labels, counterclockwise from horizontal.
    #[serde(default)]
    pub tick_angle: Option<f64>,
    /// The base of the plot's type scale, in pixels: the tick labels' size, and
    /// the number the other two furniture sizes are derived from.
    ///
    /// **One number, not three, and that is a finding rather than a
    /// simplification.** The renderer's three constants — 11 for tick labels, 13
    /// for axis names and legend titles, 16 for the title — are already one
    /// number on a 1.2 scale, rounded: `round(11 × 1.2) = 13` and
    /// `round(11 × 1.2²) = 16`. So a caller states the base and the ratio supplies
    /// the rest, which is the derivation §5's growth policy asks for; three
    /// separate properties would be ggplot2's `axis.text` / `axis.title` /
    /// `plot.title` enumeration, and §7 exists to refuse exactly that. The residue
    /// — "a big title, everything else normal" — has a direction rather than a
    /// knob: the scale is what keeps a plot looking like one plot.
    ///
    /// `None` leaves the renderer's constants alone, so no plot that did not ask
    /// moves by a pixel. `Some(11.0)` reproduces them exactly, which is what makes
    /// the default *expressible* — the same rule §7 puts on presets.
    ///
    /// Named `font_size` rather than `text_size` (`text` is a **mark**),
    /// `base_size` (ggplot2's word, and `base` is already the log base on
    /// `x(gdp, base = 2)`), or `size` (the channel and the setting both have it).
    /// It does **not** name a typeface: the engine measures text with its own
    /// width table and has no font to choose (§18).
    #[serde(default)]
    pub font_size: Option<f64>,
    /// The panel's fill. Any CSS color the rest of the grammar accepts, which
    /// includes `transparent` — the one some journals ask for, and free here
    /// rather than a property of its own because `is_valid_color` already knew it.
    #[serde(default)]
    pub background: Option<String>,
    /// The **facet strip's** fill: the band above a panel that names the level it
    /// holds. Same vocabulary as [`ThemeSpec::background`], `transparent`
    /// included.
    ///
    /// **It exists because `theme("bw")` was painting something no caller could
    /// reach.** The band was a hard-coded near-gray in four places and
    /// `write_strips` was never even handed the theme, so the journal preset
    /// turned the panel white and left five gray bars floating above it. That
    /// broke this section's own rule — *every preset is only a bundle of
    /// properties a caller could set themselves* — which is why the property and
    /// the preset's new entry are one change rather than two: making `bw` cover
    /// the strip **requires** a property for the strip to be covered by.
    ///
    /// Named `strip` and not `strip_background`, on Law 5's *short beats long
    /// unless short is ambiguous*: read aloud it is "the strip is white", exactly
    /// as `background = "white"` is "the background is white". A strip has one
    /// fill and nothing else a theme can reach, so there is no second reading for
    /// the short name to collide with.
    ///
    /// The **play** strip follows it without being asked. `write_play_strip`
    /// has always been documented as *"deliberately the facet strip's strip: same
    /// band, same fill, same type"*, and a property that moved one and not the
    /// other would make that comment false — Law 2 catching a per-guide exception
    /// before it is written.
    #[serde(default)]
    pub strip: Option<String>,
    /// The **ink** of the strip's label. `None` derives it from the band.
    ///
    /// **The derivation is the reason this is not simply a second knob.**
    /// `theme(strip = "black")` on its own would paint the default near-black
    /// label on a near-black band, printing a panel name nobody can read — a
    /// guide that is silently empty, which §12 forbids. So the ink is chosen by
    /// asking which of the default dark and white actually contrasts more against
    /// the band (`color::better_ink`), and a caller who wants the inverted strip
    /// gets it from the one property they already wrote.
    ///
    /// This field is the override, for the band whose ink is a real choice rather
    /// than a legibility question: a navy strip with gold type. Law 8 — the
    /// derivation must not be a cage.
    ///
    /// A band whose color has no luminance to read (`transparent`, `rgb(…)`)
    /// keeps the default dark ink rather than guessing, which is right for the
    /// case that actually occurs: a transparent band shows the page.
    #[serde(default)]
    pub strip_text: Option<String>,
    /// How the panel is bounded: `full` (a rectangle, `theme_bw`'s look), `axes`
    /// (the default — bottom and left only), `none`.
    ///
    /// Called `frame` and not `border` deliberately. `style(border_color = )`
    /// already means *a mark's rim* — a bar's outline, a point's edge — and
    /// reusing the word one scope up for the panel's rectangle is exactly the
    /// double meaning §13 exists to catch. A frame is what surrounds the picture;
    /// a border is what a shape is drawn with.
    #[serde(default)]
    pub frame: Option<String>,
    /// How much room the plot asks for, in pixels. `None` takes the canvas.
    ///
    /// **One meaning in two contexts, which is Law 6 and the reason this is a
    /// theme property rather than a composition argument.** Alone, a plot's width
    /// and height are the image's; composed onto a page, they are its *cell's* —
    /// and the sentence that says a marginal histogram is 120px tall is the same
    /// sentence either way. Siblings that ask for nothing split what is left, so
    /// the common page (nobody asks) divides evenly.
    ///
    /// Separate from [`ThemeSpec::ratio`], which is a statement about the *panel*
    /// and deliberately never resizes the image: a ratio shrinks the panel inside
    /// the room it was given, this decides how much room that is.
    #[serde(default)]
    pub width: Option<f64>,
    #[serde(default)]
    pub height: Option<f64>,
}

/// The presets, and the whole list. Adding one is growth-policy paperwork, not a
/// free-for-all — which is the difference between this and a hundred arguments.
pub const THEME_PRESETS: &[&str] = &["gog", "minimal", "bw"];

/// The panel's default fill, and the frame's color. `PANEL_BG` is a hair off
/// white; `"bw"` is the preset that takes it the rest of the way.
pub const THEME_FRAME_COLOR: &str = "#5a5a64";

impl ThemeSpec {
    /// The effective theme: the caller's own values over the preset's.
    ///
    /// A preset a caller cannot adjust sends them back to asking for knobs, which
    /// is the failure spec §7 exists to prevent — so `theme("minimal", ratio = 1)`
    /// takes the preset's gridlines and the caller's ratio.
    ///
    /// **Every preset is only a bundle of properties a caller could set
    /// themselves**, which is the rule that keeps a preset from becoming a hidden
    /// vocabulary: `theme("bw")` is `theme(background = "white", frame = "full")`
    /// and can be written either way.
    pub fn resolved(&self) -> ThemeSpec {
        let mut out = self.clone();
        match self.preset.as_deref() {
            Some("minimal") => {
                if out.grid.is_none() { out.grid = Some("none".to_string()); }
            }
            // `theme_bw`'s look, and the name is the misleading part of it: what
            // goes black and white is the *furniture*, never the data. A white
            // panel inside a full rectangle is what a journal figure is usually
            // asked for.
            // The strip joined 2026-07-28, and the preset was **wrong without
            // it** rather than merely incomplete: a white panel under a gray
            // band is not the journal look, and the gray reproduces badly in
            // print, which is the one place this preset is for. The framed panel
            // does the separating a tinted band was doing on screen.
            Some("bw") => {
                if out.background.is_none() { out.background = Some("white".to_string()); }
                if out.frame.is_none() { out.frame = Some("full".to_string()); }
                if out.strip.is_none() { out.strip = Some("white".to_string()); }
            }
            _ => {}
        }
        out
    }

    /// The panel's fill, defaulting to the renderer's own.
    pub fn background_or(&self, fallback: &str) -> String {
        self.background.clone().unwrap_or_else(|| fallback.to_string())
    }

    /// The strip band's fill, defaulting to the renderer's own.
    pub fn strip_or(&self, fallback: &str) -> String {
        self.strip.clone().unwrap_or_else(|| fallback.to_string())
    }

    /// Does the panel get a full rectangle rather than the two axis lines?
    pub fn frame_is_full(&self) -> bool {
        self.frame.as_deref() == Some("full")
    }

    /// Is the panel bounded at all?
    pub fn frame_drawn(&self) -> bool {
        self.frame.as_deref() != Some("none")
    }

    /// Does the x axis draw gridlines? (In the flat space they run vertically.)
    pub fn grid_x(&self) -> bool {
        matches!(self.grid.as_deref(), None | Some("both") | Some("x"))
    }

    /// Does the y axis draw gridlines? (In the flat space they run horizontally.)
    pub fn grid_y(&self) -> bool {
        matches!(self.grid.as_deref(), None | Some("both") | Some("y"))
    }
}

// ---------------------------------------------------------------------------
// FacetSpec — small multiples: which columns split the plot into panels
//
// Faceting is Wilkinson's "frames of frames": the facet variable's categories
// form an outer frame whose cells each hold a copy of the inner x/y frame.
// It is written with operators, not an encoding — `plot | facet(cyl)` puts
// panels side by side, `plot / facet(drv)` stacks them, both together cross
// into a grid — because a facet variable is a property of the *plot's* frame,
// not of any one layer or channel.
//
// The grid is a **crossing** in Wilkinson's sense: every row × column
// combination gets a panel, and a combination with no rows still gets an empty
// one, because the frame says the combination is possible even when this data
// has no example of it. (His nesting — draw only the combinations that exist —
// is a different operation, deliberately not claimed by these operators.)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FacetSpec {
    /// Column whose categories run across the page, one panel column each.
    #[serde(default)]
    pub col: Option<String>,
    /// Column whose categories run down the page, one panel row each.
    #[serde(default)]
    pub row: Option<String>,
    /// How many panels before the ribbon turns — `facet(g, wrap = 4)`.
    ///
    /// A one-dimensional facet is a line of panels, and past about six of them
    /// each is narrower than the tick labels under it. This folds the line into
    /// a rectangle. **Which way the line runs is not stated here**: `|` runs
    /// across and `/` runs down, so the operator that made the facet already
    /// said it, and `wrap` only says where the line breaks. That is one number
    /// where ggplot2 has `nrow` *and* `ncol`, and it cannot contradict itself.
    ///
    /// Meaningless on a crossing, which already fills a rectangle —
    /// `legality::check_facet` refuses it there rather than picking a reading.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wrap: Option<usize>,
}

// ---------------------------------------------------------------------------
// BrushDef — a bound on one column's values, which the reader may move
//
// The selection is a **predicate over rows**, and this is how a sentence states
// one: not as a region on the screen but as a bound on a *column*. Everything
// else follows from that choice.
//
// It is hand-writable, so a brush has an honest printed form — `at` says where
// the selection opens exactly as `SpaceView`'s two angles say where a cube
// opens, and the mouse takes the reader anywhere else from there. It needs no
// names and no cross-references: two composed plots respond to one bound because
// they name the same column, which is Bind-Once's world already. And it survives
// every context a sub-expression can appear in, because "gdp is bounded" is true
// of the layer alone, layered, faceted, played and composed (Law 6).
//
// **A brush highlights; it never filters.** Filtering rows before the statistics
// run is what `limits` does, so a brush that filtered would be `limits` with a
// mouse on it — the double meaning §13 exists to catch. The two are the same
// shape and different operations, and the pipeline position is what tells them
// apart: `limit_cut` runs before the transform, a brush after the picture is
// composed. Spec §15.
//
// Nothing selected is the resting state, and it is what a still page shows: with
// neither `at` nor `levels` the plot draws exactly as it would with no brush at
// all, byte for byte, the same way an unplayed plot carries no timing.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BrushDef {
    /// The column the bound is read on.
    ///
    /// **Empty means the positions this plot binds**, which is bare `brush` —
    /// the founding sentence's spelling, and the one that lets a reader select a
    /// region without the author having decided in advance which columns they
    /// were allowed to explore. It is a *declaration* rather than a bound: it
    /// says both positions are selectable and nothing is selected yet, and the
    /// reader's first drag replaces it with one bound per axis. So it never
    /// carries `at` — there is no single axis for one to belong to, and naming a
    /// column is how you say which axis you meant.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub field: String,
    /// Where the selection opens on a column that measures, in the column's own
    /// units. `None` means nothing is selected yet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<[f64; 2]>,
    /// Which slots are selected on a column of categories. The counterpart of
    /// `at`, and never written beside it: which of the two applies is decided by
    /// the *column's type*, exactly as the column decides whether `color` hands
    /// out a palette or a ramp.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub levels: Option<Vec<String>>,
}

impl BrushDef {
    pub fn new(field: impl Into<String>) -> Self {
        Self { field: field.into(), at: None, levels: None }
    }

    /// Bare `brush`: the plot's bound positions, nothing selected yet.
    pub fn positions() -> Self {
        Self { field: String::new(), at: None, levels: None }
    }

    /// Is this the bare form — a region the reader may draw, rather than a
    /// bound on one named column?
    pub fn is_positions(&self) -> bool {
        self.field.is_empty()
    }

    /// Where the selection opens, on a column that measures.
    pub fn at(mut self, lo: f64, hi: f64) -> Self {
        self.at = Some([lo, hi]);
        self
    }

    /// Which slots are selected, on a column of categories.
    pub fn levels(mut self, levels: Vec<String>) -> Self {
        self.levels = Some(levels);
        self
    }

    /// Has the reader selected anything? A brush that has not is the resting
    /// state, and the renderer skips its whole two-pass path for it — the
    /// `nframes < 2` discipline, which is what keeps an unbrushed plot's bytes
    /// exactly what they were before this type existed.
    pub fn is_resting(&self) -> bool {
        self.at.is_none() && self.levels.is_none()
    }
}

// ---------------------------------------------------------------------------
// PlotSpec — the top-level plot specification
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlotSpec {
    /// Name of the default data table (resolved from the caller's data registry).
    #[serde(default)]
    pub data: Option<String>,
    pub layers: Vec<Layer>,
    #[serde(default)]
    pub coord: CoordSpace,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub x_axis: AxisSpec,
    #[serde(default)]
    pub y_axis: AxisSpec,
    /// The third position's axis. Present for the same reason `z` is a channel
    /// and not a mode: the positions are a family of three, and giving two of
    /// them a label override and not the third is the per-channel exception
    /// Law 1 exists to catch. Read only in 3-D — a `z_label` on a flat plot
    /// labels an axis that is not drawn, which `check_space` reports.
    #[serde(default)]
    pub z_axis: AxisSpec,
    /// Coordinate-space encodings, written before any mark — the default column
    /// for every layer that does not name its own (see [`PlotSpec::position_for`]).
    ///
    /// The *axis* is shared whatever these say; only which column a layer reads
    /// for it can be local.
    #[serde(default)]
    pub x: Option<ChannelDef>,
    #[serde(default)]
    pub y: Option<ChannelDef>,
    #[serde(default)]
    pub z: Option<ChannelDef>,
    /// Plot-scoped non-positional channels — written before any mark, and so
    /// applying to every layer that can accept them.
    ///
    /// Scope is decided by position, not by the channel's identity — `x`/`y`/`z`
    /// included. A channel written ahead of the marks is plot-scoped and reaches
    /// every layer that can accept it; one written after a mark binds to that
    /// mark alone. A layer that binds the channel itself wins — the same
    /// nearest-wins rule that governs `data()`.
    ///
    /// This comment used to say `x`/`y`/`z` were *always* plot-scoped "because
    /// every layer shares the axes". Half of that is true and stays: every layer
    /// does share the axes. But sharing an axis never required every table to
    /// spell its column the same way, and conflating the two is what stopped a
    /// second `data()` from naming its own positions (spec §8).
    #[serde(default, deserialize_with = "deserialize_encodings")]
    pub channels: HashMap<Channel, ChannelDef>,
    /// Optional ordering for the categorical axis, whichever axis carries it.
    /// `None` = data order (first-appearance).
    #[serde(default)]
    pub order: Option<OrderSpec>,
    /// Categorical color palette. Default: "gog" (20-color).
    #[serde(default)]
    pub palette: PaletteDef,
    /// Small multiples — panels split by the categories of a column.
    #[serde(default)]
    pub facet: Option<FacetSpec>,
    /// The reader's selections — a bound per column, dimming the rows outside
    /// it. Plot-scoped rather than per-layer, because a predicate over rows is
    /// a fact about the data and every layer reading that column answers to it.
    ///
    /// Empty is the overwhelmingly common case and is skipped on the wire, so a
    /// spec that names no brush serializes to exactly the JSON it always did.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub brush: Vec<BrushDef>,
    /// Plot furniture — the page rather than the ink. Spec §7.
    #[serde(default)]
    pub theme: ThemeSpec,
}

impl PlotSpec {
    pub fn new() -> Self {
        Self {
            data: None,
            layers: Vec::new(),
            coord: CoordSpace::default(),
            title: None,
            x_axis: AxisSpec::default(),
            y_axis: AxisSpec::default(),
            z_axis: AxisSpec::default(),
            x: None,
            y: None,
            z: None,
            channels: HashMap::new(),
            order: None,
            palette: PaletteDef::default(),
            facet: None,
            brush: Vec::new(),
            theme: ThemeSpec::default(),
        }
    }

    /// The column names whose absence makes a *row* unplottable — every channel
    /// binding (the plot-scoped `x`/`y`/`z` and `channels`, and every layer's own
    /// encodings) plus the facet columns that assign a row to a panel.
    ///
    /// The wire boundary drops a row only when one of *these* is missing in it,
    /// the way a plot drops on the aesthetics it actually uses: a real dataset
    /// like penguins carries `NA` in columns a given plot never touches, and
    /// dropping a row for a missing value it does not read would be a silent lie
    /// about n. `order` is excluded — a missing sort key sorts to an end, it does
    /// not unplace the row — and so is `data`, which names a table, not a column.
    ///
    /// **`brush` is excluded too, and for a stronger reason than `order`'s.** A
    /// brush places nothing, so a missing value in a brushed column leaves the
    /// row exactly where it was and merely outside the selection. Counting it
    /// here would drop rows the moment a reader brushed, which would make n
    /// depend on the mouse — and would break the promise that a plot with
    /// nothing selected draws exactly what it drew before the brush was written.
    pub fn mapped_fields(&self) -> std::collections::HashSet<String> {
        let mut fields = std::collections::HashSet::new();
        for def in [self.x.as_ref(), self.y.as_ref(), self.z.as_ref()].into_iter().flatten() {
            fields.insert(def.field.clone());
        }
        for def in self.channels.values() {
            fields.insert(def.field.clone());
        }
        for layer in &self.layers {
            for def in layer.encodings.values() {
                fields.insert(def.field.clone());
            }
        }
        if let Some(f) = &self.facet {
            fields.extend(f.col.iter().chain(f.row.iter()).cloned());
        }
        fields
    }

    /// Facet into panel columns by a category column — the `|` operator.
    pub fn facet_col(mut self, field: impl Into<String>) -> Self {
        let mut f = self.facet.unwrap_or_default();
        f.col = Some(field.into());
        self.facet = Some(f);
        self
    }

    /// Facet into panel rows by a category column — the `/` operator.
    pub fn facet_row(mut self, field: impl Into<String>) -> Self {
        let mut f = self.facet.unwrap_or_default();
        f.row = Some(field.into());
        self.facet = Some(f);
        self
    }

    /// Fold the ribbon of panels after `n` of them — `facet(g, wrap = n)`.
    pub fn facet_wrap(mut self, n: usize) -> Self {
        let mut f = self.facet.unwrap_or_default();
        f.wrap = Some(n);
        self.facet = Some(f);
        self
    }

    /// Bind a plot-scoped channel — shared by every layer that can accept it.
    pub fn channel(mut self, channel: Channel, field: impl Into<String>) -> Self {
        self.channels.insert(channel, ChannelDef::field(field));
        self
    }

    pub fn data(mut self, name: impl Into<String>) -> Self {
        self.data = Some(name.into());
        self
    }

    /// Bind the x-axis to a column name.  Plot-scoped: shared by all layers.
    pub fn x(mut self, field: impl Into<String>) -> Self {
        self.x = Some(ChannelDef::field(field));
        self
    }

    /// Bind the y-axis to a column name.  Plot-scoped: shared by all layers.
    pub fn y(mut self, field: impl Into<String>) -> Self {
        self.y = Some(ChannelDef::field(field));
        self
    }

    /// Bind the z-axis to a column name.  Plot-scoped: shared by all layers.
    pub fn z(mut self, field: impl Into<String>) -> Self {
        self.z = Some(ChannelDef::field(field));
        self
    }

    /// Bind the x-axis with an explicit scale — `x(gdp, scale = "log")`.
    pub fn x_scaled(mut self, field: impl Into<String>, scale: ScaleType) -> Self {
        self.x = Some(ChannelDef::field(field).with_scale(scale));
        self
    }

    /// Bind the y-axis with an explicit scale.
    pub fn y_scaled(mut self, field: impl Into<String>, scale: ScaleType) -> Self {
        self.y = Some(ChannelDef::field(field).with_scale(scale));
        self
    }

    /// Bind the x-axis with a stated domain — `x(hour, limits = c(0, 24))`.
    pub fn x_limited(mut self, field: impl Into<String>, lo: Option<f64>, hi: Option<f64>) -> Self {
        self.x = Some(ChannelDef::field(field).with_limits(lo, hi));
        self
    }

    /// Bind the y-axis with a stated domain.
    pub fn y_limited(mut self, field: impl Into<String>, lo: Option<f64>, hi: Option<f64>) -> Self {
        self.y = Some(ChannelDef::field(field).with_limits(lo, hi));
        self
    }

    /// Bind the z-axis with an explicit scale.
    pub fn z_scaled(mut self, field: impl Into<String>, scale: ScaleType) -> Self {
        self.z = Some(ChannelDef::field(field).with_scale(scale));
        self
    }

    /// Bind the x-axis to a log scale on an explicit base.
    pub fn x_log_base(mut self, field: impl Into<String>, base: f64) -> Self {
        self.x = Some(ChannelDef::field(field).with_scale(ScaleType::Log).with_base(base));
        self
    }

    /// Bind the y-axis to a log scale on an explicit base.
    pub fn y_log_base(mut self, field: impl Into<String>, base: f64) -> Self {
        self.y = Some(ChannelDef::field(field).with_scale(ScaleType::Log).with_base(base));
        self
    }

    pub fn layer(mut self, layer: Layer) -> Self {
        self.layers.push(layer);
        self
    }

    // -----------------------------------------------------------------------
    // Where a position comes from
    //
    // Every layer shares the axes; what a shared axis never required is that
    // every table spell its column the same way. So a position binding resolves
    // like `data()` does — nearest wins — and the two questions below are the
    // only two anyone asks about it (spec §8).
    // -----------------------------------------------------------------------

    /// The column *this layer* reads for a position channel: its own if it names
    /// one, **otherwise the axis's**.
    ///
    /// The fallback is not the plot's binding but [`axis_def`](Self::axis_def),
    /// and the difference is the whole rule. A position written after a mark
    /// binds to that mark — position decides scope, for `x`/`y`/`z` as for every
    /// other channel — but `point + x(gdp) + y(life) + line` must still draw a
    /// line, and it does, because a layer that names no position of its own
    /// reads the one the axis goes by. Without that, the book's dominant idiom
    /// (317 expressions write `mark + x(…)`) would silently leave every later
    /// layer unplaced.
    ///
    /// So: one coordinate space, one scale, one set of ticks; only *which column
    /// of this layer's table* supplies the values is local. A layer naming its
    /// own *scale* would be a second axis, which §18 refuses —
    /// `check_layer_position` enforces that.
    pub fn position_for<'a>(&'a self, layer: &'a Layer, channel: &Channel) -> Option<&'a ChannelDef> {
        if matches!(channel, Channel::X | Channel::Y | Channel::Z) {
            if let Some(def) = layer.encodings.get(channel) {
                return Some(def);
            }
        }
        self.axis_def(channel)
    }

    /// The plot-level binding for a position channel, ignoring any layer's own.
    pub fn position(&self, channel: &Channel) -> Option<&ChannelDef> {
        match channel {
            Channel::X => self.x.as_ref(),
            Channel::Y => self.y.as_ref(),
            Channel::Z => self.z.as_ref(),
            _ => None,
        }
    }

    /// The name the shared axis goes by — what its label and ticks are derived
    /// from, and the one name every layer's column is resolved *to*.
    ///
    /// The plot's binding when there is one; otherwise the first layer that
    /// names its own, in spec order. A plot whose layers each name a different
    /// column still has one axis, so it needs one name, and "the first one
    /// written" is the only rule here that does not require ranking the layers.
    pub fn axis_def(&self, channel: &Channel) -> Option<&ChannelDef> {
        if let Some(def) = self.position(channel) {
            return Some(def);
        }
        if !matches!(channel, Channel::X | Channel::Y | Channel::Z) {
            return None;
        }
        self.layers.iter().find_map(|l| l.encodings.get(channel))
    }

    pub fn coord(mut self, coord: CoordSpace) -> Self {
        self.coord = coord;
        self
    }

    pub fn brush(mut self, brush: BrushDef) -> Self {
        self.brush.push(brush);
        self
    }

    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Override the x-axis label (default: auto-derived from the x channel's field name).
    pub fn x_label(mut self, label: impl Into<String>) -> Self {
        self.x_axis.label = Some(label.into());
        self
    }

    /// Override the y-axis label (default: auto-derived from the y channel's field name).
    pub fn y_label(mut self, label: impl Into<String>) -> Self {
        self.y_axis.label = Some(label.into());
        self
    }

    /// Override the z-axis label (default: auto-derived from the z channel's field name).
    pub fn z_label(mut self, label: impl Into<String>) -> Self {
        self.z_axis.label = Some(label.into());
        self
    }

    /// Aim for `n` ticks on both position axes — the Rust-side convenience for
    /// what a binding writes per channel as `x(<column>, tick_count = n)`.
    pub fn tick_count(mut self, n: usize) -> Self {
        for def in [self.x.as_mut(), self.y.as_mut()].into_iter().flatten() {
            def.tick_count = Some(n);
        }
        self
    }

    /// Order the categorical axis by `field` ascending (smallest first / A→Z).
    pub fn order(mut self, field: impl Into<String>) -> Self {
        self.order = Some(OrderSpec { field: field.into(), descending: false });
        self
    }

    /// Order the categorical axis by `field` descending (largest first / Z→A).
    pub fn order_desc(mut self, field: impl Into<String>) -> Self {
        self.order = Some(OrderSpec { field: field.into(), descending: true });
        self
    }
}

impl Default for PlotSpec {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Figure — a plot, or a page of them
//
// Composition is *presentational*: separate plots arranged on one page, each
// keeping its own coordinate space. That is what tells it from faceting, which
// is *semantic* — one plot split by a variable, sharing everything (§11).
// Wilkinson keeps the two apart for the same reason: "some tabled graphics are
// really two or more graphics glued together".
//
// One rule carries everything the arrangement does not: **the same column on the
// same axis in two composed plots is one axis** — one scale, one panel extent,
// drawn once. It is what makes a marginal plot fall out of composition rather
// than needing a chart type of its own, and it is why there is no `spacer` atom:
// the blank corner of a marginal plot is the space a *shared* extent leaves over.
// `render::page` is where it is resolved.
// ---------------------------------------------------------------------------

/// Which way a page's cells run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Arrange {
    /// Left to right — R's `|`, JavaScript's `beside()`.
    Beside,
    /// Top to bottom — R's `/`, JavaScript's `below()`.
    Below,
}

/// A page: cells running one way, each a plot or a page of its own.
///
/// Nesting is what gives the marginal plot its shape — `top / (main | right)` is
/// a `Below` of two cells, the second of them a `Beside`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageSpec {
    pub arrange: Arrange,
    pub cells: Vec<Figure>,
}

/// What the engine is asked to draw: one plot, or a page of them.
///
/// Untagged, with the page tried first: a `PlotSpec` requires `layers` and a
/// `PageSpec` requires `arrange` and `cells`, so the two shapes cannot be
/// mistaken for one another — and every spec ever written stays valid on the
/// wire, which is what keeps four bindings from needing a flag day.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Figure {
    Page(PageSpec),
    Plot(Box<PlotSpec>),
}

impl Figure {
    /// Every plot on the page, in reading order. One plot is a page of one.
    pub fn plots(&self) -> Vec<&PlotSpec> {
        match self {
            Figure::Plot(p) => vec![p],
            Figure::Page(page) => page.cells.iter().flat_map(Figure::plots).collect(),
        }
    }

    /// Is this a single plot? The check and the renderer both branch on it.
    pub fn is_page(&self) -> bool {
        matches!(self, Figure::Page(_))
    }

    /// How much room this figure asks for along one direction, in pixels, or
    /// `None` if it asks for nothing and will take an even share.
    ///
    /// `theme(width =, height =)` is where a plot asks. A *page* asks for what
    /// its cells add up to when they run the way the question is asked, and for
    /// the widest of them when they run across it — and only when every cell has
    /// asked, since one cell wanting to fill makes the whole page want to.
    ///
    /// It lives here rather than in the renderer because the check needs the same
    /// arithmetic — a page whose cells ask for more than the canvas has is
    /// refused, and refused with the number it asked for.
    pub fn ask(&self, horizontal: bool) -> Option<f64> {
        match self {
            Figure::Plot(spec) => {
                let theme = spec.theme.resolved();
                if horizontal { theme.width } else { theme.height }
            }
            Figure::Page(page) => {
                let asks: Vec<Option<f64>> =
                    page.cells.iter().map(|c| c.ask(horizontal)).collect();
                if asks.iter().any(Option::is_none) {
                    return None;
                }
                let along = (page.arrange == Arrange::Beside) == horizontal;
                let sizes = asks.into_iter().flatten();
                Some(if along { sizes.sum() } else { sizes.fold(0.0, f64::max) })
            }
        }
    }
}

impl From<PlotSpec> for Figure {
    fn from(spec: PlotSpec) -> Self {
        Figure::Plot(Box::new(spec))
    }
}

// ---------------------------------------------------------------------------
// Operator `*` — derive (combine a mark with a transform to form a layer)
//
// `Mark * Transform → Layer`   e.g.  Mark::Bar  * Transform::Bin
// `Layer * Transform → Layer`  e.g.  (Mark::Bar * Transform::Bin) * Transform::Smooth
//
// This is the "add-a-stroke" derivation operator from the grammar: a finite
// set of atoms, recombined to express any derived chart type without any
// special-cased names.
// ---------------------------------------------------------------------------

impl std::ops::Mul<Transform> for Mark {
    type Output = Layer;

    /// `mark * transform` — create a new layer from this mark, seeded with
    /// one transform.  Additional transforms may be chained with further `*`.
    fn mul(self, t: Transform) -> Layer {
        Layer::new(self).transform(t)
    }
}

impl std::ops::Mul<Transform> for Layer {
    type Output = Layer;

    /// `layer * transform` — append another transform to an existing layer.
    fn mul(self, t: Transform) -> Layer {
        self.transform(t)
    }
}

// `PlotSpec + Layer` — add a finished layer to a spec with the `+` operator,
// mirroring the `+` assembly operator from the grammar.
impl std::ops::Add<Layer> for PlotSpec {
    type Output = PlotSpec;

    fn add(self, layer: Layer) -> PlotSpec {
        self.layer(layer)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Where a position comes from — one axis, its own column (spec §8)
    // -----------------------------------------------------------------------

    /// The whole resolution rule as one differential assertion: the *same* plot,
    /// read from two layers, gives each the column it named and the axis's to the
    /// one that named nothing. Written over both so it cannot pass by describing
    /// either separately.
    #[test]
    fn a_layer_reads_its_own_position_column_and_otherwise_the_axis() {
        let spec = PlotSpec::new()
            .x("gdp")
            .y("life")
            .layer(Layer::new(Mark::Point))
            .layer(
                Layer::new(Mark::Text)
                    .encode(Channel::X, "at")
                    .encode(Channel::Y, "val"),
            );

        let (plain, noted) = (&spec.layers[0], &spec.layers[1]);
        for (ch, plot_col, own_col) in
            [(Channel::X, "gdp", "at"), (Channel::Y, "life", "val")]
        {
            assert_eq!(
                spec.position_for(plain, &ch).map(|c| c.field.as_str()),
                Some(plot_col),
                "a layer naming no {ch:?} must read the axis's column"
            );
            assert_eq!(
                spec.position_for(noted, &ch).map(|c| c.field.as_str()),
                Some(own_col),
                "a layer naming its own {ch:?} must read that one"
            );
            // The axis itself is unmoved by either: one coordinate space.
            assert_eq!(spec.axis_def(&ch).map(|c| c.field.as_str()), Some(plot_col));
        }
    }

    /// With no plot-level binding the axis takes the first layer that names one —
    /// which is what keeps `point + x(gdp) + y(life) + line` drawing a line. The
    /// book's dominant idiom writes positions after a mark, so this is the common
    /// path, not an edge case.
    #[test]
    fn with_no_plot_binding_the_axis_is_the_first_layer_that_names_one() {
        let spec = PlotSpec::new()
            .layer(Layer::new(Mark::Point).encode(Channel::X, "gdp"))
            .layer(Layer::new(Mark::Line));

        assert_eq!(spec.axis_def(&Channel::X).map(|c| c.field.as_str()), Some("gdp"));
        assert_eq!(
            spec.position_for(&spec.layers[1], &Channel::X).map(|c| c.field.as_str()),
            Some("gdp"),
            "the second layer names no x, so it reads the axis the first one set"
        );
        // Nothing named a y, so there is no y axis to fall back to.
        assert!(spec.axis_def(&Channel::Y).is_none());
    }

    /// A plot-level binding outranks a layer's for the *axis*, while each layer
    /// still reads its own column. The two questions are separate and this pins
    /// that they stay separate.
    #[test]
    fn the_plot_binding_names_the_axis_even_when_layers_name_their_own() {
        let spec = PlotSpec::new()
            .x("gdp")
            .layer(Layer::new(Mark::Point).encode(Channel::X, "at"));

        assert_eq!(spec.axis_def(&Channel::X).map(|c| c.field.as_str()), Some("gdp"));
        assert_eq!(
            spec.position_for(&spec.layers[0], &Channel::X).map(|c| c.field.as_str()),
            Some("at")
        );
    }

    /// Non-position channels are untouched by any of this — `position_for` is
    /// about the three coordinate channels and nothing else.
    #[test]
    fn position_resolution_does_not_reach_the_other_channels() {
        let spec = PlotSpec::new()
            .x("gdp")
            .layer(Layer::new(Mark::Point).encode(Channel::Color, "continent"));
        assert!(spec.position_for(&spec.layers[0], &Channel::Color).is_none());
        assert!(spec.axis_def(&Channel::Color).is_none());
    }

    #[test]
    fn mapped_fields_are_the_positions_and_panels_a_row_needs() {
        // The set the wire boundary drops a missing row on: every channel that
        // places or encodes a row, and the facet column that assigns its panel.
        // NOT the order key — a missing sort value sorts to an end, it does not
        // remove the row — and NOT an unmapped column that merely rides along.
        let spec = PlotSpec::new()
            .x("flipper")
            .y("count")
            .channel(Channel::Color, "species")
            .layer(Layer::new(Mark::Point).encode(Channel::Size, "mass"))
            .facet_col("island");
        let spec = PlotSpec { order: Some(OrderSpec { field: "sorter".into(), descending: true }), ..spec };

        let fields = spec.mapped_fields();
        for want in ["flipper", "count", "species", "mass", "island"] {
            assert!(fields.contains(want), "`{want}` positions or encodes a row and must be mapped");
        }
        assert!(!fields.contains("sorter"), "an order key does not unplace a row, so it is not a drop trigger");
        assert!(!fields.contains("body_mass_g"), "a column the plot never reads cannot drop a row");
    }
}
