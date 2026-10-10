//! The theme: the page's own settings, apart from the ink.

use super::*;

// ---------------------------------------------------------------------------
// Theme — the page, not the ink (spec §7)
//
// The bindings validate too, so that a caller is told at the line they wrote.
// This exists anyway, and for the reason §14 gives: a rule implemented in a
// binding is a rule the other three get wrong. There are four of them now, and
// a fifth would inherit this check by construction and the bindings' by copying.
// ---------------------------------------------------------------------------

pub(super) fn check_theme(out: &mut Vec<Diagnostic>, spec: &PlotSpec) {
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

    if let Some(place) = spec.theme.axis_label.as_deref() {
        const PLACE: &[&str] = &["end", "beside"];
        if !PLACE.contains(&place) {
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: format!(
                    "gog: `theme(axis_label = \"{place}\")` is not a place for an axis's \
                     name. gog has {} — `\"end\"` puts each name at its axis's far end, \
                     horizontal, and `\"beside\"` centers it along the axis, turning the \
                     y name through 90 degrees.",
                    or_list(&PLACE.iter().map(|s| format!("`{s}`")).collect::<Vec<_>>())
                ),
            });
        }
    }

    // A side, and nothing else. `none` is the likeliest other word, so the
    // refusal names where leaving a key out is written: on the channel it decodes.
    if let Some(side) = spec.theme.legend.as_deref() {
        const SIDE: &[&str] = &["right", "left", "top", "bottom"];
        if !SIDE.contains(&side) {
            out.push(Diagnostic {
                kind: DiagnosticKind::Illegal,
                message: format!(
                    "gog: `theme(legend = \"{side}\")` is not a side of the plot. gog has {} \
                     — `\"bottom\"` lays the keys in a row under the plot. To leave a legend \
                     out, set `legend` to false on the channel it decodes, such as `color`.",
                    or_list(&SIDE.iter().map(|s| format!("`\"{s}\"`")).collect::<Vec<_>>())
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
                     Names that do not fit side by side are turned without it, so state \
                     an angle only to choose one, such as `tick_angle = 45`."
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

    // A size has to be a size, and a page's size is checked by the same function
    // for the same reason the two dimensions share one loop.
    // A plot alone has no plots beside it, so the space between them has nothing
    // to measure. Refused with the page it belongs on.
    if spec.theme.gap.is_some() {
        out.push(Diagnostic {
            kind: DiagnosticKind::Illegal,
            message: "gog: `theme(gap = )` is the space between the plots of an \
                      arrangement, and this `theme()` belongs to one plot. The engine spaces a \
                      plot's own panels, a facet's included. Write it on the arrangement: \
                      `(a | b) + theme(gap = 8)`."
                .to_string(),
        });
    }
    check_theme_size(
        out,
        &spec.theme,
        "the plot",
        "On its own it sizes the image; composed into an arrangement it sizes the cell, \
         which is how a marginal plot says it is thin.",
    );
}

/// The smallest plot worth drawing, in pixels. Below this there is no room for a
/// panel once the margins have taken theirs, so the answer is a refusal with the
/// number in it rather than an empty rectangle.
pub(super) const MIN_PLOT_SIZE: f64 = 40.0;

/// The smallest type size that is still type, in pixels. Its job is less to rule
/// out tiny text than to catch `theme(font_size = 1.5)` — the reading in which
/// the number is a multiplier rather than a measurement.
pub(super) const MIN_FONT_SIZE: f64 = 4.0;
