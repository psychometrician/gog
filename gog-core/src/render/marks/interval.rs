//! The `interval` mark — a low→high whisker at each x (error bars, ranges).
use std::collections::HashMap;
use std::fmt::Write;
use crate::data::DataFrame;
use crate::ir::{Channel, Layer};
use crate::render::palette::PALETTE_GOG;
use crate::render::pattern::{pattern_dasharray, PatternMap};
use crate::render::polar::Polar;
use crate::render::project::Scene;
use crate::render::svg::{unit_norm, SvgRenderer};
use crate::render::text::esc;
use crate::render::Layout;
use super::{bar_thickness_svg, Dodge};

impl SvgRenderer {
    // -----------------------------------------------------------------------
    // Mark: interval — a low→high whisker at each x (error bars, ranges)
    // -----------------------------------------------------------------------

    /// Draws a whisker from the low extent to the high one at each slot, capped at
    /// both ends. The extents arrive as *consecutive rows* — the `range` transform
    /// emits (low, high) per group — so the mark reads the frame two rows at a
    /// time, pairing them back into one span. That two-row encoding is what lets
    /// the measured axis include both extents with no special plumbing
    /// (`build_axis` already read them out of the field).
    ///
    /// **Upright or lying down.** Like `bar`, an interval stands in a slot on one
    /// axis and spans along the other, and which axis is which is read off the
    /// bindings (`legality::slot_orient`) — `interval * range + x(dept) + y(pay)`
    /// is a column of whiskers, `+ x(pay) + y(dept)` the horizontal error bar. So
    /// everything below is written once in terms of *slot* and *extent*, and only
    /// the two closures that reach the page know which of those is x.
    ///
    /// One whisker color, settable via `style()`; a `color` *channel* that
    /// splits into grouped intervals is the follow-up, not yet drawn — which is
    /// why `rule_for` keeps `color` set-only here.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn write_interval(
        &self, svg: &mut String, layer: &Layer, df: &DataFrame,
        l: &Layout, xs: (f64, f64), ys: (f64, f64),
        x_field: &str, y_field: &str,
        cat_x: Option<&[String]>, cat_y: Option<&[String]>,
        horizontal: bool,
        color_map: &HashMap<String, String>,
        clip: &str,
        // Polar: the span runs out from the center (a straight radius, exactly) and
        // the caps run across the slot (an arc, since a cap **holds** its value
        // across the whole slot rather than interpolating to it). The cap is where
        // this mark had to take the decision `box` did not: see `cap_turns` below.
        polar: Option<&Polar>,
    ) {
        // The two axes by role. The slot axis goes through the one position
        // resolution every mark shares, so a category there is no per-mark
        // exception; the extent axis is the numeric column the pair transform
        // wrote, and goes through the same resolver for symmetry.
        let (pos_field, ext_field) = if horizontal { (y_field, x_field) } else { (x_field, y_field) };
        let (pos_cats, ext_cats) = if horizontal { (cat_y, cat_x) } else { (cat_x, cat_y) };
        let Some(pos_vals) = super::positions(df, pos_field, pos_cats) else { return };
        let Some(ext_vals) = super::positions(df, ext_field, ext_cats) else { return };

        let n = pos_vals.len().min(ext_vals.len());
        if n < 2 { return; }

        // Where a (slot, extent) pair lands. The slot arrives already in pixels,
        // because the dodge offset is applied there; the extent is mapped through
        // its scale. The only two places that know which axis is which.
        let at = |pos_px: f64, ext: f64| -> (f64, f64) {
            if horizontal { (l.map_x(ext, xs.0, xs.1), pos_px) }
            else          { (pos_px, l.map_y(ext, ys.0, ys.1)) }
        };
        // An end cap runs *across* the slot, so it is perpendicular to the span
        // in whichever direction the span runs.
        let cap_ends = |p: (f64, f64), half: f64| -> ((f64, f64), (f64, f64)) {
            if horizontal { ((p.0, p.1 - half), (p.0, p.1 + half)) }
            else          { ((p.0 - half, p.1), (p.0 + half, p.1)) }
        };

        // A whisker is one stroke. `color` splits it into one hue per group (the
        // discrete split `line` makes): a set color overrides, otherwise the hue
        // comes from the bound `color` column via the shared map, else the
        // default. Width and opacity stay settings — one stroke, one of each.
        let st = &layer.style;
        let set_color = st.color.as_deref().map(esc);
        let color_field = layer.encodings.get(&Channel::Color).map(|c| c.field.as_str());
        let group_vals: Option<Vec<&str>> = color_field
            .and_then(|f| df.str_col(f))
            .map(|v| v.iter().map(String::as_str).collect());
        let stroke_w = st.size.unwrap_or(1.5);
        let stroke_o = st.opacity.unwrap_or(1.0);
        // A whisker is a path stroke (spec §4), so it takes `dash` like `line`/`step`
        // — a dashed error bar. On the span only: the short caps and the center dot
        // read as noise if dashed.
        let dash_attr = pattern_dasharray(st.pattern.as_deref());
        // A mapped `pattern` dashes each whisker by its category (spec §5) — the
        // per-pair analog of the split `line`/`bar` make; else the setting's dash.
        let pattern_map = PatternMap::resolve(layer, df);
        let draw_caps = st.caps.unwrap_or(true); // false → a bare linerange
        let draw_center = st.center.unwrap_or(true); // false → suppress the pointrange dot
        const CAP: f64 = 4.0; // half-width of the end caps, in px

        // Dodge sets grouped whiskers side by side within each slot (§5). A
        // whisker has no width to narrow, only a position to offset; the end caps
        // shrink to the sub-slot so adjacent groups' caps do not run together.
        // The slot is measured along whichever axis carries it.
        let (pos_px, pos_scale) = if horizontal { (l.h(), ys) } else { (l.w(), xs) };
        let dodge = Dodge::resolve(layer, df);
        // `bar`'s rule: pixels flat, a fraction of the turn bent, with the dodge
        // offsets inheriting whichever it is.
        let pos_span = (pos_scale.1 - pos_scale.0).max(1e-12);
        let slot_px = if dodge.is_some() { bar_thickness_svg(&pos_vals, n, pos_px, pos_scale, false) } else { 0.0 };
        let slot = match polar {
            None => slot_px,
            Some(_) => slot_px * pos_span / pos_px,
        };
        let cap_w = match &dodge {
            Some(d) => (slot_px * d.width_frac() * 0.4).min(CAP),
            None => CAP,
        };

        // A center value, when the transform supplies one (a CI's mean): the
        // interval becomes a pointrange. `range` writes no `center` column, so
        // there is no dot — the presence of a center is the statistic's call.
        // `style(center = FALSE)` then *hides* an existing one (a bare error bar).
        let centers = if draw_center { df.float_col("center") } else { None };

        // ---- polar: the same whisker, in scale units instead of pixels ---------
        let ext_scale = if horizontal { xs } else { ys };
        let uv = |s: f64, e: f64| -> (f64, f64) { if horizontal { (e, s) } else { (s, e) } };
        // One **held** stroke between two (slot, extent) pairs — `box`'s helper,
        // and for `box`'s reason: a whisker asserts its reach across the span and a
        // cap asserts where that reach stops, so neither interpolates.
        let held = |p: &Polar, a: (f64, f64), b: (f64, f64)| -> String {
            let (u0, v0) = uv(a.0, a.1);
            let (u1, v1) = uv(b.0, b.1);
            let mut d = String::new();
            p.move_to(&mut d, u0, v0);
            if (u1 - u0).abs() > 1e-12 { p.hold_to(&mut d, u0, u1, v0); }
            else { p.line_to(&mut d, u1, v1); }
            d
        };
        // **How wide a cap is when it is bent.** Flat it is 4 pixels, and it stays 4
        // pixels here: a cap is a stroke ornament marking where the span ends, and
        // its width carries no quantity — which is §18's standing rule that a
        // stroke's width is pixels, the same one that refuses `rule` a thickness in
        // data units. So the *angular* width varies with the radius the cap sits at,
        // and the two ends of one whisker subtend different angles while drawing the
        // same length of ink. (`box` parts company here on purpose: its cap is a
        // fraction of its box, so it stays angular and the glyph holds together.)
        // Upright the cap runs round the angle, lying down it runs out along the
        // radius, and only the second is a plain fraction of the panel.
        let cap_turns = |p: &Polar, e: f64| -> f64 {
            if horizontal { cap_w / p.r_max } else { p.px_as_turns(unit_norm(e, ext_scale), cap_w) }
        };

        writeln!(svg, r##"  <g clip-path="url(#{clip})">"##).unwrap();
        // Rows pair up (low, high). A trailing odd row (which `range` never
        // emits) is ignored rather than drawn as a half-interval.
        let mut i = 0;
        while i + 1 < n {
            // Both rows of a pair share one slot; read it from the low row.
            if !(pos_vals[i].is_finite() && ext_vals[i].is_finite() && ext_vals[i + 1].is_finite()) {
                i += 2;
                continue;
            }
            // This pair's color: a set color wins, else the group's hue.
            let stroke: &str = if let Some(sc) = &set_color {
                sc
            } else if let Some(gv) = &group_vals {
                gv.get(i).and_then(|g| color_map.get(*g)).map(String::as_str).unwrap_or(PALETTE_GOG[0])
            } else {
                PALETTE_GOG[0]
            };
            // The slot center, in pixels, with the dodge offset already in it.
            let pp = if horizontal { l.map_y(pos_vals[i], ys.0, ys.1) } else { l.map_x(pos_vals[i], xs.0, xs.1) }
                + dodge.as_ref().map_or(0.0, |d| d.offset_at(i, slot));
            let lo = at(pp, ext_vals[i]);
            let hi = at(pp, ext_vals[i + 1]);
            let dash = pattern_map.as_ref().map(|pm| pattern_dasharray(Some(pm.dash(pm.cat_at(i))))).unwrap_or(dash_attr);

            if let Some(p) = polar {
                // The slot center in scale units — the dodge offset is already in
                // those units (see `slot` above), so it rides along unchanged.
                let s_c = unit_norm(
                    pos_vals[i] + dodge.as_ref().map_or(0.0, |d| d.offset_at(i, slot)),
                    pos_scale,
                );
                let (e_lo, e_hi) = (ext_vals[i], ext_vals[i + 1]);
                let (n_lo, n_hi) = (unit_norm(e_lo, ext_scale), unit_norm(e_hi, ext_scale));
                let span = held(p, (s_c, n_lo), (s_c, n_hi));
                writeln!(svg,
                    r##"    <path d="{span}" fill="none" stroke="{stroke}" stroke-width="{stroke_w}" stroke-opacity="{stroke_o:.3}"{dash} stroke-linecap="round"/>"##
                ).unwrap();
                if draw_caps {
                    for (e, ne) in [(e_lo, n_lo), (e_hi, n_hi)] {
                        let h = cap_turns(p, e);
                        let capd = held(p, (s_c - h, ne), (s_c + h, ne));
                        writeln!(svg,
                            r##"    <path d="{capd}" fill="none" stroke="{stroke}" stroke-width="{stroke_w}" stroke-opacity="{stroke_o:.3}" stroke-linecap="round"/>"##
                        ).unwrap();
                    }
                }
                if let Some(cs) = centers {
                    if cs.get(i).is_some_and(|c| c.is_finite()) {
                        let (cu, cv) = uv(s_c, unit_norm(cs[i], ext_scale));
                        let (cx, cy) = p.at(cu, cv);
                        writeln!(svg,
                            r##"    <circle cx="{cx:.2}" cy="{cy:.2}" r="{:.2}" fill="{stroke}" fill-opacity="{stroke_o:.3}"/>"##,
                            self.point_radius
                        ).unwrap();
                    }
                }
                i += 2;
                continue;
            }

            writeln!(svg,
                r##"    <line x1="{:.2}" y1="{:.2}" x2="{:.2}" y2="{:.2}" stroke="{stroke}" stroke-width="{stroke_w}" stroke-opacity="{stroke_o:.3}"{dash} stroke-linecap="round"/>"##,
                lo.0, lo.1, hi.0, hi.1
            ).unwrap();
            if draw_caps {
                for end in [lo, hi] {
                    let (a, b) = cap_ends(end, cap_w);
                    writeln!(svg,
                        r##"    <line x1="{:.2}" y1="{:.2}" x2="{:.2}" y2="{:.2}" stroke="{stroke}" stroke-width="{stroke_w}" stroke-opacity="{stroke_o:.3}" stroke-linecap="round"/>"##,
                        a.0, a.1, b.0, b.1
                    ).unwrap();
                }
            }
            if let Some(cs) = centers {
                if cs.get(i).is_some_and(|c| c.is_finite()) {
                    let c = at(pp, cs[i]);
                    writeln!(svg,
                        r##"    <circle cx="{:.2}" cy="{:.2}" r="{:.2}" fill="{stroke}" fill-opacity="{stroke_o:.3}"/>"##,
                        c.0, c.1, self.point_radius
                    ).unwrap();
                }
            }
            i += 2;
        }
        writeln!(svg, "  </g>").unwrap();
    }
}

impl SvgRenderer {
    // -----------------------------------------------------------------------
    // Mark: interval, in the cube — the whisker standing on a cell
    // -----------------------------------------------------------------------

    /// A low→high span standing on a cell of the cube's floor (spec §15).
    ///
    /// **It needed no ruling of its own.** `legality::is_slot_mark` has grouped
    /// `bar`, `box` and `interval` since orientation was decided, and `slot_orient`
    /// says outright that "a bar's length, a whisker's span and a box's summary are
    /// the same question asked of the same pair of axes". Give that pair a third
    /// axis and §5's dimensionality subtraction — *the positions the space offers,
    /// less the one the mark measures along* — cuts the floor into cells and stands
    /// the span up on `z`, exactly as it does for the 3-D histogram. The only thing
    /// that differs from a bar is where the two ends come from: a baseline and a
    /// value there, a low/high pair here.
    ///
    /// The footprint, the paint order and the cell arithmetic are all shared with
    /// `bar` (`cell_edges`, `floor_order`), so one floor cannot be computed two ways.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn write_interval_3d(
        &self, svg: &mut String, layer: &Layer, df: &DataFrame,
        xs: (f64, f64), ys: (f64, f64), zs: (f64, f64),
        x_field: &str, y_field: &str, z_field: &str,
        cat_x: Option<&[String]>, cat_y: Option<&[String]>,
        color_map: &HashMap<String, String>,
        clip: &str,
        scene: &Scene,
    ) {
        let Some(z_col) = df.float_col(z_field) else { return };
        // A whisker leaves more air round it than a bar does: a bar's neighbors
        // abut and read as a surface, where a whisker is a *stroke* and needs the
        // cell it stands in to be visible as a place rather than as a tile.
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
        let stroke_w = st.size.unwrap_or(1.5);
        let stroke_o = st.opacity.unwrap_or(1.0);
        let dash_attr = pattern_dasharray(st.pattern.as_deref());
        let draw_caps = st.caps.unwrap_or(true);
        let draw_center = st.center.unwrap_or(true);
        let centers = if draw_center { df.float_col("center") } else { None };
        let base = unit_norm(0.0_f64.clamp(zs.0, zs.1), zs);

        // Rows pair up (low, high), and both rows of a pair share one cell — so the
        // pair is sorted by the *low* row's footprint, which is the same footprint.
        let pairs: Vec<usize> = (0..n).step_by(2).filter(|&i| i + 1 < n).collect();
        let order = super::floor_order(n, &x0s, &x1s, &y0s, &y1s, xs, ys, base, scene);
        let rank: std::collections::HashMap<usize, usize> =
            order.iter().enumerate().map(|(k, &i)| (i, k)).collect();
        let mut drawn = pairs.clone();
        drawn.sort_by_key(|i| rank.get(i).copied().unwrap_or(usize::MAX));

        writeln!(svg, r##"  <g clip-path="url(#{clip})">"##).unwrap();
        for i in drawn {
            if !(z_col[i].is_finite() && z_col[i + 1].is_finite()) { continue; }
            let stroke: &str = match (&set_color, color_labels) {
                (Some(c), _) => c,
                (None, Some(labels)) => labels.get(i).and_then(|l| color_map.get(l))
                    .map(String::as_str).unwrap_or(PALETTE_GOG[0]),
                _ => PALETTE_GOG[0],
            };
            // The cell's center, and the two ends of the span on `z`.
            let cx = unit_norm((x0s[i] + x1s[i]) / 2.0, xs);
            let cy = unit_norm((y0s[i] + y1s[i]) / 2.0, ys);
            let (nz_lo, nz_hi) = (unit_norm(z_col[i], zs), unit_norm(z_col[i + 1], zs));
            if ![cx, cy, nz_lo, nz_hi].iter().all(|v| v.is_finite()) { continue; }

            let a = scene.to_screen(cx, cy, nz_lo);
            let b = scene.to_screen(cx, cy, nz_hi);
            writeln!(svg,
                r##"    <line x1="{:.2}" y1="{:.2}" x2="{:.2}" y2="{:.2}" stroke="{stroke}" stroke-width="{stroke_w}" stroke-opacity="{stroke_o:.3}"{dash_attr} stroke-linecap="round"/>"##,
                a.x, a.y, b.x, b.y
            ).unwrap();

            // **A cap in the cube is a cross, and that is not a decoration.** Flat, a
            // cap runs across the slot — perpendicular to the span, in the one
            // direction left. A cube leaves *two*, and picking one of them would be
            // an arbitrary choice the reader would then have to un-read as meaning
            // something. Both, drawn to the cell's own extent, say the same thing the
            // flat cap says (here is where the span stops) and say it symmetrically.
            if draw_caps {
                for &nz in &[nz_lo, nz_hi] {
                    for (p, q) in [
                        (scene.to_screen(unit_norm(x0s[i], xs), cy, nz),
                         scene.to_screen(unit_norm(x1s[i], xs), cy, nz)),
                        (scene.to_screen(cx, unit_norm(y0s[i], ys), nz),
                         scene.to_screen(cx, unit_norm(y1s[i], ys), nz)),
                    ] {
                        writeln!(svg,
                            r##"    <line x1="{:.2}" y1="{:.2}" x2="{:.2}" y2="{:.2}" stroke="{stroke}" stroke-width="{stroke_w}" stroke-opacity="{stroke_o:.3}" stroke-linecap="round"/>"##,
                            p.x, p.y, q.x, q.y
                        ).unwrap();
                    }
                }
            }
            // A center value, when the transform supplies one — the pointrange's dot.
            if let Some(cs) = centers {
                if cs.get(i).is_some_and(|c| c.is_finite()) {
                    let c = scene.to_screen(cx, cy, unit_norm(cs[i], zs));
                    writeln!(svg,
                        r##"    <circle cx="{:.2}" cy="{:.2}" r="{:.2}" fill="{stroke}" fill-opacity="{stroke_o:.3}"/>"##,
                        c.x, c.y, self.point_radius
                    ).unwrap();
                }
            }
        }
        writeln!(svg, "  </g>").unwrap();
    }
}
