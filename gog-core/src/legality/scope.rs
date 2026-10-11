//! Scope: which layers a channel written once reaches, and what the plot itself
//! may carry.

use super::*;

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
pub(super) fn accepts_binding(mark: &Mark, channel: &Channel) -> bool {
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
        // **A column a transform writes may be left out, and a `text` layer's
        // label is one** (ruled 2026-10-09: "any transform that create column, we
        // should allow the omit of that column"). `partition`, `flow` and `layout`
        // write each node's `name`, and it is the only column a `text` layer over
        // them can show, as `count` is the only one `bar * bin` can stand on `y`.
        // Required everywhere else, `label()` was required here too, so
        // `text * flow(class, survived)` was refused for leaving out the one
        // binding the transform had already made. Supplied here, beside the box's
        // summary, so the check and the renderer agree that the layer has it.
        if layer.mark == Mark::Text && !layer.encodings.contains_key(&Channel::Label)
            && layer.transforms.iter().any(|t| matches!(t,
                Transform::Partition | Transform::Flow | Transform::Layout))
        {
            layer.encodings.insert(
                Channel::Label, ChannelDef::field(crate::transform::NODE_NAME));
        }
    }
    out
}

/// Why each mark a plot-wide channel skipped cannot take it, in the layer refusal's
/// two reasons: a mark with no such feature, and one that takes a single value for
/// the whole layer (`settable`), which `style()` sets.
fn why_not_taken(c: &str, skipped: &[(&str, bool)]) -> String {
    let named = |settable: bool| -> Vec<String> {
        skipped.iter().filter(|(_, s)| *s == settable).map(|(m, _)| format!("`{m}`")).collect()
    };
    let (none, one) = (named(false), named(true));
    let mut parts = Vec::new();
    if !none.is_empty() {
        parts.push(format!("{} ha{} no {c} feature", and_list(&none),
                           if none.len() == 1 { "s" } else { "ve" }));
    }
    if !one.is_empty() {
        parts.push(format!("{} take{} one {c} for the whole layer, not one per row",
                           and_list(&one), if one.len() == 1 { "s" } else { "" }));
    }
    parts.join(", and ")
}

pub(super) fn check_plot_scope(out: &mut Vec<Diagnostic>, spec: &PlotSpec) {
    for (channel, def) in &spec.channels {
        let c = channel_name(channel);
        let field = &def.field;

        // A mark that takes the channel either reads the plot's column or names
        // its own after it, and only the first is reached. When every one names
        // its own, the plot's binding changes nothing: `size(population) + point +
        // size(life)` drew the points by `life` and was byte for byte the sentence
        // without `size(population)`, and with a `line` beside them the note below
        // said the plot's binding applied to the points.
        let (mut taken, mut own, mut skipped) = (Vec::new(), Vec::new(), Vec::new());
        for layer in &spec.layers {
            let m = mark_name(&layer.mark);
            if !accepts_binding(&layer.mark, channel) {
                skipped.push((m, rule_for(&layer.mark, channel).settable));
            } else if let Some(mine) = layer.encodings.get(channel) {
                own.push(format!("`{m}` maps `{c}({})`", mine.field));
            } else {
                taken.push(m);
            }
        }
        skipped.dedup();
        own.dedup();
        if taken.is_empty() && !own.is_empty() {
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: format!(
                    "gog: `{c}({field})` is written for the whole plot, but every mark here \
                     with a {c} feature names its own ({}), so it reaches none of them and \
                     would change nothing. Remove `{c}({field})`, or remove a mark's own \
                     `{c}()` to let the plot's apply to it.",
                    own.join(", ")
                ),
            });
            continue;
        }
        if skipped.is_empty() {
            continue;
        }

        // **The same reason and direction the binding gets after the mark** (the
        // consistency audit's F5). Written before it, the refusal named no mark that
        // takes the channel, said a line "has no opacity feature" when it takes one
        // opacity for the whole layer, and dropped the `style()` that sets it. The
        // direction names the channel and the column apart, as the layer's does, so
        // it reads the same in all four bindings.
        if taken.is_empty() {
            // A setting binds to the mark before it, so it is placed after the mark.
            let direction = if skipped.iter().all(|(_, settable)| *settable) {
                format!(
                    "Drop the `{field}` mapping and set it with `style({c} = )` after {}, or \
                     add a mark that maps `{c}`: {}.",
                    or_list(&skipped.iter().map(|(m, _)| format!("`{m}`")).collect::<Vec<_>>()),
                    code_list_or(&marks_that_take(channel, true))
                )
            } else {
                format!(
                    "Remove the `{field}` mapping from `{c}`, or add a mark that maps it: {}.",
                    code_list_or(&marks_that_take(channel, true))
                )
            };
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: format!(
                    "gog: `{c}` maps `{field}` for the whole plot, written before the marks, \
                     and no mark here takes it: {}. {direction}",
                    why_not_taken(c, &skipped)
                ),
            });
            continue;
        }

        taken.dedup();
        out.push(Diagnostic {
            kind: DiagnosticKind::Assumption,
            message: format!(
                "gog: `{c}({field})` is written for the whole plot, so it applies to {} — \
                 {}, so {} left unchanged. Move `{c}({field})` after a mark to bind it to \
                 that mark alone.",
                or_list(&taken.iter().map(|s| format!("`{s}`")).collect::<Vec<_>>()),
                why_not_taken(c, &skipped),
                if skipped.len() == 1 { "it is" } else { "they are" },
            ),
        });
    }
}
