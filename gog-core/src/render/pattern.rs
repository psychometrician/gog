//! The **texture** aesthetic (`style(pattern = )`, Wilkinson) realized for SVG.
//!
//! One name, one realization per geometry (spec §4, the settable rule), the way
//! `color` is a fill or a stroke and `size` a radius or a width:
//!
//! - a **stroke's dash** on `line`/`step`/`interval` — [`pattern_dasharray`];
//! - a **fill's hatch** on `bar`/`box`/`area`/`ribbon` — [`FillTexture`].
//!
//! `solid` is the shared "no texture" value: on either geometry it collapses to
//! the plain paint, so a plot that names no texture is byte-for-byte what it was
//! before `pattern` grew a fill arm — no `<defs>`, no `url(#…)`.
use crate::data::DataFrame;
use crate::ir::{Channel, Layer};
use std::collections::{HashMap, HashSet};
use std::fmt::Write;

// The legal fill-texture vocabulary — `solid` plus the four hatchings — lives in
// `legality::FILL_TEXTURES`, the vocabulary owner one level down the dependency
// graph (legality depends on this module never; this module names the four it can
// *draw*, `legality` names the five it deems *legal*). `solid` is the no-texture
// identity, so only the four hatchings appear here; a test below asserts the two
// lists never drift. The parts of speech split by geometry — a stroke takes line
// *adjectives* (dashed, dotted), a fill takes texture *nouns* (hatch, crosshatch,
// grid, dots) — the `shape` precedent of a small, plain, closed set.

/// The SVG `stroke-dasharray` attribute for a stroke's `style(pattern = )` value —
/// empty for `"solid"`/unset (so a plain stroke stays byte-for-byte unchanged), a
/// dash otherwise. The leading space lets it drop into a stroke element's attribute
/// list; the line marks' round linecap renders the dotted pattern as round dots.
/// (This is the *stroke* realization of `pattern`; a fill texture is [`FillTexture`].)
pub(crate) fn pattern_dasharray(pattern: Option<&str>) -> &'static str {
    match pattern {
        Some("dashed") => r#" stroke-dasharray="6,4""#,
        Some("dotted") => r#" stroke-dasharray="1,4""#,
        _ => "",
    }
}

// Tile geometry, in user-space pixels. `userSpaceOnUse` anchors the tiling to the
// document origin, so the hatch lines up across neighboring marks rather than
// restarting per shape. Fixed for v1 — Wilkinson's *granularity* (this spacing)
// and *orientation* (the angle) are later knobs, not part of the settable half.
const TILE: f64 = 8.0;
const LINE_W: f64 = 0.9;
const DOT_R: f64 = 1.3;

/// A fill's `style(pattern = )` texture, resolved for one mark layer.
///
/// A fill texture is an SVG `<pattern>` tile drawn in the *mark's* color on a
/// transparent ground, referenced by a shape's `fill="url(#…)"`. The transparent
/// ground is deliberate: `fill-opacity` still governs the whole shape, and two
/// hatched fills that overlap show through each other (the overlaid-bar rule keeps
/// working). The tile is emitted once per (texture, color) actually used — a
/// color-split bar needs one tile per hue — and its id is hashed from that pair,
/// so identical tiles share one definition across the page and different ones never
/// collide (the discipline the color ramp uses for its gradient id; a duplicate
/// `<defs>` with the same id is harmless — the first wins, and both are identical).
///
/// `None` — a `solid`/unset pattern — is the identity: [`FillTexture::fill`]
/// returns the plain color and emits nothing.
pub(crate) struct FillTexture {
    emitted: HashSet<String>,
}

impl FillTexture {
    pub(crate) fn new() -> Self {
        FillTexture { emitted: HashSet::new() }
    }

    /// The `fill` attribute value for a shape whose texture is `texture` (a name,
    /// or `None`), drawn in `color`. `None`, `"solid"`, a stroke's dash, or any
    /// unknown value collapse to the plain color — the identity an untextured plot
    /// relies on. Each of the four hatchings emits its tile's `<defs>` into `svg`
    /// the first time this (texture, color) pair is seen in the layer, then returns
    /// `url(#id)`.
    ///
    /// Taking the texture per call (rather than fixing it at construction) is what
    /// lets one `FillTexture` serve both the `style(pattern = )` *setting* — the
    /// same texture every call — and the mapped `pattern()` *channel* — a different
    /// texture per category (`PatternMap::fill_texture`).
    pub(crate) fn fill(&mut self, svg: &mut String, texture: Option<&str>, color: &str) -> String {
        let Some(t) = texture.filter(|t| matches!(*t, "hatch" | "crosshatch" | "grid" | "dots")) else {
            return color.to_string();
        };
        let id = tile_id(t, color);
        if self.emitted.insert(id.clone()) {
            let _ = writeln!(
                svg,
                r#"    <defs><pattern id="{id}" patternUnits="userSpaceOnUse" width="{TILE}" height="{TILE}">{body}</pattern></defs>"#,
                body = tile_body(t, color)
            );
        }
        format!("url(#{id})")
    }
}

/// The mapped `pattern()` channel resolved for a layer — a category → palette-index
/// assignment, keyed on the pattern column in declared (factor-level, else
/// first-seen) order, the order the legend decodes. Geometry-agnostic: the writer
/// turns an index into a fill's hatch ([`PatternMap::fill_texture`]) or a stroke's
/// dash ([`PatternMap::dash`]) as its own geometry dictates, the one channel
/// realized two ways (spec §5).
pub(crate) struct PatternMap {
    field: String,
    index: HashMap<String, usize>,
    row_cat: Vec<String>,
}

impl PatternMap {
    /// `Some` iff `Channel::Pattern` is bound and names a string column.
    pub(crate) fn resolve(layer: &Layer, df: &DataFrame) -> Option<PatternMap> {
        let field = layer.encodings.get(&Channel::Pattern)?.field.clone();
        let col = df.str_col(&field)?;
        let index = crate::data::categories_across(&[df], &field)
            .into_iter().enumerate().map(|(i, c)| (c, i)).collect();
        Some(PatternMap { field, index, row_cat: col.to_vec() })
    }

    /// The pattern column — a mark that splits per group adds it to its grouping so
    /// `pattern(g)` on its own draws one series per category.
    pub(crate) fn field(&self) -> &str { &self.field }

    /// The palette index for a category (an unknown value sits at 0, the plain slot).
    pub(crate) fn index_of(&self, cat: &str) -> usize {
        self.index.get(cat).copied().unwrap_or(0)
    }

    /// The category of a row, for marks that draw one shape per row (`bar`, `box`).
    pub(crate) fn cat_at(&self, row: usize) -> &str {
        self.row_cat.get(row).map(String::as_str).unwrap_or("")
    }

    /// The fill texture name for a category (`"solid"` for the plain slot).
    pub(crate) fn fill_texture(&self, cat: &str) -> &'static str {
        fill_texture_for_index(self.index_of(cat))
    }

    /// The dash name for a category (`"solid"` for the plain slot).
    pub(crate) fn dash(&self, cat: &str) -> &'static str {
        dash_for_index(self.index_of(cat))
    }
}

/// A fill mark maps a category index → texture: `solid` (the plain first series),
/// then the four hatchings, cycling at five. Index 0 draws plain the way the first
/// line in a set is solid — the most legible, and it leaves `solid` meaning the
/// same "no texture" it does as a setting.
pub(crate) fn fill_texture_for_index(i: usize) -> &'static str {
    match i % 5 {
        0 => "solid",
        1 => "hatch",
        2 => "crosshatch",
        3 => "grid",
        _ => "dots",
    }
}

/// A stroke mark maps a category index → dash: `solid`, `dashed`, `dotted`, cycling
/// at three — a stroke carries fewer distinguishable textures than a fill.
pub(crate) fn dash_for_index(i: usize) -> &'static str {
    match i % 3 {
        0 => "solid",
        1 => "dashed",
        _ => "dotted",
    }
}

/// FNV-1a over `"texture|color"` — the ramp's gradient-id discipline: two ids
/// collide only when the tiles are byte-identical, the one collision that cannot
/// mislead.
fn tile_id(texture: &str, color: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in texture.bytes().chain(std::iter::once(b'|')).chain(color.bytes()) {
        h = (h ^ b as u64).wrapping_mul(0x0100_0000_01b3);
    }
    format!("tex{h:016x}")
}

/// The tile's drawing — hatch/grid as strokes, dots as a filled circle — in the
/// mark's color. A single corner-to-corner diagonal tiles into continuous
/// parallel lines (its endpoints meet the next tile's exactly); `grid` draws the
/// tile's own top and left edges, which repeat into a full grid.
fn tile_body(texture: &str, color: &str) -> String {
    let t = TILE;
    match texture {
        "hatch" => format!(
            r#"<path d="M0,{t} L{t},0" stroke="{color}" stroke-width="{LINE_W}" fill="none"/>"#
        ),
        "crosshatch" => format!(
            r#"<path d="M0,{t} L{t},0 M0,0 L{t},{t}" stroke="{color}" stroke-width="{LINE_W}" fill="none"/>"#
        ),
        "grid" => format!(
            r#"<path d="M0,0 L0,{t} M0,0 L{t},0" stroke="{color}" stroke-width="{LINE_W}" fill="none"/>"#
        ),
        "dots" => format!(
            r#"<circle cx="{c:.1}" cy="{c:.1}" r="{DOT_R}" fill="{color}"/>"#,
            c = t / 2.0
        ),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dasharray_is_empty_for_solid_and_unset() {
        // The stroke realization: only the two dashes emit an attribute, so a
        // plain stroke is untouched.
        assert_eq!(pattern_dasharray(None), "");
        assert_eq!(pattern_dasharray(Some("solid")), "");
        assert!(pattern_dasharray(Some("dashed")).contains("stroke-dasharray"));
        assert!(pattern_dasharray(Some("dotted")).contains("stroke-dasharray"));
    }

    #[test]
    fn solid_and_unset_are_the_identity() {
        // A fill that names no texture (or `solid`) returns the plain color and
        // emits nothing — the byte-for-byte guarantee an untextured plot relies on.
        for p in [None, Some("solid")] {
            let mut tex = FillTexture::new();
            let mut svg = String::new();
            assert_eq!(tex.fill(&mut svg, p, "#4e79a7"), "#4e79a7");
            assert!(svg.is_empty(), "no <defs> for {p:?}");
        }
    }

    #[test]
    fn a_texture_emits_one_def_per_color_and_reuses_it() {
        let mut tex = FillTexture::new();
        let mut svg = String::new();

        let a = tex.fill(&mut svg, Some("hatch"), "#4e79a7");
        assert!(a.starts_with("url(#tex"));
        assert_eq!(svg.matches("<pattern").count(), 1, "first use defines the tile");

        // Same color again: reference reused, no second definition.
        let a2 = tex.fill(&mut svg, Some("hatch"), "#4e79a7");
        assert_eq!(a, a2);
        assert_eq!(svg.matches("<pattern").count(), 1, "same (texture,color) reuses its def");

        // A different color: a second tile.
        let b = tex.fill(&mut svg, Some("hatch"), "#f28e2b");
        assert_ne!(a, b, "different hues get different tiles");
        assert_eq!(svg.matches("<pattern").count(), 2);
        // The tile paints in the mark's color, on a transparent ground.
        assert!(svg.contains(r##"stroke="#4e79a7""##));
        assert!(svg.contains(r##"stroke="#f28e2b""##));
        assert!(!svg.contains("<rect"), "the tile ground is transparent, not a filled rect");
    }

    #[test]
    fn each_texture_draws_a_distinct_tile() {
        // hatch one diagonal, crosshatch two, grid the orthogonal edges, dots a
        // circle — each visibly different so a reader can tell series apart.
        let mut svg = String::new();
        for t in ["hatch", "crosshatch", "grid", "dots"] {
            FillTexture::new().fill(&mut svg, Some(t), "#000");
        }
        assert_eq!(svg.matches("M0,8 L8,0").count(), 2, "hatch + crosshatch share the forward diagonal");
        assert!(svg.contains("M0,0 L8,8"), "crosshatch adds the back diagonal");
        assert!(svg.contains("M0,0 L0,8 M0,0 L8,0"), "grid draws the orthogonal edges");
        assert!(svg.contains("<circle"), "dots draws a stipple circle");
    }

    #[test]
    fn render_draws_every_texture_legality_deems_legal() {
        // The drift guard between the two lists: every legal fill texture but
        // `solid` must draw a tile, and `solid` must draw none. If `legality`
        // grows a sixth texture and this module forgets to draw it, this fails —
        // the settable-table drift the project fears, caught in the renderer.
        for &t in crate::legality::FILL_TEXTURES.iter() {
            let mut svg = String::new();
            FillTexture::new().fill(&mut svg, Some(t), "#000");
            if t == "solid" {
                assert!(svg.is_empty(), "`solid` is the no-texture identity");
            } else {
                assert!(svg.contains("<pattern"), "`{t}` is legal but draws no tile");
            }
        }
    }

    #[test]
    fn stroke_values_and_unknowns_do_not_texture_a_fill() {
        // The renderer never conjures a texture the legality check refused: a
        // stroke dash or a typo on a fill collapses to the plain color here (the
        // user already saw the directional error).
        for p in [Some("dashed"), Some("dotted"), Some("wiggly")] {
            let mut tex = FillTexture::new();
            let mut svg = String::new();
            assert_eq!(tex.fill(&mut svg, p, "#123456"), "#123456");
            assert!(svg.is_empty());
        }
    }

    #[test]
    fn the_channel_palettes_start_plain_and_are_distinct() {
        // Index 0 is the plain slot on both geometries, so `solid` keeps meaning
        // "no texture"; the rest are distinct up to the palette size, then cycle.
        assert_eq!(fill_texture_for_index(0), "solid");
        assert_eq!(dash_for_index(0), "solid");
        let fills: Vec<_> = (0..5).map(fill_texture_for_index).collect();
        assert_eq!(fills, ["solid", "hatch", "crosshatch", "grid", "dots"]);
        assert_eq!(fill_texture_for_index(5), "solid", "the five fill textures cycle");
        let dashes: Vec<_> = (0..3).map(dash_for_index).collect();
        assert_eq!(dashes, ["solid", "dashed", "dotted"]);
        assert_eq!(dash_for_index(3), "solid", "the three dashes cycle");
    }

    #[test]
    fn pattern_map_indexes_categories_in_declared_order() {
        use crate::data::DataFrame;
        use crate::ir::{Layer, Mark};
        let df = DataFrame::new().with_str(
            "g", vec!["b".into(), "a".into(), "b".into(), "c".into()],
        );
        let layer = Layer::new(Mark::Bar).encode(Channel::Pattern, "g");
        let pm = PatternMap::resolve(&layer, &df).expect("pattern is bound to a string column");
        // First-seen order: b, a, c → 0, 1, 2.
        assert_eq!(pm.index_of("b"), 0);
        assert_eq!(pm.index_of("a"), 1);
        assert_eq!(pm.index_of("c"), 2);
        assert_eq!(pm.index_of("zzz"), 0, "an unknown category sits in the plain slot");
        // Per-row and the geometry-specific lookups agree with the index.
        assert_eq!(pm.cat_at(0), "b");
        assert_eq!(pm.fill_texture("a"), "hatch");
        assert_eq!(pm.dash("a"), "dashed");
        assert_eq!(pm.fill_texture("b"), "solid");
        assert_eq!(pm.field(), "g");
        // Unbound → None.
        let plain = Layer::new(Mark::Bar);
        assert!(PatternMap::resolve(&plain, &df).is_none());
    }
}
