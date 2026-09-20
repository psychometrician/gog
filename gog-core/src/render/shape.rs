//! The seven glyphs the `shape` channel draws.
//!
//! Its own module because both the marks and the legend draw them. A helper
//! shared by two callers but reachable from only one of them is how duplication
//! starts — see the `png.rs` lesson under rule 4 in CONTRIBUTING.md.

use std::fmt::Write;

// ---------------------------------------------------------------------------
// Shapes — seven distinct mark glyphs for the `shape` channel
// ---------------------------------------------------------------------------
//
// Seven is the ceiling a *which one?* vocabulary is allowed (§10): a reader
// holds about that many kinds in mind at once, and past it a legend stops being
// a key and becomes a lookup table. `shape` is the one channel that takes the
// ceiling exactly, because a glyph shows its whole silhouette at ten pixels and
// `point` is the most-drawn mark in the book. The set is d3's symbol family
// completed — circle, cross, diamond, square, star, triangle, wye — rather than
// two outlines invented here, and rather than open/filled variants, which would
// double the vocabulary for one cue.

#[derive(Clone, Copy, Debug)]
pub(crate) enum ShapeKind { Circle, Square, Triangle, Diamond, Cross, Star, Wye }

/// The cycling rule. The modulus **is** the vocabulary size, and the Assumption
/// that warns on a column with more categories than this reads the same number
/// out of `SHAPE_NAMES` — written once so the two cannot drift apart.
pub(crate) fn shape_at_index(i: usize) -> ShapeKind {
    match i % 7 {
        0 => ShapeKind::Circle,
        1 => ShapeKind::Square,
        2 => ShapeKind::Triangle,
        3 => ShapeKind::Diamond,
        4 => ShapeKind::Cross,
        5 => ShapeKind::Star,
        _ => ShapeKind::Wye,
    }
}

/// Look up a glyph by the name `style(shape = ...)` uses. Unknown names are
/// refused by `legality::check_style` before rendering, so the fallback here is
/// unreachable in practice — it exists so `GOG_STRICT=0` still draws something.
pub(crate) fn shape_by_name(name: &str) -> ShapeKind {
    match name {
        "square"   => ShapeKind::Square,
        "triangle" => ShapeKind::Triangle,
        "diamond"  => ShapeKind::Diamond,
        "cross"    => ShapeKind::Cross,
        "star"     => ShapeKind::Star,
        "wye"      => ShapeKind::Wye,
        _          => ShapeKind::Circle,
    }
}

/// A polygon's points, from angles in **screen** coordinates (y down). The
/// algebra is the ordinary rotation; only the reading of the result flips.
fn points(verts: &[(f64, f64)]) -> String {
    verts.iter()
        .map(|(x, y)| format!("{x:.2},{y:.2}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Write one SVG shape element centered at (cx, cy) with half-size `s`.
///
/// `border` is the optional rim — `(color, width)` from `style(border_color =,
/// border_size =)` (spec §4, the settable rule). It strokes the perimeter of a
/// **filled** glyph; a `cross` has no fill, so it takes no rim (its own `color`
/// stroke is all there is), and `None` leaves every element byte-for-byte as it was.
pub(crate) fn write_shape(out: &mut String, kind: ShapeKind, cx: f64, cy: f64, s: f64, color: &str, o: f64, border: Option<(&str, f64)>) {
    // A rim only on the fillable glyphs — a `cross` is stroke-only, nothing to rim.
    let rim = match (border, kind) {
        (Some((bc, bw)), k) if !matches!(k, ShapeKind::Cross) =>
            format!(r#" stroke="{bc}" stroke-width="{bw:.2}""#),
        _ => String::new(),
    };
    match kind {
        ShapeKind::Circle => writeln!(out,
            r#"    <circle cx="{cx:.2}" cy="{cy:.2}" r="{s:.2}" fill="{color}" fill-opacity="{o:.3}"{rim}/>"#
        ).unwrap(),
        ShapeKind::Square => writeln!(out,
            r#"    <rect x="{x:.2}" y="{y:.2}" width="{w:.2}" height="{w:.2}" fill="{color}" fill-opacity="{o:.3}"{rim}/>"#,
            x = cx - s, y = cy - s, w = s * 2.0
        ).unwrap(),
        ShapeKind::Triangle => {
            let p1 = format!("{:.2},{:.2}", cx, cy - s);
            let p2 = format!("{:.2},{:.2}", cx + s * 0.866, cy + s * 0.5);
            let p3 = format!("{:.2},{:.2}", cx - s * 0.866, cy + s * 0.5);
            writeln!(out,
                r#"    <polygon points="{p1} {p2} {p3}" fill="{color}" fill-opacity="{o:.3}"{rim}/>"#
            ).unwrap();
        }
        ShapeKind::Diamond => {
            let p1 = format!("{:.2},{:.2}", cx, cy - s);
            let p2 = format!("{:.2},{:.2}", cx + s, cy);
            let p3 = format!("{:.2},{:.2}", cx, cy + s);
            let p4 = format!("{:.2},{:.2}", cx - s, cy);
            writeln!(out,
                r#"    <polygon points="{p1} {p2} {p3} {p4}" fill="{color}" fill-opacity="{o:.3}"{rim}/>"#
            ).unwrap();
        }
        ShapeKind::Cross => writeln!(out,
            r#"    <path d="M {x1:.2},{cy:.2} H {x2:.2} M {cx:.2},{y1:.2} V {y2:.2}" stroke="{color}" stroke-opacity="{o:.3}" stroke-width="{sw:.2}" stroke-linecap="round" fill="none"/>"#,
            x1 = cx - s, x2 = cx + s, y1 = cy - s, y2 = cy + s, sw = s * 0.55
        ).unwrap(),
        // Ten vertices, outer and inner alternating every 36°, starting at the
        // top. The inner radius is sin(18°)/sin(54°) — the ratio that makes the
        // five points' edges collinear, which is what reads as a star rather
        // than as a spiky blob.
        ShapeKind::Star => {
            let inner = s * 0.381_966;
            let verts: Vec<(f64, f64)> = (0..10).map(|k| {
                let r = if k % 2 == 0 { s } else { inner };
                let a = -std::f64::consts::FRAC_PI_2 + (k as f64) * std::f64::consts::PI / 5.0;
                (cx + r * a.cos(), cy + r * a.sin())
            }).collect();
            writeln!(out,
                r#"    <polygon points="{p}" fill="{color}" fill-opacity="{o:.3}"{rim}/>"#,
                p = points(&verts)
            ).unwrap();
        }
        // Three arms 120° apart, two up and one down. Each arm is a strip of
        // half-width `a`; where two arms meet, their edges cross on the bisector
        // at a / sin(60°) from the center, which is the only number here that is
        // not a choice.
        ShapeKind::Wye => {
            let a = s * 0.38;
            let inner = a / (std::f64::consts::PI / 3.0).sin();
            let mut verts: Vec<(f64, f64)> = Vec::with_capacity(9);
            for k in 0..3 {
                let th = -5.0 * std::f64::consts::FRAC_PI_6 + (k as f64) * 2.0 * std::f64::consts::PI / 3.0;
                let (c, si) = (th.cos(), th.sin());
                let (nx, ny) = (-si, c);
                verts.push((cx + s * c - a * nx, cy + s * si - a * ny));
                verts.push((cx + s * c + a * nx, cy + s * si + a * ny));
                let b = th + std::f64::consts::FRAC_PI_3;
                verts.push((cx + inner * b.cos(), cy + inner * b.sin()));
            }
            writeln!(out,
                r#"    <polygon points="{p}" fill="{color}" fill-opacity="{o:.3}"{rim}/>"#,
                p = points(&verts)
            ).unwrap();
        }
    }
}
