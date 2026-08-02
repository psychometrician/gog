//! The `path` mark — one stroke per series, through the rows **in data order**.
//!
//! Deliberately built as `write_line`'s twin, and the diff is the point: the
//! line's `by_x` closure sorts each series' indices before stroking them, and
//! this file has no such closure. Everything else — the grouping precedence, the
//! colors, the dash, the polar placement — is shared code called the same way,
//! because a path and a line differ in *vertex order* and in nothing else.
//!
//! What that one difference buys is a **direction**, which is what an arrowhead
//! needs and what a line cannot supply (its last vertex is wherever the domain
//! ends). So `style(arrow = )` lives here, and the head is drawn as an explicit
//! polygon rather than an SVG `<marker>`: a marker is one backend's mechanism
//! (Law 9), and a def-per-color is machinery this does not need.
//!
//! It also buys the **third dimension**, and for the same reason: an order is not
//! a property of any axis, so it means what it meant when the plane becomes a
//! cube, while a line's sort-by-`x` does not (`legality::rule_for` states it).
//! `path` is the only stroke that draws in `space`.
use std::collections::HashMap;
use std::fmt::Write;
use crate::data::DataFrame;
use crate::ir::{Channel, Layer};
use crate::render::palette::PALETTE_GOG;
use crate::render::pattern::{pattern_dasharray, PatternMap};
use crate::render::polar::Polar;
use crate::render::project;
use crate::render::svg::{unit_norm, SvgRenderer};
use crate::render::text::esc;
use crate::render::Layout;

/// Half the angle at the arrowhead's tip. 22.5° gives a head that reads as an
/// arrow at a glance without the needle look a narrower one has.
const HEAD_HALF_ANGLE: f64 = 22.5_f64 * std::f64::consts::PI / 180.0;

/// How long a head is, in multiples of the stroke width, and the floor it will
/// not go below. Tied to the stroke so a thick path gets a head in proportion,
/// clamped so a hairline path still shows one.
const HEAD_LEN_PER_WIDTH: f64 = 4.0;
const HEAD_LEN_MIN: f64 = 7.0;

/// The arrowhead at `tip`, aimed along the direction from `from` to `tip`.
///
/// Returns the three points of a filled triangle. Working in *page* coordinates
/// rather than data ones is deliberate and is not the page-space drawing §18
/// refuses: the head's two data-space facts (where it sits, which way it points)
/// both come from the path's own vertices, and only its *size* is in pixels —
/// the same footing as a stroke width or a point radius.
fn head_points(from: (f64, f64), tip: (f64, f64), stroke_w: f64) -> Option<String> {
    let (dx, dy) = (tip.0 - from.0, tip.1 - from.1);
    let len = (dx * dx + dy * dy).sqrt();
    // A zero-length last segment has no direction to point along. Walking further
    // back down the path to find one would be guessing at intent; drawing nothing
    // is the honest answer, and the stroke itself is unaffected.
    if !len.is_finite() || len < 1e-9 {
        return None;
    }
    let head = (stroke_w * HEAD_LEN_PER_WIDTH).max(HEAD_LEN_MIN);
    let (ux, uy) = (dx / len, dy / len);
    let (sin, cos) = (HEAD_HALF_ANGLE.sin(), HEAD_HALF_ANGLE.cos());
    // The two barbs are the reversed unit vector rotated by ±the half-angle,
    // scaled to the head length and hung off the tip.
    let barb = |s: f64| {
        (
            tip.0 - head * (ux * cos - s * uy * sin),
            tip.1 - head * (uy * cos + s * ux * sin),
        )
    };
    let (ax, ay) = barb(1.0);
    let (bx, by) = barb(-1.0);
    Some(format!("{:.2},{:.2} {ax:.2},{ay:.2} {bx:.2},{by:.2}", tip.0, tip.1))
}

impl SvgRenderer {
    // -----------------------------------------------------------------------
    // Mark: path
    // -----------------------------------------------------------------------

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn write_path(
        &self, svg: &mut String, layer: &Layer, df: &DataFrame,
        l: &Layout, xs: (f64, f64), ys: (f64, f64),
        x_field: &str, y_field: &str,
        // Either axis may carry categories — a path's two positions are the same
        // kind of thing (`rule_for`), so unlike `line` it reads both.
        cat_x: Option<&[String]>,
        cat_y: Option<&[String]>,
        color_map: &HashMap<String, String>,
        // The sequential ramp, for the other reading of `color`: a measure along
        // the route rather than a category naming the series (`StrokeRamp`).
        ramp: &[String],
        clip: &str,
        // 3-D: when `scene` is `Some`, the vertices are normalized into the unit
        // cube, projected, and the *segments* depth-sorted. When `None` this is
        // the ordinary flat route and `zs`/`z_field` go unread.
        zs: (f64, f64), z_field: &str,
        scene: Option<&project::Scene>,
        // Polar: the vertices ride round the circle and the route becomes a
        // spiral. No closing segment — see `mark_draws_in_space`. The two spaces
        // are exclusive (`check_polar` refuses `polar()` with a `z`), so at most
        // one of `scene`/`polar` is ever `Some`.
        polar: Option<&Polar>,
    ) {
        let Some(x_vals) = super::positions(df, x_field, cat_x) else { return };
        let Some(y_vals) = super::positions(df, y_field, cat_y) else { return };
        // `z` is continuous-only on a path (`rule_for`), so unlike x and y it has
        // no category branch to resolve.
        let z_vals: &[f64] = match scene {
            None => &[],
            Some(_) => df.float_col(z_field).map(Vec::as_slice).unwrap_or(&[]),
        };
        let n = if scene.is_some() {
            x_vals.len().min(y_vals.len()).min(z_vals.len())
        } else {
            x_vals.len().min(y_vals.len())
        };
        if n < 2 { return; }

        // Grouping: color first (it also colors the strokes), then `group`, then
        // a mapped `pattern` — `write_line`'s precedence, unchanged, so a split
        // path and a split line separate on the same rule.
        let pattern_map = PatternMap::resolve(layer, df);
        // A *measured* color varies along the route and so does not split it
        // into series; only a categorical one does. `group` still splits either
        // way, so one ramped route per glider is `color(altitude) + group(glider)`.
        // A contour measures itself by the level it was cut at, and nothing binds it
        // — the same courtesy `zone * bin` does for `color` and `bar * bin` for `y`.
        // So an unbound `path * density` reads its own synthesized column off the
        // ramp, which is what makes the color bar beside it true.
        let ramp_color = super::StrokeRamp::resolve(layer, df, ramp).or_else(|| {
            layer.encodings.get(&Channel::Color).is_none().then(|| {
                super::StrokeRamp::of(df, crate::transform::FIELD_LEVEL, ramp, None)
            }).flatten()
        });
        let color_field = layer.encodings.get(&Channel::Color)
            .map(|c| c.field.as_str())
            .filter(|_| ramp_color.is_none());
        let group_field = color_field
            .or_else(|| layer.encodings.get(&Channel::Group).map(|c| c.field.as_str()))
            .or_else(|| pattern_map.as_ref().map(|pm| pm.field()));

        // No zigzag warning here, and that absence is the mark's whole point. On a
        // `line`, many ungrouped rows connected in x order are usually an accident
        // worth reporting; on a `path`, connecting the rows in the order given *is*
        // what was asked for, so the same shape is the intent rather than the bug.
        let st = &layer.style;
        let stroke_w = st.size.unwrap_or(2.0);
        let stroke_o = st.opacity.unwrap_or(1.0);
        let set_color = st.color.as_deref().map(esc);
        let dash_attr = pattern_dasharray(st.pattern.as_deref());
        let arrow = st.arrow.as_deref();

        writeln!(svg, r##"  <g clip-path="url(#{clip})">"##).unwrap();

        // One series per group, each in the table's own row order. The only
        // filtering is of points no scale can place; nothing is sorted.
        let mut series: Vec<(Vec<usize>, String, &'static str)> = Vec::new();
        let keep = |b: &mut Vec<usize>| b.retain(|&i| {
            x_vals[i].is_finite() && y_vals[i].is_finite()
                && (scene.is_none() || z_vals[i].is_finite())
        });

        if group_field.is_some() {
            let Some(parts) = super::split_series(
                df, n, color_field,
                layer.encodings.get(&Channel::Group).map(|c| c.field.as_str()),
                pattern_map.as_ref().map(|pm| pm.field()),
            ) else {
                writeln!(svg, "  </g>").unwrap();
                return;
            };
            for (gi, part) in parts.iter().enumerate() {
                let mut idxs = part.rows.clone();
                keep(&mut idxs);
                let stroke = if let Some(c) = &set_color {
                    c.clone()
                } else if color_field.is_some() {
                    color_map.get(part.color_key.as_str()).cloned()
                        .unwrap_or_else(|| PALETTE_GOG[gi % PALETTE_GOG.len()].to_string())
                } else {
                    PALETTE_GOG[0].to_string()
                };
                let dash = pattern_map.as_ref()
                    .and_then(|pm| idxs.first().map(|&r| pattern_dasharray(Some(pm.dash(pm.cat_at(r))))))
                    .unwrap_or(dash_attr);
                series.push((idxs, stroke, dash));
            }
        } else {
            let mut idxs: Vec<usize> = (0..n).collect();
            keep(&mut idxs);
            series.push((
                idxs,
                set_color.clone().unwrap_or_else(|| PALETTE_GOG[0].to_string()),
                dash_attr,
            ));
        }

        // A contour's **rings** break the stroke, and neither `color` nor `group`
        // can do it: one level often encloses two separate modes, so two rings share
        // a level, and joining them into one polyline would draw a segment straight
        // across the valley between them. So the ring is what ends a stroke and the
        // level is what colors it — two questions, two columns (spec §5).
        //
        // Read as *runs* rather than as a grouping, because the transform emits each
        // ring's vertices consecutively and in traversal order. That keeps it right
        // underneath a `color`/`group` split too: each group's rows are already
        // contiguous, so splitting runs inside a series never reaches across one.
        let series = match df.float_col(crate::transform::FIELD_RING) {
            None => series,
            Some(ring) => series.into_iter().flat_map(|(idxs, stroke, dash)| {
                let mut out: Vec<(Vec<usize>, String, &'static str)> = Vec::new();
                for i in idxs {
                    let continues = out.last()
                        .and_then(|(prev, ..)| prev.last().copied())
                        .is_some_and(|p| ring.get(p) == ring.get(i));
                    match out.last_mut() {
                        Some(run) if continues => run.0.push(i),
                        _ => out.push((vec![i], stroke.clone(), dash)),
                    }
                }
                out
            }).collect(),
        };

        // The 3-D branch. A stroke that runs through the cube has no single depth
        // to sort by — its far end and its near end are different distances from
        // the camera — so the unit that can be ordered is the **segment**, and the
        // segments of every series are ordered *together*. Sorting per series
        // instead would put one whole route in front of another, which is exactly
        // wrong for two coils that thread through each other: the correct picture
        // has them interleaving, each hiding the other where it passes in front.
        if let Some(sc) = scene {
            let mut pieces: Vec<(f64, String)> = Vec::new();
            for (idxs, stroke, dash) in &series {
                if idxs.len() < 2 { continue; }
                let pts: Vec<(f64, f64, f64)> = idxs.iter().map(|&i| {
                    let p = sc.to_screen(
                        unit_norm(x_vals[i], xs),
                        unit_norm(y_vals[i], ys),
                        unit_norm(z_vals[i], zs),
                    );
                    (p.x, p.y, p.depth)
                }).collect();

                // Each segment is its own element, so the dash phase is carried
                // along the route by `segment_svg` — the same writer the flat
                // ramped stroke uses, because the two need the same thing for
                // the same reason.
                let mut run = 0.0_f64;
                for (k, w) in pts.windows(2).enumerate() {
                    let (a, b) = (w[0], w[1]);
                    if !(a.0.is_finite() && a.1.is_finite() && b.0.is_finite() && b.1.is_finite()) {
                        continue;
                    }
                    // A measured color is read off the segment; a categorical
                    // one (or none) is the series' single hue.
                    let c = match &ramp_color {
                        Some(rc) => rc.segment(idxs[k], idxs[k + 1]),
                        None => stroke.clone(),
                    };
                    pieces.push((
                        (a.2 + b.2) / 2.0,
                        super::segment_svg((a.0, a.1), (b.0, b.1), &c,
                            stroke_w, stroke_o, dash, run),
                    ));
                    run += super::seg_len((a.0, a.1), (b.0, b.1));
                }

                // A head sorts in at its tip's depth like any other piece, so an
                // arrow that ends at the back of the cube is covered by whatever
                // passes in front of it — the same rule as the stroke it caps.
                let last = pts.len() - 1;
                // Each end carries the row its tip sits on, so a ramped route's
                // head takes the color of the value it ends at rather than a
                // hue the stroke never shows there.
                let ends: &[((f64, f64, f64), (f64, f64, f64), usize)] = &match arrow {
                    Some("end")   => vec![(pts[last - 1], pts[last], last)],
                    Some("start") => vec![(pts[1], pts[0], 0)],
                    Some("both")  => vec![(pts[last - 1], pts[last], last), (pts[1], pts[0], 0)],
                    _ => vec![],
                };
                for &(from, tip, tip_row) in ends {
                    if let Some(tri) = head_points((from.0, from.1), (tip.0, tip.1), stroke_w) {
                        let c = match &ramp_color {
                            Some(rc) => rc.segment(idxs[tip_row], idxs[tip_row]),
                            None => stroke.clone(),
                        };
                        pieces.push((tip.2, format!(
                            "    <polygon points=\"{tri}\" fill=\"{c}\" \
                             fill-opacity=\"{stroke_o:.3}\" stroke=\"none\"/>\n")));
                    }
                }
            }
            // Far to near: a larger depth is farther from the camera, so painting
            // descending leaves the nearest piece on top.
            pieces.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
            for (_, piece) in &pieces {
                svg.push_str(piece);
            }
            writeln!(svg, "  </g>").unwrap();
            return;
        }

        for (idxs, stroke, dash) in &series {
            if idxs.len() < 2 { continue; }
            let pts: Vec<(f64, f64)> = idxs.iter()
                .map(|&i| super::place(l, polar, x_vals[i], y_vals[i], xs, ys))
                .collect();
            // A measured color cannot ride one `<polyline>` — an element takes
            // one `stroke` — so the route is emitted segment by segment, each
            // carrying the ramp color of the rows it joins. A categorical route
            // keeps the single polyline, byte for byte.
            if let Some(rc) = &ramp_color {
                let mut run = 0.0;
                for (k, w) in pts.windows(2).enumerate() {
                    let c = rc.segment(idxs[k], idxs[k + 1]);
                    svg.push_str(&super::segment_svg(w[0], w[1], &c, stroke_w, stroke_o, dash, run));
                    run += super::seg_len(w[0], w[1]);
                }
            } else {
            let points: String = pts.iter()
                .map(|(px, py)| format!("{px:.2},{py:.2}"))
                .collect::<Vec<_>>()
                .join(" ");
            writeln!(svg,
                r##"    <polyline points="{points}" fill="none" stroke="{stroke}" stroke-width="{stroke_w}" stroke-opacity="{stroke_o:.3}"{dash} stroke-linejoin="round" stroke-linecap="round"/>"##
            ).unwrap();
            }

            // The heads, drawn after the stroke so they sit on top of it. A dashed
            // path gets a solid head: the dash is the *route's* texture, and a head
            // chopped into dashes stops reading as an arrow.
            let last = pts.len() - 1;
            let ends: &[((f64, f64), (f64, f64), usize)] = &match arrow {
                Some("end")   => vec![(pts[last - 1], pts[last], last)],
                Some("start") => vec![(pts[1], pts[0], 0)],
                Some("both")  => vec![(pts[last - 1], pts[last], last), (pts[1], pts[0], 0)],
                _ => vec![],
            };
            for &(from, tip, tip_row) in ends {
                if let Some(tri) = head_points(from, tip, stroke_w) {
                    // A ramped route's head takes the color of the value it ends
                    // at, so the arrow agrees with the stroke it caps.
                    let c = match &ramp_color {
                        Some(rc) => rc.segment(idxs[tip_row], idxs[tip_row]),
                        None => stroke.clone(),
                    };
                    writeln!(svg,
                        r##"    <polygon points="{tri}" fill="{c}" fill-opacity="{stroke_o:.3}" stroke="none"/>"##
                    ).unwrap();
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
    fn a_head_points_along_the_last_segment_not_along_the_axes() {
        // Traveling due east on the page: the tip stays put and both barbs fall
        // *behind* it, symmetrically above and below the line of travel.
        let tri = head_points((0.0, 0.0), (100.0, 0.0), 2.0).unwrap();
        let nums: Vec<f64> = tri.replace(',', " ").split_whitespace()
            .map(|s| s.parse().unwrap()).collect();
        assert_eq!((nums[0], nums[1]), (100.0, 0.0), "the tip is the last vertex");
        assert!(nums[2] < 100.0 && nums[4] < 100.0, "both barbs sit behind the tip");
        assert!((nums[3] + nums[5]).abs() < 1e-9, "the barbs straddle the line of travel");
    }

    #[test]
    fn a_head_turns_with_the_path_rather_than_keeping_a_fixed_orientation() {
        // The same head, aimed north instead of east: the barbs now straddle the
        // *vertical*, which is what "the arrow points where the data went" means.
        let tri = head_points((0.0, 100.0), (0.0, 0.0), 2.0).unwrap();
        let nums: Vec<f64> = tri.replace(',', " ").split_whitespace()
            .map(|s| s.parse().unwrap()).collect();
        assert_eq!((nums[0], nums[1]), (0.0, 0.0));
        assert!(nums[3] > 0.0 && nums[5] > 0.0, "both barbs sit below a north-pointing tip");
        assert!((nums[2] + nums[4]).abs() < 1e-9, "the barbs straddle the line of travel");
    }

    #[test]
    fn a_zero_length_last_segment_has_no_direction_so_no_head_is_drawn() {
        // Two identical rows: nothing to point along, and guessing would be worse
        // than the bare stroke.
        assert!(head_points((5.0, 5.0), (5.0, 5.0), 2.0).is_none());
    }

    #[test]
    fn a_thicker_stroke_gets_a_proportionally_longer_head_down_to_a_floor() {
        let reach = |w: f64| {
            let tri = head_points((0.0, 0.0), (100.0, 0.0), w).unwrap();
            let nums: Vec<f64> = tri.replace(',', " ").split_whitespace()
                .map(|s| s.parse().unwrap()).collect();
            100.0 - nums[2]
        };
        assert!(reach(6.0) > reach(2.0), "a thick path earns a bigger head");
        assert!((reach(0.1) - reach(1.0)).abs() < 1e-9, "below the floor the head stops shrinking");
    }
}
