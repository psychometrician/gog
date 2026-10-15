//! What a `style()` setting may hold on each mark, the constant colors and glyph
//! names included.

use super::*;

// ---------------------------------------------------------------------------
// Constant values — colors and glyph names
//
// A set value has no data to check it against, so the value itself is the only
// thing that can be wrong. That makes validation matter *more* here than for a
// mapped channel: a misspelt column name already fails loudly, but a misspelt
// color is accepted by SVG and silently painted black. Silence is the one
// outcome gog forbids (§12).
// ---------------------------------------------------------------------------

/// The glyphs `point` can draw, in the order `shape` assigns them.
pub const SHAPE_NAMES: &[&str] =
    &["circle", "square", "triangle", "diamond", "cross", "star", "wye"];




/// The complete advice for a rejected color.
///
/// Returns the whole tail rather than a fragment, so a caller cannot append a
/// second, blander sentence that contradicts the specific one.
pub(super) fn color_advice(s: &str) -> String {
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

/// `style(reach = )` — how far a rule crosses the axis it does not name. The
/// mark half is `mark_takes_setting`, so this and the generated grid cannot
/// disagree; the value half is `REACHES`, the list the renderer reads too.
pub(super) fn check_reach(out: &mut Vec<Diagnostic>, mark: &Mark, style: &StyleSpec) {
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

/// `gap` — how much of a bar's or a box's own place is left empty beside it.
///
/// A share, so it is refused outside 0 to just under 1: at 1 nothing would be
/// left to draw. On a mark with no body standing in a slot it names nothing, and
/// the violin is pointed at `density(reach = )`, which already sets its width.
/// In `nest()` a bar is a packed region with no slot beside it, so there is no
/// air to set either.
pub(super) fn check_gap(out: &mut Vec<Diagnostic>, spec: &PlotSpec, layer: &Layer) {
    let Some(gap) = layer.style.gap else { return };
    let mark = &layer.mark;
    let m = mark_name(mark);
    if !mark_takes_setting(mark, Setting::Gap) {
        let violin = layer.transforms.contains(&Transform::Density)
            && matches!(mark, Mark::Ribbon | Mark::Area);
        let message = if violin {
            format!(
                "gog: `style(gap = )` sets the empty space beside a bar or a box, and a \
                 violin's width is set by `density(reach = )`, the share of its slot its \
                 widest point reaches. Write `{m} * density(reach = 0.3)` for a narrower \
                 violin, and remove the `gap`."
            )
        } else {
            format!(
                "gog: `style(gap = )` is the share of a bar's or a box's slot left empty, \
                 and {} `{m}` takes no width from its slot. Remove it, or use `bar` or \
                 `box`.",
                article(m)
            )
        };
        out.push(Diagnostic { kind: DiagnosticKind::Illegal, message });
        return;
    }
    if space_of(spec) == SpaceKind::Nest {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `style(gap = )` sets the empty space beside a {m} in its slot, and in \
                 `nest()` a {m} is a packed region with no slot. Remove the `gap`."
            ),
        });
        return;
    }
    if !(0.0..1.0).contains(&gap) {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `style(gap = {gap})` is outside 0 to 1. It is the share of each {m}'s \
                 slot left empty, or of its part of the slot when it is dodged: 0 makes \
                 neighbors touch, and 0.5 leaves half. At 1 nothing would be left to draw."
            ),
        });
    }
}

/// Check one layer's constant settings against the same `(mark, channel)` table
/// the mapped channels use.
/// What a *which one?* vocabulary hands out, singular and plural, for the
/// warning's words. Read off the same `texture_of` split that decides which
/// vocabulary applies, so a `line` is never told it draws textures.
pub(super) fn which_one_kinds(mark: &Mark, ch: &Channel) -> (&'static str, &'static str) {
    match (ch, texture_of(mark)) {
        (Channel::Shape, _) => ("glyph", "glyphs"),
        (Channel::Pattern, Some(Texture::Dash)) => ("dash", "dashes"),
        (Channel::Pattern, Some(Texture::Hatch)) => ("texture", "textures"),
        _ => ("kind", "kinds"),
    }
}

/// A *which one?* channel hands out a fixed vocabulary and then starts over, so a
/// column with more categories than kinds draws two groups the same way.
///
/// That plot is legal and it draws, so this is an **Assumption** rather than a
/// refusal (Law 8 — never forbid the ugly-but-legal). Saying nothing is the part
/// §12 forbids: a distinction the reader wrote down is silently gone from the
/// picture, and unlike a dropped row it leaves no gap to notice.
///
/// **The threshold is `setting_values`**, the one function the refusal message and
/// the generated settings tables also read. The vocabulary's size, the renderer's
/// modulus and this limit are therefore one number, which is the whole reason
/// growing a set and warning past it are a single piece of work: split them and
/// the number is written twice and drifts once.
/// A border beside a *mapped* `shape` whose categories include the `cross`.
///
/// The border draws on every glyph with a fill and on no cross, because a cross is
/// two strokes with nothing inside to outline. `check_border` refuses the cross as a
/// *setting*, where nothing at all would draw; here the rest of the points do get
/// their outline, so the plot is legal and this is an **Assumption** (Law 8), but
/// the categories that lost theirs are named, since a border missing from one
/// category leaves nothing on the page to notice (§12). The categories come from
/// `categories_across`, the order the renderer and the legend hand glyphs out in.
pub(super) fn check_border_on_crosses(out: &mut Vec<Diagnostic>, df: &DataFrame, mark: &Mark, layer: &Layer) {
    let style = &layer.style;
    if *mark != Mark::Point
        || (style.border_color.is_none() && style.border_size.is_none())
        || style.border_size == Some(0.0)
    {
        return;
    }
    let Some(field) = layer.encodings.get(&Channel::Shape).map(|c| c.field.as_str()) else {
        return;
    };
    let Some(cross) = SHAPE_NAMES.iter().position(|&s| s == "cross") else { return };
    let crossed: Vec<String> = crate::data::categories_across(&[df], field)
        .into_iter()
        .enumerate()
        .filter(|(i, _)| i % SHAPE_NAMES.len() == cross)
        .map(|(_, c)| format!("`{c}`"))
        .collect();
    if crossed.is_empty() {
        return;
    }
    let names = match crossed.as_slice() {
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
        [] => return,
    };
    out.push(Diagnostic {
        kind: DiagnosticKind::Assumption,
        message: format!(
            "gog: `shape({field})` draws {names} as a `cross`, and a cross is two strokes \
             with no fill, so `border_color` and `border_size` outline every other point \
             and not those. The glyphs follow the column's order: the fifth category, and \
             every seventh after it, is a cross, so a column of four categories or fewer \
             draws none."
        ),
    });
}

pub(super) fn check_which_one_cycles(out: &mut Vec<Diagnostic>, df: &DataFrame, mark: &Mark, layer: &Layer) {
    for ch in [Channel::Shape, Channel::Pattern] {
        let Some(field) = layer.encodings.get(&ch).map(|c| c.field.clone()) else {
            continue;
        };
        let kinds = setting_values(mark, channel_name(&ch)).len();
        // A mark with no vocabulary for this channel has a refusal of its own;
        // adding a second message about counting would bury it.
        if kinds == 0 {
            continue;
        }
        // The renderer indexes the same list, from the same function, so the
        // count this warns about is the count that draws.
        let n = crate::data::categories_across(&[df], &field).len();
        if n <= kinds {
            continue;
        }
        let (one, many) = which_one_kinds(mark, &ch);
        let over = n - kinds;
        // A count of one takes a singular verb. Worth the branch: the commonest
        // case is a column one past the limit, so the ungrammatical reading is
        // the one most readers would have met.
        let (subject, verb) = match over {
            1 => ("1 category", "shares"),
            _ => ("categories", "share"),
        };
        let subject = match over {
            1 => subject.to_string(),
            _ => format!("{over} {subject}"),
        };
        out.push(Diagnostic {
            kind: DiagnosticKind::Assumption,
            message: format!(
                "gog: `{c}({field})` has {n} categories and `{m}` draws {kinds} \
                 {many}, so the set starts over: {subject} {verb} a {one} with an \
                 earlier one. Use `color({field})`, whose default palette has \
                 twenty, or give each category its own panel with `facet({field})`.",
                c = channel_name(&ch),
                m = mark_name(mark),
            ),
        });
    }
}

pub(super) fn check_style(
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
                     Remove it, or use a mark that has one: {}.",
                    article(m), code_list_or(&marks_that_take(&channel, false))
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

/// **`opacity` fades what `color` paints**, and nothing else (spec §7).
///
/// That is the rule every mark already followed, stated once: a filled glyph fades
/// its fill, a stroke fades its stroke, and a cross, which has no fill, fades its
/// stroke because its stroke is what `color` paints. A border is drawn in its own
/// color, set with `border_color`, and at full strength. The two halves stay
/// independent, which is what lets a faint highlight carry a solid frame.
///
/// So `style(color = "none")` beside an `opacity`, set or mapped, is an opacity with
/// nothing to fade. It used to be accepted and dropped: the natural spelling of a
/// see-through hollow point, `style(color = "none", border_color = "steelblue",
/// opacity = 0.3)`, drew solid rings and said nothing (§12). The border's own color
/// is where its transparency goes, so when the sentence names both halves the
/// direction is the exact color that draws what was asked.
pub(super) fn check_nothing_to_fade(out: &mut Vec<Diagnostic>, mark: &Mark, layer: &Layer) {
    let st = &layer.style;
    let paints_nothing = st.color.as_deref().is_some_and(|c| {
        matches!(c.trim().to_ascii_lowercase().as_str(), "none" | "transparent")
    });
    if !paints_nothing {
        return;
    }
    let (asked, set) = match (st.opacity, layer.encodings.get(&Channel::Opacity)) {
        (Some(v), _) => (format!("style(opacity = {v})"), Some(v)),
        (None, Some(d)) => (format!("opacity({})", d.field), None),
        (None, None) => return,
    };
    let color = st.color.as_deref().unwrap_or("none").trim();
    let m = mark_name(mark);
    let direction = if !mark_takes_setting(mark, Setting::BorderColor) {
        format!("Drop `{asked}`, or give the {m} a color for it to fade.")
    } else {
        // The color that draws what was asked: the border's own color with the
        // opacity written into it as an alpha byte. Only when both are known.
        let exact = set.zip(st.border_color.as_deref()).and_then(|(v, b)| {
            crate::color::parse_rgb(b).map(|rgb| {
                let alpha = (v.clamp(0.0, 1.0) * 255.0).round() as u32;
                (format!("#{rgb:06x}{alpha:02x}"), b.trim().to_string(), (v * 100.0).round())
            })
        });
        match exact {
            Some((hex, b, pct)) => format!(
                "A border is drawn in its own color, so its transparency goes there: \
                 `style(color = \"{color}\", border_color = \"{hex}\")` draws a {b} outline \
                 at {pct}%."
            ),
            None => format!(
                "A border is drawn in its own color, set once for the layer, so its \
                 transparency goes there: `border_color = \"#4682b44d\"` is steelblue at 30%. \
                 Drop `{asked}`, or give the {m} a fill for it to fade."
            ),
        }
    };
    out.push(Diagnostic {
        kind: DiagnosticKind::Illegal,
        message: format!(
            "gog: `{asked}` fades what `color` paints, and `color = \"{color}\"` paints \
             nothing, so there is nothing here for it to fade. {direction}"
        ),
    });
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
/// `caps` — the short crossbars at a whisker's ends — belongs to the two marks
/// that draw whiskers, `interval` and `box`. On any other mark it names nothing;
/// refuse with direction rather than silently ignore it.
pub(super) fn check_caps(out: &mut Vec<Diagnostic>, mark: &Mark, style: &StyleSpec) {
    if style.caps.is_none() || mark_takes_setting(mark, Setting::Caps) {
        return;
    }
    let m = mark_name(mark);
    out.push(Diagnostic {
        kind: DiagnosticKind::Illegal,
        message: format!(
            "gog: `style(caps = )` is a whisker setting — the crossbars at a whisker's \
             ends, on an `interval` or a `box` — and {} `{m}` has no whiskers. Remove it, \
             or use `interval`.",
            article(m)
        ),
    });
}

/// `center` — whether an interval draws its center dot (a confidence interval's
/// mean) — is an interval-only setting, `caps`'s twin. On any other mark it names
/// nothing; refuse with direction rather than silently ignore it.
pub(super) fn check_center(out: &mut Vec<Diagnostic>, mark: &Mark, style: &StyleSpec) {
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

/// The `line` that traces a bordered ribbon's two edges, written as the reader
/// would add it (`line * range`, `line * bounds(lower, upper)`), or `None` when no
/// such line draws. It is the ribbon's own pair transform on a `line`, and it is
/// offered only when a trial `check()` of it, alone in the plot, passes: the
/// count hint's rule, since a suggested sentence the engine refuses sends the
/// reader from one refusal to the next.
pub(super) fn ribbon_edge_line(
    spec: &PlotSpec,
    layer: &Layer,
    data: &HashMap<String, DataFrame>,
) -> Option<String> {
    if layer.mark != Mark::Ribbon
        || (layer.style.border_color.is_none() && layer.style.border_size.is_none())
    {
        return None;
    }
    let pair = layer.transforms.iter().find(|t| is_pair_transform(t))?;
    let mut line = layer.clone();
    line.mark = Mark::Line;
    line.style = StyleSpec::default();
    line.transforms = vec![pair.clone()];
    let mut trial = spec.clone();
    trial.layers = vec![line];
    trial_draws(&trial, data)
        .then(|| format!("line * {}", written_pair(layer, pair)))
}

/// Whether an interval's statistic computes a center for `center` to show: the
/// mean of `confidence` and `deviation`, and the fitted value of `smooth_band`
/// (`transform::centered_pairs` and `smooth_band` write `SPAN_CENTER`). `range`
/// and `bounds` compute only the two ends. A test draws every pair transform and
/// holds this list to what the renderer drew.
pub(super) fn computes_center(transforms: &[Transform]) -> bool {
    transforms.iter().any(|t| matches!(t, Transform::Confidence | Transform::Deviation | Transform::SmoothBand))
}

/// `style(center = TRUE)` on an interval whose statistic computes no center. The
/// setting hides a center that exists and cannot make one (`marks/interval.qmd`,
/// "Hiding the center dot"), so on `range` or `bounds` it drew no dot and said
/// nothing: an instruction accepted and dropped, which §12 forbids. Refused since
/// 2026-10-13, found walking the settings grid; the author: "yes to give it a
/// message". Only a written `TRUE` is asked about. Unset, it is the default and
/// asks for nothing, and `FALSE` asks for no dot and gets none. The message writes
/// the setting blank, `style(center = )`, as every engine message does, since the
/// same words reach four languages that spell the value four ways.
///
/// Each way to a center is offered only if a trial `check()` of it passes: the
/// same layer on `confidence` or `deviation`, and a `point * mean` layer beside
/// the interval with the setting removed.
pub(super) fn check_center_exists(
    out: &mut Vec<Diagnostic>,
    spec: &PlotSpec,
    layer: &Layer,
    data: &HashMap<String, DataFrame>,
) {
    if layer.mark != Mark::Interval || layer.style.center != Some(true) || computes_center(&layer.transforms) {
        return;
    }
    // With no pair at all the interval is refused for that, by `check_span_needs_range`.
    let Some(pair) = layer.transforms.iter().find(|t| is_pair_transform(t)) else { return };
    let draws = |l: Layer| {
        let mut trial = spec.clone();
        trial.layers = vec![l];
        trial_draws(&trial, data)
    };
    let swapped: Vec<&str> = [Transform::Confidence, Transform::Deviation].into_iter()
        .filter(|with| {
            let mut l = layer.clone();
            l.style.center = None;
            l.transforms = layer.transforms.iter()
                .map(|t| if is_pair_transform(t) { with.clone() } else { t.clone() })
                .collect();
            draws(l)
        })
        .map(|t| transform_name(&t))
        .collect();
    // The second layer is offered with the setting removed, and tried that way:
    // beside an interval that still asks for a center, `point * mean` was refused
    // with this same message (the 0.6.1 round's A32).
    let mut mean = layer.clone();
    mean.mark = Mark::Point;
    mean.style = StyleSpec::default();
    mean.transforms = vec![Transform::Mean];
    let mut uncentered = layer.clone();
    uncentered.style.center = None;
    let mut ways: Vec<String> = Vec::new();
    if !swapped.is_empty() {
        let names = swapped.iter().map(|t| format!("`interval * {t}`")).collect::<Vec<_>>();
        ways.push(format!("{}, which {} the mean as the center", names.join(" or "),
                          if names.len() == 1 { "computes" } else { "compute" }));
    }
    let both = {
        let mut trial = spec.clone();
        trial.layers = vec![uncentered, mean];
        trial_draws(&trial, data)
    };
    if both {
        ways.push("remove the setting and add `point * mean` to draw the mean as a second \
                   layer".to_string());
    }
    let p = written_pair(layer, pair);
    let direction = match ways.len() {
        0 => " Remove it.".to_string(),
        _ => format!(" Use {}.", ways.join(", or ")),
    };
    out.push(Diagnostic {
        kind: DiagnosticKind::Illegal,
        message: format!(
            "gog: `style(center = )` asks this interval for its center dot, but `{p}` \
             computes none: it gives only the two ends of each span, and the setting can \
             hide a center but not make one.{direction}"
        ),
    });
}

/// `nudge` moves a `text` label off its point so a superposed `point + text`
/// does not draw the label on the dot. Text-only (a nudge on a `point` or `bar`
/// would misplace the data itself), and one of four plain directions — anything
/// else is refused with direction rather than guessed.
pub(super) fn check_nudge(out: &mut Vec<Diagnostic>, mark: &Mark, style: &StyleSpec) {
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

/// `angle` turns a `text` label, in degrees counterclockwise from horizontal
/// (spec §5, "A turned label"). Text-only, `nudge`'s geometry class: a word is the
/// one glyph read along a direction. Any finite number of degrees is a turn, so
/// only the shape is refused: a value that is not a number of degrees at all.
pub(super) fn check_angle(out: &mut Vec<Diagnostic>, mark: &Mark, style: &StyleSpec) {
    let Some(a) = style.angle else { return };
    if !mark_takes_setting(mark, Setting::Angle) {
        let m = mark_name(mark);
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: format!(
                "gog: `style(angle = )` is a `text` setting: it turns a label so a long \
                 name fits where it sits, and {} `{m}` has no label to turn. Remove it, or \
                 name the rows with a `text` layer and turn that.",
                article(m)
            ),
        });
        return;
    }
    if !a.is_finite() {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: "gog: `style(angle = )` needs a number of degrees, counterclockwise \
                      from horizontal: `90` reads upward and `-90` downward.".to_string(),
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
pub(super) fn check_arrow(out: &mut Vec<Diagnostic>, mark: &Mark, style: &StyleSpec) {
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
        Mark::Line | Mark::Step | Mark::Interval | Mark::Path | Mark::Rule
        | Mark::Edge => Some(Texture::Dash),
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
pub(super) fn marks_with_texture(t: Texture) -> String {
    let names: Vec<String> = ALL_MARKS
        .iter()
        .filter(|m| texture_of(m) == Some(t))
        .map(|m| format!("`{}`", mark_name(m)))
        .collect();
    names.join("/")
}

/// A vocabulary written out for a refusal: `"solid" (the default), "dashed", …`.
///
/// **Read from the array rather than typed beside it.** `shape`'s refusal has
/// always joined `SHAPE_NAMES`, and the two `pattern` refusals spelled their
/// lists out instead — so growing the vocabularies left both advising the
/// previous release's values while the engine drew the new ones. A reader who
/// wrote `"stripes"` wrongly was told the five that existed before it did.
pub(super) fn advise(values: &[&str]) -> String {
    values.iter()
        .enumerate()
        .map(|(i, v)| match i {
            0 => format!("\"{v}\" (the default)"),
            _ => format!("\"{v}\""),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// The legal dash values for a stroke, the counterpart of [`FILL_TEXTURES`].
pub(crate) const STROKE_DASHES: [&str; 5] =
    ["solid", "dashed", "dotted", "dotdash", "longdash"];

/// The directions `style(nudge = )` accepts. Named here beside the other closed
/// vocabularies rather than only inside its own refusal message, so
/// `setting_values` can hand it to the book like every other one.
pub(crate) const NUDGES: [&str; 4] = ["up", "down", "left", "right"];

pub(super) fn check_pattern(out: &mut Vec<Diagnostic>, mark: &Mark, style: &StyleSpec) {
    let Some(p) = style.pattern.as_deref() else { return };
    let m = mark_name(mark);
    match texture_of(mark) {
        // The path strokes — the dash pattern, built.
        Some(Texture::Dash) => {
            if !STROKE_DASHES.contains(&p) {
                out.push(Diagnostic {
                    kind: DiagnosticKind::Illegal,
                    message: format!(
                        "gog: `style(pattern = \"{p}\")` is not a stroke pattern. Use {}.",
                        advise(&STROKE_DASHES)
                    ),
                });
            }
        }
        // The fills — a hatch texture, the settable rule's fill arm (§4), now drawn
        // (`render::pattern`). `solid` (shared with the strokes) is the no-texture
        // default; the five hatchings texture. A stroke's dash value or a typo is
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
                        "gog: `style(pattern = )` on {} `{m}`: {lead} Use {}.",
                        article(m),
                        advise(&FILL_TEXTURES)
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

/// A halo on `text` whose color is left to the panel, over a panel that paints
/// nothing. An unset `border_color` is the panel's background (`write_halos`),
/// and a `transparent` or `none` background would stroke the halo in no color at
/// all: the setting accepted and nothing drawn, which §12 forbids. The fix is one
/// word, so the refusal names it.
pub(super) fn check_halo_ground(out: &mut Vec<Diagnostic>, mark: &Mark, style: &StyleSpec, spec: &PlotSpec) {
    let asks_for_panel_color = *mark == Mark::Text
        && style.border_color.is_none()
        && style.border_size.is_some_and(|w| w > 0.0);
    let see_through = spec.theme.resolved().background.as_deref()
        .is_some_and(crate::color::paints_nothing);
    if asks_for_panel_color && see_through {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: "gog: a halo on `text` takes the panel's background color when \
                      `border_color` is not set, and this panel's background is transparent, \
                      so the halo would hide nothing. Name the halo's color, for example \
                      `style(border_color = \"white\")`.".to_string(),
        });
    }
}

/// `edge` is the line that traces a ribbon's two edges, written out and already
/// checked to draw (`ribbon_edge_line`), or `None`; only a `ribbon` reads it.
pub(super) fn check_border(
    out: &mut Vec<Diagnostic>,
    mark: &Mark,
    style: &StyleSpec,
    transforms: &[Transform],
    edge: Option<&str>,
) {
    if style.border_color.is_none() && style.border_size.is_none() {
        return;
    }
    let m = mark_name(mark);
    // Which marks carry a border is `mark_takes_setting` (the closed-glyph fills —
    // spec §4, the settable rule), shared with the generated grid so the two agree.
    // On a mark that does, the value needs checking: a valid color, a non-negative
    // width. And one glyph: `point`'s border draws on the glyphs with a fill, and a
    // `cross` is two strokes with none, so it has no outline to draw. This comment
    // used to leave that to the renderer, which drew nothing and said nothing: set
    // on every point, the border was dropped from every point and the plot came out
    // byte for byte the one without it (§12). A size of 0 asks for no border and
    // drops nothing, so it passes. A cross that only some categories draw with is
    // `check_border_on_crosses`'s, since the rest still get their border.
    if *mark == Mark::Point
        && style.shape.as_deref().is_some_and(|s| s.eq_ignore_ascii_case("cross"))
        && style.border_size != Some(0.0)
    {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: "gog: a `cross` is two strokes with no fill, so it has no outline for \
                      `border_color` or `border_size` to draw, and the plot would come out \
                      the same without them. Color the cross with `style(color = )`, and \
                      draw it larger and thicker with `style(size = )`; or choose a glyph \
                      with a fill, such as `style(shape = \"circle\")`, to give each point \
                      an outline."
                .to_string(),
        });
    }
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
        Mark::Line | Mark::Step | Mark::Interval | Mark::Rule | Mark::Edge => out.push(Diagnostic {
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
        // Under `flow` no mark traces a band's edge (`line * flow` is refused), so the
        // direction is the one border a flow has: the strata a `zone` frames.
        Mark::Ribbon if transforms.contains(&Transform::Flow) => out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: "gog: the bands of a `flow` have no border, and no mark traces their \
                      edges: `line * flow` is refused. The strata between the bands take one: \
                      `zone * flow(<a>, <b>) + style(border_color = \"white\")` frames each \
                      stratum.".to_string(),
        }),
        // A ribbon's edges are its pair's two boundary curves, and a `line` with the
        // same transform traces both (spec §4, the curve fills' edge row). The message
        // said only "layer a `line`" until 2026-10-13, which left the reader to work
        // out `line * range`; it now hands back that sentence, built from the
        // ribbon's own transform. A ribbon with no pair (a violin's `density`) has no
        // such line, and keeps the general direction.
        Mark::Ribbon => out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: match edge {
                Some(line) => format!(
                    "gog: a `ribbon` is a filled band with no border of its own — add \
                     `{line}` to trace both of its edges as a `line` layer, which \
                     `style()` can color, size and dash."
                ),
                None => "gog: a `ribbon` is a filled band with no border of its own — layer a \
                         `line` for a visible edge along one of its boundaries.".to_string(),
            },
        }),
        Mark::Path => out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: "gog: a `path` is a stroke, and a stroke has no separate border — \
                      `style(color = )` sets the line itself, `style(size = )` its width. \
                      (The same answer `line` gives.)".to_string(),
        }),
        // All six carry a border and are handled above, so none reaches here — a
        // `surface`'s being its mesh lines (spec §15), a `zone`'s the frame round
        // each region it fills, a `text` label's the halo around its letters.
        Mark::Bar | Mark::Box | Mark::Point | Mark::Surface | Mark::Zone | Mark::Text => {}
    }
}
