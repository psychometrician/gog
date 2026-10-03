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
//!
//! **And white inside a black line unless the sentence says otherwise**
//! (2026-10-03, at the author's word). The default was the palette's first
//! color, solid: the bands' own blue, with nothing marking where one slot ends,
//! so every flow in the book wrote `style(color = "white", border_color =
//! "black")` to make its slots readable. A default every reader has to override
//! is the wrong default; ggalluvial's strata draw the same white and black.
use std::fmt::Write;
use crate::data::DataFrame;
use crate::ir::{Channel, Layer};
use crate::render::palette::{NEUTRAL_INK, PALETTE_GOG};
use crate::render::pattern::{FillTexture, PatternMap};
use crate::render::svg::SvgRenderer;
use crate::render::polar::{Polar, RING_NAME_GAP};
use crate::render::text::{esc, estimate_cap_height};
use crate::render::{Layout, Whole};
use crate::transform::{CELL_END, CELL_LOWER, CELL_START, CELL_UPPER, FLOW_ARRIVE, FLOW_PATH, FLOW_STAGE, NODE_NAME};

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

/// A slot's fill and outline when `style()` names neither (see the module note).
const SLOT_FILL: &str = "white";
const SLOT_LINE: &str = "black";

/// A slot's outline. `border_edge` gives every other fill no line until one is
/// asked for, and a line in the panel's color when only its width is; a slot
/// keeps its black line in both cases, since the line is what separates one
/// slot from the next. `border_size = 0` removes it.
fn slot_edge(st: &crate::ir::StyleSpec) -> String {
    match st.border_size {
        Some(w) if w == 0.0 => r#"stroke="none""#.to_string(),
        w => format!(
            r#"stroke="{}" stroke-width="{:.2}""#,
            st.border_color.as_deref().map(esc).unwrap_or_else(|| SLOT_LINE.to_string()),
            w.unwrap_or(1.0),
        ),
    }
}

/// Where a slot is, for the page: which column it is a place of, which place,
/// and its outline in the panel's own units.
///
/// **A click on a slot is how a reader moves a flow's brush**, and the page cannot
/// find a slot by itself: a slot's extent is the paths' running sum, which only
/// the flow's layout knows, and a page working it out would be a second copy of
/// that layout in another language. So the engine states the outline it drew and
/// the page tests a pointer against it, which is `data-gog-panel`'s bargain made
/// for one more fact. The outline is a polygon rather than a rectangle so that a
/// slot drawn as anything else is described the same way.
///
/// An empty `<g>`, as the panel's is: it carries no ink, so the drawn slot stays
/// the bytes it was, and it is written only when the sentence names a brush.
fn write_slot_shape(svg: &mut String, field: &str, place: &str, corners: &[(f64, f64)]) {
    let shape = corners.iter().map(|(x, y)| format!("{x:.2},{y:.2}")).collect::<Vec<_>>().join(" ");
    writeln!(
        svg,
        r#"    <g data-gog-slot="{}" data-gog-slot-field="{}" data-gog-shape="{shape}"/>"#,
        esc(place), esc(field),
    ).unwrap();
}

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
        brushed: bool,
    ) {
        let (lo_name, hi_name) = if down { (CELL_START, CELL_END) } else { (CELL_LOWER, CELL_UPPER) };
        let (Some(stage), Some(lo), Some(hi)) = (
            df.str_col(FLOW_STAGE), df.float_col(lo_name), df.float_col(hi_name),
        ) else {
            return;
        };
        let name = df.str_col(NODE_NAME);
        let Some(cats) = (if down { cat_y } else { cat_x }) else { return };
        let st = &layer.style;
        let opacity = st.opacity.unwrap_or(1.0);
        let edge = slot_edge(st);
        // A layered flow's slots are thin boxes beside their names, and each is a
        // place of both columns, so a click reads either one or `name`.
        let layered = layer.flow_is_layered();
        let fields = slot_fields(layer);
        writeln!(svg, r#"  <g clip-path="url(#{clip})">"#).unwrap();
        // A slot is a fill, so `pattern` hatches it as it hatches every other
        // fill (the settable rule). `check_flow` took the setting and this
        // writer had no texture code until 2026-10-03, the defect the bands
        // carried until 2026-09-27. One paint serves every slot: a slot takes no
        // mapped aesthetic, so nothing varies it row by row.
        let fill = FillTexture::new().fill(svg, st.pattern.as_deref(),
                                           st.color.as_deref().unwrap_or(SLOT_FILL));
        for r in 0..stage.len() {
            let Some(k) = cats.iter().position(|c| *c == stage[r]) else { continue };
            let (x0, x1, y0, y1) = if down {
                let (a, b) = match layered {
                    true => layered_edges(l.map_y(k as f64, ys.0, ys.1)),
                    false => (l.map_y(k as f64 - SLOT_HALF, ys.0, ys.1), l.map_y(k as f64 + SLOT_HALF, ys.0, ys.1)),
                };
                (l.map_x(lo[r], xs.0, xs.1), l.map_x(hi[r], xs.0, xs.1), a.min(b), a.max(b))
            } else {
                let (a, b) = match layered {
                    true => layered_edges(l.map_x(k as f64, xs.0, xs.1)),
                    false => (l.map_x(k as f64 - SLOT_HALF, xs.0, xs.1), l.map_x(k as f64 + SLOT_HALF, xs.0, xs.1)),
                };
                (a, b, l.map_y(hi[r], ys.0, ys.1), l.map_y(lo[r], ys.0, ys.1))
            };
            writeln!(
                svg,
                r#"    <rect x="{:.2}" y="{:.2}" width="{:.2}" height="{:.2}" fill="{}" fill-opacity="{:.3}" {}/>"#,
                x0, y0, x1 - x0, y1 - y0, esc(&fill), opacity, edge,
            ).unwrap();
            if brushed {
                if let Some(n) = name.and_then(|n| n.get(r)) {
                    let field = match layered {
                        true => fields.clone(),
                        false => format!("{}|{NODE_NAME}", stage[r]),
                    };
                    write_slot_shape(svg, &field, n, &[(x0, y0), (x1, y0), (x1, y1), (x0, y1)]);
                }
            }
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
        let layered = layer.flow_is_layered();
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
            // two coordinates exchanged. A layered flow's band can skip a layer,
            // and is one curve across every gap it skips.
            let d = if down {
                let (c0, c1) = (l.map_y(k0 as f64, ys.0, ys.1), l.map_y(k1 as f64, ys.0, ys.1));
                let half = match layered {
                    true => LAYERED_SLOT_PX / 2.0,
                    false => (l.map_y(k0 as f64 + SLOT_HALF, ys.0, ys.1) - c0).abs(),
                };
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
                let (x0, x1) = match layered {
                    true => (layered_edges(l.map_x(k0 as f64, xs.0, xs.1)).1,
                             layered_edges(l.map_x(k1 as f64, xs.0, xs.1)).0),
                    false => (l.map_x(k0 as f64 + SLOT_HALF, xs.0, xs.1), l.map_x(k1 as f64 - SLOT_HALF, xs.0, xs.1)),
                };
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

    /// A layered flow's names, each beside its slot: after it along the layers,
    /// and before it in the last layer, where nothing follows. The slots are
    /// too thin to hold a name, so the name sits over the bands leaving the
    /// place, which are translucent, as most published Sankey diagrams put it.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn write_layered_names(
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
    ) {
        let label_field = layer.encodings.get(&Channel::Label).map(|c| c.field.as_str())
            .unwrap_or(NODE_NAME);
        let (lo_name, hi_name) = if down { (CELL_START, CELL_END) } else { (CELL_LOWER, CELL_UPPER) };
        let (Some(names), Some(stage), Some(lo), Some(hi)) = (
            df.str_col(label_field), df.str_col(FLOW_STAGE), df.float_col(lo_name), df.float_col(hi_name),
        ) else {
            return;
        };
        let Some(cats) = (if down { cat_y } else { cat_x }) else { return };
        let st = &layer.style;
        let fs = st.size.unwrap_or(self.font_md);
        let opacity = st.opacity.unwrap_or(1.0);
        let fill = st.color.as_deref().map(esc).unwrap_or_else(|| NEUTRAL_INK.to_string());
        let cap = estimate_cap_height(fs);
        // Which layer comes last is read off the layers' numbers, among every
        // layer the axis holds: not the axis's list order, which runs bottom to
        // top down a page, and not this frame's rows, which a selection's pass
        // cuts down to the places it draws.
        let number = |s: &str| s.parse::<usize>().unwrap_or(0);
        let last = cats.iter().map(|c| number(c)).max().unwrap_or(0);
        writeln!(svg, r##"  <g font-family="system-ui,sans-serif" font-size="{fs}">"##).unwrap();
        for r in 0..names.len().min(stage.len()).min(lo.len()).min(hi.len()) {
            let Some(k) = cats.iter().position(|c| *c == stage[r]) else { continue };
            let mid = (lo[r] + hi[r]) / 2.0;
            let after = number(&stage[r]) < last;
            let (x, y, anchor) = if down {
                let (a, b) = layered_edges(l.map_y(k as f64, ys.0, ys.1));
                let x = l.map_x(mid, xs.0, xs.1);
                match after {
                    true => (x, a.max(b) + LAYERED_NAME_GAP + cap, "middle"),
                    false => (x, a.min(b) - LAYERED_NAME_GAP, "middle"),
                }
            } else {
                let (a, b) = layered_edges(l.map_x(k as f64, xs.0, xs.1));
                let y = l.map_y(mid, ys.0, ys.1) + cap / 2.0;
                match after {
                    true => (b + LAYERED_NAME_GAP, y, "start"),
                    false => (a - LAYERED_NAME_GAP, y, "end"),
                }
            };
            writeln!(svg,
                r#"    <text x="{x:.2}" y="{y:.2}" text-anchor="{anchor}" fill="{fill}" fill-opacity="{opacity:.3}">{}</text>"#,
                esc(&names[r]),
            ).unwrap();
        }
        writeln!(svg, "  </g>").unwrap();
    }

    // -----------------------------------------------------------------------
    // The shared flow — two columns that name one set of places
    //
    // `flow(from, to, shared = TRUE)` puts every place on the one count axis, so
    // a band runs from one place to another *on the same axis*. Flat, the slots
    // stand along the count axis and each band arches into the panel: the arc
    // diagram. In `polar()` the count goes on the angle, as a pie's does, so the
    // slots make one ring and each band curves through the middle: the chord
    // diagram. In both, a band leaves its slot along the free axis, sideways on
    // the plane and toward the center on the ring, and the farther apart its two
    // places, the farther it reaches. The reach is the mark's convention, as a
    // stage flow's S-curve is; only the two ends' intervals are data (Law 9).
    // -----------------------------------------------------------------------

    /// A shared flow's slots: one per place, standing on the count axis, each in
    /// two parts with no line between them. The first part holds the ends where
    /// the place is named in the first column and is drawn in the slot's fill;
    /// the second holds the ends where it is named in the second column and is
    /// drawn a shade darker. One outline goes around the whole place.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn write_shared_slots(
        &self,
        svg: &mut String,
        layer: &Layer,
        df: &DataFrame,
        l: &Layout,
        xs: (f64, f64),
        ys: (f64, f64),
        down: bool,
        clip: &str,
        brushed: bool,
        pol: Option<&Polar>,
    ) {
        let (lo_name, hi_name) = if down { (CELL_START, CELL_END) } else { (CELL_LOWER, CELL_UPPER) };
        let (Some(lo), Some(hi)) = (df.float_col(lo_name), df.float_col(hi_name)) else { return };
        let arrive = df.float_col(FLOW_ARRIVE);
        let name = df.str_col(NODE_NAME);
        let st = &layer.style;
        let opacity = st.opacity.unwrap_or(1.0);
        let edge = slot_edge(st);
        let outlined = st.border_size != Some(0.0);
        let fields = slot_fields(layer);
        writeln!(svg, r#"  <g clip-path="url(#{clip})">"#).unwrap();
        let fill = FillTexture::new().fill(svg, st.pattern.as_deref(),
                                           st.color.as_deref().unwrap_or(SLOT_FILL));
        let line = SharedLine::new(l, down);
        // A stretch of the count axis as a slot's shape: a sector of the ring, or
        // a strip standing on the line. The page's outline of the whole place is
        // the same shape as points.
        let corners_of = |a: f64, b: f64| -> Vec<(f64, f64)> {
            match pol {
                Some(p) => ring_outline(p, count_u(a, xs, ys, down), count_u(b, xs, ys, down), ring_inner(p)),
                None => {
                    let (a, b) = (line.along(a, l, xs, ys), line.along(b, l, xs, ys));
                    vec![line.at(a, 0.0), line.at(b, 0.0), line.at(b, SHARED_SLOT_PX), line.at(a, SHARED_SLOT_PX)]
                }
            }
        };
        let shape_of = |a: f64, b: f64| -> String {
            match pol {
                Some(p) => p.sector(count_u(a, xs, ys, down), count_u(b, xs, ys, down), ring_inner(p), 1.0),
                None => format!("M {} Z", corners_of(a, b).iter()
                    .map(|(x, y)| format!("{x:.2},{y:.2}")).collect::<Vec<_>>().join(" L ")),
            }
        };
        for r in 0..lo.len().min(hi.len()) {
            let d = shape_of(lo[r], hi[r]);
            // The fill, the second part's shade over it, then the outline over
            // both, so the shade darkens no part of the line.
            writeln!(svg, r#"    <path d="{d}" fill="{}" fill-opacity="{opacity:.3}"/>"#, esc(&fill)).unwrap();
            if let Some(turn) = arrive.and_then(|a| a.get(r)).copied().filter(|t| *t < hi[r]) {
                writeln!(svg, r#"    <path d="{}" fill="black" fill-opacity="{SECOND_PART_SHADE:.3}"/>"#,
                         shape_of(turn.max(lo[r]), hi[r])).unwrap();
            }
            if outlined {
                writeln!(svg, r#"    <path d="{d}" fill="none" {edge}/>"#).unwrap();
            }
            if brushed {
                if let Some(n) = name.and_then(|n| n.get(r)) {
                    write_slot_shape(svg, &fields, n, &corners_of(lo[r], hi[r]));
                }
            }
        }
        writeln!(svg, "  </g>").unwrap();
    }

    /// A shared flow's bands: each runs from the interval its amount leaves to
    /// the interval it arrives at, both on the count axis. The rows arrive in
    /// pairs sharing [`FLOW_PATH`], the leaving end first.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn write_shared_bands(
        &self,
        svg: &mut String,
        layer: &Layer,
        df: &DataFrame,
        whole: &Whole<'_>,
        l: &Layout,
        xs: (f64, f64),
        ys: (f64, f64),
        down: bool,
        color_map: &std::collections::HashMap<String, String>,
        clip: &str,
        pol: Option<&Polar>,
    ) {
        let (lo_name, hi_name) = if down { (CELL_START, CELL_END) } else { (CELL_LOWER, CELL_UPPER) };
        let (Some(path), Some(lo), Some(hi)) = (
            df.str_col(FLOW_PATH), df.float_col(lo_name), df.float_col(hi_name),
        ) else {
            return;
        };
        let st = &layer.style;
        let opacity = st.opacity.unwrap_or(BAND_OPACITY);
        let hue_col = layer.encodings.get(&Channel::Color).and_then(|def| df.str_col(&def.field));
        let pattern_map = PatternMap::resolve(layer, df, whole);
        let mut tex = FillTexture::new();
        let line = SharedLine::new(l, down);
        // How far a band reaches per pixel it spans, read off every band the layer
        // draws in any panel or moment, so a selection, which hands each pass a
        // share of the rows, can never resize the arches it dims.
        let widest = whole.shares.iter().copied().chain(std::iter::once(df))
            .map(|f| widest_band(f, lo_name, hi_name, &line, l, xs, ys))
            .fold(0.0, f64::max);
        let reach = if widest > 0.0 { (SHARED_REACH * line.room(l) / widest).min(0.5) } else { 0.5 };
        writeln!(svg, r#"  <g clip-path="url(#{clip})">"#).unwrap();
        for r in 0..path.len().saturating_sub(1) {
            if path[r + 1] != path[r] {
                continue;
            }
            let d = match pol {
                Some(p) => ring_band(p, (lo[r], hi[r]), (lo[r + 1], hi[r + 1]), xs, ys, down),
                None => {
                    let a = sorted(line.along(lo[r], l, xs, ys), line.along(hi[r], l, xs, ys));
                    let b = sorted(line.along(lo[r + 1], l, xs, ys), line.along(hi[r + 1], l, xs, ys));
                    let (p0, q0) = if a.0 + a.1 <= b.0 + b.1 { (a, b) } else { (b, a) };
                    arch(&line, p0, q0, reach)
                }
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
            writeln!(svg, r#"    <path d="{d}" fill="{}" fill-opacity="{opacity:.3}"/>"#, esc(&fill)).unwrap();
        }
        writeln!(svg, "  </g>").unwrap();
    }

    /// A shared flow's names, each just beyond its slot, on the side away from
    /// the bands: under the count axis or left of it on the plane, where its
    /// numbers would have stood, and outward from the ring in `polar()`. The
    /// slots are too thin to hold a name across, and a name inside one ran into
    /// its neighbors'.
    ///
    /// On the plane the frame sized that room from these names, turned by
    /// `angle` when they do not fit side by side, as crowded category names are,
    /// so the writer turns them by the same angle. `style(angle = )` turns them by
    /// the angle it states instead.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn write_shared_names(
        &self,
        svg: &mut String,
        layer: &Layer,
        df: &DataFrame,
        l: &Layout,
        xs: (f64, f64),
        ys: (f64, f64),
        down: bool,
        pol: Option<&Polar>,
        angle: Option<f64>,
    ) {
        let label_field = layer.encodings.get(&Channel::Label).map(|c| c.field.as_str())
            .unwrap_or(NODE_NAME);
        let (lo_name, hi_name) = if down { (CELL_START, CELL_END) } else { (CELL_LOWER, CELL_UPPER) };
        let (Some(names), Some(lo), Some(hi)) = (
            df.str_col(label_field), df.float_col(lo_name), df.float_col(hi_name),
        ) else {
            return;
        };
        let st = &layer.style;
        let fs = st.size.unwrap_or(self.font_md);
        let opacity = st.opacity.unwrap_or(1.0);
        let fill = st.color.as_deref().map(esc).unwrap_or_else(|| NEUTRAL_INK.to_string());
        let cap = estimate_cap_height(fs);
        let line = SharedLine::new(l, down);
        // Outside the panel, in the room the frame left them, so not clipped to it.
        writeln!(svg, r##"  <g font-family="system-ui,sans-serif" font-size="{fs}">"##).unwrap();
        for r in 0..names.len().min(lo.len()).min(hi.len()) {
            let mid = (lo[r] + hi[r]) / 2.0;
            let (x, y, turn, anchor) = match pol {
                Some(p) => {
                    let t = p.angle(count_u(mid, xs, ys, down));
                    let (x, y) = p.polar_px(t, p.r_max + RING_NAME_GAP);
                    // Each name runs outward from the ring, read left to right on
                    // either side: on the left half it is turned a half turn more
                    // and anchored at its end, so it never reads upside down.
                    let right = t.sin() >= 0.0;
                    let deg = t.to_degrees() - 90.0 + if right { 0.0 } else { 180.0 };
                    (x, y + cap / 2.0, Some((deg, x, y)), if right { "start" } else { "end" })
                }
                None if down => {
                    // Under the axis, at the slot's middle: upright when the names
                    // fit side by side, and turned otherwise, ending at the axis.
                    let along = line.along(mid, l, xs, ys);
                    let y = l.y1 + SHARED_NAME_GAP;
                    match st.angle.or(angle).filter(|a| *a != 0.0) {
                        Some(a) => {
                            // A name turned upright is centered on its slot by half
                            // its letters' height, which now lies across the page.
                            let x = along + if (a - 90.0).abs() < 1e-9 { cap / 2.0 } else { 0.0 };
                            (x, y, Some((-a, x, y)), "end")
                        }
                        None => (along, y + cap, None, "middle"),
                    }
                }
                None => {
                    // Left of the axis, level with the slot's middle.
                    let along = line.along(mid, l, xs, ys);
                    let x = l.x0 - SHARED_NAME_GAP;
                    (x, along + cap / 2.0, st.angle.map(|a| (-a, x, along)), "end")
                }
            };
            let turned = turn.map(|(a, tx, ty)| format!(r#" transform="rotate({a:.2} {tx:.2} {ty:.2})""#))
                .unwrap_or_default();
            writeln!(svg,
                r#"    <text x="{x:.2}" y="{y:.2}"{turned} text-anchor="{anchor}" fill="{fill}" fill-opacity="{opacity:.3}">{}</text>"#,
                esc(&names[r]),
            ).unwrap();
        }
        writeln!(svg, "  </g>").unwrap();
    }
}

/// A layered flow's slot thickness along the layers, in pixels. Thin, as a
/// Sankey diagram's boxes are, since the layers stand far apart and the names
/// stand beside the slots rather than in them; it carries no data.
pub(crate) const LAYERED_SLOT_PX: f64 = 14.0;

/// The gap between a layered flow's slot and its name, in pixels.
const LAYERED_NAME_GAP: f64 = 5.0;

/// A layered slot's two edges along the layers, around its layer's center.
fn layered_edges(center: f64) -> (f64, f64) {
    (center - LAYERED_SLOT_PX / 2.0, center + LAYERED_SLOT_PX / 2.0)
}

/// The gap between a flat shared flow's axis and the names beyond it, in pixels,
/// where a tick and its number would have stood.
const SHARED_NAME_GAP: f64 = 6.0;

/// A shared flow's slot thickness across the count axis, in pixels. It carries
/// no data, as a stage flow's slot width carries none, and it is wide enough to
/// hold a name across it at the book's text size.
pub(crate) const SHARED_SLOT_PX: f64 = 20.0;

/// How much darker a shared flow slot's second part is: black laid over the
/// slot's own fill at this opacity. Laid over rather than mixed, so it shades any
/// fill a slot can take, a hatch or a color named in any form, and white becomes
/// a light gray. Like the slot's thickness it carries no data and has no knob:
/// which part is shaded is said by the order of the two columns.
const SECOND_PART_SHADE: f64 = 0.10;

/// The farthest a band may reach from the count axis, as a share of the room the
/// panel leaves on the plane. A band reaches half the distance it spans, as a
/// half circle does, unless the longest band would then leave the panel; then
/// every band reaches proportionally less, so the arches keep their nesting.
const SHARED_REACH: f64 = 0.95;


/// The count axis of a shared flow on the plane: which page direction it runs
/// in, where its slots stand, and which way the bands reach.
struct SharedLine {
    /// The count runs along `x` (the axis is across the page, at its foot).
    across: bool,
    /// The page coordinate of the slots' outer edge: the panel's foot or its
    /// left side, on the count axis itself.
    base: f64,
}

impl SharedLine {
    fn new(l: &Layout, down: bool) -> Self {
        // A count on `x` runs the axis across the foot of the panel, and the bands
        // rise from it; a count on `y` runs it up the left side, and they reach to
        // the right. Either way the slots stand on the axis the ticks are read on.
        if down { SharedLine { across: true, base: l.y1 } } else { SharedLine { across: false, base: l.x0 } }
    }

    /// The page coordinate along the axis of a count.
    fn along(&self, v: f64, l: &Layout, xs: (f64, f64), ys: (f64, f64)) -> f64 {
        if self.across { l.map_x(v, xs.0, xs.1) } else { l.map_y(v, ys.0, ys.1) }
    }

    /// The page point `inward` pixels from the axis into the panel, at `along`:
    /// 0 is on the axis, [`SHARED_SLOT_PX`] the slots' inner edge, where the
    /// bands begin. One distance from one line, so a slot's two edges cannot land
    /// on one another, which is how the first build drew every slot zero pixels
    /// tall: an offset read from the slot's inner edge when it was positive and
    /// from the axis when it was negative put both edges at the inner one.
    fn at(&self, along: f64, inward: f64) -> (f64, f64) {
        if self.across { (along, self.base - inward) } else { (self.base + inward, along) }
    }

    /// The room the panel leaves the bands beyond the slots, in pixels.
    fn room(&self, l: &Layout) -> f64 {
        if self.across { (l.y1 - l.y0) - SHARED_SLOT_PX } else { (l.x1 - l.x0) - SHARED_SLOT_PX }
    }
}

fn sorted(a: f64, b: f64) -> (f64, f64) {
    if a <= b { (a, b) } else { (b, a) }
}

/// The widest any band in `df` spans along the axis, in pixels: from the far
/// side of one end to the far side of the other.
fn widest_band(df: &DataFrame, lo_name: &str, hi_name: &str, line: &SharedLine, l: &Layout,
               xs: (f64, f64), ys: (f64, f64)) -> f64 {
    let (Some(path), Some(lo), Some(hi)) = (df.str_col(FLOW_PATH), df.float_col(lo_name), df.float_col(hi_name))
    else {
        return 0.0;
    };
    let mut widest: f64 = 0.0;
    for r in 0..path.len().saturating_sub(1) {
        if path[r + 1] != path[r] {
            continue;
        }
        let ends = [line.along(lo[r], l, xs, ys), line.along(hi[r], l, xs, ys),
                    line.along(lo[r + 1], l, xs, ys), line.along(hi[r + 1], l, xs, ys)];
        let (a, b) = ends.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), &v| (a.min(v), b.max(v)));
        widest = widest.max(b - a);
    }
    widest
}

/// One band on the plane: an arch from the interval `p` to the interval `q`
/// (page coordinates along the axis, `p` first), its outer edge joining their
/// far sides and its inner edge their near sides. Each edge rises to `reach`
/// times the distance it spans, a cubic standing in for a half circle.
fn arch(line: &SharedLine, p: (f64, f64), q: (f64, f64), reach: f64) -> String {
    let (outer, inner) = (reach * (q.1 - p.0), reach * (q.0 - p.1).max(0.0));
    let (co, ci) = (outer * 4.0 / 3.0, inner * 4.0 / 3.0);
    // Measured from the slots' inner edge, where every band begins.
    let pt = |along: f64, off: f64| {
        let (x, y) = line.at(along, SHARED_SLOT_PX + off);
        format!("{x:.2},{y:.2}")
    };
    format!("M {} C {} {} {} L {} C {} {} {} Z",
            pt(p.0, 0.0), pt(p.0, co), pt(q.1, co), pt(q.1, 0.0),
            pt(q.0, 0.0), pt(q.0, ci), pt(p.1, ci), pt(p.1, 0.0))
}

/// A count's place around the ring, as a share of one turn.
fn count_u(v: f64, xs: (f64, f64), ys: (f64, f64), down: bool) -> f64 {
    let (a, b) = if down { xs } else { ys };
    if (b - a).abs() < 1e-12 { 0.0 } else { (v - a) / (b - a) }
}

/// The ring's inner edge, as a share of the radius: the slots are
/// [`SHARED_SLOT_PX`] thick, measured in from the rim.
fn ring_inner(p: &Polar) -> f64 {
    (1.0 - SHARED_SLOT_PX / p.r_max.max(SHARED_SLOT_PX * 2.0)).max(0.5)
}

/// A slot's outline on the ring, for the page: the outer arc forward, then the
/// inner arc back, sampled finely enough that a click near its curved edge lands
/// where the reader sees the edge.
fn ring_outline(p: &Polar, u0: f64, u1: f64, v_in: f64) -> Vec<(f64, f64)> {
    let steps = (((u1 - u0).abs() * 120.0).ceil() as usize).clamp(2, 120);
    let mut pts: Vec<(f64, f64)> = (0..=steps)
        .map(|k| p.at(u0 + (u1 - u0) * k as f64 / steps as f64, 1.0)).collect();
    pts.extend((0..=steps).rev().map(|k| p.at(u0 + (u1 - u0) * k as f64 / steps as f64, v_in)));
    pts
}

/// One band on the ring: its leaving end's arc, a curve to its arriving end, that
/// end's arc, and a curve back, the two arcs both drawn forward round the ring.
/// Each curve bends through the center of the circle, the chord diagram's own
/// convention: every band crosses the middle whatever lies between its places, so
/// two bands are told apart by where they meet the ring and never by how deep
/// they reach, which carries no amount.
fn ring_band(p: &Polar, a: (f64, f64), b: (f64, f64), xs: (f64, f64), ys: (f64, f64), down: bool) -> String {
    let v_in = ring_inner(p);
    let u = |v: f64| count_u(v, xs, ys, down);
    let (a0, a1) = sorted(u(a.0), u(a.1));
    let (b0, b1) = sorted(u(b.0), u(b.1));
    let curve = |to: f64| -> String {
        let (x, y) = p.at(to, v_in);
        format!("Q {:.2} {:.2} {x:.2} {y:.2} ", p.cx, p.cy)
    };
    let mut d = String::new();
    p.move_to(&mut d, a0, v_in);
    p.hold_to(&mut d, a0, a1, v_in);
    d.push_str(&curve(b0));
    p.hold_to(&mut d, b0, b1, v_in);
    d.push_str(&curve(a0));
    d.push('Z');
    d
}

/// **A shared or layered flow is drawn on white and with no frame, unless
/// `theme()` says otherwise** (2026-10-03, at the author's word; the layered
/// flow joined the same day, for the same reason). Neither of its axes measures
/// anything a reader can use, so neither is drawn, and the frame's two lines
/// were left bounding nothing: along the count the slots stand on the line and
/// draw it themselves, and the line across it was an axis with nothing on it.
/// The gray panel also pulled the translucent bands toward gray. A background
/// or a frame the theme names, or that its preset names, is kept, since these
/// are defaults and not rulings: `theme(background = "#f5f5f8", frame = "axes")`
/// draws the panel every other plot has. Runs on the renderer's resolved copy of
/// the spec, so nothing the author wrote changes.
pub(crate) fn link_flow_ground(spec: &mut crate::ir::PlotSpec) {
    if !spec.layers.iter().any(|l| l.flow_reads_links()) {
        return;
    }
    let mut theme = spec.theme.resolved();
    theme.background.get_or_insert_with(|| "white".to_string());
    theme.frame.get_or_insert_with(|| "none".to_string());
    spec.theme = theme;
}

/// The columns a slot is a place of, for the page's click: a stage flow's slot
/// belongs to its stage, a shared flow's to both columns, and either one to the
/// flow's own `name`, which a brush may name to select a place at either end.
fn slot_fields(layer: &Layer) -> String {
    let mut fields: Vec<String> = layer.flow.as_ref().map(|f| f.stages.clone()).unwrap_or_default();
    fields.push(NODE_NAME.to_string());
    fields.join("|")
}

