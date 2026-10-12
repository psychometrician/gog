//! Law 7's minimum syllable: which positions a layer must bind, and the two
//! relaxations of that rule.

use super::*;

// ---------------------------------------------------------------------------
// The bar with no position axis — one slot, divided by its split
//
// Law 7 says a visual is a mark plus its *required* positions, and `bar` requires
// an `x`. There is exactly one shape where it does not: when a split the bar takes
// (`color` or `pattern`) supplies the segmentation, the bar has one slot and the
// split divides it.
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
    // Any channel that splits a bar, which is `color` and `pattern`: the same
    // split that piles and dodges a bar everywhere else divides its one slot here
    // (Law 2). This asked for `group`, which a bar refuses, and not `pattern`, so
    // a hatched one-slot pile was refused while a colored one drew.
    layer.mark == Mark::Bar
        && splitting_channels(&Mark::Bar).iter().any(|ch| layer.encodings.contains_key(ch))
}

/// Is this bar **one pile** the split divides, with no key axis at all? Law 7's
/// relaxation (spec §4) is for a bar that needs no position axis, and
/// `bar_divides_one_slot` says only that the split is there.
///
/// A `y` naming a column that a tally does not read *is* a position axis: a tally
/// makes its measure itself, so that column can only be the key, as `slot_orient`
/// reads it, and `bar * count + y(season)` lies on its side. Adding `* stack +
/// color(dir)` must not change what `y(season)` means (Law 6). Asked only whether
/// `x` was missing, the relaxation swallowed that key: it drew one pile and used
/// the column to title the axis. The pile stands where nothing names a key: no
/// `y`, a `y` a statistic reads as its measure (`bar * sum * stack + y(population)`,
/// the share column and the pie), or a `y` naming no column, which titles the
/// tally's own axis (`y(count)`).
///
/// **A pile's measure is a number, so a text column is always a key** (2026-10-11).
/// With no statistic, `bar + y(continent) + color(continent)` read `continent` as
/// the length of the pile, found nothing to add up, and drew an empty panel in
/// silence (§12). Read as a key, the bar is short of its measure, and the missing
/// position says so.
pub fn bar_is_one_pile(spec: &PlotSpec, layer: &Layer, df: Option<&DataFrame>) -> bool {
    if !bar_divides_one_slot(layer) || spec.position_for(layer, &Channel::X).is_some() {
        return false;
    }
    let keyed = spec.position_for(layer, &Channel::Y)
        .is_some_and(|y| names_a_key(layer, df, &y.field));
    !keyed
}

/// Does a pile's one position name its key rather than its measure? A tally makes
/// its measure itself, so a column it is handed is the key; a text column cannot be
/// added up, so it is the key whatever the chain.
fn names_a_key(layer: &Layer, df: Option<&DataFrame>, field: &str) -> bool {
    let kind = df.and_then(|d| actual_type(d, field));
    kind == Some(VarType::Discrete)
        || (synthesizes_measure(&layer.mark, &layer.transforms) && kind.is_some())
}

/// **The same pile, horizontal** (ruled 2026-10-11, the consistency audit's F8):
/// `y` left out and `x` naming the column the pile measures, so
/// `bar * sum * stack + x(population) + color(continent)` is one horizontal pile
/// as `y(population)` makes one upright pile. It was refused for its missing `y`
/// ("`sum` adds up the column named there"), because a lone continuous `x` reads
/// as the axis the bars stand on, which is right for `bin` (it cuts that axis)
/// and wrong for a statistic (it measures it). So the rule is the upright one,
/// mirrored: with no other position, a tally's `x` is its key, and so is a text
/// column, and anything else is the measure.
pub fn bar_is_one_pile_sideways(spec: &PlotSpec, layer: &Layer, df: Option<&DataFrame>) -> bool {
    if !bar_divides_one_slot(layer) || spec.position_for(layer, &Channel::Y).is_some() {
        return false;
    }
    spec.position_for(layer, &Channel::X)
        .is_some_and(|x| !names_a_key(layer, df, &x.field))
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
pub fn x_needs_no_binding(spec: &PlotSpec, layer: &Layer, df: Option<&DataFrame>) -> bool {
    bar_is_one_pile(spec, layer, df)
        || layer.transforms.contains(&Transform::Partition)
        || layer.transforms.contains(&Transform::Flow)
        // A sideways cluster tree: leaves on `y`, and `x` carries the merge
        // distance the transform synthesizes — bound by nobody, named by
        // itself, the same standing a lying-down histogram's count has.
        || (layer.transforms.contains(&Transform::Cluster)
            && spec.position_for(layer, &Channel::Y).is_some())
        || space_of(spec) == SpaceKind::Nest
        || space_of(spec) == SpaceKind::Network
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
pub(super) fn check_layer_position(out: &mut Vec<Diagnostic>, spec: &PlotSpec, layer: &Layer) {
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
            ("its own limits", format!("{c}(<column>, limits = )"))
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
pub(super) fn z_refusal_kind(mark: &Mark) -> DiagnosticKind {
    match mark {
        // The path/region family: ruled out, not owed. See `z_refusal`.
        Mark::Line | Mark::Step | Mark::Area | Mark::Ribbon => DiagnosticKind::Illegal,
        _ => DiagnosticKind::Unsupported,
    }
}

/// What to say when a mark has no `z`, and there are three true sentences rather
/// than one (spec §15).
pub(super) fn z_refusal(mark: &Mark, field: &str) -> String {
    let m = mark_name(mark);
    match mark {
        // **Decided, and argued in `rule_for`'s `Mark::Path` row since M8a.** A
        // line is a *function*: it sorts by `x` and draws one `y` for each, read
        // left to right along a domain — and a cube has no left to right. `x` is
        // one of three equal positions, and at some viewing angles it runs into the
        // page and becomes depth (`project::Scene`'s own test pins that), so the
        // sort would be along an axis the reader cannot see. The direction is
        // `path`, which is exactly `line` with that sort removed.
        //
        // The direction is the mark's own. `path` is `line` with the sort removed,
        // and a `step`'s stairs, too, are only that sort drawn square; an `area` or a
        // `ribbon` is a fill with no plane to stand on in a cube, so what `path`
        // gives it is its edge.
        Mark::Line | Mark::Step | Mark::Area | Mark::Ribbon => {
            let a = if matches!(mark, Mark::Area) { "An" } else { "A" };
            let direction = match mark {
                Mark::Line => format!(
                    "For a route through three dimensions use `path`, which is `{m}` with that \
                     sort removed"),
                Mark::Step => "For a route through three dimensions use `path`, which joins the \
                               rows in the order they come instead of sorting them".to_string(),
                _ => format!(
                    "A fill has no plane to stand on in a cube, so draw the {m}'s edge with \
                     `path`, which joins the rows in the order they come"),
            };
            format!(
                "gog: `{m}` reads a *domain* left to right — it sorts by `x` and draws one value \
                 for each — and a cube has no left to right: `x` is one of three equal positions, \
                 and at some viewing angles it runs into the page and becomes depth. {a} `{m}` in \
                 space would be sorted by an axis the reader cannot see, so this is refused rather \
                 than drawn. {direction}: `path + x(<a>) + y(<b>) + z({field})`."
            )
        }
        // A rule placed by `z`, the level plane; `rule_plane` words it for the axis
        // a rule is actually placed by, which the caller knows and this does not.
        Mark::Rule => rule_plane("z", field),
        // **Blocked for the same reason, plus a vocabulary gap that has to be
        // decided before it could be drawn at all.** `bounds` names two pairs — a
        // measure pair and a domain pair — so a zone can never be bounded on a
        // *third* axis, and one it does not bound it spans whole. Every zone in a
        // cube is therefore a slab, with a plane's sorting problem.
        Mark::Zone => format!(
            "gog: `zone` shades the region its `bounds` name and spans the axes they do not — \
             and `bounds` names two pairs, so in a cube a zone always spans one axis whole and \
             is a **slab**. Like a 3-D `rule`, its footprint runs across the whole floor. A slab \
             is valid grammar, but this engine does not draw it: placing it among the data would \
             mean working out, piece by piece, what hides what. \
             Draw it flat, or stand a solid on the floor with `bar + x(<a>) + y(<b>) + z({field})`."
        ),
        // Anything else is an ordinary unbuilt cell, and says so.
        _ => format!(
            "gog: `z` is valid grammar for `{m}`, but this engine does not draw it — \
             `z({field})` would have no visual effect. Remove it, or use a channel that renders."
        ),
    }
}

/// The 3-D `rule` refusal, worded for the sentence written: which axis the rule
/// is placed by, and which two it spans.
///
/// **Blocked, and on the one thing M8a deliberately does not have.** A rule
/// spans the axes it is not placed by; in a cube that is *two* of them, so the
/// mark is a plane. Marks are depth-sorted by footprint, and a plane's footprint
/// runs across the whole floor, a level plane covering it and an upright one
/// crossing it, so it can only land wholly in front of or wholly behind every
/// mark, and a reference plane's entire job is to cut through them. That needs
/// per-element occlusion, which is M8b (§16). It said "spans the ones it does
/// not name" and "a plane's footprint is the whole floor", which fit no sentence
/// a reader writes: a rule names one position and a vertical plane's footprint
/// is a line.
pub(super) fn rule_plane(axis: &str, field: &str) -> String {
    let spans: Vec<&str> = ["x", "y", "z"].into_iter().filter(|a| *a != axis).collect();
    format!(
        "gog: `rule + {axis}({field})` in a cube is a **plane**: it marks `{field}` on \
         `{axis}` and spans `{}` and `{}`. Marks in space are sorted by their footprint, and \
         a plane's footprint runs across the whole floor, so it could only be drawn wholly in \
         front of or wholly behind the data when its job is to cut through it. A plane in a \
         cube is valid grammar, but this engine does not draw it: that would mean working \
         out, piece by piece, what hides what. Draw the plot flat, where a rule is a line.",
        spans[0], spans[1]
    )
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

pub(super) fn check_rule(out: &mut Vec<Diagnostic>, spec: &PlotSpec, df: &DataFrame, layer: &Layer) {
    if layer.mark != Mark::Rule {
        return;
    }
    // **A layer naming both positions asks for two lines per row.** A rule is one
    // line per row, placed by one column and spanning the other axis, so only one of
    // the two could be read. `rule_axis` reads the layer's own `x` first, and the
    // `y` beside it used to go unread and unreported: a rug down one edge and none
    // up the other, from a sentence that named both (§12). Refused toward the two
    // layers the reader meant, which is how a rug on both axes is written.
    //
    // **In a cube `z` is a position like the other two** (ruled 2026-09-28). A rule
    // there is placed by one of `x`, `y` or `z` and is a plane across the other
    // two, so a layer naming two of the three is refused as it is on the page. It
    // was read as the plane `x = a` with `z` only turning the cube on, which left
    // the column `z` named with no effect, and `rule + z(altitude)`, the level
    // plane a reader means by a ceiling, was told it had no position at all.
    let own: Vec<String> = [Channel::X, Channel::Y, Channel::Z].iter()
        .filter_map(|ch| layer.encodings.get(ch)
            .map(|d| format!("{}({})", channel_name(ch), d.field)))
        .collect();
    if own.len() >= 2 {
        let names: Vec<&str> = own.iter().map(String::as_str).collect();
        let count = if own.len() == 2 { "two" } else { "three" };
        let direction = match (layer.encodings.get(&Channel::X), layer.encodings.get(&Channel::Y)) {
            (Some(xd), Some(yd)) if own.len() == 2 => format!(
                "For lines on both axes, write two layers, one per axis: `rule + x({}) + rule + \
                 y({})`.", xd.field, yd.field),
            _ => format!(
                "Write one position on each `rule` layer: {}.",
                code_list_or(&own.iter().map(|p| format!("rule + {p}")).collect::<Vec<_>>()
                    .iter().map(String::as_str).collect::<Vec<_>>())),
        };
        // A line on the page and a plane in a cube, so the cube's case says only
        // what both share.
        let lead = match layer.encodings.contains_key(&Channel::Z) {
            true => "`rule` is placed by one column",
            false => "`rule` draws one line per row, placed by one column",
        };
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: {lead}, and this layer names {count} — {} — so only one of them could \
                 be drawn. {direction}",
                code_list(&names)
            ),
        });
        return;
    }
    // One position named on the layer places it, `z` included.
    if !own.is_empty() || rule_axis(spec, df, layer).is_some() {
        return;
    }
    // A plot-wide `z` whose column this layer's table holds places it too, when
    // neither `x` nor `y` does: the level plane again, written before the mark.
    if spec.axis_def(&Channel::Z).is_some_and(|d| actual_type(df, &d.field).is_some()) {
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

/// A plot with no mark draws nothing, whatever else it names.
///
/// **Law 7's second half** (spec §4): a visual is a mark plus its required
/// positions, and neither renders alone. The first half has always been enforced,
/// per layer: `data(t) + point` is refused and names both positions it lacks. This
/// half had no check at all, because every check here is asked of a layer and a
/// plot with no mark has none to ask. So `data(t) + x(gdp)` drew a full grid with
/// both axes ticked 0.0 to 1.0 under the name "Gdp" and said nothing: made-up axes
/// for a column the engine never read, the empty panel §12 forbids. A `color()`
/// alone, a `title()` alone and `data(t)` alone drew the same empty panel.
///
/// The direction is read from the positions the plot does name, so it is always a
/// sentence that draws: `point` for two positions or three, and for one a
/// transform that invents the other, `bar * count` for a category and `bar * bin`
/// for a number. It names columns, never calls (§12).
pub(super) fn check_has_a_mark(
    out: &mut Vec<Diagnostic>,
    spec: &PlotSpec,
    data: &HashMap<String, DataFrame>,
) {
    if !spec.layers.is_empty() {
        return;
    }
    let named: Vec<(&str, &str)> = [("x", &spec.x), ("y", &spec.y), ("z", &spec.z)]
        .into_iter()
        .filter_map(|(c, def)| def.as_ref().map(|d| (c, d.field.as_str())))
        .collect();
    let names = named.iter().map(|(c, f)| format!("`{c}` names `{f}`")).collect::<Vec<_>>();
    let has = |c: &str| named.iter().any(|(n, _)| *n == c);
    let empty = "so the plot would be an empty panel";
    let message = match named.as_slice() {
        [] => "gog: this plot has no mark, so it would be an empty panel. Add a mark and the \
               positions it needs, such as `point` with an `x` and a `y` column."
            .to_string(),
        [(c @ ("x" | "y"), field)] => {
            let table = spec.data.as_ref().and_then(|name| data.get(name));
            let (mark, what) = match table.and_then(|d| actual_type(d, field)) {
                Some(VarType::Discrete) => {
                    ("bar * count", format!("counts the rows for each value of `{field}`"))
                }
                _ => ("bar * bin", format!("shows how the values of `{field}` are spread")),
            };
            format!(
                "gog: `{c}` names `{field}`, but no mark draws it, {empty}. Add a mark to \
                 carry it: `{mark}` {what}."
            )
        }
        _ if has("x") && has("y") => format!(
            "gog: {}, but no mark draws them, {empty}. Add a mark to carry them: `point` \
             draws a dot for each row.",
            and_list(&names)
        ),
        _ => {
            let lacking: Vec<String> = ["x", "y"]
                .into_iter()
                .filter(|c| !has(c))
                .map(|c| format!("`{c}`"))
                .collect();
            format!(
                "gog: {}, but no mark draws {}, {empty}. Add a mark such as `point`, and \
                 the {} it needs.",
                and_list(&names),
                if named.len() == 1 { "it" } else { "them" },
                and_list(&lacking),
            )
        }
    };
    out.push(Diagnostic { kind: DiagnosticKind::Illegal, message });
}

/// **In `polar()`, one pile is a pie, and a pie turns along `y`** (ruled
/// 2026-10-11, following the consistency audit's F8). A plot with one position
/// bound makes that position the angle, so the upright pile bends into a pie whose
/// full turn is the pile. The horizontal pile bent into a ring instead: its slot
/// became a band of radius, and its angle axis ran past the total, so the ring did
/// not close. Refused with the pie as the direction, since before F8 this sentence
/// was refused anyway and a refusal can be relaxed if a ring is ever wanted.
pub(super) fn check_sideways_pile_in_polar(
    out: &mut Vec<Diagnostic>, spec: &PlotSpec, layer: &Layer, df: Option<&DataFrame>,
) {
    if space_of(spec) != SpaceKind::Polar || !bar_is_one_pile_sideways(spec, layer, df) {
        return;
    }
    let Some(x) = spec.position_for(layer, &Channel::X) else { return };
    let f = &x.field;
    out.push(Diagnostic {
        kind: DiagnosticKind::Illegal,
        message: format!(
            "gog: one pile with its column on `x` is a horizontal bar, and `polar()` bends \
             one pile into a pie only along `y`, where the pile's length becomes the full \
             turn. For the pie, write `y({f})` in place of `x({f})`. Without `polar()`, \
             `x({f})` draws the horizontal bar."
        ),
    });
}
