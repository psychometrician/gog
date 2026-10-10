//! The palette vocabulary: which names exist, what kind each one is, and where
//! each may be used.

use super::*;

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
pub(super) fn tick(names: &[&str]) -> Vec<String> {
    names.iter().map(|s| format!("`{s}`")).collect()
}

/// Check the plot-level palette.
///
/// A palette *is* a set of constants, so it validates against the same color
/// rule `style(color = ...)` uses. Before this existed, `palette("red")` was
/// read as a palette *name*, failed to match one, and fell through to the
/// default — the plot rendered in the wrong colors with nothing said.
pub(super) fn check_palette(out: &mut Vec<Diagnostic>, spec: &PlotSpec, data: &HashMap<String, DataFrame>) {
    let before = out.len();
    check_palette_value(out, spec, data);
    // **Validity first, usage second.** A misspelled palette is a mistake about the
    // palette; having nothing to color is a mistake about the sentence around it.
    // Reporting the second while the first is true would send a reader to bind a
    // column when what they typed was `stelblue`.
    if out.len() == before {
        check_palette_is_used(out, spec);
    }
}

/// **A palette with nothing to color does nothing, and saying nothing about it is
/// the silent drop §12 forbids.** `palette("viridis")` on a plot that maps no
/// `color` rendered byte for byte identically to the same sentence without it,
/// which is exactly what the manual's Design Laws chapter promises never happens:
/// *"It does not quietly drop the channel and hand you a plot that looks
/// finished."* The engine was contradicting the book in writing.
///
/// **Inert for every other channel**, measured rather than assumed: rendering with
/// and without a palette gives identical bytes for `pattern`, `shape` and `size`,
/// and differs only for `color`. So no reading of the sentence makes it mean
/// something, and refusing costs nobody a plot they wanted.
///
/// **A color need not be written to be mapped**, which is the half that makes this
/// more than a one-line check. A `zone * density` puts its estimate on `color` with
/// no `color()` in the sentence at all, and twelve chunks of the manual are written
/// that way — `field_measure` is what knows, and asking only for a binding would
/// have refused every heatmap in the book.
pub(super) fn check_palette_is_used(out: &mut Vec<Diagnostic>, spec: &PlotSpec) {
    if matches!(spec.palette, PaletteDef::Auto) {
        return;
    }
    // Both scopes are asked. A plot-scoped `color` reaches only the layers that
    // accept it, so a spec can carry the binding while no layer does — and that
    // case is the *binding's* to report, not the palette's.
    let mapped = spec.channels.contains_key(&Channel::Color)
        || spec.layers.iter().any(|l| {
            l.encodings.contains_key(&Channel::Color) || field_measure(l).is_some()
        });
    if !mapped {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message:
                "gog: `palette()` chooses the colors a `color` mapping hands out, and \
                 nothing here maps `color`, so it would have no effect. Bind a column \
                 with `color(<column>)` to use it — or, to paint every mark one color, \
                 that is a setting rather than a palette: `style(color = \"…\")`."
                    .to_string(),
        });
    }
}

/// The channel that tells categories apart without color on this mark, if it has
/// one: a glyph's `shape`, else the dash or hatch `pattern` draws. Read off
/// [`rule_for`], so a mark that gains either channel is advised it without an
/// edit here.
pub(super) fn colorless_split(mark: &Mark) -> Option<&'static str> {
    let draws = |ch: Channel| {
        let r = rule_for(mark, &ch);
        r.obligation != Obligation::Cannot && r.renders == Some(VarType::Discrete)
    };
    if draws(Channel::Shape) {
        Some("shape")
    } else if draws(Channel::Pattern) {
        Some("pattern")
    } else {
        None
    }
}

/// The print-figure sentence of the gray-ramp refusal: which channel to reach for
/// instead of a grayer palette, named for the marks that carry `color` here. One
/// channel reads as one instruction; two marks wanting two channels are each
/// named, since advice for one would be wrong for the other. Empty when no mark
/// carrying `color` has either.
pub(super) fn colorless_split_advice(spec: &PlotSpec) -> String {
    let mut wants: Vec<(&'static str, &'static str)> = Vec::new();
    for layer in &spec.layers {
        let colored = layer.encodings.contains_key(&Channel::Color)
            || spec.channels.contains_key(&Channel::Color);
        let Some(ch) = colored.then(|| colorless_split(&layer.mark)).flatten() else { continue };
        let pair = (ch, mark_name(&layer.mark));
        if !wants.contains(&pair) {
            wants.push(pair);
        }
    }
    let mut channels: Vec<&str> = wants.iter().map(|(c, _)| *c).collect();
    channels.sort_unstable();
    channels.dedup();
    match channels.as_slice() {
        [] => String::new(),
        [one] => format!(
            " For categories in a figure that has to print in black and white, reach \
             for `{one}(<column>)` instead — it tells them apart without spending color."
        ),
        _ => {
            let each: Vec<String> = wants.iter()
                .map(|(c, m)| format!("`{c}(<column>)` on the `{m}` layer"))
                .collect();
            format!(
                " For categories in a figure that has to print in black and white, reach \
                 for {} instead — each tells them apart without spending color.",
                each.join(" and "),
            )
        }
    }
}

pub(super) fn check_palette_value(
    out: &mut Vec<Diagnostic>, spec: &PlotSpec, data: &HashMap<String, DataFrame>,
) {

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
                             way, {} diverge from a center — or give `palette()` your own \
                             stops, the colors themselves, such as \"white\" and \"navy\".",
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
                    // It is the channel that separates categories without
                    // spending color at all, and **which one is the mark's**:
                    // a glyph's `shape`, a stroke's dash or a fill's hatch
                    // through `pattern`. This named `pattern` whatever the mark,
                    // so on a point plot it sent the reader to a channel a point
                    // refuses. A mark with neither (a `text`, a `surface`) is
                    // given no such sentence rather than a wrong one.
                    let print = match n == "gray" {
                        true => colorless_split_advice(spec),
                        false => String::new(),
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
                     color, give `palette()` several colors, \"{name}\" among them."
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
                    message: "gog: `palette()` was given no colors. Give it at least one, \
                              such as \"steelblue\" and \"tomato\"."
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
        PaletteDef::Levels(named) => check_named_palette(out, spec, data, named),
    }
}

/// What `color` is bound to on one layer, as far as a named palette cares: a
/// column of numbers, or a column of categories and its levels.
pub(super) enum ColoredBy {
    /// A number has no levels to name — a numeric column, a date, or a cell's
    /// measurement (`zone * count`), which is a number the engine computed.
    Numbers(String),
    /// A category column. `drawn` is what the legend and the color map will
    /// hold, in their order (a declared order, else first appearance);
    /// `declared` adds the levels a factor states and no row carries, which are
    /// real levels of the column even though nothing draws them.
    Levels { field: String, drawn: Vec<String>, declared: Vec<String> },
}

/// Every layer's answer to *what does `color` split this layer by*, for the
/// named palette's checks.
///
/// The categories are read where the color map will read them, so the two
/// cannot disagree about which levels exist: the layer's own table, and for the
/// one categorical color a transform invents — a network's node `name` — the
/// endpoint columns the layout draws its nodes from, in the order it meets them.
/// A layer whose color column neither the table nor that rule can see is left
/// out; the missing-column refusal elsewhere is its report.
pub(super) fn colored_by(spec: &PlotSpec, data: &HashMap<String, DataFrame>) -> Vec<ColoredBy> {
    let mut out = Vec::new();
    for layer in &spec.layers {
        let def = layer.encodings.get(&Channel::Color);
        if color_is_the_measurement(spec, layer) || (def.is_none() && field_measure(layer).is_some()) {
            let field = def.map(|d| d.field.clone())
                .or_else(|| measurement_column(spec, layer).map(str::to_string))
                .unwrap_or_default();
            out.push(ColoredBy::Numbers(field));
            continue;
        }
        let Some(def) = def else { continue };
        let Some(df) = layer.data.as_ref().or(spec.data.as_ref()).and_then(|n| data.get(n))
        else { continue };
        match actual_type(df, &def.field) {
            Some(VarType::Continuous) => out.push(ColoredBy::Numbers(def.field.clone())),
            Some(_) => out.push(ColoredBy::Levels {
                field: def.field.clone(),
                drawn: crate::data::categories_across(&[df], &def.field),
                declared: df.levels(&def.field).map(<[String]>::to_vec).unwrap_or_default(),
            }),
            None => {
                let Some(ls) = layer.layout.as_ref()
                    .filter(|_| layer.transforms.contains(&Transform::Layout))
                    .filter(|_| def.field == crate::transform::NODE_NAME)
                else { continue };
                let (Some(a), Some(b)) = (df.str_col(&ls.from), df.str_col(&ls.to)) else { continue };
                // The layout's own rule for which rows stand a node: both ends named
                // and different (`transform::graph_layout`).
                let mut nodes: Vec<String> = Vec::new();
                for (from, to) in a.iter().zip(b) {
                    if from.is_empty() || to.is_empty() || from == to {
                        continue;
                    }
                    for name in [from, to] {
                        if !nodes.contains(name) {
                            nodes.push(name.clone());
                        }
                    }
                }
                out.push(ColoredBy::Levels { field: def.field.clone(), drawn: nodes, declared: Vec::new() });
            }
        }
    }
    out
}

/// A level list for a refusal: each level quoted, since a level can hold a
/// comma (`"Korea, Rep."`), and cut off past thirty with a count of the rest,
/// since a column of every country would otherwise be a paragraph of names.
pub(super) fn quoted_levels(levels: &[String]) -> String {
    const SHOWN: usize = 30;
    let mut items: Vec<String> = levels.iter().take(SHOWN).map(|l| format!("\"{l}\"")).collect();
    if levels.len() > SHOWN {
        items.push(format!("{} more", levels.len() - SHOWN));
    }
    and_list(&items)
}

/// Check a palette that binds a color to each level by name (spec §10, "A color
/// bound to a level by name").
///
/// **Refused in both directions, and that is the rule the author ruled in.** A
/// name that is no level would color nothing, and a typo is exactly how that
/// happens: `Asai = "tomato"` beside a column holding `Asia` used to draw Asia in
/// some other color with nothing said. A drawn level with no name would have to
/// be given a color nobody chose, which is the silent default §12 forbids. So
/// the names and the drawn levels must match, and each refusal lists what the
/// reader needs to make them match.
///
/// **Validity first, then the match.** A bad color or a name given twice is a
/// mistake about the palette itself and is reported on its own terms. Among the
/// match refusals the unknown name comes first and a missing level is said only
/// when every name is a level: a single misspelling produces both — `Asai`
/// unknown and `Asia` left out — and the one sentence that fixes it is the
/// first.
pub(super) fn check_named_palette(
    out: &mut Vec<Diagnostic>,
    spec: &PlotSpec,
    data: &HashMap<String, DataFrame>,
    named: &LevelColors,
) {
    if named.0.is_empty() {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: "gog: `palette()` was given no colors. Give it at least one level and \
                      its color, such as Asia as \"tomato\" and Europe as \"steelblue\"."
                .into(),
        });
        return;
    }

    let before = out.len();
    // A name written twice. R's named vectors and Julia's pairs can both carry
    // one, and whichever came last would otherwise win without a word.
    let mut seen: Vec<&str> = Vec::new();
    for (level, _) in &named.0 {
        if seen.contains(&level.as_str()) {
            continue;
        }
        seen.push(level);
        let given: Vec<String> = named.0.iter()
            .filter(|(l, _)| l == level)
            .map(|(_, c)| format!("\"{c}\""))
            .collect();
        if given.len() > 1 {
            let times = if given.len() == 2 { "twice".to_string() } else { format!("{} times", given.len()) };
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: format!(
                    "gog: `palette()` names \"{level}\" {times}, as {}. A level has one \
                     color, so keep one of them.",
                    or_list(&given),
                ),
            });
        }
    }
    for (level, color) in &named.0 {
        if !is_valid_color(color) {
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: format!(
                    "gog: `palette()` gives \"{level}\" the color \"{color}\", which is not a \
                     color.{}",
                    color_advice(color)
                ),
            });
        }
    }
    if out.len() > before {
        return;
    }

    let colored = colored_by(spec, data);
    if let Some(ColoredBy::Numbers(field)) =
        colored.iter().find(|c| matches!(c, ColoredBy::Numbers(_)))
    {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `palette()` names levels, but `color` is bound to `{field}`, a column of \
                 numbers, and numbers have no levels to name. For a ramp, give its stops \
                 without names, such as \"white\" and \"navy\", or name a ramp, such as \
                 `palette(\"viridis\")`."
            ),
        });
        return;
    }

    // The levels across every colored layer, merged in order the way the color
    // map merges them, and the columns they come from for the message.
    let (mut fields, mut drawn, mut known): (Vec<String>, Vec<String>, Vec<String>) =
        (Vec::new(), Vec::new(), Vec::new());
    for c in &colored {
        let ColoredBy::Levels { field, drawn: d, declared } = c else { continue };
        if !fields.contains(field) {
            fields.push(field.clone());
        }
        for level in d.iter().chain(declared) {
            if !known.contains(level) {
                known.push(level.clone());
            }
        }
        for level in d {
            if !drawn.contains(level) {
                drawn.push(level.clone());
            }
        }
    }
    // Nothing is colored: `check_palette_is_used` says so, in the words that
    // apply to every palette, once this function has found nothing wrong.
    if fields.is_empty() {
        return;
    }
    let cols = or_list(&fields.iter().map(|f| format!("`{f}`")).collect::<Vec<_>>());
    let listed = if known.len() > drawn.len() { &known } else { &drawn };

    let mut unknown = false;
    for level in seen.iter().filter(|l| !known.iter().any(|k| k == *l)) {
        unknown = true;
        // A near miss, and only one no other name already claims — suggesting a
        // level that already has its color would send the reader to write it twice.
        let near = drawn.iter()
            .filter(|d| !seen.contains(&d.as_str()))
            .map(|d| (d, edit_distance(&level.to_lowercase(), &d.to_lowercase())))
            .filter(|(d, dist)| *dist <= 2 && *dist < d.chars().count())
            .min_by_key(|(_, dist)| *dist)
            .map(|(d, _)| format!(" Did you mean \"{d}\"?"))
            .unwrap_or_default();
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `palette()` names \"{level}\", and {cols} has no level called that, so \
                 its color would go to nothing.{near} The levels of {cols} are {}.",
                quoted_levels(listed),
            ),
        });
    }
    if unknown {
        return;
    }

    let missing: Vec<String> = drawn.iter()
        .filter(|d| named.get(d).is_none())
        .map(|d| format!("\"{d}\""))
        .collect();
    if !missing.is_empty() {
        let (drawn_in, fix) = match missing.len() {
            1 => ("it would be drawn in a color", "Give it a color too"),
            _ => ("they would be drawn in colors", "Give each of them a color too"),
        };
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `palette()` names colors for levels of {cols} and leaves out {}. Once \
                 colors are named, every level drawn needs one, or {drawn_in} nobody chose. \
                 {fix}, or pass the colors without names to hand them out in the column's \
                 order.",
                and_list(&missing),
            ),
        });
    }
}
