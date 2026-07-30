//! The five glyphs the `shape` channel draws.
//!
//! Its own module because both the marks and the legend draw them. A helper
//! shared by two callers but reachable from only one of them is how duplication
//! starts — see the `png.rs` lesson under rule 4 in CONTRIBUTING.md.

use std::fmt::Write;

// ---------------------------------------------------------------------------
// Shapes — five distinct mark glyphs for the `shape` channel
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
pub(crate) enum ShapeKind { Circle, Square, Triangle, Diamond, Cross }

pub(crate) fn shape_at_index(i: usize) -> ShapeKind {
    match i % 5 {
        0 => ShapeKind::Circle,
        1 => ShapeKind::Square,
        2 => ShapeKind::Triangle,
        3 => ShapeKind::Diamond,
        _ => ShapeKind::Cross,
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
        _          => ShapeKind::Circle,
    }
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
    }
}

