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
//! is how a network reader keeps a dense diagram legible. `pattern` maps the
//! same way `color` does, one dash per category and one per row, which is
//! how every other stroke draws it.
//!
//! **An edge has a direction, so it takes `style(arrow = )`** (2026-09-24),
//! `path`'s setting with `path`'s three ends: `"end"` puts the head at the `to`
//! node, `"start"` at the `from` node, `"both"` at each, the order
//! `layout(from, to)` states. A head stops short of the dot a `point * layout`
//! layer draws at its node, so a node drawn on top does not hide it.
use std::collections::HashMap;
use std::fmt::Write;
use crate::data::DataFrame;
use crate::ir::{Channel, Layer, Mark, PlotSpec, Transform};
use crate::render::encode::{opacity_at, radius_at, OPACITY_DEFAULT};
use crate::render::palette::PALETTE_GOG;
use crate::render::pattern::{pattern_dasharray, PatternMap};
use crate::render::project::Scene;
use crate::render::svg::{unit_norm, SvgRenderer};
use crate::render::text::esc;
use crate::render::Layout;
use crate::scale;
use crate::transform::{EDGE_X, EDGE_Y, EDGE_Z, LAYOUT_X, LAYOUT_Y, LAYOUT_Z, NODE_NAME};

/// A stroke's default width, matched to `line`'s hairline weight so a network
/// reads as connections under glyphs rather than as competing ink.
const EDGE_WIDTH: f64 = 1.5;

/// Which ends of an edge carry a head: `(at the from node, at the to node)`.
fn head_ends(arrow: Option<&str>) -> (bool, bool) {
    match arrow {
        Some("end") => (false, true),
        Some("start") => (true, false),
        Some("both") => (true, true),
        _ => (false, false),
    }
}

/// The point `pull` px short of `tip`, back along the segment from `from`, or
/// `None` when the segment is too short to leave any stroke for a head.
fn short_of(from: (f64, f64), tip: (f64, f64), pull: f64) -> Option<(f64, f64)> {
    let (dx, dy) = (tip.0 - from.0, tip.1 - from.1);
    let len = (dx * dx + dy * dy).sqrt();
    (len.is_finite() && len > pull + 1e-9)
        .then(|| (tip.0 - dx / len * pull, tip.1 - dy / len * pull))
}

impl SvgRenderer {
    /// How far short of each node's center a head stops: the radius of the dot a
    /// `point * layout` layer draws there, the largest when two do, read the way
    /// the point writer reads it (a mapped `size`, a set one, or the default).
    /// A node no layer draws is only a place, and a head reaches its center.
    pub(crate) fn node_radii(&self, spec: &PlotSpec, eff: &[DataFrame]) -> HashMap<String, f64> {
        let mut out: HashMap<String, f64> = HashMap::new();
        for (layer, df) in spec.layers.iter().zip(eff) {
            if layer.mark != Mark::Point || !layer.transforms.contains(&Transform::Layout) {
                continue;
            }
            let Some(names) = df.str_col(NODE_NAME) else { continue };
            let size_def = layer.encodings.get(&Channel::Size);
            let sizes = size_def.and_then(|c| df.float_col(&c.field));
            let size_scale = match sizes {
                Some(c) => scale::ChannelScale::of(c, size_def),
                None => scale::ChannelScale::unbound(),
            };
            let set = layer.style.size.unwrap_or(self.point_radius);
            for (i, name) in names.iter().enumerate() {
                let r = match sizes {
                    Some(col) => radius_at(size_scale.fraction(col.get(i).copied().unwrap_or(f64::NAN))),
                    None => set,
                };
                let at = out.entry(name.clone()).or_insert(0.0);
                *at = at.max(r);
            }
        }
        out
    }

    /// The heads of one edge, drawn in page coordinates between `a` (its `from`
    /// node) and `b` (its `to` node), in the stroke's color and a solid fill: a
    /// dash is the route's texture, and a dashed head stops reading as an arrow,
    /// which is `path`'s ruling too.
    #[allow(clippy::too_many_arguments)]
    fn write_heads(
        &self, svg: &mut String, layer: &Layer, df: &DataFrame, row: usize,
        a: (f64, f64), b: (f64, f64), stroke: &str, opacity: f64, width: f64,
        radii: &HashMap<String, f64>,
    ) {
        let (at_from, at_to) = head_ends(layer.style.arrow.as_deref());
        if !(at_from || at_to) {
            return;
        }
        let Some(lay) = layer.layout.as_ref() else { return };
        let node = |col: &str| -> f64 {
            df.str_col(col)
                .and_then(|c| c.get(row))
                .and_then(|n| radii.get(n))
                .map_or(0.0, |r| r + super::HEAD_GAP)
        };
        let ends = [(at_to, a, b, node(&lay.to)), (at_from, b, a, node(&lay.from))];
        for (wanted, from, tip, pull) in ends {
            if !wanted {
                continue;
            }
            let Some(tip) = short_of(from, tip, pull) else { continue };
            if let Some(tri) = super::head_points(from, tip, width) {
                writeln!(svg,
                    r#"    <polygon points="{tri}" fill="{stroke}" fill-opacity="{opacity:.3}" stroke="none"/>"#
                ).unwrap();
            }
        }
    }

    /// One resolved stroke per row: its color, its opacity and its dash.
    #[allow(clippy::too_many_arguments, clippy::type_complexity)]
    fn edge_strokes(
        &self,
        layer: &Layer,
        df: &DataFrame,
        color_map: &HashMap<String, String>,
    ) -> (Vec<(String, f64, &'static str)>, f64) {
        let st = &layer.style;
        let hue_col = layer.encodings.get(&Channel::Color)
            .and_then(|def| df.str_col(&def.field));
        let opacity_vals = layer.encodings.get(&Channel::Opacity)
            .and_then(|c| df.float_col(&c.field));
        let op_scale = match opacity_vals {
            Some(c) => scale::ChannelScale::of(c, layer.encodings.get(&Channel::Opacity)),
            None => scale::ChannelScale::unbound(),
        };
        // A mapped dash, read as `line` reads it: the category's index in the
        // column's order picks one of the five, and the legend decodes the same
        // order. It was grammar the engine did not draw, refused as Unsupported
        // after the dash-recycling note had already been printed for it.
        let dashes = PatternMap::resolve(layer, df);
        let set_dash = pattern_dasharray(st.pattern.as_deref());
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
            let dash = match &dashes {
                Some(pm) => pattern_dasharray(Some(pm.dash(pm.cat_at(r)))),
                None => set_dash,
            };
            paint.push((esc(stroke), opacity, dash));
        }
        let width = st.size.unwrap_or(EDGE_WIDTH);
        (paint, width)
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
        radii: &HashMap<String, f64>,
    ) {
        let (Some(x0), Some(y0), Some(x1), Some(y1)) = (
            df.float_col(LAYOUT_X), df.float_col(LAYOUT_Y),
            df.float_col(EDGE_X), df.float_col(EDGE_Y),
        ) else {
            return;
        };
        let (paint, width) = self.edge_strokes(layer, df, color_map);
        writeln!(svg, r#"  <g clip-path="url(#{clip})">"#).unwrap();
        for r in 0..x0.len() {
            let a = (l.map_x(x0[r], xs.0, xs.1), l.map_y(y0[r], ys.0, ys.1));
            let b = (l.map_x(x1[r], xs.0, xs.1), l.map_y(y1[r], ys.0, ys.1));
            let (stroke, opacity, dash) = &paint[r];
            svg.push_str(&super::segment_svg(a, b, stroke, width, *opacity, dash, 0.0));
            self.write_heads(svg, layer, df, r, a, b, stroke, *opacity, width, radii);
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
        radii: &HashMap<String, f64>,
    ) {
        let (Some(x0), Some(y0), Some(z0), Some(x1), Some(y1), Some(z1)) = (
            df.float_col(LAYOUT_X), df.float_col(LAYOUT_Y), df.float_col(LAYOUT_Z),
            df.float_col(EDGE_X), df.float_col(EDGE_Y), df.float_col(EDGE_Z),
        ) else {
            return;
        };
        let (paint, width) = self.edge_strokes(layer, df, color_map);
        let mut pieces: Vec<(f64, String)> = Vec::with_capacity(x0.len());
        for r in 0..x0.len() {
            let a = scene.to_screen(
                unit_norm(x0[r], xs), unit_norm(y0[r], ys), unit_norm(z0[r], zs));
            let b = scene.to_screen(
                unit_norm(x1[r], xs), unit_norm(y1[r], ys), unit_norm(z1[r], zs));
            let (stroke, opacity, dash) = &paint[r];
            // The heads ride with their stroke in the depth sort, so a nearer
            // edge covers a farther edge's head as it covers its stroke.
            let mut piece =
                super::segment_svg((a.x, a.y), (b.x, b.y), stroke, width, *opacity, dash, 0.0);
            self.write_heads(&mut piece, layer, df, r, (a.x, a.y), (b.x, b.y), stroke,
                             *opacity, width, radii);
            pieces.push(((a.depth + b.depth) / 2.0, piece));
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
