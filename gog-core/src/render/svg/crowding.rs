//! Crowding: what the canvas has no room to print in full, how it is thinned,
//! and the warning that says so.

use super::*;

/// A category axis's ticks with every `stride`-th name kept, the first included.
/// Its tick marks and gridlines go with the names, since a mark at a place the
/// axis does not name points at nothing a reader can look up.
pub(super) fn thin_names(t: &TickSpec, stride: usize) -> TickSpec {
    let (values, labels) = t.values.iter().zip(&t.labels)
        .step_by(stride.max(1))
        .map(|(v, l)| (*v, l.clone()))
        .unzip();
    ticks_with_labels(values, labels)
}

/// A numeric axis's ticks with every `stride`-th kept, labels, marks and
/// gridlines together, as [`thin_names`] keeps names. On an evenly spaced linear
/// axis the kept ticks start where one lands on a multiple of the wider step, so a
/// thinned 10, 20, 30, 40, 50 reads 20, 40 rather than 10, 30, 50: the numbers a
/// coarser step would have chosen. Elsewhere (a log axis, a calendar, an uneven
/// set) they start at the first, as names do.
pub(super) fn thin_numbers(t: &TickSpec, stride: usize, linear: bool) -> TickSpec {
    let from = crate::render::layout::numbers_from(&t.values, stride, linear);
    let (values, labels) = t.values.iter().zip(&t.labels)
        .skip(from)
        .step_by(stride.max(1))
        .map(|(v, l)| (*v, l.clone()))
        .unzip();
    ticks_with_labels(values, labels)
}

/// The names of some legends, as one subject with its verb: "the `G` legend
/// needs", "the `G` and `S` legends need".
pub(super) fn legends_named(legends: &[&crate::render::legend::LegendBox]) -> (String, &'static str, &'static str) {
    let names: Vec<String> = legends.iter().map(|b| format!("`{}`", b.title)).collect();
    match names.as_slice() {
        [one] => (format!("{one} legend"), "needs", "it was"),
        [init @ .., last] => (format!("{} and {last} legends", init.join(", ")), "need", "they were"),
        [] => (String::from("legend"), "needs", "it was"),
    }
}

/// A legend laid across that is wider than the plot even with one key to a line.
pub(super) fn legend_too_wide_across(
    legends: &[crate::render::legend::LegendBox], left_out: &[usize], fonts: (f64, f64), room: f64,
) -> Diagnostic {
    let out: Vec<&crate::render::legend::LegendBox> = left_out.iter().map(|&i| &legends[i]).collect();
    let needs = out.iter().map(|b| b.across(fonts, room).least).fold(0.0_f64, f64::max);
    let (which, verb, it) = legends_named(&out);
    Diagnostic {
        kind: crate::legality::DiagnosticKind::Assumption,
        message: format!(
            "gog: the {which} {verb} {needs:.0}px of width even with one key to a line, and \
             laid across the plot there is room for {room:.0}, so {it} left out. Give the plot \
             more room with `theme(width = )`, or put the legend beside the plot with \
             `theme(legend = \"right\")`."
        ),
    }
}

/// A legend laid across that would leave the panels shorter than itself: the
/// width rule (`legend_too_wide`) read down instead of across.
pub(super) fn legend_too_tall_across(
    legends: &[crate::render::legend::LegendBox], side: LegendSide, band: f64, height: f64,
) -> Diagnostic {
    let all: Vec<&crate::render::legend::LegendBox> = legends.iter().collect();
    let (which, verb, it) = legends_named(&all);
    let place = if side == LegendSide::Top { "above" } else { "below" };
    Diagnostic {
        kind: crate::legality::DiagnosticKind::Assumption,
        message: format!(
            "gog: the {which} {verb} {band:.0}px of height, and {place} the {height:.0}px plot \
             that would leave its panel shorter than that, so {it} left out and the panel takes \
             the room. Give the plot more room with `theme(height = )` — on a composed page \
             that is a share of the page, so the other plots have to give some up."
        ),
    }
}

pub(super) fn legend_too_wide(legends: &[crate::render::legend::LegendBox], widest: f64, width: f64) -> Diagnostic {
    let names: Vec<String> = legends.iter().map(|b| format!("`{}`", b.title)).collect();
    let (which, verb, it) = match names.as_slice() {
        [one] => (format!("{one} legend"), "needs", "it was"),
        [init @ .., last] => (format!("{} and {last} legends", init.join(", ")), "need", "they were"),
        [] => (String::from("legend"), "needs", "it was"),
    };
    Diagnostic {
        kind: crate::legality::DiagnosticKind::Assumption,
        message: format!(
            "gog: the {which} {verb} {widest:.0}px of width, and beside {} the {width:.0}px plot \
             would leave its panel narrower than that, so {it} left out and the panel takes \
             the room. Give the plot more room with `theme(width = )` — on a composed page \
             that is a share of the page, so the other plots have to give some up.",
            if names.len() == 1 { "it" } else { "them" },
        ),
    }
}

pub(super) fn thinned_numbers(axis: char, field: &str, asked: usize, chosen: usize, kept: usize) -> Diagnostic {
    let field = if field.is_empty() { "<column>" } else { field };
    let (room, bigger) = match axis {
        'x' => ("side by side", "a wider plot (`theme(width = )`)"),
        _ => ("one above another", "a taller plot (`theme(height = )`)"),
    };
    Diagnostic {
        kind: crate::legality::DiagnosticKind::Assumption,
        message: format!(
            "gog: `{axis}({field}, tick_count = {asked})` asks for about {asked} ticks, and \
             the {chosen} ticks it chose do not fit {room} at this size, so {kept} of them are \
             drawn. A smaller `tick_count`, or {bigger}, draws them all."
        ),
    }
}

/// Say that a category axis draws one name in every `stride`, and how to name
/// them all (spec §12): the names that were not printed are information the
/// binding carried, so thinning them is an Assumption, never silent.
///
/// The direction names the column rather than a call (§12), and it is the one
/// that works for both readings of a crowded axis: a row of names across the
/// bottom gets its room by running down the side instead, where a taller plot
/// buys every name a line; a column of names already down the side just needs
/// the taller plot.
#[allow(clippy::too_many_arguments)]
/// The room a strip name keeps clear of its strip's ends, in pixels.
pub(super) const STRIP_PAD: f64 = 4.0;

/// The smallest share of the usual size a strip name may be shrunk to before it
/// is shortened instead: below this it stops being readable.
pub(super) const STRIP_FONT_FLOOR: f64 = 0.7;

/// The size a family of strip names is written at, the names to write, and how
/// many had to be shortened.
///
/// Every name has to fit the room its strip gives it, or the names run into each
/// other and the strips stop saying which panel is which. They did: seven maps in
/// a row, one per continent, wrote "Seven seas (open ocean)" across its
/// neighbor's name. One size for the whole family, so that no strip reads as more
/// important than another, shrunk only as far as the longest name needs and never
/// below `STRIP_FONT_FLOOR`. A name still too long there is cut short with an
/// ellipsis, and the count goes back to the caller, which says so. Names that
/// already fit come back unchanged, at the usual size, so a plot whose names fit
/// is drawn exactly as before.
pub(super) fn fit_strip_names(names: &[&str], room: f64, font: f64) -> (f64, Vec<String>, usize) {
    let widest = names.iter().map(|n| estimate_text_width(n, font)).fold(0.0, f64::max);
    if widest <= room.max(0.0) || widest <= 0.0 {
        return (font, names.iter().map(|n| n.to_string()).collect(), 0);
    }
    let floor = font * STRIP_FONT_FLOOR;
    // Two decimals, so the size reads as a size in the SVG.
    let size = ((font * room.max(0.0) / widest).max(floor) * 100.0).floor() / 100.0;
    let mut shortened = 0;
    let out = names.iter().map(|&n| {
        if estimate_text_width(n, size) <= room {
            return n.to_string();
        }
        shortened += 1;
        let mut kept: Vec<char> = n.chars().collect();
        while !kept.is_empty() {
            kept.pop();
            let candidate = format!("{}…", kept.iter().collect::<String>().trim_end());
            if estimate_text_width(&candidate, size) <= room {
                return candidate;
            }
        }
        "…".to_string()
    }).collect();
    (size, out, shortened)
}

/// Strip names cut short to fit (`fit_strip_names`).
pub(super) fn shortened_strip_names(n: usize) -> Diagnostic {
    let (count, verb) = if n == 1 { ("1 panel name".to_string(), "does") } else { (format!("{n} panel names"), "do") };
    Diagnostic {
        kind: crate::legality::DiagnosticKind::Assumption,
        message: format!(
            "gog: {count} {verb} not fit {}, even at a smaller size, and {} shortened with \"…\". \
             A wider plot (`theme(width = )`) or fewer panels in a row (`wrap = `) shows {} in full.",
            if n == 1 { "its strip" } else { "their strips" },
            if n == 1 { "is" } else { "are" },
            if n == 1 { "it" } else { "them" },
        ),
    }
}

pub(super) fn thinned_names(axis: char, field: &str, n: usize, stride: usize, pitch: f64, across: f64,
                 angle: Option<f64>, height: f64) -> Diagnostic {
    let field = if field.is_empty() { "<column>" } else { field };
    let kept = n.div_ceil(stride.max(1));
    // One decimal, unless that would print the two numbers equal (8.87 against
    // 8.92 is "8.9px apart, and a name needs 8.9px"), which reads as no reason.
    let digits = if format!("{pitch:.1}") == format!("{across:.1}") { 2 } else { 1 };
    // Upright names side by side need their width, not a line's height.
    let need = match axis == 'x' && angle.unwrap_or(0.0) == 0.0 {
        true => "less than the names are wide".to_string(),
        false => format!("and a name needs {across:.digits$}px"),
    };
    let (how, fix) = match (axis, angle) {
        ('x', Some(a)) if (a - 90.0).abs() < 1e-9 => (
            "do not fit side by side, even turned to read upward".to_string(),
            format!("swap the two axes so `{field}` runs down the y axis, in a taller plot \
                     (`theme(height = {height})`)"),
        ),
        ('x', a) => (
            format!("do not fit side by side at `tick_angle = {}`", a.unwrap_or(0.0)),
            format!("turn them with `theme(tick_angle = 90)`, or swap the two axes so \
                     `{field}` runs down the y axis, in a taller plot \
                     (`theme(height = {height})`)"),
        ),
        _ => (
            "do not fit one above another".to_string(),
            format!("make the plot taller (`theme(height = {height})`)"),
        ),
    };
    Diagnostic {
        kind: crate::legality::DiagnosticKind::Assumption,
        message: format!(
            "gog: the {n} names of `{field}` on the {axis} axis {how}: they are {pitch:.digits$}px \
             apart, {need}. One in every {stride} is drawn ({kept} of {n}), with its tick; \
             the rest keep their places, unnamed. To name every one, {fix}, or draw fewer \
             rows."
        ),
    }
}
