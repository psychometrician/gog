//! The `rule` mark — one position from the data, the other extent from the panel.
//!
//! Every other mark in this directory reads two positions and draws between
//! them. This one reads *one* and asks the panel for the rest, which is what
//! makes it both the reference line and the rug: those differ only in how far it
//! reaches (`style(reach = )`), not in what it is.
//!
//! The whole file turns on one sentence — **a rule spans the axis it does not
//! name, whole** — and the polar readings are that sentence unchanged rather
//! than cases added to it. Spanning the radial axis whole is a spoke from the
//! center to the rim; spanning the angular axis whole is a ring all the way
//! round. Neither needed a decision here, which is the test spec §4 sets for a
//! Law 7 relaxation.
use std::collections::HashMap;
use std::fmt::Write;
use crate::data::DataFrame;
use crate::ir::{Channel, Layer};
use crate::legality::rule_axis;
use crate::render::palette::PALETTE_GOG;
use crate::render::pattern::{pattern_dasharray, PatternMap};
use crate::render::polar::Polar;
use crate::render::svg::{unit_norm, SvgRenderer};
use crate::render::text::esc;
use crate::render::Layout;

/// How far an `"edge"` rule reaches across the axis it does not name, as a
/// fraction of that axis's own length, and the pixel bounds it is held between.
///
/// Derived rather than a parameter, on `nudge`'s precedent (spec §7): a rug tick
/// is "short compared with the panel", and the panel is the only thing that can
/// say how long that is. The clamp keeps a tick visible in a small facet panel
/// and stops it becoming a second plot in a large one.
const EDGE_FRAC: f64 = 0.035;
const EDGE_MIN: f64 = 4.0;
const EDGE_MAX: f64 = 14.0;

/// How many segments approximate a partial ring. Only the `"edge"` arc needs
/// sampling — a full ring is an SVG `<circle>`, which is exact.
const ARC_STEPS: usize = 24;

fn edge_len(axis_px: f64) -> f64 {
    (axis_px.abs() * EDGE_FRAC).clamp(EDGE_MIN, EDGE_MAX)
}

impl SvgRenderer {
    // -----------------------------------------------------------------------
    // Mark: rule
    // -----------------------------------------------------------------------

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn write_rule(
        &self, svg: &mut String, layer: &Layer, df: &DataFrame,
        l: &Layout, xs: (f64, f64), ys: (f64, f64),
        // The whole plot spec's positions: which of the two this layer's table
        // answers is what places the rule, so the axis cannot be decided here.
        spec: &crate::ir::PlotSpec,
        cat_x: Option<&[String]>,
        cat_y: Option<&[String]>,
        color_map: &HashMap<String, String>,
        clip: &str,
        polar: Option<&Polar>,
    ) {
        // `rule_axis` is the single statement of Law 7's second relaxation, and
        // the renderer asks it rather than re-deriving it — the same discipline
        // that keeps `slot_orient` from being re-read per mark. `None` means the
        // layer was refused by `check_rule`, so there is nothing to draw.
        let Some(axis) = rule_axis(spec, df, layer) else { return };
        let on_x = axis == Channel::X;

        // The *axis* name, not the layer's own column name: by the time a frame
        // reaches a mark it has been resolved onto the shared axis, so a layer
        // that named its own column is already reading under this one (spec §8,
        // and `resolve_positions` in `svg.rs`).
        let Some(field) = spec.axis_def(&axis).map(|c| c.field.as_str()) else { return };
        let cats = if on_x { cat_x } else { cat_y };
        let Some(vals) = super::positions(df, field, cats) else { return };

        let scale = if on_x { xs } else { ys };

        let st = &layer.style;
        let stroke_w = st.size.unwrap_or(1.5);
        let stroke_o = st.opacity.unwrap_or(0.9);
        let set_color = st.color.as_deref().map(esc);
        let dash_attr = pattern_dasharray(st.pattern.as_deref());
        let to_edge = st.reach.as_deref() == Some("edge");

        // Color and dash are per *row* here, not per series: a rule has nothing
        // to connect, so each row is already its own segment and needs no
        // grouping pass at all. That absence is the mark's shape showing through
        // — `write_line` and `write_path` both open with one.
        let pattern_map = PatternMap::resolve(layer, df);
        let color_field = layer.encodings.get(&Channel::Color).map(|c| c.field.as_str());
        let color_vals = color_field.and_then(|f| df.str_col(f));
        let cat_order = color_field.map(|f| crate::data::categories_across(&[df], f));

        writeln!(svg, r##"  <g clip-path="url(#{clip})">"##).unwrap();

        for (row, &v) in vals.iter().enumerate() {
            if !v.is_finite() {
                continue;
            }
            let stroke = match (&set_color, &color_vals, &cat_order) {
                (Some(c), _, _) => c.clone(),
                (None, Some(cv), Some(order)) => {
                    let key = cv.get(row).map(String::as_str).unwrap_or("");
                    color_map.get(key).cloned().unwrap_or_else(|| {
                        let i = order.iter().position(|c| c == key).unwrap_or(0);
                        PALETTE_GOG[i % PALETTE_GOG.len()].to_string()
                    })
                }
                _ => PALETTE_GOG[0].to_string(),
            };
            let dash = pattern_map
                .as_ref()
                .map(|pm| pattern_dasharray(Some(pm.dash(pm.cat_at(row)))))
                .unwrap_or(dash_attr);
            let stroke_attrs = format!(
                r##"fill="none" stroke="{stroke}" stroke-width="{stroke_w}" stroke-opacity="{stroke_o:.3}"{dash} stroke-linecap="round""##
            );

            let u = unit_norm(v, scale);
            match polar {
                // Bent, the sentence reads as the two shapes a circle has. On the
                // angular axis the rule spans the radius whole — a spoke; on the
                // radial axis it spans the turn whole — a ring.
                Some(p) => {
                    if on_x {
                        let theta = p.angle(u);
                        // "The start of the other axis" is the center, which is
                        // where a flat x-rule's ticks sit on the bottom edge. The
                        // same rule, bent.
                        let outer = if to_edge { edge_len(p.r_max) } else { p.r_max };
                        let (x0, y0) = p.polar_px(theta, 0.0);
                        let (x1, y1) = p.polar_px(theta, outer);
                        writeln!(svg,
                            r##"    <line x1="{x0:.2}" y1="{y0:.2}" x2="{x1:.2}" y2="{y1:.2}" {stroke_attrs}/>"##
                        ).unwrap();
                    } else {
                        let r = p.radius(u);
                        if to_edge {
                            // A stub of the turn, sampled: the arc is the one
                            // shape here that is not a straight line or a whole
                            // circle, and a few segments draw it exactly enough.
                            let sweep = edge_len(p.r_max) / p.r_max.max(1e-9);
                            let pts: Vec<String> = (0..=ARC_STEPS)
                                .map(|i| {
                                    let t = i as f64 / ARC_STEPS as f64;
                                    let (px, py) = p.polar_px(p.angle(t * sweep), r);
                                    format!("{px:.2},{py:.2}")
                                })
                                .collect();
                            writeln!(svg,
                                r##"    <polyline points="{}" {stroke_attrs}/>"##,
                                pts.join(" ")
                            ).unwrap();
                        } else {
                            writeln!(svg,
                                r##"    <circle cx="{:.2}" cy="{:.2}" r="{r:.2}" {stroke_attrs}/>"##,
                                p.cx, p.cy
                            ).unwrap();
                        }
                    }
                }
                // Flat: a straight line across the panel, or a tick standing on
                // the start of the axis it crosses — the bottom for a rule on x,
                // the left for one on y.
                None => {
                    if on_x {
                        let px = l.map_x(v, xs.0, xs.1);
                        let y1 = if to_edge { l.y1 - edge_len(l.h()) } else { l.y0 };
                        writeln!(svg,
                            r##"    <line x1="{px:.2}" y1="{:.2}" x2="{px:.2}" y2="{y1:.2}" {stroke_attrs}/>"##,
                            l.y1
                        ).unwrap();
                    } else {
                        let py = l.map_y(v, ys.0, ys.1);
                        let x1 = if to_edge { l.x0 + edge_len(l.w()) } else { l.x1 };
                        writeln!(svg,
                            r##"    <line x1="{:.2}" y1="{py:.2}" x2="{x1:.2}" y2="{py:.2}" {stroke_attrs}/>"##,
                            l.x0
                        ).unwrap();
                    }
                }
            }
        }

        writeln!(svg, "  </g>").unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_edge_reach_is_a_short_fraction_of_the_axis_held_between_two_bounds() {
        // The derived length tracks the panel between the clamps, and stops
        // tracking it outside them — a tick stays visible in a small facet and
        // never becomes a second plot in a large one.
        assert!(edge_len(200.0) > edge_len(150.0), "it tracks the panel between the bounds");
        assert_eq!(edge_len(10.0), EDGE_MIN, "a tiny panel still shows a tick");
        assert_eq!(edge_len(10_000.0), EDGE_MAX, "a huge panel does not grow a second plot");
        assert!(edge_len(300.0) < 300.0 * 0.1, "a rug is short compared with the panel");
    }
}
