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
use crate::render::svg::SvgRenderer;
use crate::render::text::esc;
use crate::render::Layout;
use crate::transform::{BAND_LOWER, BAND_UPPER, CELL_LOWER, CELL_UPPER, FLOW_STAGE};

/// Half a slot's thickness, in category units — a stage sits at integer `k` and
/// its slots run `k ± this`. One constant, no knob: the width carries no data,
/// so a parameter would be a taste dial (§18's `tri` warning).
const SLOT_HALF: f64 = 0.055;

/// A band's paint is deliberately translucent: bands cross, and a crossing two
/// opaque ribbons would hide is most of what an alluvial diagram shows.
const BAND_OPACITY: f64 = 0.45;

impl SvgRenderer {
    /// The node slots — one thin rectangle per (stage, category), spanning the
    /// interval the layout stacked for it.
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
        clip: &str,
    ) {
        let (Some(stage), Some(lo), Some(hi)) = (
            df.str_col(FLOW_STAGE), df.float_col(CELL_LOWER), df.float_col(CELL_UPPER),
        ) else {
            return;
        };
        let Some(cats) = cat_x else { return };
        let st = &layer.style;
        let fill = st.color.as_deref().unwrap_or(PALETTE_GOG[0]);
        let opacity = st.opacity.unwrap_or(1.0);
        let edge = super::border_edge(st);
        writeln!(svg, r#"  <g clip-path="url(#{clip})">"#).unwrap();
        for r in 0..stage.len() {
            let Some(k) = cats.iter().position(|c| *c == stage[r]) else { continue };
            let x0 = l.map_x(k as f64 - SLOT_HALF, xs.0, xs.1);
            let x1 = l.map_x(k as f64 + SLOT_HALF, xs.0, xs.1);
            let y0 = l.map_y(hi[r], ys.0, ys.1);
            let y1 = l.map_y(lo[r], ys.0, ys.1);
            writeln!(
                svg,
                r#"    <rect x="{:.2}" y="{:.2}" width="{:.2}" height="{:.2}" fill="{}" fill-opacity="{:.3}" {}/>"#,
                x0, y0, x1 - x0, y1 - y0, esc(fill), opacity, edge,
            ).unwrap();
        }
        writeln!(svg, "  </g>").unwrap();
    }

    /// The bands — one cubic-sided shape per (path, adjacent-stage gap), running
    /// from the path's interval at the left stage to its interval at the right.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn write_flow_bands(
        &self,
        svg: &mut String,
        layer: &Layer,
        df: &DataFrame,
        l: &Layout,
        xs: (f64, f64),
        ys: (f64, f64),
        cat_x: Option<&[String]>,
        color_map: &std::collections::HashMap<String, String>,
        clip: &str,
    ) {
        let (Some(stage), Some(l0), Some(u0), Some(l1), Some(u1)) = (
            df.str_col(FLOW_STAGE),
            df.float_col(CELL_LOWER), df.float_col(CELL_UPPER),
            df.float_col(BAND_LOWER), df.float_col(BAND_UPPER),
        ) else {
            return;
        };
        let Some(cats) = cat_x else { return };
        let st = &layer.style;
        let opacity = st.opacity.unwrap_or(BAND_OPACITY);
        let hue_col = layer.encodings.get(&Channel::Color)
            .and_then(|def| df.str_col(&def.field));
        writeln!(svg, r#"  <g clip-path="url(#{clip})">"#).unwrap();
        for r in 0..stage.len() {
            let Some(k) = cats.iter().position(|c| *c == stage[r]) else { continue };
            if k + 1 >= cats.len() {
                continue;
            }
            let x0 = l.map_x(k as f64 + SLOT_HALF, xs.0, xs.1);
            let x1 = l.map_x((k + 1) as f64 - SLOT_HALF, xs.0, xs.1);
            let mx = (x0 + x1) / 2.0;
            let a_hi = l.map_y(u0[r], ys.0, ys.1);
            let a_lo = l.map_y(l0[r], ys.0, ys.1);
            let b_hi = l.map_y(u1[r], ys.0, ys.1);
            let b_lo = l.map_y(l1[r], ys.0, ys.1);
            let fill = hue_col
                .and_then(|c| c.get(r))
                .and_then(|v| color_map.get(v))
                .map(|s| s.as_str())
                .or(st.color.as_deref())
                .unwrap_or(PALETTE_GOG[0]);
            writeln!(
                svg,
                r#"    <path d="M {x0:.2},{a_hi:.2} C {mx:.2},{a_hi:.2} {mx:.2},{b_hi:.2} {x1:.2},{b_hi:.2} L {x1:.2},{b_lo:.2} C {mx:.2},{b_lo:.2} {mx:.2},{a_lo:.2} {x0:.2},{a_lo:.2} Z" fill="{}" fill-opacity="{opacity:.3}"/>"#,
                esc(fill),
            ).unwrap();
        }
        writeln!(svg, "  </g>").unwrap();
    }
}
