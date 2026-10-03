//! The flow diagram's two writers — the node slots and the bands between them.
//!
//! Neither is a mark of its own: `zone * flow` reads the slots and `ribbon *
//! flow` the bands, the way `area` and `ribbon` share the violin's reading of
//! `density` (spec §15, the flow entry). What is genuinely new here is the
//! geometry: the band is drawn as a cubic curve between its two end intervals,
//! the first curve command this renderer emits — every other path in the crate
//! is a polyline or a polygon. The curve is the mark's own convention, exactly
//! as a `step`'s corner is: the IR carries the two anchors and nothing else, so
//! any backend redraws the connection from them (Law 9).
//!
//! The slots are **thin on purpose**. A slot's width means nothing — the data
//! is the interval it spans on the measure axis — so, like a bar's arbitrary
//! thickness, it is a convention, and the thin one leaves the gap between
//! stages to the bands, which are the ink a reader follows.
use std::fmt::Write;
use crate::data::DataFrame;
use crate::ir::{Channel, Layer};
use crate::render::palette::PALETTE_GOG;
use crate::render::pattern::{FillTexture, PatternMap};
use crate::render::svg::SvgRenderer;
use crate::render::text::esc;
use crate::render::{Layout, Whole};
use crate::transform::{CELL_END, CELL_LOWER, CELL_START, CELL_UPPER, FLOW_PATH, FLOW_STAGE};

/// Half a slot's thickness, in category units — a stage sits at integer `k` and
/// its slots run `k ± this`. One constant, no knob: the width carries no data,
/// so a parameter would be a taste dial (§18's `tri` warning).
///
/// **Wide enough to hold a name** (0.12 since 2026-10-02, from 0.055). A slot is
/// where a reader finds which place a band passes through, and at 0.055 the names
/// a `text * flow` layer writes spilled across the bands on either side, so a
/// diagram with every slot named still read as unlabeled. Chosen by the author
/// from renders at 0.055, 0.12 and 0.17, the last being ggalluvial's default.
/// 0.12 holds "Female" at the book's size and leaves the bands room to curve.
const SLOT_HALF: f64 = 0.12;

/// A band's paint is deliberately translucent: bands cross, and a crossing two
/// opaque ribbons would hide is most of what an alluvial diagram shows.
const BAND_OPACITY: f64 = 0.45;

impl SvgRenderer {
    /// The node slots — one rectangle per (stage, category), spanning the
    /// interval the layout stacked for it. Stages run along `x` and the count up
    /// `y`, or, in a flow that runs top to bottom (`down`), stages down `y` and
    /// the count along `x`.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn write_flow_nodes(
        &self,
        svg: &mut String,
        layer: &Layer,
        df: &DataFrame,
        l: &Layout,
        xs: (f64, f64),
        ys: (f64, f64),
        cat_x: Option<&[String]>,
        cat_y: Option<&[String]>,
        down: bool,
        clip: &str,
    ) {
        let (lo_name, hi_name) = if down { (CELL_START, CELL_END) } else { (CELL_LOWER, CELL_UPPER) };
        let (Some(stage), Some(lo), Some(hi)) = (
            df.str_col(FLOW_STAGE), df.float_col(lo_name), df.float_col(hi_name),
        ) else {
            return;
        };
        let Some(cats) = (if down { cat_y } else { cat_x }) else { return };
        let st = &layer.style;
        let fill = st.color.as_deref().unwrap_or(PALETTE_GOG[0]);
        let opacity = st.opacity.unwrap_or(1.0);
        let edge = super::border_edge(st);
        writeln!(svg, r#"  <g clip-path="url(#{clip})">"#).unwrap();
        for r in 0..stage.len() {
            let Some(k) = cats.iter().position(|c| *c == stage[r]) else { continue };
            let (x0, x1, y0, y1) = if down {
                let (a, b) = (l.map_y(k as f64 - SLOT_HALF, ys.0, ys.1), l.map_y(k as f64 + SLOT_HALF, ys.0, ys.1));
                (l.map_x(lo[r], xs.0, xs.1), l.map_x(hi[r], xs.0, xs.1), a.min(b), a.max(b))
            } else {
                (l.map_x(k as f64 - SLOT_HALF, xs.0, xs.1), l.map_x(k as f64 + SLOT_HALF, xs.0, xs.1),
                 l.map_y(hi[r], ys.0, ys.1), l.map_y(lo[r], ys.0, ys.1))
            };
            writeln!(
                svg,
                r#"    <rect x="{:.2}" y="{:.2}" width="{:.2}" height="{:.2}" fill="{}" fill-opacity="{:.3}" {}/>"#,
                x0, y0, x1 - x0, y1 - y0, esc(fill), opacity, edge,
            ).unwrap();
        }
        writeln!(svg, "  </g>").unwrap();
    }

    /// The bands — one cubic-sided shape per adjacent pair of a path's rows,
    /// running from the path's interval at one stage to its interval at the
    /// next. The pairing is `interval`'s two-rows reading with a key: rows
    /// arrive path-major in stage order, and consecutive rows sharing
    /// [`FLOW_PATH`] are one band's two ends.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn write_flow_bands(
        &self,
        svg: &mut String,
        layer: &Layer,
        df: &DataFrame,
        whole: &Whole<'_>,
        l: &Layout,
        xs: (f64, f64),
        ys: (f64, f64),
        cat_x: Option<&[String]>,
        cat_y: Option<&[String]>,
        down: bool,
        color_map: &std::collections::HashMap<String, String>,
        clip: &str,
    ) {
        let (lo_name, hi_name) = if down { (CELL_START, CELL_END) } else { (CELL_LOWER, CELL_UPPER) };
        let (Some(path), Some(stage), Some(lo), Some(hi)) = (
            df.str_col(FLOW_PATH), df.str_col(FLOW_STAGE),
            df.float_col(lo_name), df.float_col(hi_name),
        ) else {
            return;
        };
        let Some(cats) = (if down { cat_y } else { cat_x }) else { return };
        let st = &layer.style;
        let opacity = st.opacity.unwrap_or(BAND_OPACITY);
        let hue_col = layer.encodings.get(&Channel::Color)
            .and_then(|def| df.str_col(&def.field));
        // A band is a fill, so `pattern` hatches it as it hatches every other fill
        // (the settable rule). The legend drew the hatch and the bands were solid:
        // `check_flow` took the channel and this writer had no texture code.
        let pattern_map = PatternMap::resolve(layer, df, whole);
        let mut tex = FillTexture::new();
        writeln!(svg, r#"  <g clip-path="url(#{clip})">"#).unwrap();
        for r in 0..path.len().saturating_sub(1) {
            if path[r + 1] != path[r] {
                continue;
            }
            let (Some(k0), Some(k1)) = (
                cats.iter().position(|c| *c == stage[r]),
                cats.iter().position(|c| *c == stage[r + 1]),
            ) else {
                continue;
            };
            // The band leaves the side of one slot that faces the next and enters
            // the facing side of the next, curving across the gap between them.
            // Down a page the stages are rows, so the same shape is drawn with its
            // two coordinates exchanged.
            let d = if down {
                let (c0, c1) = (l.map_y(k0 as f64, ys.0, ys.1), l.map_y(k1 as f64, ys.0, ys.1));
                let half = (l.map_y(k0 as f64 + SLOT_HALF, ys.0, ys.1) - c0).abs();
                let toward = if c1 >= c0 { 1.0 } else { -1.0 };
                let (y0, y1) = (c0 + toward * half, c1 - toward * half);
                let my = (y0 + y1) / 2.0;
                let a_lo = l.map_x(lo[r], xs.0, xs.1);
                let a_hi = l.map_x(hi[r], xs.0, xs.1);
                let b_lo = l.map_x(lo[r + 1], xs.0, xs.1);
                let b_hi = l.map_x(hi[r + 1], xs.0, xs.1);
                format!("M {a_lo:.2},{y0:.2} C {a_lo:.2},{my:.2} {b_lo:.2},{my:.2} {b_lo:.2},{y1:.2} \
                         L {b_hi:.2},{y1:.2} C {b_hi:.2},{my:.2} {a_hi:.2},{my:.2} {a_hi:.2},{y0:.2} Z")
            } else {
                let x0 = l.map_x(k0 as f64 + SLOT_HALF, xs.0, xs.1);
                let x1 = l.map_x(k1 as f64 - SLOT_HALF, xs.0, xs.1);
                let mx = (x0 + x1) / 2.0;
                let a_hi = l.map_y(hi[r], ys.0, ys.1);
                let a_lo = l.map_y(lo[r], ys.0, ys.1);
                let b_hi = l.map_y(hi[r + 1], ys.0, ys.1);
                let b_lo = l.map_y(lo[r + 1], ys.0, ys.1);
                format!("M {x0:.2},{a_hi:.2} C {mx:.2},{a_hi:.2} {mx:.2},{b_hi:.2} {x1:.2},{b_hi:.2} \
                         L {x1:.2},{b_lo:.2} C {mx:.2},{b_lo:.2} {mx:.2},{a_lo:.2} {x0:.2},{a_lo:.2} Z")
            };
            let fill = hue_col
                .and_then(|c| c.get(r))
                .and_then(|v| color_map.get(v))
                .map(|s| s.as_str())
                .or(st.color.as_deref())
                .unwrap_or(PALETTE_GOG[0]);
            let texture = pattern_map.as_ref().map(|pm| pm.fill_texture(pm.cat_at(r)))
                .or(st.pattern.as_deref());
            let fill = tex.fill(svg, texture, fill);
            writeln!(
                svg,
                r#"    <path d="{d}" fill="{}" fill-opacity="{opacity:.3}"/>"#,
                esc(&fill),
            ).unwrap();
        }
        writeln!(svg, "  </g>").unwrap();
    }
}
