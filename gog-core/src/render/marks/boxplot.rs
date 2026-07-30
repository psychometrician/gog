//! The `box` mark — the five-number summary drawn as box-and-whisker.
use std::collections::HashMap;
use std::fmt::Write;
use crate::data::DataFrame;
use crate::ir::{Channel, Layer};
use crate::render::palette::PALETTE_GOG;
use crate::render::encode::OPACITY_DEFAULT;
use crate::render::pattern::{FillTexture, PatternMap};
use crate::render::polar::Polar;
use crate::render::project::Scene;
use crate::render::svg::{unit_norm, SvgRenderer, OVERLAY_FILL};
use crate::render::text::esc;
use crate::render::Layout;
use super::{bar_thickness_svg, Dodge};

impl SvgRenderer {
    // -----------------------------------------------------------------------
    // Mark: box — the five-number summary drawn as box-and-whisker
    // -----------------------------------------------------------------------

    /// At each x, draws the box-and-whisker of `y`'s distribution in that group:
    /// a box from the lower quartile to the upper, a line at the median, and
    /// whiskers with end caps out to the whisker ends. The `box` transform (injected
    /// by the mark) supplies the summary — the two whisker ends as a low/high pair of
    /// rows in `y_field` (exactly as `range` does, so the axis already spans them),
    /// and the quartiles in the `lower`/`middle`/`upper` columns.
    ///
    /// Under the default Tukey rule the whiskers stop at 1.5·IQR and the points
    /// beyond arrive as **outlier rows** — flagged by a `NaN` in `middle` — which
    /// this draws as small dots. So the loop partitions the rows on that sentinel:
    /// box rows pair up (low, high) for the boxes, outlier rows scatter as points.
    ///
    /// `color` splits the summary into one box per group (the discrete split
    /// `bar`/`interval` make); a set color overrides. Split boxes share their slot
    /// and so **overlap** — the same behavior split whiskers have, resolved by
    /// `dodge` — so their fills are translucent to stay legible meanwhile.
    ///
    /// **Upright or lying down**, like `bar` and `interval`: a box stands in a slot
    /// on one axis and summarizes along the other, and the bindings say which
    /// (`legality::slot_orient`). `box + x(dept) + y(pay)` is a column of boxes,
    /// `box + x(pay) + y(dept)` the horizontal box plot — the form with room for
    /// long category names. Everything below is written once in terms of *slot* and
    /// *extent*; only the closures that reach the page know which of those is x.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn write_box(
        &self, svg: &mut String, layer: &Layer, df: &DataFrame,
        l: &Layout, xs: (f64, f64), ys: (f64, f64),
        x_field: &str, y_field: &str,
        cat_x: Option<&[String]>, cat_y: Option<&[String]>,
        horizontal: bool,
        color_map: &HashMap<String, String>,
        clip: &str,
        // Polar: the body is a wedge (`Polar::sector`, `bar`'s shape) and the
        // line-work is arcs and radii. **Every stroke a box draws is *held***: the
        // median asserts one value across the whole slot, a whisker asserts its
        // reach across the whole extent, a cap marks where that reach stops. None
        // of them interpolates, which is exactly why this mark could not be drawn
        // with the chords `line` bends into, and why it needed `hold_to`.
        polar: Option<&Polar>,
    ) {
        // The slot axis goes through the one position resolution every mark
        // shares, so a category there is no exception.
        let (pos_field, ext_field) = if horizontal { (y_field, x_field) } else { (x_field, y_field) };
        let pos_cats = if horizontal { cat_y } else { cat_x };
        let Some(pos_vals) = super::positions(df, pos_field, pos_cats) else { return };

        // The extent axis holds the two whisker ends (min, max) as consecutive
        // rows; the quartiles ride alongside. It is read as floats directly: the
        // relational rule says exactly one axis measures, and this is the one.
        let Some(ext_vals) = df.float_col(ext_field) else { return };
        let (Some(lower), Some(middle), Some(upper)) =
            (df.float_col("lower"), df.float_col("middle"), df.float_col("upper")) else { return };

        let n = pos_vals.len().min(ext_vals.len());
        if n < 2 { return; }

        // Where a (slot, extent) pair lands. The slot arrives already in pixels
        // (the dodge offset is applied there); the extent is mapped through its
        // scale. A line *across* the slot — the median bar, an end cap — runs
        // perpendicular to the box's length, whichever way that points.
        let at = |pos_px: f64, ext: f64| -> (f64, f64) {
            if horizontal { (l.map_x(ext, xs.0, xs.1), pos_px) }
            else          { (pos_px, l.map_y(ext, ys.0, ys.1)) }
        };
        let across = |p: (f64, f64), half: f64| -> ((f64, f64), (f64, f64)) {
            if horizontal { ((p.0, p.1 - half), (p.0, p.1 + half)) }
            else          { ((p.0 - half, p.1), (p.0 + half, p.1)) }
        };

        // Box width: the categorical bar thickness, narrowed — a box reads as a box,
        // not a bar, and the whiskers want air on either side. `bar_thickness_svg`
        // takes the min gap between distinct x's, so the duplicated pair-rows (gap 0)
        // are ignored just as they are for the axis.
        const BOX_WIDTH_FRAC: f64 = 0.62; // of the categorical slot a bar would fill
        // The full categorical slot, measured along whichever axis carries it. A
        // dodge narrows each group's box to `1/G` of it and offsets them across it,
        // so grouped boxes sit side by side (§5).
        let (pos_px, pos_scale) = if horizontal { (l.h(), ys) } else { (l.w(), xs) };
        // `bar`'s rule, unchanged: flat the slot is a count of pixels, bent it is a
        // fraction of the turn, and the dodge offsets inherit whichever it is.
        let pos_span = (pos_scale.1 - pos_scale.0).max(1e-12);
        let slot_px = bar_thickness_svg(&pos_vals, n, pos_px, pos_scale, false);
        let slot = match polar {
            None => slot_px,
            Some(_) => slot_px * pos_span / pos_px,
        };
        let dodge = Dodge::resolve(layer, df);
        let box_w = slot * dodge.as_ref().map_or(1.0, Dodge::width_frac) * BOX_WIDTH_FRAC;
        let half = box_w / 2.0;
        // Whisker end caps, narrower than the box (convention).
        let cap = half * 0.5;

        let st = &layer.style;
        // `pattern` hatches the box *body* fill (spec §4/§5) — setting or channel;
        // the line-work (outline, whiskers, median, caps) is drawn solid below, so a
        // box reads as a box. `solid`/unset is the identity.
        let mut tex = FillTexture::new();
        let pattern_map = PatternMap::resolve(layer, df);
        let set_color = st.color.as_deref().map(esc);
        let color_field = layer.encodings.get(&Channel::Color).map(|c| c.field.as_str());
        let group_vals: Option<Vec<&str>> = color_field
            .and_then(|f| df.str_col(f))
            .map(|v| v.iter().map(String::as_str).collect());

        // Partition the rows. A box row carries finite quartiles (a real `middle`);
        // an outlier row is the NaN-`middle` sentinel the summary appends under the
        // Tukey rule — drawn as a point, not part of a box. Box rows still arrive as
        // consecutive (low, high) pairs even when outliers sit between groups,
        // because filtering the sentinels out leaves the pairs intact.
        let box_rows: Vec<usize> = (0..n).filter(|&i| pos_vals[i].is_finite() && middle[i].is_finite()).collect();
        let outlier_rows: Vec<usize> = (0..n)
            .filter(|&i| pos_vals[i].is_finite() && ext_vals[i].is_finite() && middle[i].is_nan())
            .collect();

        // A box turns translucent only when two actually *share* a slot — i.e.
        // `color` splits one group into several boxes that overlap. `dodge` resolves
        // that by setting them side by side, so a dodged box draws solid; only a true
        // overlay needs the see-through fill. Colored-but-not-split (one box per
        // slot, each its own hue) does not overlap either. Mirrors `write_bars`' test.
        let overlaid = dodge.is_none() && {
            let bp: Vec<f64> = box_rows.chunks_exact(2).map(|p| pos_vals[p[0]]).collect();
            bp.iter().enumerate().any(|(k, &p)| bp[..k].iter().any(|&q| (q - p).abs() < 1e-9))
        };
        let fill_o = st.opacity.unwrap_or(if overlaid { OVERLAY_FILL } else { OPACITY_DEFAULT });
        let outlier_o = st.opacity.unwrap_or(0.85);
        let stroke_w = st.size.unwrap_or(1.5);
        // The box is a closed-glyph fill (spec §4): `border_color` paints its
        // line-work — outline, whiskers, median, caps — distinct from the fill, and
        // `border_size` sets that line-work's width (else `size`, the general name).
        let line_w = st.border_size.unwrap_or(stroke_w);
        let border_color = st.border_color.as_deref();

        // The fill color of a box: a set color wins, else the group's hue, else the
        // default. Its line-work takes `border_color` when set, else the fill color.
        let color_at = |i: usize| -> &str {
            if let Some(sc) = &set_color { sc }
            else if let Some(gv) = &group_vals {
                gv.get(i).and_then(|g| color_map.get(*g)).map(String::as_str).unwrap_or(PALETTE_GOG[0])
            } else { PALETTE_GOG[0] }
        };

        // ---- polar: the same box, in scale units instead of pixels -------------
        // The extent axis, for normalizing. `uv` is the only place that knows which
        // of (slot, extent) is the angle: bent upright, the slot rides the angle and
        // the box is a wedge; lying down, the slot rides the radius and the box is a
        // band of the ring. Both are sectors, which is why one expression covers the
        // pair here exactly as the bounding box does flat.
        let ext_scale = if horizontal { xs } else { ys };
        let uv = |s: f64, e: f64| -> (f64, f64) { if horizontal { (e, s) } else { (s, e) } };
        // One **held** stroke, from one (slot, extent) pair to another. Held is not
        // read off the endpoints — every stroke here asserts its value across the
        // whole segment (see the `polar` parameter) — so the only question is which
        // way the segment runs: round the angle, where holding a value is an arc, or
        // out from the center, where it is exactly the radius.
        let held = |p: &Polar, a: (f64, f64), b: (f64, f64)| -> String {
            let (u0, v0) = uv(a.0, a.1);
            let (u1, v1) = uv(b.0, b.1);
            let mut d = String::new();
            p.move_to(&mut d, u0, v0);
            if (u1 - u0).abs() > 1e-12 { p.hold_to(&mut d, u0, u1, v0); }
            else { p.line_to(&mut d, u1, v1); }
            d
        };

        writeln!(svg, r##"  <g clip-path="url(#{clip})">"##).unwrap();
        // One box per (low, high) pair of box rows. `chunks_exact` drops any lone
        // trailing box row, which cannot form a box.
        for pair in box_rows.chunks_exact(2) {
            let (i, j) = (pair[0], pair[1]);
            let (w_lo, w_hi, q1, med, q3) = (ext_vals[i], ext_vals[j], lower[i], middle[i], upper[i]);
            if ![w_lo, w_hi, q1, med, q3].iter().all(|v| v.is_finite()) { continue; }
            let fill_color = color_at(i);
            let line = border_color.unwrap_or(fill_color); // outline, whiskers, median
            // The slot center, in pixels, with the dodge offset already in it.
            let pp = if horizontal { l.map_y(pos_vals[i], ys.0, ys.1) } else { l.map_x(pos_vals[i], xs.0, xs.1) }
                + dodge.as_ref().map_or(0.0, |d| d.offset_at(i, slot));
            let p_wlo = at(pp, w_lo);
            let p_q1  = at(pp, q1);
            let p_med = at(pp, med);
            let p_q3  = at(pp, q3);
            let p_whi = at(pp, w_hi);

            // The box: lower quartile to upper, `box_w` across the slot. A zero-IQR
            // group (every value equal) collapses to a flat line, which is honest.
            // The two ends are already screen points, so the rectangle is their
            // bounding box grown by half a slot the other way — one expression for
            // both orientations rather than a mirrored copy.
            let (bx, by, bw, bh) = if horizontal {
                (p_q1.0.min(p_q3.0), pp - half, (p_q3.0 - p_q1.0).abs(), box_w)
            } else {
                (pp - half, p_q3.1.min(p_q1.1), box_w, (p_q1.1 - p_q3.1).abs())
            };
            // Texture the body fill only; `line` (the outline) keeps the box legible.
            // The channel picks the texture by this box's category; else the setting.
            let texture = pattern_map.as_ref().map(|pm| pm.fill_texture(pm.cat_at(i))).or(st.pattern.as_deref());
            let fill = tex.fill(svg, texture, fill_color);

            if let Some(p) = polar {
                // The slot center and the five summary values, all in scale units —
                // the dodge offset is already in those units (see `slot` above), so
                // it rides along unchanged.
                let s_units = pos_vals[i] + dodge.as_ref().map_or(0.0, |d| d.offset_at(i, slot));
                let s_c = unit_norm(s_units, pos_scale);
                let half_u = half / pos_span;
                let (e_wlo, e_q1, e_med, e_q3, e_whi) = (
                    unit_norm(w_lo, ext_scale), unit_norm(q1, ext_scale),
                    unit_norm(med, ext_scale), unit_norm(q3, ext_scale),
                    unit_norm(w_hi, ext_scale),
                );
                let (a_u, a_v) = uv(s_c - half_u, e_q1);
                let (b_u, b_v) = uv(s_c + half_u, e_q3);
                let body = p.sector(a_u, b_u, a_v, b_v);
                if !(body.contains("NaN") || body.contains("inf")) {
                    writeln!(svg,
                        r##"    <path d="{body}" fill="{fill}" fill-opacity="{fill_o:.3}" stroke="{line}" stroke-width="{line_w}"/>"##
                    ).unwrap();
                }
                // The median, then each whisker with its cap — the same three
                // strokes as below, each held rather than interpolated.
                let median = held(p, (s_c - half_u, e_med), (s_c + half_u, e_med));
                writeln!(svg,
                    r##"    <path d="{median}" fill="none" stroke="{line}" stroke-width="{:.2}" stroke-linecap="butt"/>"##,
                    line_w + 1.0
                ).unwrap();
                // **How wide a cap is when it is bent**, and the answer is
                // `interval`'s because a cap is the same thing in both marks: a
                // stroke ornament saying where the span stops, whose width carries
                // no quantity — §18's rule that a stroke's width is pixels. Flat
                // that reads as *half the box's width*, and the polar reading which
                // keeps that true is half the box's width **as drawn**, measured
                // once at the box's own mid-radius and then spent as pixels at
                // whatever radius each cap sits at.
                //
                // Holding the *angle* instead was tried and measured: it gives one
                // constant 11.16° cap whose ink runs 5.65px to 44.94px inside a
                // single plot, because a cap is drawn at the whisker's radius and
                // not at the box's — so the relationship it was meant to preserve
                // is not preserved by the angle either, and the far caps swamp the
                // boxes. One rule for both marks; no per-mark exception (Law 2).
                let r_mid = p.radius((e_q1 + e_q3) / 2.0);
                let cap_px = half_u * std::f64::consts::TAU * r_mid * 0.5;
                for (from_e, to_e) in [(e_q3, e_whi), (e_q1, e_wlo)] {
                    let whisker = held(p, (s_c, from_e), (s_c, to_e));
                    writeln!(svg,
                        r##"    <path d="{whisker}" fill="none" stroke="{line}" stroke-width="{line_w}" stroke-linecap="butt"/>"##
                    ).unwrap();
                    let cap_u = if horizontal { cap_px / p.r_max } else { p.px_as_turns(to_e, cap_px) };
                    let capd = held(p, (s_c - cap_u, to_e), (s_c + cap_u, to_e));
                    writeln!(svg,
                        r##"    <path d="{capd}" fill="none" stroke="{line}" stroke-width="{line_w}" stroke-linecap="round"/>"##
                    ).unwrap();
                }
                continue;
            }

            writeln!(svg,
                r##"    <rect x="{bx:.2}" y="{by:.2}" width="{bw:.2}" height="{bh:.2}" fill="{fill}" fill-opacity="{fill_o:.3}" stroke="{line}" stroke-width="{line_w}"/>"##
            ).unwrap();
            // The median: a full-width line across the box, drawn heavier than the
            // outline so it reads through the fill.
            let (ma, mb) = across(p_med, half);
            writeln!(svg,
                r##"    <line x1="{:.2}" y1="{:.2}" x2="{:.2}" y2="{:.2}" stroke="{line}" stroke-width="{:.2}" stroke-linecap="butt"/>"##,
                ma.0, ma.1, mb.0, mb.1, line_w + 1.0
            ).unwrap();
            // The whiskers: box out to the high whisker, box out to the low, capped.
            for (from, to) in [(p_q3, p_whi), (p_q1, p_wlo)] {
                writeln!(svg,
                    r##"    <line x1="{:.2}" y1="{:.2}" x2="{:.2}" y2="{:.2}" stroke="{line}" stroke-width="{line_w}" stroke-linecap="butt"/>"##,
                    from.0, from.1, to.0, to.1
                ).unwrap();
                let (ca, cb) = across(to, cap);
                writeln!(svg,
                    r##"    <line x1="{:.2}" y1="{:.2}" x2="{:.2}" y2="{:.2}" stroke="{line}" stroke-width="{line_w}" stroke-linecap="round"/>"##,
                    ca.0, ca.1, cb.0, cb.1
                ).unwrap();
            }
        }
        // Outliers: a small filled dot at each value past the fence, in the box's hue.
        // Smaller than a `point` glyph so it reads as a flagged extreme, not a datum.
        const OUTLIER_R: f64 = 2.4;
        for &o in &outlier_rows {
            let stroke = color_at(o);
            // A flagged extreme is a *place*, so in polar it goes through `place`
            // like every other glyph — the dodge offset is in scale units there and
            // pixels here, exactly as it is for the boxes above.
            let (px, py) = match polar {
                Some(_) => {
                    let s_units = pos_vals[o] + dodge.as_ref().map_or(0.0, |d| d.offset_at(o, slot));
                    let (ux, vy) = uv(s_units, ext_vals[o]);
                    super::place(l, polar, ux, vy, xs, ys)
                }
                None => {
                    let pp = if horizontal { l.map_y(pos_vals[o], ys.0, ys.1) } else { l.map_x(pos_vals[o], xs.0, xs.1) }
                        + dodge.as_ref().map_or(0.0, |d| d.offset_at(o, slot));
                    at(pp, ext_vals[o])
                }
            };
            writeln!(svg,
                r##"    <circle cx="{px:.2}" cy="{py:.2}" r="{OUTLIER_R}" fill="{stroke}" fill-opacity="{outlier_o:.3}"/>"##
            ).unwrap();
        }
        writeln!(svg, "  </g>").unwrap();
    }
}

impl SvgRenderer {
    // -----------------------------------------------------------------------
    // Mark: box, in the cube — the five-number summary standing on a cell
    // -----------------------------------------------------------------------

    /// The five-number summary standing on a cell of the cube's floor (spec §15).
    ///
    /// `interval`'s derivation, for `interval`'s reason: the three slot marks are one
    /// family (`legality::is_slot_mark`), and giving their pair of axes a third turns
    /// the floor into cells with the measurement standing up on `z`. What differs is
    /// only how much of the span is drawn — a whisker is one stroke where this is a
    /// solid between the quartiles with two strokes reaching out to the extremes.
    ///
    /// **The body is `write_solid`**, the same routine a 3-D bar's column is, because
    /// they are the same shape and differ only in where their two ends come from.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn write_box_3d(
        &self, svg: &mut String, layer: &Layer, df: &DataFrame,
        xs: (f64, f64), ys: (f64, f64), zs: (f64, f64),
        x_field: &str, y_field: &str, z_field: &str,
        cat_x: Option<&[String]>, cat_y: Option<&[String]>,
        color_map: &HashMap<String, String>,
        clip: &str,
        scene: &Scene,
    ) {
        let Some(z_col) = df.float_col(z_field) else { return };
        let (Some(lower), Some(middle), Some(upper)) =
            (df.float_col("lower"), df.float_col("middle"), df.float_col("upper")) else { return };

        // `interval`'s slot fraction, for `interval`'s reason: a box needs the cell
        // it stands in to read as a place, and its whiskers need air beside it.
        const SLOT_FILL: f64 = 0.62;
        let Some((x0s, x1s)) = super::cell_edges(
            df, (crate::transform::CELL_START, crate::transform::CELL_END),
            x_field, cat_x, SLOT_FILL) else { return };
        let Some((y0s, y1s)) = super::cell_edges(
            df, (crate::transform::CELL_LOWER, crate::transform::CELL_UPPER),
            y_field, cat_y, SLOT_FILL) else { return };

        let n = z_col.len().min(x0s.len()).min(y0s.len());
        if n < 2 { return; }

        let st = &layer.style;
        let set_color = st.color.as_deref().map(esc);
        let color_labels = layer.encodings.get(&Channel::Color).and_then(|c| df.str_col(&c.field));
        // **Opaque, where a flat box is not**, and it is `bar`'s answer rather than a
        // second opinion: a solid in the cube is placed by painter's order, and a
        // translucent one lets the far box show through the near one and undoes the
        // depth sort that just ran. Flat, the translucency is there to keep *overlaid*
        // boxes legible — a problem the floor solves here by giving each its own cell.
        let fill_o = st.opacity.unwrap_or(1.0);
        let line_w = st.border_size.unwrap_or(st.size.unwrap_or(1.5));
        let border_color = st.border_color.as_deref();
        let base = unit_norm(0.0_f64.clamp(zs.0, zs.1), zs);

        // Box rows carry a finite `middle`; the NaN-`middle` sentinel marks an
        // outlier row, drawn as a dot. `interval`'s pairing, unchanged.
        let box_rows: Vec<usize> = (0..n).filter(|&i| middle[i].is_finite()).collect();
        let outlier_rows: Vec<usize> = (0..n)
            .filter(|&i| z_col[i].is_finite() && middle[i].is_nan()).collect();

        let order = super::floor_order(n, &x0s, &x1s, &y0s, &y1s, xs, ys, base, scene);
        let rank: std::collections::HashMap<usize, usize> =
            order.iter().enumerate().map(|(k, &i)| (i, k)).collect();
        let mut pairs: Vec<usize> = box_rows.chunks_exact(2).map(|p| p[0]).collect();
        pairs.sort_by_key(|i| rank.get(i).copied().unwrap_or(usize::MAX));

        writeln!(svg, r##"  <g clip-path="url(#{clip})">"##).unwrap();
        for i in pairs {
            let j = i + 1;
            if j >= n { continue; }
            let (w_lo, w_hi, q1, med, q3) = (z_col[i], z_col[j], lower[i], middle[i], upper[i]);
            if ![w_lo, w_hi, q1, med, q3].iter().all(|v| v.is_finite()) { continue; }
            let fill_color: &str = match (&set_color, color_labels) {
                (Some(c), _) => c,
                (None, Some(labels)) => labels.get(i).and_then(|l| color_map.get(l))
                    .map(String::as_str).unwrap_or(PALETTE_GOG[0]),
                _ => PALETTE_GOG[0],
            };
            let line = border_color.unwrap_or(fill_color);

            let (nx0, nx1) = (unit_norm(x0s[i], xs), unit_norm(x1s[i], xs));
            let (ny0, ny1) = (unit_norm(y0s[i], ys), unit_norm(y1s[i], ys));
            let (cx, cy) = ((nx0 + nx1) / 2.0, (ny0 + ny1) / 2.0);
            let (n_q1, n_q3) = (unit_norm(q1, zs), unit_norm(q3, zs));

            // The whiskers first, so the body paints over the half of each that runs
            // inside it — the flat plot's order, and the reason a whisker there is
            // drawn from the quartile rather than from the center.
            for (from, to) in [(q3, w_hi), (q1, w_lo)] {
                let a = scene.to_screen(cx, cy, unit_norm(from, zs));
                let b = scene.to_screen(cx, cy, unit_norm(to, zs));
                writeln!(svg,
                    r##"    <line x1="{:.2}" y1="{:.2}" x2="{:.2}" y2="{:.2}" stroke="{line}" stroke-width="{line_w}" stroke-linecap="butt"/>"##,
                    a.x, a.y, b.x, b.y
                ).unwrap();
                // The cap is a cross, for `write_interval_3d`'s reason: a cube leaves
                // two directions across the span and choosing one would be arbitrary.
                let nz = unit_norm(to, zs);
                for (p, q) in [
                    (scene.to_screen(nx0, cy, nz), scene.to_screen(nx1, cy, nz)),
                    (scene.to_screen(cx, ny0, nz), scene.to_screen(cx, ny1, nz)),
                ] {
                    writeln!(svg,
                        r##"    <line x1="{:.2}" y1="{:.2}" x2="{:.2}" y2="{:.2}" stroke="{line}" stroke-width="{line_w}" stroke-linecap="round"/>"##,
                        p.x, p.y, q.x, q.y
                    ).unwrap();
                }
            }

            // The body: the same solid a 3-D bar draws, between the quartiles.
            super::write_solid(svg, scene, (nx0, nx1), (ny0, ny1), (n_q1, n_q3), fill_color, fill_o);

            // The median is a **plane through the box**, and what is visible of it is
            // where that plane meets the box's *front-facing* sides. Drawing the whole
            // quad would put its two far edges on top of an opaque solid they are
            // inside — a line where the reader cannot see, which is the same fault
            // back-face culling exists to prevent in `write_solid`, so it is the same
            // test: a side is facing us when its projected winding is negative.
            let nz_med = unit_norm(med, zs);
            let corner = |k: usize| -> (f64, f64) {
                [(nx0, ny0), (nx1, ny0), (nx1, ny1), (nx0, ny1)][k]
            };
            // **A degenerate box is the exception, and it restores the flat rule.**
            // A group whose quartiles coincide (one observation, or every value
            // equal) has a box of no height, which flat "collapses to a flat line,
            // which is honest". Here its four sides have zero area, so the winding
            // test below culls every one of them and the median would vanish
            // entirely — a mark drawing *less* in the cube than on the page, which is
            // the Law-2 gap. What is actually visible of a zero-height box is the
            // whole quad, seen face-on from above, so all four edges are drawn.
            let flat_box = (n_q3 - n_q1).abs() < 1e-9;
            for k in 0..4 {
                let (a, b) = (corner(k), corner((k + 1) % 4));
                // The side face this median edge lies on, as its four corners: the
                // two floor corners and the two at the box's top.
                let f = [
                    scene.to_screen(a.0, a.1, n_q1.min(n_q3)),
                    scene.to_screen(b.0, b.1, n_q1.min(n_q3)),
                    scene.to_screen(b.0, b.1, n_q1.max(n_q3)),
                    scene.to_screen(a.0, a.1, n_q1.max(n_q3)),
                ];
                let area: f64 = (0..4).map(|i| {
                    let (p, q) = (&f[i], &f[(i + 1) % 4]);
                    p.x * q.y - q.x * p.y
                }).sum();
                // SVG's y grows downward, so a face turned toward us has negative
                // signed area — `write_solid`'s rule, read for one edge rather than
                // for a whole polygon.
                if !flat_box && area >= 0.0 { continue; }
                let (p, q) = (scene.to_screen(a.0, a.1, nz_med), scene.to_screen(b.0, b.1, nz_med));
                writeln!(svg,
                    r##"    <line x1="{:.2}" y1="{:.2}" x2="{:.2}" y2="{:.2}" stroke="{line}" stroke-width="{:.2}" stroke-linecap="butt"/>"##,
                    p.x, p.y, q.x, q.y, line_w + 1.0
                ).unwrap();
            }
        }

        // Outliers: a small dot at each flagged extreme, standing at its cell's center.
        const OUTLIER_R: f64 = 2.4;
        let outlier_o = st.opacity.unwrap_or(0.85);
        for &o in &outlier_rows {
            if !z_col[o].is_finite() { continue; }
            let stroke: &str = match (&set_color, color_labels) {
                (Some(c), _) => c,
                (None, Some(labels)) => labels.get(o).and_then(|l| color_map.get(l))
                    .map(String::as_str).unwrap_or(PALETTE_GOG[0]),
                _ => PALETTE_GOG[0],
            };
            let cx = unit_norm((x0s[o] + x1s[o]) / 2.0, xs);
            let cy = unit_norm((y0s[o] + y1s[o]) / 2.0, ys);
            let p = scene.to_screen(cx, cy, unit_norm(z_col[o], zs));
            writeln!(svg,
                r##"    <circle cx="{:.2}" cy="{:.2}" r="{OUTLIER_R}" fill="{stroke}" fill-opacity="{outlier_o:.3}"/>"##,
                p.x, p.y
            ).unwrap();
        }
        writeln!(svg, "  </g>").unwrap();
    }
}
