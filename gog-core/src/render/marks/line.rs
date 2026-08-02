//! The `line` mark — one polyline per series, in x order.
use std::collections::HashMap;
use std::fmt::Write;
use crate::data::DataFrame;
use crate::ir::{Channel, Layer, Transform};
use crate::render::palette::PALETTE_GOG;
use crate::render::pattern::{pattern_dasharray, PatternMap};
use crate::render::polar::Polar;
use crate::render::svg::SvgRenderer;
use crate::render::text::esc;
use crate::render::Layout;

impl SvgRenderer {
    // -----------------------------------------------------------------------
    // Mark: line
    // -----------------------------------------------------------------------

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn write_line(
        &self, svg: &mut String, layer: &Layer, df: &DataFrame,
        l: &Layout, xs: (f64, f64), ys: (f64, f64),
        x_field: &str, y_field: &str,
        // The domain's categories, when x carries them. A line is read along x,
        // so a category is simply a place on that reading — the vertex order,
        // the sort and the stroke are the numeric line's, unchanged.
        cat_x: Option<&[String]>,
        color_map: &HashMap<String, String>,
        // The sequential ramp, for the other reading of `color`: a measure along
        // the stroke rather than a category naming the series (`StrokeRamp`).
        ramp: &[String],
        clip: &str,
        // Polar: the same series, read round the circle instead of across the
        // page — Wilkinson's polar time series (§9.1.6.4). The vertices move; the
        // grouping, the colors and the dash do not.
        polar: Option<&Polar>,
    ) {
        let Some(x_vals) = super::positions(df, x_field, cat_x) else { return };
        // y is the measure and stays numeric — `rule_for` keeps it continuous,
        // because a line traces a quantity along the domain.
        let Some(y_vals) = df.float_col(y_field) else { return };
        let n = x_vals.len().min(y_vals.len());
        if n < 2 { return; }

        // Grouping: color takes priority (it also colors the lines), then group,
        // then a mapped `pattern` — so `pattern(g)` on its own draws one dashed line
        // per category (spec §5), a color-free way to tell series apart.
        let pattern_map = PatternMap::resolve(layer, df);
        // A *measured* color varies along the stroke and so does not split it
        // into series; only a categorical one does. `group` still splits either
        // way, which is what lets a ramped route be drawn once per group.
        let ramp_color = super::StrokeRamp::resolve(layer, df, ramp);
        let color_field = layer.encodings.get(&Channel::Color)
            .map(|c| c.field.as_str())
            .filter(|_| ramp_color.is_none());
        let group_field = color_field
            .or_else(|| layer.encodings.get(&Channel::Group).map(|c| c.field.as_str()))
            .or_else(|| pattern_map.as_ref().map(|pm| pm.field()));

        // A pair transform (`range`/`confidence`/`bounds`) turns a `line` into the
        // *two boundary curves* of a band — the unfilled counterpart to a `ribbon`
        // filling the same pair. The rows arrive low-then-high per x, and the line
        // splits them into a low locus and a high locus rather than connecting them
        // in one zigzag.
        let is_pair = layer.transforms.iter().any(|t| matches!(
            t, Transform::Range | Transform::Confidence | Transform::Bounds));

        // Warn when a line has many ungrouped rows and no synthesizing transform —
        // connecting all points in x order is rarely the user's intention. A
        // transform that reduces each x-group to a single value makes the connected
        // line *intentional* (a trend, a summary curve) and silences the warning:
        // the smoothers (`smooth`/`density`) and the whole aggregation family
        // (`mean`/`sum`/…, `count`, `proportion`) all leave one point per x, so no
        // accidental zigzag is possible — the exact false positive the canonical
        // `ribbon * range + line * mean` band-and-trend plot would otherwise hit.
        // A pair transform draws two clean boundaries, not a zigzag, so it is exempt
        // too.
        let has_clean_transform = is_pair || layer.transforms.iter().any(|t| matches!(
            t,
            Transform::Smooth | Transform::Density
                | Transform::Mean | Transform::Median | Transform::Sum
                | Transform::Max | Transform::Min
                | Transform::Count | Transform::Proportion
        ));
        if group_field.is_none() && !has_clean_transform && n > 5 {
            eprintln!(
                "gog: `line` has {n} rows and no group or color channel — all points \
                 will be connected in x order. If you have multiple series, add \
                 `color(<field>)` or `group(<field>)` to draw one line per category \
                 (e.g. `+ color(country)`)."
            );
        }

        // A polyline is one stroke, which is exactly why `size` and `opacity`
        // are refused as *channels* here and accepted as *settings*: one stroke
        // has one width and one opacity, they just cannot vary row by row.
        let st = &layer.style;
        let stroke_w = st.size.unwrap_or(2.0);
        let stroke_o = st.opacity.unwrap_or(1.0);
        let set_color = st.color.as_deref().map(esc);
        // A dashed/dotted stroke — paint, so it rides here beside width and opacity.
        // The round linecap turns the dotted pattern into round dots. `""` (solid or
        // unset) adds no attribute, so a plain line is byte-for-byte unchanged.
        let dash_attr = pattern_dasharray(st.pattern.as_deref());

        writeln!(svg, r##"  <g clip-path="url(#{clip})">"##).unwrap();

        // Turn a group's rows into the polyline(s) to stroke, in x order. Normally
        // one series; a pair transform splits it into two — the low boundary (rows
        // at even positions, the pair-rows arriving low-then-high) and the high —
        // each a curve of its own. Non-finite points a scale cannot place drop out
        // rather than break a line.
        let by_x = |b: &mut Vec<usize>| {
            b.retain(|&i| x_vals[i].is_finite() && y_vals[i].is_finite());
            b.sort_by(|&a, &c| x_vals[a].partial_cmp(&x_vals[c]).unwrap_or(std::cmp::Ordering::Equal));
            // A wrapped angular domain has no repeated endpoint to close on, so
            // the last vertex is joined back to the first — the radar's closing
            // segment. Flat, and on a measured angle, this is a no-op.
            super::close_if_wrapped(b, polar);
        };
        let mut series: Vec<(Vec<usize>, String, &'static str)> = Vec::new();
        let mut add = |ordered: Vec<usize>, stroke: String, dash: &'static str| {
            if is_pair {
                let mut lo: Vec<usize> = ordered.iter().step_by(2).copied().collect();
                let mut hi: Vec<usize> = ordered.iter().skip(1).step_by(2).copied().collect();
                by_x(&mut lo);
                by_x(&mut hi);
                series.push((lo, stroke.clone(), dash));
                series.push((hi, stroke, dash));
            } else {
                let mut idxs = ordered;
                by_x(&mut idxs);
                series.push((idxs, stroke, dash));
            }
        };

        if group_field.is_some() {
            // Every channel that splits, splits — see `marks::split_series`. Binding
            // `color` beside `group` used to discard the `group`, which drew one
            // series where the sentence asked for one per combination.
            let Some(parts) = super::split_series(
                df, n, color_field,
                layer.encodings.get(&Channel::Group).map(|c| c.field.as_str()),
                pattern_map.as_ref().map(|pm| pm.field()),
            ) else {
                writeln!(svg, "  </g>").unwrap();
                return;
            };
            for (gi, part) in parts.iter().enumerate() {
                // Rows for this series, in original (pair) order.
                let ordered: Vec<usize> = part.rows.clone();
                // A set color wins; else the series' hue (`color`), else the
                // default — `group` separates series without inventing a color.
                let stroke = if let Some(c) = &set_color {
                    c.clone()
                } else if color_field.is_some() {
                    color_map.get(part.color_key.as_str()).cloned()
                        .unwrap_or_else(|| PALETTE_GOG[gi % PALETTE_GOG.len()].to_string())
                } else {
                    PALETTE_GOG[0].to_string()
                };
                // A mapped `pattern` dashes each series by its category (any of its
                // rows); else the layer's `style(pattern = )` setting, or none.
                let dash = pattern_map.as_ref()
                    .and_then(|pm| ordered.first().map(|&r| pattern_dasharray(Some(pm.dash(pm.cat_at(r))))))
                    .unwrap_or(dash_attr);
                add(ordered, stroke, dash);
            }
        } else {
            add((0..n).collect(), set_color.clone().unwrap_or_else(|| PALETTE_GOG[0].to_string()), dash_attr);
        }

        for (idxs, stroke, dash) in &series {
            if idxs.len() < 2 { continue; }
            // In polar the vertices ride round the circle, and no explicit closing
            // segment is needed: the angular axis is periodic and fitted flush, so
            // a series that spans the whole range has its last vertex on top of
            // its first and the curve closes itself (Wilkinson §9.1.6 aligns the
            // scale minimum with 0 radians and the maximum with 2π).
            // A measured color cannot ride one `<polyline>` — an element takes
            // one `stroke` — so the stroke is emitted segment by segment, each
            // carrying the ramp color of the rows it joins and the running dash
            // phase. Every other line keeps the single polyline below, byte for
            // byte, so this reading costs the ordinary one nothing.
            if let Some(rc) = &ramp_color {
                let pts: Vec<(f64, f64)> = idxs.iter()
                    .map(|&i| super::place(l, polar, x_vals[i], y_vals[i], xs, ys))
                    .collect();
                let mut run = 0.0;
                for (k, w) in pts.windows(2).enumerate() {
                    let c = rc.segment(idxs[k], idxs[k + 1]);
                    svg.push_str(&super::segment_svg(w[0], w[1], &c, stroke_w, stroke_o, dash, run));
                    run += super::seg_len(w[0], w[1]);
                }
                continue;
            }
            let points: String = idxs.iter()
                .map(|&i| {
                    let (px, py) = super::place(l, polar, x_vals[i], y_vals[i], xs, ys);
                    format!("{px:.2},{py:.2}")
                })
                .collect::<Vec<_>>()
                .join(" ");
            writeln!(svg,
                r##"    <polyline points="{points}" fill="none" stroke="{stroke}" stroke-width="{stroke_w}" stroke-opacity="{stroke_o:.3}"{dash} stroke-linejoin="round" stroke-linecap="round"/>"##
            ).unwrap();
        }

        writeln!(svg, "  </g>").unwrap();
    }
}
