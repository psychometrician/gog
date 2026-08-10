//! Color *scales* — which colors a `color` channel hands out.
//!
//! Two shapes, chosen by the bound column rather than by a second atom:
//! a categorical palette assigns one color per category, a sequential ramp
//! interpolates along a continuum. `palette()` names both.
//!
//! Distinct from [`crate::color`], which answers "is this string a color, and
//! what color is it". This module answers "which colors should this scale
//! use". The first has no idea what a plot is; this one does.

use std::collections::HashMap;

use crate::color::css_rgb;
use crate::data::DataFrame;
use crate::ir::{Channel, PaletteDef, PlotSpec};
use crate::legality::{Diagnostic, DiagnosticKind};
use crate::render::text::esc;

// ---------------------------------------------------------------------------
// Named palettes — base colors; overflow handled by HSL wheel
// ---------------------------------------------------------------------------

/// gog default — 20 visually distinct colors.
pub(crate) const PALETTE_GOG: &[&str] = &[
    "#4e79a7", "#f28e2b", "#e15759", "#76b7b2", "#59a14f",
    "#edc948", "#b07aa1", "#ff9da7", "#9c755f", "#bab0ac",
    "#a0cbe8", "#ffbe7d", "#ff9d9a", "#86bcb6", "#8cd17d",
    "#b6992d", "#f1ce63", "#d7b5a6", "#499894", "#d4a6c8",
];

/// ColorBrewer's Set2 — the muted palette, for paint that covers ground.
///
/// [`PALETTE_GOG`] is tuned for points and lines, where a color occupies a few
/// pixels and has to carry from across the page. A fill has the opposite
/// problem: it already holds the reader's attention by covering ground, so the
/// saturation that made a 4.5px dot findable is more weight than a bar needs,
/// and a row of them competes with the numbers they exist to show. These are
/// the same eight jobs at lower saturation.
///
/// **No contrast floor here, unlike the ramps.** `gog_derived_ramps_keep_their_
/// pale_end_on_the_page` holds a bar against the panel because a ramp's pale end
/// has to be distinguishable from *absence* — a faint dot and a missing row look
/// alike. A categorical color answers "which one", never "how much", so it only
/// has to be distinguishable from the *other categories*, and a yellow category
/// is legitimately yellow. Measured rather than assumed: this palette's palest
/// entry holds 1.27:1 against the panel, between `PALETTE_GOG`'s 1.40 and
/// `PALETTE_OKABE`'s 1.11, so a floor that failed it would fail both of those
/// first. What *is* checked is `categorical_palettes_hand_out_distinct_colors`.
///
/// Copyright (c) 2002 Cynthia Brewer, Mark Harrower, and The Pennsylvania State
/// University, licensed under Apache 2.0 — the license gog itself ships under.
/// See the repository's `NOTICE`.
pub(crate) const PALETTE_SOFT: &[&str] = &[
    "#66c2a5", "#fc8d62", "#8da0cb", "#e78ac3",
    "#a6d854", "#ffd92f", "#e5c494", "#b3b3b3",
];

/// Okabe-Ito (2008) — designed for colorblindness; 8 base + 8 light variants.
pub(crate) const PALETTE_OKABE: &[&str] = &[
    // Base 8 (original Okabe-Ito, black replaced with dark gray)
    "#e69f00", "#56b4e9", "#009e73", "#0072b2", "#d55e00", "#cc79a7", "#f0e442", "#333333",
    // Light variants (L +18%) for overflow
    "#f2c55c", "#8acdef", "#4dc49e", "#4da7d9", "#e68a55", "#dda3c2", "#f5ee7d", "#777777",
];

// ---------------------------------------------------------------------------
// Continuous ramps
//
// A sequential scale is **one hue, light to dark**. The lightness ordering is
// what makes it readable as "more" — it survives grayscale, print, and color
// blindness, and it does not compete with the categorical palette when both
// appear in one figure. A rainbow encodes magnitude as hue, which has no
// intrinsic order, and is the classic way to make a legible chart illegible.
// ---------------------------------------------------------------------------

/// Default sequential ramp — a single blue, light to dark.
///
/// Built on the hue of `PALETTE_GOG[0]`, so the continuous and categorical
/// scales read as one system: the middle stop is that color. Checked rather
/// than eyeballed — lightness is monotone (L 70 → 48 → 29), the hue spread is
/// 5°, and the light end holds 2.10:1 against the `#f5f5f8` panel.
///
/// That last constraint is stricter than the usual advice for sequential
/// scales, which lets the lightest step fade into the surface. That advice
/// assumes heatmaps and choropleths — large filled regions. gog draws *points*,
/// and a 4.5px dot at 1.25:1 is invisible, so the range is compressed to keep
/// the low end on the page.
pub(crate) const RAMP_BLUE: &[&str] = &["#8faed5", "#4375a6", "#004383"];

/// Perceptually uniform, multi-hue. Not the default — hue carries no order, so
/// this trades the instant readability of a single hue for finer discrimination
/// across many levels. Worth having for dense data, and it is what people
/// arriving from matplotlib or ggplot2 expect to be able to ask for.
pub(crate) const RAMP_VIRIDIS: &[&str] = &[
    "#440154", "#414487", "#2a788e", "#22a884", "#7ad151", "#fde725",
];

/// Viridis's three siblings, sampled from matplotlib's tables (public domain).
///
/// Shipped **verbatim**, and keeping their proper nouns where the ramps gog
/// derives take plain ones, because for these the name *is* the palette: nobody
/// asks for "the black-to-yellow one". They are one published family, so
/// shipping three of the four and refusing the fourth would be a gap with no
/// argument behind it.
///
/// All four run paler at the top than [`RAMP_BLUE`] is allowed to, and that is
/// the trade perceptual uniformity asks for rather than an oversight: `magma`
/// ends on a cream that holds 1.03:1 against the panel, so a lone point at the
/// very top of the scale is nearly invisible where a filled region would be
/// fine. It is also why the default is a single hue and not one of these.
pub(crate) const RAMP_MAGMA: &[&str] = &[
    "#000004", "#3b0f70", "#8c2981", "#de4968", "#fe9f6d", "#fcfdbf",
];

/// See [`RAMP_MAGMA`] — same family, hotter hue path.
pub(crate) const RAMP_INFERNO: &[&str] = &[
    "#000004", "#420a68", "#932667", "#dd513a", "#fca50a", "#fcffa4",
];

/// See [`RAMP_MAGMA`] — same family, running blue through magenta to yellow.
pub(crate) const RAMP_PLASMA: &[&str] = &[
    "#0d0887", "#6a00a8", "#b12a90", "#e16462", "#fca636", "#f0f921",
];

/// See [`RAMP_MAGMA`] — the family's colorblind-optimized member, and the one
/// to ask for when the ramp itself has to survive dichromacy rather than merely
/// be readable by most people.
pub(crate) const RAMP_CIVIDIS: &[&str] = &[
    "#00204d", "#31446b", "#666970", "#958f78", "#cab969", "#ffea46",
];

/// The one gray a scale turns through when it means "nothing here" — the center
/// of both diverging ramps and the pale end of [`RAMP_GRAY`]. Shared rather than
/// repeated, so "no difference" is one color across the whole grammar and a
/// reader who learns it on one ramp reads it on the others.
///
/// Its value is set by the panel, not by taste: it is the lightest gray that
/// still holds 2.10:1 against `#f5f5f8` with a little room to spare, which is
/// what `gog_derived_ramps_keep_their_pale_end_on_the_page` checks.
pub(crate) const NEUTRAL: &str = "#a9a9a9";

/// Grayscale — the ramp a printed figure keeps.
///
/// Journals ask for figures that survive black-and-white reproduction;
/// `theme(background = "transparent")` already answered that for the furniture
/// and nothing answered it for the data. Derived rather than imported, so it
/// meets the bar [`RAMP_BLUE`] set: monotone in lightness, and the pale end
/// stays on the page.
///
/// There is deliberately **no categorical gray palette** beside it. Grays stop
/// being tellable apart after about four, and the grammar already has the right
/// answer for a print figure with categories in it — `pattern`, the texture
/// aesthetic, which separates them without spending color at all.
pub(crate) const RAMP_GRAY: &[&str] = &[NEUTRAL, "#6e6e6e", "#333333"];

// ---------------------------------------------------------------------------
// Diverging ramps
//
// The third kind, and the one the ramp vocabulary was missing. A sequential
// ramp means *more*; a diverging ramp means *away from a center, in two
// directions* — which is what a residual, a change, a correlation and an
// anomaly all are.
//
// **Where the center lands is `limits`'s job, not a parameter of its own.**
// ggplot2 spells it `scale_color_gradient2(midpoint = 0)`, which keeps the
// data's own ends and rescales each arm to reach them: on a column running
// -2..10 the negative arm then spends half the ramp on a sixth of the range,
// and equal color distance stops meaning equal data distance. gog already has
// the atom that says where a scale's middle is — a symmetric stated domain, so
// `color(change, limits = c(-10, 10))` puts zero on the middle stop and leaves
// the ramp linear across both arms. Same sentence, no new parameter, and the
// arithmetic is the reader's rather than hidden in the scale.
//
// Both are gog's own rather than ColorBrewer's, for the reason the first one
// records: the center has to stay visible.
// ---------------------------------------------------------------------------

/// Blue → neutral → red. Residuals, change, temperature anomaly.
///
/// **The center is a light gray, not white**, which is [`RAMP_BLUE`]'s ruling
/// one kind over. ColorBrewer's RdBu centers on `#f7f7f7` — the panel color, near
/// enough — and on a choropleth that reads as "no difference here". gog draws
/// *points*, where a mark at zero that cannot be seen is indistinguishable from a
/// row that is not there, so the center holds the same contrast against the panel
/// every gog-derived pale end does.
///
/// The low end is [`RAMP_BLUE`]'s own dark end, so the two read as one family,
/// and the arms are matched in lightness: an unbalanced diverging ramp makes one
/// sign look stronger than the other at equal magnitude, which is a claim about
/// the data that the data did not make. `diverging_ramps_are_balanced` holds
/// both properties.
pub(crate) const RAMP_BLUE_RED: &[&str] =
    &["#004383", "#5b8ec4", NEUTRAL, "#cd7268", "#8b1a1a"];

/// Brown → neutral → teal. The colorblind-safe diverging ramp.
///
/// Blue and red keep their distance under the common dichromacies, so
/// [`RAMP_BLUE_RED`] is not unsafe; brown and teal separate further still, and
/// this is the one to reach for when the *sign* of the value is the whole point
/// of the plot.
pub(crate) const RAMP_BROWN_TEAL: &[&str] =
    &["#6b3d10", "#b08050", NEUTRAL, "#4c968f", "#00524c"];

/// Every ramp, by the name a caller writes. A **table** rather than a `match`
/// so it can be walked as well as queried: `plot.rs` reads it against
/// `legality`'s three name lists in both directions, which is the only thing
/// stopping a name that is legal to write from resolving to the default's
/// colors in silence.
pub(crate) const RAMPS: &[(&str, &[&str])] = &[
    ("blue", RAMP_BLUE),
    ("viridis", RAMP_VIRIDIS),
    ("magma", RAMP_MAGMA),
    ("inferno", RAMP_INFERNO),
    ("plasma", RAMP_PLASMA),
    ("cividis", RAMP_CIVIDIS),
    ("gray", RAMP_GRAY),
    ("blue_red", RAMP_BLUE_RED),
    ("brown_teal", RAMP_BROWN_TEAL),
];

/// Every categorical palette, by name. See [`RAMPS`] for why it is a table.
pub(crate) const PALETTES: &[(&str, &[&str])] =
    &[("gog", PALETTE_GOG), ("okabe", PALETTE_OKABE), ("soft", PALETTE_SOFT)];

pub(crate) fn named_ramp(name: &str) -> Option<&'static [&'static str]> {
    RAMPS.iter().find(|(n, _)| *n == name).map(|(_, stops)| *stops)
}

pub(crate) fn named_palette(name: &str) -> Option<&'static [&'static str]> {
    PALETTES.iter().find(|(n, _)| *n == name).map(|(_, colors)| *colors)
}

/// A color as RGB, from either notation.
///
/// Ramp stops are *interpolated*, so they need numbers — which is why
/// `palette(c("white", "navy"))` has to resolve names and not only hex. Color
/// names work everywhere else a color is accepted; making ramp stops the one
/// exception would be exactly the kind of silent letter the grammar refuses.
pub(crate) fn parse_color(s: &str) -> Option<(f64, f64, f64)> {
    let t = s.trim();
    let v = if let Some(h) = t.strip_prefix('#') {
        let full = match h.len() {
            3 => h.chars().flat_map(|c| [c, c]).collect::<String>(),
            6 | 8 => h[..6].to_string(),
            _ => return None,
        };
        u32::from_str_radix(&full, 16).ok()?
    } else {
        css_rgb(&t.to_ascii_lowercase())?
    };
    Some((
        ((v >> 16) & 0xff) as f64,
        ((v >> 8) & 0xff) as f64,
        (v & 0xff) as f64,
    ))
}

/// Sample a ramp at `t` in `0..=1`.
///
/// Interpolates in sRGB between adjacent stops. That is not perceptually exact,
/// but the stops are close enough together that the error stays under a
/// just-noticeable difference — and picking the stops in HCL is what makes the
/// ramp uniform, not the interpolation between them.
pub(crate) fn ramp_at(stops: &[&str], t: f64) -> String {
    if stops.is_empty() {
        return PALETTE_GOG[0].to_string();
    }
    if stops.len() == 1 {
        return stops[0].to_string();
    }
    let t = t.clamp(0.0, 1.0);
    let span = (stops.len() - 1) as f64;
    let pos = t * span;
    let i = (pos.floor() as usize).min(stops.len() - 2);
    let f = pos - i as f64;
    let (Some(a), Some(b)) = (parse_color(stops[i]), parse_color(stops[i + 1])) else {
        return PALETTE_GOG[0].to_string();
    };
    let mix = |x: f64, y: f64| (x + (y - x) * f).round() as u8;
    format!("#{:02x}{:02x}{:02x}", mix(a.0, b.0), mix(a.1, b.1), mix(a.2, b.2))
}

/// Darken a color toward black — `0.0` leaves it alone, `1.0` is black.
///
/// **A solid needs its faces told apart.** A projected box drawn in one flat color
/// is a silhouette: the edges between its faces vanish, and a field of them reads as
/// a jumble rather than as columns standing on a floor. So the 3-D `bar` shades its
/// faces, and the shade is what says which way a face points.
///
/// Keyed to the **data axis** a face belongs to rather than to where the light would
/// fall, which is the choice worth recording: a lamp fixed in *screen* space would
/// re-shade every face as `space(turn = )` swung the scene, so the same bar would
/// change color when the reader turned it, and a hue that moves without the data
/// moving is the silent wrongness §12 forbids. Fixed to the axes, turning the cube
/// re-arranges the faces and leaves each one's color alone.
///
/// Lives here beside [`ramp_at`] and [`parse_color`] because this module owns the
/// codebase's color arithmetic; `color.rs` below it answers only *is this a color*.
pub(crate) fn shade(color: &str, amount: f64) -> String {
    let Some((r, g, b)) = parse_color(color) else { return color.to_string() };
    let k = 1.0 - amount.clamp(0.0, 1.0);
    let u = |v: f64| (v * k).round().clamp(0.0, 255.0) as u8;
    format!("#{:02x}{:02x}{:02x}", u(r), u(g), u(b))
}

/// The stops to use for a continuous color channel.
pub(crate) fn resolve_ramp(pal: &PaletteDef) -> Vec<String> {
    let stops: Vec<&str> = match pal {
        PaletteDef::Auto => RAMP_BLUE.to_vec(),
        PaletteDef::Named(n) => named_ramp(n).unwrap_or(RAMP_BLUE).to_vec(),
        // A caller-supplied ramp: the same vector that names one color per
        // category when the column is text becomes the stops when it is numeric.
        PaletteDef::Custom(c) => return c.iter().map(|s| esc(s)).collect(),
    };
    stops.into_iter().map(str::to_string).collect()
}

/// Generate an HSL color string for overflow beyond named palette sizes.
pub(crate) fn hsl_color(index: usize, total: usize) -> String {
    let hue = (index as f64 / total as f64) * 360.0;
    // Saturation 65%, lightness 45% — vivid but not garish
    hsl_to_hex(hue, 0.65, 0.45)
}

pub(crate) fn hsl_to_hex(h: f64, s: f64, l: f64) -> String {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = l - c / 2.0;
    let (r, g, b) = if h < 60.0       { (c, x, 0.0) }
                    else if h < 120.0  { (x, c, 0.0) }
                    else if h < 180.0  { (0.0, c, x) }
                    else if h < 240.0  { (0.0, x, c) }
                    else if h < 300.0  { (x, 0.0, c) }
                    else               { (c, 0.0, x) };
    let u = |v: f64| ((v + m) * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}", u(r), u(g), u(b))
}

/// Resolve a `PaletteDef` to a concrete list of hex strings.
pub(crate) fn resolve_palette(pal: &PaletteDef) -> Vec<String> {
    match pal {
        // Auto on a text column means the categorical default.
        PaletteDef::Auto => PALETTE_GOG.iter().map(|s| s.to_string()).collect(),
        // "gog" resolves through the table; anything else reaching here was
        // already refused by `check_palette`, so the fallback only matters under
        // `GOG_STRICT=0` and the default is the safe thing to draw.
        PaletteDef::Named(name) => named_palette(name)
            .unwrap_or(PALETTE_GOG)
            .iter()
            .map(|s| s.to_string())
            .collect(),
        // Custom colors are user-supplied and land in `fill=`/`stroke=`
        // attributes, so escape them once here rather than at every use site.
        PaletteDef::Custom(colors) => colors.iter().map(|c| esc(c)).collect(),
    }
}

/// Build a label → hex color map from all color channels in the spec.
/// Colors are assigned in first-appearance order across all effective DataFrames.
/// If categories exceed the palette size, additional colors are generated via
/// the HSL wheel — so we never repeat a color.
///
/// A count mismatch on a custom palette is reported through `remarks`, never
/// `eprintln!`: the browser engine has no stderr, so a warning printed there
/// reaches the CLI's users and nobody else.
pub(crate) fn build_color_map(
    spec: &PlotSpec,
    eff: &[DataFrame],
    remarks: &mut Vec<Diagnostic>,
) -> HashMap<String, String> {
    let base = resolve_palette(&spec.palette);
    let is_custom = matches!(&spec.palette, PaletteDef::Custom(_));

    // Categories in display order — a declared level order if the column has
    // one, otherwise first appearance. The axis and the legend ask the same
    // function, so a color cannot be assigned in one order and decoded in
    // another.
    let mut order: Vec<String> = Vec::new();
    for (layer, df) in spec.layers.iter().zip(eff.iter()) {
        if let Some(cd) = layer.encodings.get(&Channel::Color) {
            for v in crate::data::categories_across(&[df], &cd.field) {
                if !order.contains(&v) {
                    order.push(v);
                }
            }
        }
    }

    let n_categories = order.len();
    let n_colors     = base.len();

    // Warn on mismatch only for custom palettes — named palettes are designed
    // to handle any count via overflow, so no warning needed there.
    if is_custom {
        if n_categories > n_colors {
            remarks.push(Diagnostic {
                kind: DiagnosticKind::Assumption,
                message: format!(
                    "gog: custom palette has {n_colors} color{} but color() has {n_categories} unique \
                     value{} — {} additional color{} generated automatically via HSL wheel. \
                     Provide {n_categories} colors to palette() to suppress this warning.",
                    if n_colors == 1 { "" } else { "s" },
                    if n_categories == 1 { "" } else { "s" },
                    n_categories - n_colors,
                    if n_categories - n_colors == 1 { " was" } else { "s were" },
                ),
            });
        } else if n_colors > n_categories && n_categories > 0 {
            remarks.push(Diagnostic {
                kind: DiagnosticKind::Assumption,
                message: format!(
                    "gog: custom palette has {n_colors} colors but color() only has {n_categories} \
                     unique value{} — {} color{} unused.",
                    if n_categories == 1 { "" } else { "s" },
                    n_colors - n_categories,
                    if n_colors - n_categories == 1 { " is" } else { "s are" },
                ),
            });
        }
    }

    let total = n_categories.max(n_colors);
    order.into_iter().enumerate().map(|(i, label)| {
        let color = if i < n_colors {
            base[i].clone()
        } else {
            hsl_color(i, total)
        };
        (label, color)
    }).collect()
}


// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_palette_colors_are_escaped() {
        // Colors land in `fill="..."`; a bare quote would break out of the attribute.
        let pal = PaletteDef::Custom(vec![r#"#fff" onload="x"#.to_string()]);
        let resolved = resolve_palette(&pal);
        assert!(!resolved[0].contains('"'), "quote survived into an attribute value");
    }

    #[test]
    fn a_ramp_interpolates_names_as_well_as_hex() {
        // `palette(c("white", "navy"))` is the natural thing to write, and it
        // silently fell back to the default until stops learned to resolve CSS
        // names — hex-only would have made ramp stops the one place a color
        // name does not work.
        let stops = ["white", "navy"];
        assert_eq!(ramp_at(&stops, 0.0), "#ffffff");
        assert_eq!(ramp_at(&stops, 1.0), "#000080");
        let mid = ramp_at(&stops, 0.5);
        assert_ne!(mid, "#ffffff");
        assert_ne!(mid, "#000080");
        assert_ne!(mid, PALETTE_GOG[0], "a fallback here means the stops did not parse");

        // Hex forms still work, long and short.
        assert_eq!(ramp_at(&["#fff", "#000"], 0.0), "#ffffff");
        assert_eq!(ramp_at(&["#ffffff", "#000000"], 1.0), "#000000");
    }

    /// Relative luminance, WCAG's definition — what "light" and "dark" actually
    /// mean when a ramp claims to be ordered. The gamma step matters: a naive
    /// weighted sum of the raw bytes calls `#0000ff` lighter than it looks and
    /// would let a genuinely disordered ramp pass.
    fn lum(hex: &str) -> f64 {
        let (r, g, b) = parse_color(hex).unwrap_or_else(|| panic!("unparseable stop {hex}"));
        let ch = |v: f64| {
            let c = v / 255.0;
            if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
        };
        0.2126 * ch(r) + 0.7152 * ch(g) + 0.0722 * ch(b)
    }

    #[test]
    fn the_default_ramp_runs_light_to_dark() {
        // Lightness ordering is what makes a sequential scale readable; if the
        // ends were ever swapped the plot would still render and quietly mean
        // the opposite.
        let lo = lum(&ramp_at(RAMP_BLUE, 0.0));
        let mid = lum(&ramp_at(RAMP_BLUE, 0.5));
        let hi = lum(&ramp_at(RAMP_BLUE, 1.0));
        assert!(lo > mid && mid > hi, "expected light→dark, got {lo} {mid} {hi}");
    }

    #[test]
    fn every_sequential_ramp_is_ordered() {
        // The property that makes a sequential scale mean "more" is that
        // lightness moves one way and never turns around. A mistyped hex
        // anywhere in a six-stop table still renders, and still looks like a
        // color ramp — this is what notices.
        for name in ["blue", "viridis", "magma", "inferno", "plasma", "cividis", "gray"] {
            let stops = named_ramp(name).unwrap();
            let l: Vec<f64> = stops.iter().map(|s| lum(s)).collect();
            let down = l.windows(2).all(|w| w[0] > w[1]);
            let up = l.windows(2).all(|w| w[0] < w[1]);
            assert!(down || up, "{name} turns around in lightness: {l:?}");
        }
    }

    #[test]
    fn categorical_palettes_hand_out_distinct_colors() {
        // The one property a categorical palette must have. Two categories
        // sharing a color is not a cosmetic slip: the legend still lists both,
        // so the plot claims to distinguish them and does not, and no amount of
        // reading it carefully recovers which is which. The overflow wheel
        // guarantees this past the end of a palette, and nothing guaranteed it
        // *inside* one until here.
        for (name, colors) in PALETTES {
            for (i, c) in colors.iter().enumerate() {
                assert!(parse_color(c).is_some(), "{name}[{i}] is not a color: {c}");
                assert!(
                    !colors[..i].contains(c),
                    "{name} hands out {c} twice, so two categories get one color"
                );
            }
        }
    }

    #[test]
    fn diverging_ramps_are_balanced() {
        // Three claims, and each one is a way the ramp could lie about the data.
        for name in ["blue_red", "brown_teal"] {
            let stops = named_ramp(name).unwrap();
            assert_eq!(stops.len(), 5, "{name}: two arms and a center");
            let l: Vec<f64> = stops.iter().map(|s| lum(s)).collect();

            // 1. The center is the lightest point, and lightness falls away from
            //    it in both directions — that is what makes the middle read as
            //    the middle without consulting the legend.
            assert!(l[0] < l[1] && l[1] < l[2], "{name}: low arm does not rise: {l:?}");
            assert!(l[2] > l[3] && l[3] > l[4], "{name}: high arm does not fall: {l:?}");

            // 2. The arms match. An unbalanced ramp makes one sign look stronger
            //    than the other at equal magnitude, which is a claim the data
            //    never made. 0.02 is tight — the ramps ship at under 0.007.
            assert!((l[0] - l[4]).abs() < 0.02, "{name}: ends differ by {:.4}", (l[0] - l[4]).abs());
            assert!((l[1] - l[3]).abs() < 0.02, "{name}: mids differ by {:.4}", (l[1] - l[3]).abs());

            // 3. Both turn through the shared neutral — structurally, since the
            //    arrays name the constant — and that neutral is *actually*
            //    neutral. A center with a hue in it would tip every value near
            //    zero toward one of the two signs.
            assert_eq!(stops[2], NEUTRAL, "{name}: center is not the shared neutral");
        }
        let (r, g, b) = parse_color(NEUTRAL).unwrap();
        assert!(r == g && g == b, "the neutral has a hue in it: {NEUTRAL}");
        assert_eq!(RAMP_GRAY[0], NEUTRAL, "the gray ramp starts somewhere else");
    }

}
