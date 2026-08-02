//! The `area` mark — a filled region between the data and a baseline.
use std::collections::HashMap;
use std::fmt::Write;
use crate::data::DataFrame;
use crate::ir::{Channel, Layer, Transform};
use crate::render::palette::PALETTE_GOG;
use crate::render::encode::OPACITY_DEFAULT;
use crate::render::pattern::{FillTexture, PatternMap};
use crate::render::polar::Polar;
use crate::render::svg::SvgRenderer;
use crate::render::text::esc;
use crate::render::Layout;

impl SvgRenderer {
    // -----------------------------------------------------------------------
    // Mark: area
    // -----------------------------------------------------------------------

    /// A filled region between the data and a baseline.
    ///
    /// Deliberately built as `write_line`'s twin — same x-sort, same
    /// color/group split, same skip-what-cannot-be-placed rule — because an
    /// area *is* a line plus the ground beneath it. Where the two differ, the
    /// difference is stated: a polygon closes back along the baseline, and it
    /// takes a fill rather than a stroke.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn write_area(
        &self, svg: &mut String, layer: &Layer, df: &DataFrame,
        l: &Layout, xs: (f64, f64), ys: (f64, f64),
        x_field: &str, y_field: &str,
        // The domain's categories — the region spans them in axis order, filling
        // down to the same baseline it fills to on a number line.
        cat_x: Option<&[String]>,
        // Where the region is measured from, in scale units: zero on a linear
        // axis, negative infinity on a log one (clamped to the axis foot) —
        // the same rule, and the same reason, as `write_bars`.
        base: f64,
        color_map: &HashMap<String, String>,
        clip: &str,
        // Polar: the region is bent round the circle, so the baseline it closes
        // along is a ring rather than a straight line.
        polar: Option<&Polar>,
    ) {
        let Some(x_vals) = super::positions(df, x_field, cat_x) else { return };
        // y is the measure: it carries the height the region is filled to, so it
        // stays numeric whatever the domain is.
        let Some(y_vals) = df.float_col(y_field) else { return };
        let n = x_vals.len().min(y_vals.len());
        if n < 2 { return; }

        // `pattern(col)` maps one hatch per category (spec §5); the setting fixes one
        // for the whole layer. A region draws per group, so a bound `pattern` joins
        // the split precedence below — `pattern(g)` on its own then draws one region
        // per category, textured, all in the default hue.
        let pattern_map = PatternMap::resolve(layer, df);
        let color_field = layer.encodings.get(&Channel::Color).map(|c| c.field.as_str());
        let group_field = color_field
            .or_else(|| layer.encodings.get(&Channel::Group).map(|c| c.field.as_str()))
            .or_else(|| pattern_map.as_ref().map(|pm| pm.field()));

        // Stack piles the split into abutting bands (§5): each vertex's floor is the
        // cumulative height of the groups below it (carried per row in `stack_base`),
        // not the shared baseline, so a group fills between its own lower and upper
        // boundaries. Stacking is a *geometric* change, not an opacity one: the bands
        // do not overlap, so each keeps the ordinary area weight — the pile-up that
        // made split regions bury each other (the retired Assumption) is simply gone.
        let stacked = layer.transforms.iter().any(|t| matches!(t, Transform::Stack));
        let base_col = if stacked { df.float_col(crate::transform::STACK_BASE) } else { None };

        // One region has one fill, which is why `opacity` is a setting here and
        // not a channel. `size` is not even settable: an area's perimeter is
        // pinned by x, y and the baseline, so there is no extent left to set.
        let st = &layer.style;
        let fill_o = st.opacity.unwrap_or(OPACITY_DEFAULT);
        let set_color = st.color.as_deref().map(esc);
        // The tile emitter — dedups tiles across this layer's regions.
        let mut tex = FillTexture::new();

        // The baseline in screen pixels — one horizontal line every region
        // closes along, computed once.
        let base_px = l.map_y(base.clamp(ys.0, ys.1), ys.0, ys.1);
        if !base_px.is_finite() { return; }

        // The boundary in x order, then back along the baseline to close. No
        // stroke: an area is a region, and the edge on top of one is a `line`
        // layered over it — superposition is the grammar's own answer, and it
        // keeps the border question (a setting, if it is ever anything) out of
        // a mark that would otherwise quietly grow one.
        let polygon = |svg: &mut String, idxs: &[usize], fill: &str| {
            let mut pts = String::with_capacity(idxs.len() * 32 + 32);
            // The upper boundary, left to right.
            for &i in idxs {
                let (px, py) = super::place(l, polar, x_vals[i], y_vals[i], xs, ys);
                let _ = write!(pts, "{px:.2},{py:.2} ");
            }
            // Then close the region. Stacked, it retraces each vertex's own floor
            // (right edge back to left), so the band sits exactly on the group below;
            // otherwise it closes along the flat baseline.
            //
            // Polar takes the retracing path too, whether stacked or not: there the
            // baseline is a *ring*, and closing it with the two straight segments
            // below would cut a chord across the circle. Retracing every vertex at
            // its floor follows the ring at the boundary's own resolution.
            if base_col.is_some() || polar.is_some() {
                for &i in idxs.iter().rev() {
                    let b = base_col.and_then(|bs| bs.get(i).copied()).unwrap_or(base);
                    let (px, py) = super::place(l, polar, x_vals[i], b.clamp(ys.0, ys.1), xs, ys);
                    let _ = write!(pts, "{px:.2},{py:.2} ");
                }
            } else {
                let _ = write!(pts, "{:.2},{:.2} {:.2},{:.2}",
                    l.map_x(x_vals[*idxs.last().unwrap()], xs.0, xs.1), base_px,
                    l.map_x(x_vals[idxs[0]], xs.0, xs.1), base_px);
            }
            let _ = writeln!(svg,
                r#"    <polygon points="{pts}" fill="{fill}" fill-opacity="{fill_o:.3}"/>"#);
        };

        // Only points this scale can place, sorted by x — a region drawn in
        // data order would fold over itself.
        let ordered = |filter: &dyn Fn(usize) -> bool| -> Vec<usize> {
            let mut idxs: Vec<usize> = (0..n)
                .filter(|&i| filter(i))
                .filter(|&i| x_vals[i].is_finite() && y_vals[i].is_finite())
                .collect();
            idxs.sort_by(|&a, &b| {
                x_vals[a].partial_cmp(&x_vals[b]).unwrap_or(std::cmp::Ordering::Equal)
            });
            // The filled radar: a wrapped angular domain repeats no endpoint, so
            // the boundary is carried back to its first vertex or the region is
            // left with a wedge cut out of it. `line` closes the same way.
            super::close_if_wrapped(&mut idxs, polar);
            idxs
        };

        writeln!(svg, r##"  <g clip-path="url(#{clip})">"##).unwrap();

        if group_field.is_some() {
            // Every channel that splits, splits — see `marks::split_series`.
            let Some(parts) = super::split_series(
                df, n, color_field,
                layer.encodings.get(&Channel::Group).map(|c| c.field.as_str()),
                pattern_map.as_ref().map(|pm| pm.field()),
            ) else {
                writeln!(svg, "  </g>").unwrap();
                return;
            };
            let mut series_of = vec![usize::MAX; n];
            for (si, p) in parts.iter().enumerate() {
                for &r in &p.rows { series_of[r] = si; }
            }

            // Regions overlap where they share x. Drawing largest-first would
            // be a guess about which series matters; drawing in category order
            // matches the legend, and `stack` is the designed answer to the
            // overlap itself.
            for (gi, part) in parts.iter().enumerate() {
                let idxs = ordered(&|i| series_of[i] == gi);
                if idxs.len() < 2 { continue; }

                let fill: &str = if let Some(c) = &set_color {
                    c
                } else if color_field.is_some() {
                    color_map.get(part.color_key.as_str()).map(String::as_str)
                        .unwrap_or(PALETTE_GOG[gi % PALETTE_GOG.len()])
                } else {
                    PALETTE_GOG[0]
                };
                // The channel textures by this region's category (any row of it);
                // else the setting's fixed texture.
                let texture = pattern_map.as_ref().map(|pm| pm.fill_texture(pm.cat_at(idxs[0]))).or(st.pattern.as_deref());
                let fill_url = tex.fill(svg, texture, fill);
                polygon(svg, &idxs, &fill_url);
            }
        } else {
            let idxs = ordered(&|_| true);
            if idxs.len() >= 2 {
                let fill_url = tex.fill(svg, st.pattern.as_deref(), set_color.as_deref().unwrap_or(PALETTE_GOG[0]));
                polygon(svg, &idxs, &fill_url);
            }
        }

        writeln!(svg, "  </g>").unwrap();
    }
}
