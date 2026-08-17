//! The `edge` mark — the stroke whose two endpoints the layout supplies.
//!
//! One row is one edge, which no other stroke says: `path` chains consecutive
//! rows, `interval` spans one axis from one slot, `line` is functional on `x`.
//! Both endpoints arrive as `layout`-synthesized columns, so the writer reads
//! four columns flat and six in the cube and never sees a binding — the
//! `ymin`/`ymax` ruling holding for a second endpoint (spec §15, the network
//! entry).
//!
//! Aesthetics are a stroke's. `color` maps per row over any column the edge
//! table carries — an edge is 1:1 with its row, so nothing has to be
//! aggregated first. `opacity` maps continuously, the one stroke where it
//! does: each row is its own stroke with its own value, and fading by weight
//! is how a network reader keeps a dense diagram legible.
use std::collections::HashMap;
use std::fmt::Write;
use crate::data::DataFrame;
use crate::ir::{Channel, Layer};
use crate::render::encode::{opacity_at, OPACITY_DEFAULT};
use crate::render::palette::PALETTE_GOG;
use crate::render::pattern::pattern_dasharray;
use crate::render::project::Scene;
use crate::render::svg::{unit_norm, SvgRenderer};
use crate::render::text::esc;
use crate::render::Layout;
use crate::scale;
use crate::transform::{EDGE_X, EDGE_Y, EDGE_Z, LAYOUT_X, LAYOUT_Y, LAYOUT_Z};

/// A stroke's default width, matched to `line`'s hairline weight so a network
/// reads as connections under glyphs rather than as competing ink.
const EDGE_WIDTH: f64 = 1.5;

impl SvgRenderer {
    /// One resolved stroke per row: endpoints in device space plus its paint.
    #[allow(clippy::too_many_arguments, clippy::type_complexity)]
    fn edge_strokes(
        &self,
        layer: &Layer,
        df: &DataFrame,
        color_map: &HashMap<String, String>,
    ) -> (Vec<(String, f64)>, f64, &'static str) {
        let st = &layer.style;
        let hue_col = layer.encodings.get(&Channel::Color)
            .and_then(|def| df.str_col(&def.field));
        let opacity_vals = layer.encodings.get(&Channel::Opacity)
            .and_then(|c| df.float_col(&c.field));
        let op_scale = match opacity_vals {
            Some(c) => scale::ChannelScale::of(c, layer.encodings.get(&Channel::Opacity)),
            None => scale::ChannelScale::unbound(),
        };
        let n = df.len();
        let mut paint = Vec::with_capacity(n);
        for r in 0..n {
            let stroke = hue_col
                .and_then(|c| c.get(r))
                .and_then(|v| color_map.get(v))
                .map(|s| s.as_str())
                .or(st.color.as_deref())
                .unwrap_or(PALETTE_GOG[0]);
            let opacity = match opacity_vals {
                Some(vals) => opacity_at(op_scale.fraction(vals[r])),
                None => st.opacity.unwrap_or(OPACITY_DEFAULT),
            };
            paint.push((esc(stroke), opacity));
        }
        let width = st.size.unwrap_or(EDGE_WIDTH);
        let dash = pattern_dasharray(st.pattern.as_deref());
        (paint, width, dash)
    }

    /// The flat form: one `<line>` per row between the layout's two endpoints.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn write_edge(
        &self,
        svg: &mut String,
        layer: &Layer,
        df: &DataFrame,
        l: &Layout,
        xs: (f64, f64),
        ys: (f64, f64),
        color_map: &HashMap<String, String>,
        clip: &str,
        _scene: Option<&Scene>,
    ) {
        let (Some(x0), Some(y0), Some(x1), Some(y1)) = (
            df.float_col(LAYOUT_X), df.float_col(LAYOUT_Y),
            df.float_col(EDGE_X), df.float_col(EDGE_Y),
        ) else {
            return;
        };
        let (paint, width, dash) = self.edge_strokes(layer, df, color_map);
        writeln!(svg, r#"  <g clip-path="url(#{clip})">"#).unwrap();
        for r in 0..x0.len() {
            let a = (l.map_x(x0[r], xs.0, xs.1), l.map_y(y0[r], ys.0, ys.1));
            let b = (l.map_x(x1[r], xs.0, xs.1), l.map_y(y1[r], ys.0, ys.1));
            let (stroke, opacity) = &paint[r];
            svg.push_str(&super::segment_svg(a, b, stroke, width, *opacity, dash, 0.0));
        }
        writeln!(svg, "  </g>").unwrap();
    }

    /// The cube form: both endpoints projected, every stroke depth-sorted by
    /// its midpoint so nearer edges paint over farther ones — the painter's
    /// rule the cube already lives by, per stroke because a segment has one
    /// usable depth where a route has many.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn write_edge_3d(
        &self,
        svg: &mut String,
        layer: &Layer,
        df: &DataFrame,
        xs: (f64, f64),
        ys: (f64, f64),
        zs: (f64, f64),
        color_map: &HashMap<String, String>,
        clip: &str,
        scene: &Scene,
    ) {
        let (Some(x0), Some(y0), Some(z0), Some(x1), Some(y1), Some(z1)) = (
            df.float_col(LAYOUT_X), df.float_col(LAYOUT_Y), df.float_col(LAYOUT_Z),
            df.float_col(EDGE_X), df.float_col(EDGE_Y), df.float_col(EDGE_Z),
        ) else {
            return;
        };
        let (paint, width, dash) = self.edge_strokes(layer, df, color_map);
        let mut pieces: Vec<(f64, String)> = Vec::with_capacity(x0.len());
        for r in 0..x0.len() {
            let a = scene.to_screen(
                unit_norm(x0[r], xs), unit_norm(y0[r], ys), unit_norm(z0[r], zs));
            let b = scene.to_screen(
                unit_norm(x1[r], xs), unit_norm(y1[r], ys), unit_norm(z1[r], zs));
            let (stroke, opacity) = &paint[r];
            pieces.push((
                (a.depth + b.depth) / 2.0,
                super::segment_svg((a.x, a.y), (b.x, b.y), stroke, width, *opacity, dash, 0.0),
            ));
        }
        // Far to near; ties break on the emit index the sort preserves.
        pieces.sort_by(|p, q| q.0.partial_cmp(&p.0).unwrap_or(std::cmp::Ordering::Equal));
        writeln!(svg, r#"  <g clip-path="url(#{clip})">"#).unwrap();
        for (_, piece) in pieces {
            svg.push_str(&piece);
        }
        writeln!(svg, "  </g>").unwrap();
    }
}
