//! The **violin** — a per-slot `density`, drawn as a width (spec §5).
//!
//! Not a mark of its own, which is the whole design: it is `ribbon`'s geometry and
//! `area`'s, fed by the slot reading of `density`. So this file holds one routine
//! that both marks call, differing by a single `bool` — where the region closes.
//! A `ribbon` closes on its own reflection (the violin), an `area` on the slot's
//! center line (the half violin), and those are the two marks' existing definitions
//! read against a slot instead of against an axis. `legality::slot_density` is the
//! authority on *whether* a layer is one of these; this file only draws it.
use std::collections::HashMap;
use std::fmt::Write;
use crate::data::DataFrame;
use crate::ir::{Channel, Layer, Mark};
use crate::render::palette::PALETTE_GOG;
use crate::render::encode::OPACITY_DEFAULT;
use crate::render::pattern::{pattern_dasharray, FillTexture, PatternMap};
use crate::render::polar::Polar;
use crate::render::svg::{SvgRenderer, OVERLAY_FILL};
use crate::render::text::esc;
use crate::render::Layout;
use crate::transform::SLOT_WIDTH;
use super::{bar_thickness_svg, Dodge};

/// What the mark does with the estimate once it has been laid across the slot.
///
/// Three shapes, one per mark, and each is that mark's own identity read against a
/// slot instead of an axis — which is why none of them needed a new name.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Slot {
    /// `ribbon` — closes on its own reflection, reaching both ways. The violin.
    Mirrored,
    /// `area` — closes on the slot's line, reaching one way. The half violin, and
    /// laid down, the ridgeline.
    Halved,
    /// `line`/`step` — closes on nothing, tracing the estimate as a stroke. The
    /// "filled, or two edges" rule (`line * bounds`) against a slot.
    Traced,
}

impl SvgRenderer {
    /// One filled shape per (slot, group): the density of the measure column within
    /// that category, laid across the category's slot.
    ///
    /// The estimate arrives in [`SLOT_WIDTH`] **unscaled** — a density in the
    /// measure column's own reciprocal units, which is not a number of pixels — so
    /// the mapping to the page happens here: divide by the largest value *anywhere
    /// in the frame*, and the fattest violin fills its slot while every other one
    /// keeps its true proportion to it. Taking the maximum over the whole frame
    /// rather than per violin is what makes the widths comparable at all, and it is
    /// why `transform::slot_density` deliberately leaves them unnormalized: a
    /// maximum taken inside the split would rescale each group against itself, so
    /// two groups of wildly different spread would come out looking alike.
    ///
    /// `density(compare = )` has already been spent by the time the numbers arrive
    /// here — it chose whether each group's estimate carries its row count — so this
    /// routine reads the same way whichever was asked for, and there is exactly one
    /// place that knows the difference.
    ///
    /// **Upright or lying down**, like `bar`, `box` and `interval`: the violins
    /// stand in slots on one axis and spread along the other, and the bindings say
    /// which (`legality::slot_density` returns the orientation it read). Everything
    /// below is written in terms of *slot* and *extent*; only `at` knows which is x.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn write_violin(
        &self, svg: &mut String, layer: &Layer, df: &DataFrame,
        l: &Layout, xs: (f64, f64), ys: (f64, f64),
        x_field: &str, y_field: &str,
        cat_x: Option<&[String]>, cat_y: Option<&[String]>,
        horizontal: bool,
        // Which of the three shapes this layer's mark makes of the estimate.
        shape: Slot,
        color_map: &HashMap<String, String>,
        clip: &str,
        polar: Option<&Polar>,
    ) {
        let (pos_field, ext_field) = if horizontal { (y_field, x_field) } else { (x_field, y_field) };
        let pos_cats = if horizontal { cat_y } else { cat_x };
        let Some(pos_vals) = super::positions(df, pos_field, pos_cats) else { return };
        let Some(ext_vals) = df.float_col(ext_field) else { return };
        let Some(widths) = df.float_col(SLOT_WIDTH) else { return };

        let n = pos_vals.len().min(ext_vals.len()).min(widths.len());
        if n < 2 { return }

        // The tallest estimate in the frame — the one that fills its slot. Everything
        // else is drawn as its fraction. A frame of zeros (a degenerate group) would
        // divide by nothing, so it draws nothing rather than a NaN polygon.
        let peak = widths.iter().take(n).cloned().filter(|v| v.is_finite())
            .fold(f64::NEG_INFINITY, f64::max);
        if !(peak > 0.0) { return }

        let st = &layer.style;
        let set_color = st.color.as_deref().map(esc);
        let mut tex = FillTexture::new();
        let pattern_map = PatternMap::resolve(layer, df);
        let color_field = layer.encodings.get(&Channel::Color).map(|c| c.field.as_str());
        // The split that makes two violins share one slot. `color` wins over `group`,
        // the precedence every other mark uses.
        let group_field = color_field
            .or_else(|| layer.encodings.get(&Channel::Group).map(|c| c.field.as_str()))
            .or_else(|| pattern_map.as_ref().map(|pm| pm.field()));
        let group_vals: Option<Vec<&str>> = group_field
            .and_then(|f| df.str_col(f))
            .map(|v| v.iter().map(String::as_str).collect());

        // The slot, in the position axis's own units — `write_box`'s conversion, and
        // for its reason: flat, a slot is a count of pixels; bent, it is a fraction of
        // the turn, and `place` below wants scale units in either space.
        let (pos_px, pos_scale) = if horizontal { (l.h(), ys) } else { (l.w(), xs) };
        let pos_span = (pos_scale.1 - pos_scale.0).max(1e-12);
        let slot_px = bar_thickness_svg(&pos_vals, n, pos_px, pos_scale, false);
        let slot_units = slot_px * pos_span / pos_px;
        let dodge = Dodge::resolve(layer, df);
        // How far the widest shape reaches **from** the slot's line, in slots — one
        // way for an `area` or a stroke, both ways for a `ribbon`. Measured one-way
        // rather than as a total so the number means the same thing to every shape,
        // which is what keeps a half violin exactly half of a violin at any `reach`.
        // Past 0.5 the shapes leave their slots and run into their neighbors, which
        // is the ridgeline being asked for rather than a mistake to guard against.
        //
        // **One slot is 1.0 here, not `slot_units`.** `bar_thickness_svg` returns four
        // fifths of the spacing — a categorical bar leaves a fifth empty, and that gap
        // is what says the categories are separate — so measuring `reach` against it
        // would make the number mean four fifths of what it says, and disagree with
        // `legality::slot_reach`, which grows the axis in whole categories. The slot
        // axis of a violin is always categorical (`slot_density` requires it), and
        // `positions` puts category *k* at *k*, so center-to-center is exactly 1.0 in
        // the scale units `place` wants, flat or bent. The dodge offset keeps
        // `slot_units`, since sharing a slot *is* the bar-thickness question.
        let reach = layer.density.as_ref().and_then(|d| d.reach)
            .filter(|r| r.is_finite() && *r > 0.0)
            .unwrap_or(crate::ir::DEFAULT_REACH);
        let half = dodge.as_ref().map_or(1.0, Dodge::width_frac) * reach;

        // Where a (slot, extent) pair lands, in whichever space and orientation.
        let at = |slot: f64, ext: f64| -> (f64, f64) {
            let (u, v) = if horizontal { (ext, slot) } else { (slot, ext) };
            super::place(l, polar, u, v, xs, ys)
        };

        // One violin is the rows of one slot in one group. The transform emits them
        // contiguously and in ascending order of the measure, so a run is exactly the
        // block between changes of (slot, group) — no sort, and the outline is traced
        // in the order the estimate was sampled.
        let key_at = |i: usize| (pos_vals[i].to_bits(), group_vals.as_ref().map(|g| g[i]));
        let mut runs: Vec<Vec<usize>> = Vec::new();
        for i in 0..n {
            if !(pos_vals[i].is_finite() && ext_vals[i].is_finite() && widths[i].is_finite()) { continue }
            match runs.last_mut() {
                Some(r) if key_at(*r.last().unwrap()) == key_at(i) => r.push(i),
                _ => runs.push(vec![i]),
            }
        }

        // Two violins in one slot overlap, so they draw translucent to stay legible —
        // `write_box`'s rule, and a `dodge` that sets them side by side takes it back.
        let overlaid = dodge.is_none() && runs.iter().enumerate().any(|(k, r)| {
            let p = pos_vals[r[0]];
            runs[..k].iter().any(|q| (pos_vals[q[0]] - p).abs() < 1e-9)
        });
        let fill_o = st.opacity.unwrap_or(if overlaid { OVERLAY_FILL } else { OPACITY_DEFAULT });

        writeln!(svg, r##"  <g clip-path="url(#{clip})">"##).unwrap();

        for run in &runs {
            if run.len() < 2 { continue }
            let i0 = run[0];
            let center = pos_vals[i0] + dodge.as_ref().map_or(0.0, |d| d.offset_at(i0, slot_units));
            let fill_color: &str = if let Some(sc) = &set_color { sc }
                else if let Some(gv) = &group_vals {
                    color_map.get(gv[i0]).map(String::as_str).unwrap_or(PALETTE_GOG[0])
                } else { PALETTE_GOG[0] };
            // The estimate itself, traced in sampling order at the offset the fills
            // use — so a stroke layered over a fill lands exactly on its edge, which
            // is the whole point of drawing the edge as a separate layer (`area +
            // line`) rather than as a border setting.
            let mut pts = String::with_capacity(run.len() * 96);
            for &i in run {
                let (px, py) = at(center + half * (widths[i] / peak), ext_vals[i]);
                let _ = write!(pts, "{px:.2},{py:.2} ");
            }
            if shape == Slot::Traced {
                if pts.contains("NaN") || pts.contains("inf") { continue }
                let stroke_w = st.size.unwrap_or(1.5);
                let stroke_o = st.opacity.unwrap_or(0.9);
                let dash = pattern_dasharray(st.pattern.as_deref());
                let stroke = if let Some(sc) = &set_color { sc.as_str() } else { fill_color };
                // A staircase, for a `step`, is the same vertices with the corners
                // squared — Law 2, and the mark's one difference from `line`
                // everywhere else in the engine.
                let d = if layer.mark == Mark::Step { stair(&pts) } else { format!("M{pts}") };
                let _ = writeln!(svg,
                    r#"    <path d="{d}" fill="none" stroke="{stroke}" stroke-width="{stroke_w}" stroke-opacity="{stroke_o:.3}"{dash}/>"#);
                continue;
            }

            let texture = pattern_map.as_ref()
                .map(|pm| pm.fill_texture(pm.cat_at(i0)))
                .or(st.pattern.as_deref());
            let fill = tex.fill(svg, texture, fill_color);

            // Back along the other side to close it — the reflection when mirrored,
            // the bare slot line when halved. One polygon either way, closed by SVG,
            // so the half violin needs no separate baseline walk the way `write_area`
            // does: its baseline *is* the return leg.
            for &i in run.iter().rev() {
                let off = if shape == Slot::Mirrored { -half * (widths[i] / peak) } else { 0.0 };
                let (px, py) = at(center + off, ext_vals[i]);
                let _ = write!(pts, "{px:.2},{py:.2} ");
            }
            if pts.contains("NaN") || pts.contains("inf") { continue }
            let _ = writeln!(svg,
                r#"    <polygon points="{pts}" fill="{fill}" fill-opacity="{fill_o:.3}"/>"#);
        }

        writeln!(svg, "  </g>").unwrap();
    }
}

/// Square the corners of a traced estimate — a `step`'s reading of the same
/// vertices (Law 2: the mark's one difference from `line`, wherever it draws).
///
/// Takes the polyline `write_violin` already built, so the two marks cannot drift
/// apart about *where* the estimate is; only about how the segments between its
/// points are drawn.
fn stair(pts: &str) -> String {
    let mut it = pts.split_whitespace().filter_map(|p| {
        let (a, b) = p.split_once(',')?;
        Some((a.parse::<f64>().ok()?, b.parse::<f64>().ok()?))
    });
    let Some((x0, y0)) = it.next() else { return String::new() };
    let mut d = format!("M{x0:.2},{y0:.2}");
    let mut prev = (x0, y0);
    for (x, y) in it {
        let _ = write!(d, " L{x:.2},{:.2} L{x:.2},{y:.2}", prev.1);
        prev = (x, y);
    }
    d
}
