//! The `ribbon` mark — a filled band between a low and a high boundary.
use std::collections::HashMap;
use std::fmt::Write;
use crate::data::DataFrame;
use crate::ir::{Channel, Layer};
use crate::render::palette::PALETTE_GOG;
use crate::render::encode::OPACITY_DEFAULT;
use crate::render::pattern::{FillTexture, PatternMap};
use crate::render::polar::Polar;
use crate::render::svg::{SvgRenderer, OVERLAY_FILL};
use crate::render::text::esc;
use crate::render::Layout;

impl SvgRenderer {
    // -----------------------------------------------------------------------
    // Mark: ribbon — a filled band between a low and a high boundary
    // -----------------------------------------------------------------------

    /// A filled band spanning from a low boundary to a high one across x — the
    /// confidence / spread band. It is `area`'s geometry (one filled polygon, no
    /// stroke — the edge on a band is a layered `line`) fed by `interval`'s
    /// machinery: a range transform emits the two boundaries as a **low/high pair
    /// of rows** per x (`range`, `confidence`), exactly as it does for `interval`,
    /// and this pairs them back — row `i` the floor, row `i+1` the ceiling — into
    /// one band instead of one whisker per x. Where [`Self::write_area`] closes
    /// every region on a single flat baseline, a ribbon closes on its own lower
    /// boundary, traced vertex by vertex (like a stacked area's floor).
    ///
    /// `color`/`group` split it into one band per group (the discrete split `area`
    /// makes). Split bands share their x and so overlap; unlike `area` — which
    /// stays opaque and points at `stack` — a ribbon carries no baseline height to
    /// stack, so its honest overlap answer is transparency, and split bands draw
    /// translucent by default (the rule `write_box` uses for overlaid boxes). A set
    /// `style(opacity = )` overrides.
    ///
    /// **Polar needs no arc**, and that is worth stating because the refusal this
    /// mark carried for two days said it did ("a band's two boundaries" were listed
    /// among the straight edges that would have to bend). They are not: a boundary
    /// runs through the data's own vertices, so its segments are the chords
    /// `line`/`area`/`path` have drawn in this space since it shipped. The one
    /// thing a band could have needed is a *ring* to close along — and it never
    /// closes on a ring, because it closes on its own lower boundary, vertex by
    /// vertex. That is precisely the retracing path `write_area` switches to in
    /// polar for exactly this reason, which this mark had already been doing flat
    /// since it was written. So bending it is `place` plus the wrap.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn write_ribbon(
        &self, svg: &mut String, layer: &Layer, df: &DataFrame,
        l: &Layout, xs: (f64, f64), ys: (f64, f64),
        x_field: &str, y_field: &str,
        // The domain's categories — a band across them is the spread profile, the
        // filled counterpart to one `interval` whisker per category.
        cat_x: Option<&[String]>,
        color_map: &HashMap<String, String>,
        clip: &str,
        // Polar: the band is bent round the circle — the radar band, `ribbon`'s
        // reading of the same categorical domain that makes `line` a radar.
        polar: Option<&Polar>,
    ) {
        let Some(x_vals) = super::positions(df, x_field, cat_x) else { return };
        // The extents are numeric whatever the domain is — a range transform
        // synthesizes floats, and `rule_for` keeps the measure continuous.
        let Some(y_vals) = df.float_col(y_field) else { return };
        let n = x_vals.len().min(y_vals.len());
        if n < 2 { return; }

        let st = &layer.style;
        let set_color = st.color.as_deref().map(esc);
        // `pattern` hatches the band (spec §4/§5) — the setting fixes one for the
        // layer, the channel maps one per category. A band draws per group, so a
        // bound `pattern` joins the split precedence below.
        let mut tex = FillTexture::new();
        let pattern_map = PatternMap::resolve(layer, df);
        let color_field = layer.encodings.get(&Channel::Color).map(|c| c.field.as_str());
        let group_field = color_field
            .or_else(|| layer.encodings.get(&Channel::Group).map(|c| c.field.as_str()))
            .or_else(|| pattern_map.as_ref().map(|pm| pm.field()));

        // Pair the rows a range transform emits — row i the low boundary, i+1 the
        // high — into (x, lo, hi) triples, dropping any pair a scale cannot place
        // and sorting by x (a band drawn in data order would fold over itself, the
        // same reason `write_area` sorts). Both rows of a pair carry the same group,
        // so a per-group slice of the row indices stays correctly paired.
        let band = |rows: &[usize]| -> Vec<(f64, f64, f64)> {
            let mut tri: Vec<(f64, f64, f64)> = rows
                .chunks_exact(2)
                .map(|p| (x_vals[p[0]], y_vals[p[0]], y_vals[p[1]]))
                .filter(|&(x, lo, hi)| x.is_finite() && lo.is_finite() && hi.is_finite())
                .collect();
            tri.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
            // The radar band: a wrapped angular domain repeats no endpoint, so the
            // band is carried back to its first x or it is left with a wedge cut
            // out of it — `line` and `area` close the same way and for the same
            // reason (`close_if_wrapped`, which takes row indices where this takes
            // the paired triples).
            if tri.len() >= 2 && polar.is_some_and(|p| p.wraps()) {
                tri.push(tri[0]);
            }
            tri
        };

        // The band as one polygon: the high boundary left→right, then the low
        // boundary right→left to close it.
        let polygon = |svg: &mut String, tri: &[(f64, f64, f64)], fill: &str, fill_o: f64| {
            if tri.len() < 2 { return; }
            let mut pts = String::with_capacity(tri.len() * 64 + 32);
            for &(x, _, hi) in tri {
                let (px, py) = super::place(l, polar, x, hi, xs, ys);
                let _ = write!(pts, "{px:.2},{py:.2} ");
            }
            for &(x, lo, _) in tri.iter().rev() {
                let (px, py) = super::place(l, polar, x, lo, xs, ys);
                let _ = write!(pts, "{px:.2},{py:.2} ");
            }
            let _ = writeln!(svg,
                r#"    <polygon points="{pts}" fill="{fill}" fill-opacity="{fill_o:.3}"/>"#);
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

            // Two or more bands share their x and overlap, so they draw translucent
            // to stay legible — a ribbon's stack-free answer to the overlap `area`
            // sends to `stack`. A lone colored band (each x its own hue) does not.
            let overlaid = parts.len() >= 2;
            let fill_o = st.opacity.unwrap_or(if overlaid { OVERLAY_FILL } else { OPACITY_DEFAULT });

            for (gi, part) in parts.iter().enumerate() {
                let rows: Vec<usize> = part.rows.clone();
                let tri = band(&rows);
                let fill: &str = if let Some(c) = &set_color {
                    c
                } else if color_field.is_some() {
                    color_map.get(part.color_key.as_str()).map(String::as_str)
                        .unwrap_or(PALETTE_GOG[gi % PALETTE_GOG.len()])
                } else {
                    PALETTE_GOG[0]
                };
                let texture = pattern_map.as_ref().map(|pm| pm.fill_texture(pm.cat_at(rows[0]))).or(st.pattern.as_deref());
                let fill_url = tex.fill(svg, texture, fill);
                polygon(svg, &tri, &fill_url, fill_o);
            }
        } else {
            let fill_o = st.opacity.unwrap_or(OPACITY_DEFAULT);
            let rows: Vec<usize> = (0..n).collect();
            let tri = band(&rows);
            let fill_url = tex.fill(svg, st.pattern.as_deref(), set_color.as_deref().unwrap_or(PALETTE_GOG[0]));
            polygon(svg, &tri, &fill_url, fill_o);
        }

        writeln!(svg, "  </g>").unwrap();
    }
}
