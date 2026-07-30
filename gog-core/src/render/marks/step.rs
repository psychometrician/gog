//! The `step` mark — `line`'s staircase twin; a histogram silhouette when binned.
use std::collections::HashMap;
use std::fmt::Write;
use crate::data::DataFrame;
use crate::ir::{Channel, Layer, Transform};
use crate::render::palette::PALETTE_GOG;
use crate::render::pattern::{pattern_dasharray, PatternMap};
use crate::render::polar::Polar;
use crate::render::svg::{unit_norm, SvgRenderer};
use crate::render::text::esc;
use crate::render::Layout;

impl SvgRenderer {
    // -----------------------------------------------------------------------
    // Mark: step
    // -----------------------------------------------------------------------

    /// `write_line`'s staircase twin: one stroke per group, but the path holds
    /// each value until it changes instead of slanting between points.
    ///
    /// Two shapes, chosen the way `write_bars` chooses contiguous-vs-gapped — by
    /// whether a `bin` ran:
    /// - **`step * bin`** (a histogram) traces the silhouette: flat across each
    ///   bin at its count, vertical between bins, dropping to the baseline at both
    ///   ends so it sits on the axis. Bin edges come from the center spacing.
    /// - **plain `step`** (a CDF, a survival curve, a rate) holds `y` from one x
    ///   to the next, then jumps — no baseline; the value steps where it changes.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn write_step(
        &self, svg: &mut String, layer: &Layer, df: &DataFrame,
        l: &Layout, xs: (f64, f64), ys: (f64, f64),
        x_field: &str, y_field: &str,
        // The domain's categories — `line`'s row, so `step`'s too (Law 2). A
        // categorical staircase holds each category's value across its slot.
        cat_x: Option<&[String]>,
        ext_base: f64,
        color_map: &HashMap<String, String>,
        // The sequential ramp, for the other reading of `color`: a measure along
        // the staircase rather than a category naming the series (`StrokeRamp`).
        ramp: &[String],
        clip: &str,
        // Polar: a **tread** holds one value across a span of angle, which is an arc
        // at constant radius; a **riser** changes the value at one angle, which is
        // exactly the radius. So the staircase alternates `hold_to` and `line_to`,
        // and the tread is the segment this whole space was waiting on — a tread
        // drawn as a chord would cut inside the value it is holding, which is the
        // silent wrongness §12 forbids and the reason `step` was refused here.
        polar: Option<&Polar>,
    ) {
        let Some(x_vals) = super::positions(df, x_field, cat_x) else { return };
        let Some(y_vals) = df.float_col(y_field) else { return };
        let n = x_vals.len().min(y_vals.len());
        if n == 0 { return; }

        // The silhouette shape needs bin edges, and `bin` is refused on a
        // categorical axis (`check_distribution_axis`), so a categorical step is
        // always the hold-until-change path below.
        let is_hist = layer.transforms.iter().any(|t| matches!(t, Transform::Bin));

        // Same grouping precedence as `write_line`: `color` splits and colors;
        // a bare `group` splits without coloring; a mapped `pattern` splits and
        // dashes (spec §5), so `pattern(g)` alone draws one staircase per category.
        let pattern_map = PatternMap::resolve(layer, df);
        // `line`'s split, unchanged: a measured color varies along the staircase
        // and does not group it; only a categorical one does.
        let ramp_color = super::StrokeRamp::resolve(layer, df, ramp);
        let color_field = layer.encodings.get(&Channel::Color)
            .map(|c| c.field.as_str())
            .filter(|_| ramp_color.is_none());
        let group_field = color_field
            .or_else(|| layer.encodings.get(&Channel::Group).map(|c| c.field.as_str()))
            .or_else(|| pattern_map.as_ref().map(|pm| pm.field()));

        // One stroke per series, so `size`/`opacity` are settings, not channels —
        // the same reasoning as `line`. A slightly thinner default than `line`'s
        // 2.0, because a step is usually read as an outline.
        let st = &layer.style;
        let stroke_w = st.size.unwrap_or(1.5);
        let stroke_o = st.opacity.unwrap_or(1.0);
        let set_color = st.color.as_deref().map(esc);

        // Where the histogram silhouette meets the axis at its two ends. Kept in
        // *data* units, because the vertices below are: a staircase's corners are
        // the two axes' own values, and which page they land on is the coordinate
        // space's business rather than the builder's (`place`).
        let base_v = ext_base.clamp(ys.0, ys.1);

        // The stepped point string for one group's rows, or `None` if too few to
        // draw. Sorts by x, then builds the silhouette (binned) or the
        // hold-until-change path (plain).
        // Each output point carries the **source row whose value it expresses**,
        // because a staircase's points are derived rather than one-per-row: a
        // row contributes both the corner where its value starts and the corner
        // where it ends. A measured color needs that provenance, and it makes
        // the reading exact — a tread joins one row to itself and so shows that
        // row's own color, while a riser joins two rows and blends between
        // them, which is what a riser *is*.
        let path_for = |idxs: &mut Vec<usize>| -> Option<(Vec<(f64, f64)>, Vec<usize>)> {
            idxs.retain(|&i| x_vals[i].is_finite() && y_vals[i].is_finite());
            idxs.sort_by(|&a, &b| x_vals[a].partial_cmp(&x_vals[b]).unwrap_or(std::cmp::Ordering::Equal));
            if idxs.len() < 2 { return None; }

            let mut pts: Vec<(f64, f64)> = Vec::new();
            let mut rows: Vec<usize> = Vec::new();
            if is_hist {
                // Bins are contiguous and equal-width, so the width is the center
                // spacing and the edges are center ± half.
                let half = (x_vals[idxs[1]] - x_vals[idxs[0]]) / 2.0;
                pts.push((x_vals[idxs[0]] - half, base_v));
                rows.push(idxs[0]);
                for &i in idxs.iter() {
                    pts.push((x_vals[i] - half, y_vals[i]));
                    rows.push(i);
                    pts.push((x_vals[i] + half, y_vals[i]));
                    rows.push(i);
                }
                let last = *idxs.last().unwrap();
                pts.push((x_vals[last] + half, base_v));
                rows.push(last);
            } else {
                // Hold y from one x to the next, then jump (steps-post): the
                // value *was* y until the next x, so a straight slant would draw
                // a change that never happened.
                for (k, &i) in idxs.iter().enumerate() {
                    if k > 0 {
                        pts.push((x_vals[i], y_vals[idxs[k - 1]]));
                        rows.push(idxs[k - 1]);
                    }
                    pts.push((x_vals[i], y_vals[i]));
                    rows.push(i);
                }
                // The staircase's closing tread, and the one place a step needed a
                // rule `line` and `area` already had. On a wrapped angular domain
                // the categories exhaust the turn, so the **last** category's slot
                // runs from its own angle round to the first one's — and flat that
                // slot does not exist, which is why the last value gets no tread
                // there. Carrying it to `first + one period` rather than back to
                // `first` matters: the same point, but reached forwards through the
                // wrap instead of the long way round against the sweep.
                if polar.is_some_and(|p| p.wraps()) && idxs.len() >= 2 {
                    let last = *idxs.last().unwrap();
                    pts.push((x_vals[idxs[0]] + (xs.1 - xs.0), y_vals[last]));
                    rows.push(last);
                }
            }
            Some((pts, rows))
        };

        // Miter joins and butt caps keep the corners square — a staircase, not
        // the rounded polyline `line` draws. A dash rides here too (paint, not
        // geometry); `""` when solid/unset leaves the staircase unchanged.
        // The setting's dash is the default; a mapped `pattern` overrides it per
        // group, so the dash is written on each polyline, not baked into the shared
        // attributes.
        let dash_attr = pattern_dasharray(st.pattern.as_deref());
        let stroke_attrs = format!(
            r#"fill="none" stroke-width="{stroke_w}" stroke-opacity="{stroke_o:.3}" stroke-linejoin="miter" stroke-linecap="butt""#
        );

        // One staircase, either as the single polyline it has always been or —
        // when `color` maps a measure — as one element per tread and riser.
        // The vertices arrive in data units, so each writer maps them itself. Every
        // segment this builder emits is axis-aligned — a tread or a riser, never
        // both — which is what lets the polar writer identify a tread by its equal
        // radii: it is reading the staircase's own structure back, not guessing
        // that two data values happened to coincide (`Polar::hold_to`).
        let is_tread = |a: (f64, f64), b: (f64, f64)| (a.1 - b.1).abs() < 1e-12;
        let write_stair = |svg: &mut String, pts: &[(f64, f64)], rows: &[usize],
                           stroke: &str, dash: &str| {
            if let Some(p) = polar {
                let norm = |v: (f64, f64)| (unit_norm(v.0, xs), unit_norm(v.1, ys));
                if let Some(rc) = &ramp_color {
                    let mut run = 0.0;
                    for (k, w) in pts.windows(2).enumerate() {
                        let c = rc.segment(rows[k], rows[k + 1]);
                        let ((u0, v0), (u1, v1)) = (norm(w[0]), norm(w[1]));
                        let mut d = String::new();
                        p.move_to(&mut d, u0, v0);
                        if is_tread(w[0], w[1]) { p.hold_to(&mut d, u0, u1, v0); }
                        else { p.line_to(&mut d, u1, v1); }
                        let off = if dash.is_empty() { String::new() }
                                  else { format!(r#" stroke-dashoffset="{run:.2}""#) };
                        writeln!(svg, r#"    <path d="{d}" stroke="{c}"{dash}{off} {stroke_attrs}/>"#).unwrap();
                        // A tread's length is its **arc**, not the chord under it —
                        // the dash phase advances along the ink, and using the chord
                        // would let the pattern drift backwards round the circle.
                        run += if is_tread(w[0], w[1]) {
                            p.radius(v0) * ((u1 - u0).abs() * std::f64::consts::TAU)
                        } else {
                            super::seg_len(p.at(u0, v0), p.at(u1, v1))
                        };
                    }
                    return;
                }
                let mut d = String::new();
                let (u_first, v_first) = norm(pts[0]);
                p.move_to(&mut d, u_first, v_first);
                for w in pts.windows(2) {
                    let ((u0, v0), (u1, v1)) = (norm(w[0]), norm(w[1]));
                    if is_tread(w[0], w[1]) { p.hold_to(&mut d, u0, u1, v0); }
                    else { p.line_to(&mut d, u1, v1); }
                }
                writeln!(svg, r#"    <path d="{d}" stroke="{stroke}"{dash} {stroke_attrs}/>"#).unwrap();
                return;
            }
            let px: Vec<(f64, f64)> = pts.iter()
                .map(|&(x, y)| (l.map_x(x, xs.0, xs.1), l.map_y(y, ys.0, ys.1)))
                .collect();
            if let Some(rc) = &ramp_color {
                let mut run = 0.0;
                for (k, w) in px.windows(2).enumerate() {
                    let c = rc.segment(rows[k], rows[k + 1]);
                    // Butt caps, not the round ones a slanted stroke takes: a
                    // staircase's corners are square, and rounding them would
                    // round off the very thing that makes it a step.
                    svg.push_str(&super::segment_svg_capped(
                        w[0], w[1], &c, stroke_w, stroke_o, dash, run, "butt"));
                    run += super::seg_len(w[0], w[1]);
                }
                return;
            }
            let points = px.iter().map(|(x, y)| format!("{x:.2},{y:.2}"))
                .collect::<Vec<_>>().join(" ");
            writeln!(svg, r#"    <polyline points="{points}" stroke="{stroke}"{dash} {stroke_attrs}/>"#).unwrap();
        };

        // A pair transform turns a step into the *two boundary staircases* of a
        // band — the stepped counterpart to `line * range`, and to a `ribbon`
        // filling the pair (control limits, a stepped envelope). The rows arrive
        // low-then-high per x, so each group splits into a low locus and a high one.
        let is_pair = layer.transforms.iter().any(|t| matches!(
            t, Transform::Range | Transform::Confidence | Transform::Bounds));
        let boundaries = |ordered: Vec<usize>| -> Vec<Vec<usize>> {
            if is_pair {
                vec![
                    ordered.iter().step_by(2).copied().collect(),
                    ordered.iter().skip(1).step_by(2).copied().collect(),
                ]
            } else {
                vec![ordered]
            }
        };

        writeln!(svg, r##"  <g clip-path="url(#{clip})">"##).unwrap();

        if let Some(gf) = group_field {
            let gvals: Vec<&str> = match df.str_col(gf) {
                Some(sv) => sv.iter().map(String::as_str).collect(),
                None => { writeln!(svg, "  </g>").unwrap(); return; }
            };
            let mut seen = std::collections::HashSet::new();
            let mut groups: Vec<&str> = Vec::new();
            for &g in &gvals { if seen.insert(g) { groups.push(g); } }

            for (gi, group) in groups.iter().enumerate() {
                let ordered: Vec<usize> = (0..n).filter(|&i| gvals[i] == *group).collect();
                let stroke: &str = if let Some(c) = &set_color {
                    c
                } else if color_field.is_some() {
                    color_map.get(*group).map(String::as_str).unwrap_or(PALETTE_GOG[gi % PALETTE_GOG.len()])
                } else {
                    PALETTE_GOG[0]
                };
                // A mapped `pattern` dashes this series by its category; else the setting.
                let dash = pattern_map.as_ref()
                    .and_then(|pm| ordered.first().map(|&r| pattern_dasharray(Some(pm.dash(pm.cat_at(r))))))
                    .unwrap_or(dash_attr);
                for mut b in boundaries(ordered) {
                    if let Some((pts, rows)) = path_for(&mut b) {
                        write_stair(svg, &pts, &rows, stroke, dash);
                    }
                }
            }
        } else {
            let stroke = set_color.as_deref().unwrap_or(PALETTE_GOG[0]);
            for mut b in boundaries((0..n).collect()) {
                if let Some((pts, rows)) = path_for(&mut b) {
                    write_stair(svg, &pts, &rows, stroke, dash_attr);
                }
            }
        }

        writeln!(svg, "  </g>").unwrap();
    }
}
