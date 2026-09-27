//! The `point` mark — a glyph at each (x, y); the `jitter` spread and the `dodge`
//! swarm live here too.
use std::collections::{BTreeMap, HashMap};
use std::fmt::Write;
use crate::data::DataFrame;
use crate::ir::{Channel, Layer, Transform};
use crate::legality::{Diagnostic, DiagnosticKind};
use crate::render::palette::{ramp_at, PALETTE_GOG};
use crate::render::polar::Polar;
use crate::render::project;
use crate::render::shape::{shape_at_index, shape_by_name, write_shape, ShapeKind};
use crate::render::encode::{opacity_at, radius_at, OPACITY_DEFAULT};
use crate::render::svg::{unit_norm, SvgRenderer};
use crate::render::text::esc;
use crate::render::{hash01, Layout, Whole};
use super::{bar_thickness_svg, Dodge};

impl SvgRenderer {
    // -----------------------------------------------------------------------
    // Mark: point
    // -----------------------------------------------------------------------

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn write_points(
        &self, svg: &mut String, layer: &Layer, df: &DataFrame, whole: &Whole<'_>,
        l: &Layout, xs: (f64, f64), ys: (f64, f64),
        x_field: &str, y_field: &str,
        cat_x: Option<&[String]>, cat_y: Option<&[String]>,
        color_map: &HashMap<String, String>,
        ramp: &[String],
        clip: &str,
        // 3-D: when `scene` is `Some`, x/y/z are normalized into the unit cube,
        // projected, and depth-sorted. When `None` this is the ordinary 2-D path
        // and `zs`/`z_field` go unread — the flat plot is the degenerate case.
        zs: (f64, f64), z_field: &str,
        scene: Option<&project::Scene>,
        // Polar: a glyph at an angle and a radius instead of an x and a y. The
        // two spaces are exclusive — `check_polar` refuses `polar()` with a `z` —
        // so at most one of `scene`/`polar` is ever `Some`.
        polar: Option<&Polar>,
        // Globe: a glyph at its place on the sphere's facing hemisphere. `x` is
        // longitude and `y` latitude, read raw — the sphere places degrees, not
        // fitted fractions — and a row this view cannot see keeps its slot with
        // no coordinate, so the finite-skip below drops what the caller has
        // already counted and reported. Exclusive with both `scene` and `polar`
        // (`check_globe` refuses the pairs), so at most one of the three is
        // ever `Some`.
        globe: Option<&crate::render::globe::Globe>,
    ) -> SwarmTally {
        // Where each dot sits and how large it is: one routine, shared with the
        // repelled labels that have to clear these dots (`panel_dots`), so the
        // dot a label steps around is the dot this writer draws.
        let Some((coords, radii, swarm)) = self.dot_geometry(layer, df, whole, l, xs, ys, x_field,
            y_field, cat_x, cat_y, zs, z_field, scene, polar, globe) else { return SwarmTally::default() };

        let color_labels = layer.encodings.get(&Channel::Color).and_then(|c| df.str_col(&c.field));
        // A numeric color column takes the sequential ramp instead of the
        // categorical palette. Which one applies is decided by the column, not
        // by a second atom.
        let color_vals   = layer.encodings.get(&Channel::Color).and_then(|c| df.float_col(&c.field));
        let shape_labels = layer.encodings.get(&Channel::Shape).and_then(|c| df.str_col(&c.field));
        let opacity_vals = layer.encodings.get(&Channel::Opacity).and_then(|c| df.float_col(&c.field));

        // One scale object per continuous channel. Each knows whether it runs
        // logarithmically, so `color`, `size` and `opacity` all inherit the log
        // scale from the same place instead of three parallel copies. Each is the
        // whole layer's, so a dot in one panel, moment or pass reads the same
        // legend as a dot in another.
        let color_scale = whole.channel_scale(layer, &Channel::Color);

        // A set value replaces the renderer's built-in default. It never
        // competes with a mapped channel — `legality::check_style` refuses a
        // layer that both maps and sets the same feature, so at most one of the
        // two is present here.
        let st = &layer.style;
        let default_color = st.color.as_deref().map(esc).unwrap_or_else(|| PALETTE_GOG[0].to_string());
        let default_opacity = st.opacity.unwrap_or(OPACITY_DEFAULT);
        let default_shape = st.shape.as_deref().map(shape_by_name).unwrap_or(ShapeKind::Circle);

        // Build shape lookup: unique string → ShapeKind (in first-appearance order).
        // Same ordering as the shape legend, from the same fit of the whole layer —
        // a glyph assigned in one order and decoded in another is worse than either.
        let shape_order: Vec<String> = match layer.encodings.get(&Channel::Shape) {
            Some(cd) => whole.categories(&cd.field),
            None => Vec::new(),
        };
        let shape_map: Vec<(&str, ShapeKind)> = shape_order.iter().enumerate()
            .map(|(i, s)| (s.as_str(), shape_at_index(i)))
            .collect();

        // Opacity scale: data range → [OPACITY_MIN, OPACITY_MAX].
        let op_scale = whole.channel_scale(layer, &Channel::Opacity);

        let n = coords.len();
        let mut order: Vec<usize> = (0..n).collect();
        // Depth-sorted wherever there is a depth: the cube's, or the sphere's
        // facing hemisphere, where a nearer place paints over a farther one.
        if scene.is_some() || globe.is_some() {
            order.sort_by(|&a, &b| {
                coords[b].2.partial_cmp(&coords[a].2).unwrap_or(std::cmp::Ordering::Equal)
            });
        }

        writeln!(svg, r##"  <g clip-path="url(#{clip})">"##).unwrap();
        // The optional rim (spec §4, the settable rule): `border_color`/`border_size`
        // stroke each filled glyph's perimeter. A bare `border_size` takes a dark
        // default color so it still shows; a bare `border_color` a 1px default width.
        // `write_shape` draws it on the fillable glyphs and skips a `cross`.
        let border: Option<(&str, f64)> = match (layer.style.border_color.as_deref(), layer.style.border_size) {
            (None, None) => None,
            (bc, bw) => Some((bc.unwrap_or("#3c3c46"), bw.unwrap_or(1.0).max(0.0))),
        };

        for &i in &order {
            let (cx, cy, _) = coords[i];
            // A value a log scale cannot place has no coordinate. Skipping is
            // reported once by `warn_unplaceable`; emitting `cx="NaN"` would
            // produce SVG no renderer accepts.
            if !cx.is_finite() || !cy.is_finite() { continue }

            let ramped: String;
            let color: &str = if let Some(labels) = color_labels {
                let lbl = labels.get(i).map(String::as_str).unwrap_or("");
                color_map.get(lbl).map(String::as_str).unwrap_or(&default_color)
            } else if let Some(vals) = color_vals {
                let f = color_scale.fraction(vals.get(i).copied().unwrap_or(f64::NAN));
                ramped = ramp_at(&ramp.iter().map(String::as_str).collect::<Vec<_>>(), f);
                &ramped
            } else {
                &default_color
            };

            let shape = if let Some(labels) = shape_labels {
                let lbl = labels.get(i).map(String::as_str).unwrap_or("");
                shape_map.iter().find(|(s, _)| *s == lbl).map(|(_, k)| *k).unwrap_or(ShapeKind::Circle)
            } else {
                default_shape
            };

            let radius = radii[i];

            let opacity = match opacity_vals {
                Some(col) => opacity_at(op_scale.fraction(col.get(i).copied().unwrap_or(f64::NAN))),
                None => default_opacity,
            };

            write_shape(svg, shape, cx, cy, radius, color, opacity, border);
        }
        writeln!(svg, "  </g>").unwrap();
        swarm
    }

    /// Where each row's dot sits on the page (with a depth, for the spaces that
    /// have one) and the radius it is drawn at: a mapped `size`, a set one, or the
    /// default. `None` when a position column cannot be read, and nothing draws.
    ///
    /// Split out of [`write_points`](Self::write_points) because a repelled label
    /// has to know the same two facts about every dot in its panel, and a second
    /// copy of this reading is a second answer to *where is that dot* that could
    /// drift from the one the reader sees. The swarm is placed here for the same
    /// reason: a label steps around the dot where the swarm put it. The third
    /// value is what the swarm could not clear, for the caller to report.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn dot_geometry(
        &self, layer: &Layer, df: &DataFrame, whole: &Whole<'_>,
        l: &Layout, xs: (f64, f64), ys: (f64, f64),
        x_field: &str, y_field: &str,
        cat_x: Option<&[String]>, cat_y: Option<&[String]>,
        zs: (f64, f64), z_field: &str,
        scene: Option<&project::Scene>,
        polar: Option<&Polar>,
        globe: Option<&crate::render::globe::Globe>,
    ) -> Option<(Vec<(f64, f64, f64)>, Vec<f64>, SwarmTally)> {
        // A point places against either axis the same way, so both positions go
        // through the one resolution (`super::positions`): a numeric column as it
        // stands, a string column as its category index. A category on x is a
        // strip plot, on y a horizontal one — no per-mark and no per-axis
        // exception, which is exactly what sharing the resolver enforces.
        let x_vals = super::positions(df, x_field, cat_x)?;
        let y_vals = super::positions(df, y_field, cat_y)?;

        // Resolve z the same way x and y were resolved above — a numeric column
        // directly, or a category mapped to its index. Read only in 3-D.
        let owned_z: Vec<f64>;
        let z_vals: &[f64] = if scene.is_none() {
            &[]
        } else if let Some(vals) = df.float_col(z_field) {
            vals
        } else {
            let cats = whole.categories(z_field);
            match df.str_col(z_field) {
                Some(str_vals) => {
                    owned_z = str_vals.iter()
                        .map(|s| cats.iter().position(|c| c == s).map(|i| i as f64).unwrap_or(0.0))
                        .collect();
                    &owned_z
                }
                None => &[],
            }
        };

        // Screen position and a depth for every row. In 2-D the depth is a
        // constant and the draw order is the data order; in 3-D each coordinate
        // is normalized into the unit cube, projected, and the points are painted
        // far-to-near so a nearer point lands on top of a farther one.
        let n = if scene.is_some() {
            x_vals.len().min(y_vals.len()).min(z_vals.len())
        } else {
            x_vals.len().min(y_vals.len())
        };

        // `point * jitter` spreads coincident points sideways within their slot
        // (spec §5). Only a *categorical* position axis is spread — a numeric axis
        // carries a measured value jitter must not move, so its band is zero — and
        // only in 2-D (the strip plot is a flat form). A continuous/continuous plot
        // never reaches here: `legality::check_jitter` refuses it.
        let jitter = Jitter::resolve(layer);
        // The band is a slot's width, in pixels on the flat path. Bent into a
        // circle a slot has no pixel width, so there the band is measured in scale
        // units and the spread is applied to the datum *before* it is placed —
        // nudging the finished pixel would push every point the same way on the
        // page whatever angle it sits at (`bar_thickness` makes the same swap).
        let (jx_band, jy_band) = if jitter.active && scene.is_none() {
            // `jitter(amount)` scales the slot-derived band; bare `jitter` is 1.0.
            let to_units = |band: f64, scale: (f64, f64), px: f64| match polar {
                Some(_) => band * (scale.1 - scale.0).abs().max(1e-12) / px.max(1e-12),
                None => band,
            };
            let bx = if df.str_col(x_field).is_some() {
                to_units(bar_thickness_svg(&x_vals, n, l.w(), xs, false) * jitter.amount, xs, l.w())
            } else { 0.0 };
            let by = if df.str_col(y_field).is_some() {
                to_units(bar_thickness_svg(&y_vals, n, l.h(), ys, false) * jitter.amount, ys, l.h())
            } else { 0.0 };
            (bx, by)
        } else {
            (0.0, 0.0)
        };

        let mut coords: Vec<(f64, f64, f64)> = (0..n).map(|i| {
            if let Some(g) = globe {
                return match g.place(x_vals[i], y_vals[i]) {
                    Some(s) => (s.x, s.y, s.depth),
                    None => (f64::NAN, f64::NAN, 0.0),
                };
            }
            match scene {
            None => {
                let jx = jitter.offset(i, x_vals[i], y_vals[i], 0, jx_band);
                let jy = jitter.offset(i, x_vals[i], y_vals[i], 0x5DEECE66D, jy_band);
                match polar {
                    Some(_) => {
                        let (px, py) = super::place(l, polar, x_vals[i] + jx, y_vals[i] + jy, xs, ys);
                        (px, py, 0.0)
                    }
                    None => (
                        l.map_x(x_vals[i], xs.0, xs.1) + jx,
                        l.map_y(y_vals[i], ys.0, ys.1) + jy,
                        0.0,
                    ),
                }
            }
            Some(sc) => {
                let p = sc.to_screen(
                    unit_norm(x_vals[i], xs),
                    unit_norm(y_vals[i], ys),
                    unit_norm(z_vals[i], zs),
                );
                (p.x, p.y, p.depth)
            }
        }}).collect();
        // Size scale: data range → [SIZE_MIN_R, SIZE_MAX_R]. A set value replaces
        // the renderer's built-in default; `legality::check_style` refuses a layer
        // that both maps and sets it, so at most one of the two is present here.
        let size_vals = layer.encodings.get(&Channel::Size).and_then(|c| df.float_col(&c.field));
        let size_scale = whole.channel_scale(layer, &Channel::Size);
        let default_radius = layer.style.size.unwrap_or(self.point_radius);
        let radii: Vec<f64> = (0..n).map(|i| match size_vals {
            Some(col) => radius_at(size_scale.fraction(col.get(i).copied().unwrap_or(f64::NAN))),
            None => default_radius,
        }).collect();

        // `point * dodge` — the swarm (spec §5). Placed last, from the radii the
        // dots are drawn at, and only on the flat path: `legality::check_swarm`
        // refuses the cube and the disc, so neither ever carries one here.
        let mut swarm = SwarmTally::default();
        if scene.is_none() && polar.is_none() && globe.is_none() {
            if let Some(s) = Swarm::resolve(layer, df, x_field, y_field) {
                // A rim is ink too: a set border widens every glyph by half its stroke.
                let rim = match (layer.style.border_color.as_ref(), layer.style.border_size) {
                    (None, None) => 0.0,
                    (_, w) => w.unwrap_or(1.0).max(0.0) / 2.0,
                };
                let cats = if s.along_x { cat_x } else { cat_y };
                swarm = s.place(&mut coords, &radii, rim, &x_vals, &y_vals, n, l, xs, ys, cats);
            }
        }
        Some((coords, radii, swarm))
    }

    /// Every dot the `point` layers draw in one panel at one moment, as
    /// `(x, y, radius)` on the page: what a repelled label steps around.
    ///
    /// Read from each point layer's **own** frame, not the label layer's. A label
    /// layer given its own small table (the few rows worth naming) sits over a
    /// point layer holding every row, and the unnamed dots are exactly the ones
    /// the label table cannot tell it about. Each is read whole, whatever the
    /// selection: a dimmed dot is still drawn, and still under a word that lands
    /// on it.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn panel_dots(
        &self, spec: &crate::ir::PlotSpec, eff: &[DataFrame], wholes: &[Whole<'_>],
        l: &Layout, xs: (f64, f64), ys: (f64, f64),
        x_field: &str, y_field: &str,
        cat_x: Option<&[String]>, cat_y: Option<&[String]>,
        polar: Option<&Polar>,
        globe: Option<&crate::render::globe::Globe>,
    ) -> Vec<(f64, f64, f64)> {
        let mut out = Vec::new();
        for ((layer, df), whole) in spec.layers.iter().zip(eff).zip(wholes) {
            if layer.mark != crate::ir::Mark::Point || df.is_empty() { continue }
            let Some((coords, radii, _)) = self.dot_geometry(layer, df, whole, l, xs, ys, x_field,
                y_field, cat_x, cat_y, (0.0, 1.0), "", None, polar, globe) else { continue };
            for (&(x, y, _), &r) in coords.iter().zip(&radii) {
                if x.is_finite() && y.is_finite() { out.push((x, y, r)); }
            }
        }
        out
    }
}

/// The spread for a `point * jitter` strip plot (spec §5) — `dodge`'s render-stage
/// kin. Where many points share a categorical position they land on one line and
/// hide the density; jitter nudges each one sideways within its slot. It is
/// **bounded to the slot**, so — unlike `stack` — it never moves the scale domain;
/// and it offsets **only a categorical position axis**, never a measured one (the
/// caller decides per-axis and passes a zero band for a continuous axis, which
/// `legality::check_jitter` has already ruled out for the both-continuous case).
///
/// The offset is **deterministic**: `hash01` of a seed mixed from the row's index
/// and its data, so coincident points still separate (the index differs) yet the
/// same spec always renders the same picture (the property the IR exists to
/// guarantee). No clock, no global RNG.
struct Jitter {
    active: bool,
    /// Multiplier on the slot-derived band — `jitter(amount)`, default 1.0. The
    /// spread is a free legibility choice (unlike `dodge`'s determined width), so
    /// it takes a knob; `0.0` collapses it back to no jitter.
    amount: f64,
}

impl Jitter {
    fn resolve(layer: &Layer) -> Jitter {
        let active = layer.transforms.iter().any(|t| matches!(t, Transform::Jitter));
        // A negative amount would only mirror the (symmetric) spread, so a clamp at
        // 0 is harmless; the R binding already refuses negatives with direction.
        let amount = layer.jitter.as_ref().and_then(|j| j.amount).unwrap_or(1.0).max(0.0);
        Jitter { active, amount }
    }

    /// A signed offset in `[-band/2, band/2]` for the point on `row`, seeded from
    /// the row and its data so it is stable across runs. `salt` decorrelates the x
    /// and y spreads (a point must not shift along the diagonal). A zero band — a
    /// continuous axis, or no jitter — yields no offset.
    fn offset(&self, row: usize, x: f64, y: f64, salt: u64, band: f64) -> f64 {
        if !self.active || band <= 0.0 {
            return 0.0;
        }
        let seed = (row as u64)
            .wrapping_mul(0x9E3779B97F4A7C15)
            ^ x.to_bits().rotate_left(17)
            ^ y.to_bits().rotate_left(43)
            ^ salt;
        (hash01(seed) - 0.5) * band
    }
}

/// The air left between two swarmed glyphs, in pixels, beyond their radii: the
/// hairline that keeps two touching dots reading as two rather than as one
/// figure eight once antialiasing has softened their edges.
const SWARM_AIR: f64 = 0.5;

/// `point * dodge` — the beeswarm (spec §5), and `dodge` read on a glyph.
///
/// **One rule for every mark `dodge` takes.** A split tiles each slot exactly as
/// it tiles a bar's (`Dodge`, unchanged), and the marks that still collide inside
/// a tile move apart along the slot by the least amount that clears them. A bar
/// never collides inside its tile, so the rule draws it as it always has; points
/// do, and they swarm. Wilkinson's `point.dodge.symmetric`, his Figure 8.26.
///
/// **The measure axis stays true.** A point moves along the category axis only,
/// the way `jitter` does, so every value is read where it was drawn. Where
/// `jitter` moves a point by a seeded amount, the swarm moves it by the least
/// amount its neighbors force, a determined value, so it takes no knob.
///
/// Placed at the render stage from the drawn glyph sizes, as `jitter` and `repel`
/// are, because what collides is ink on the page and not a value in the table.
struct Swarm {
    /// The categories run along `x`, so points move horizontally; `false` is the
    /// horizontal strip, where the categories run along `y`.
    along_x: bool,
    /// The tiles a split cuts each slot into — the bar's tiling. `None` when
    /// nothing splits the points, and the whole slot is one tile.
    tiles: Option<Dodge>,
}

impl Swarm {
    fn resolve(layer: &Layer, df: &DataFrame, x_field: &str, y_field: &str) -> Option<Swarm> {
        if !layer.transforms.contains(&Transform::Dodge) {
            return None;
        }
        let along_x = match (df.str_col(x_field).is_some(), df.str_col(y_field).is_some()) {
            (true, false) => true,
            (false, true) => false,
            // Two categories or none: `legality::check_swarm` refused the sentence,
            // and under `GOG_STRICT=0` the points are drawn where they stand.
            _ => return None,
        };
        let slot_field = if along_x { x_field } else { y_field };
        Some(Swarm { along_x, tiles: Dodge::resolve(layer, df, slot_field) })
    }

    /// Move every point to its place in its tile's swarm, and say what could not
    /// be cleared.
    #[allow(clippy::too_many_arguments)]
    fn place(
        &self, coords: &mut [(f64, f64, f64)], radii: &[f64], rim: f64,
        x_vals: &[f64], y_vals: &[f64], n: usize,
        l: &Layout, xs: (f64, f64), ys: (f64, f64), cats: Option<&[String]>,
    ) -> SwarmTally {
        let (slot_vals, values, slot_px, slot_scale) = if self.along_x {
            (x_vals, y_vals, l.w(), xs)
        } else {
            (y_vals, x_vals, l.h(), ys)
        };
        // The band a bar in this slot would fill, measured the way `bar` measures
        // it, and never wider than one category's: a frame holding only every other
        // category would hand each one two slots' worth, and a swarm grown into it
        // would stand in the empty neighbor's place.
        let band = bar_thickness_svg(slot_vals, n, slot_px, slot_scale, false)
            .min(bar_thickness_svg(&[0.0, 1.0], 2, slot_px, slot_scale, false));
        let tiles = self.tiles.as_ref().map_or(1.0, Dodge::count);
        let half = band / tiles / 2.0;

        // The points that can collide are the ones in one tile of one slot. Keyed
        // in slot order, so a report names the crowded slots in axis order.
        let mut by_tile: BTreeMap<(i64, usize), Vec<usize>> = BTreeMap::new();
        for i in 0..n {
            let (cx, cy, _) = coords[i];
            if !(cx.is_finite() && cy.is_finite() && values[i].is_finite() && slot_vals[i].is_finite()) {
                continue;
            }
            let tile = self.tiles.as_ref().and_then(|d| d.rank(i)).unwrap_or(0);
            by_tile.entry((slot_vals[i].round() as i64, tile)).or_default().push(i);
        }

        let mut tally = SwarmTally::default();
        for ((slot, _), mut rows) in by_tile {
            // Placement order: by value, then by row. Deterministic, so one
            // sentence is one picture whatever order the table arrived in.
            rows.sort_by(|&a, &b| values[a].partial_cmp(&values[b])
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.cmp(&b)));
            let measure_px = |c: (f64, f64, f64)| if self.along_x { c.1 } else { c.0 };
            let v: Vec<f64> = rows.iter().map(|&i| measure_px(coords[i])).collect();
            let r: Vec<f64> = rows.iter().map(|&i| radii[i] + rim).collect();
            let (off, crowded) = swarm_offsets(&v, &r, half);
            for (k, &i) in rows.iter().enumerate() {
                let shift = self.tiles.as_ref().map_or(0.0, |d| d.offset_at(i, band)) + off[k];
                if self.along_x { coords[i].0 += shift } else { coords[i].1 += shift }
            }
            tally.points += rows.len();
            if crowded > 0 {
                tally.crowded += crowded;
                let name = cats
                    .and_then(|c| usize::try_from(slot).ok().and_then(|s| c.get(s)))
                    .cloned()
                    .unwrap_or_default();
                if !name.is_empty() && !tally.slots.contains(&name) {
                    tally.slots.push(name);
                }
            }
        }
        tally
    }
}

/// The swarm inside one tile: how far along the slot each point moves, and how
/// many still overlap another once all of them are placed.
///
/// `v` is each point's page position along the **measure** axis and `r` its
/// radius, both in placement order (by value, then by row). Each point in turn
/// takes the offset nearest the tile's center at which it touches nothing placed
/// before it: 0 when nothing is in the way, otherwise a position just touching one
/// of its placed neighbors, which is where the nearest clear offset always lies.
/// When the two sides are equally near, the side holding fewer points wins, so the
/// swarm grows symmetrically about the center.
///
/// **A point never leaves its tile.** One whose nearest clear offset lies past the
/// edge is held at the edge instead, on the side with fewer points held there,
/// and it overlaps its neighbors in plain sight. It is not an obstacle to the
/// points after it, which keeps the search local however many are held.
fn swarm_offsets(v: &[f64], r: &[f64], half: f64) -> (Vec<f64>, usize) {
    let n = v.len();
    let r_max = r.iter().copied().fold(0.0f64, f64::max);
    // Farther apart than this along the measure axis, two points cannot touch.
    let reach = 2.0 * r_max + SWARM_AIR;
    let mut off = vec![0.0; n];
    let mut placed: Vec<usize> = Vec::new();
    let mut first = 0usize;
    let (mut right, mut left) = (0usize, 0usize);
    let (mut held_right, mut held_left) = (0usize, 0usize);
    let mut cands: Vec<f64> = Vec::new();
    for i in 0..n {
        // The placed points near enough to touch this one. Placement runs in value
        // order, so they are always a tail of `placed`.
        while first < placed.len() && (v[i] - v[placed[first]]).abs() >= reach {
            first += 1;
        }
        let near = &placed[first..];
        cands.clear();
        cands.push(0.0);
        for &j in near {
            let d = r[i] + r[j] + SWARM_AIR;
            let dv = v[i] - v[j];
            if dv.abs() < d {
                let h = (d * d - dv * dv).sqrt();
                cands.push(off[j] + h);
                cands.push(off[j] - h);
            }
        }
        let prefer = if right <= left { 1.0 } else { -1.0 };
        cands.sort_by(|a, b| {
            let (aa, bb) = (a.abs(), b.abs());
            if (aa - bb).abs() > 1e-9 {
                aa.partial_cmp(&bb).unwrap_or(std::cmp::Ordering::Equal)
            } else {
                (b * prefer).partial_cmp(&(a * prefer)).unwrap_or(std::cmp::Ordering::Equal)
            }
        });
        let clear = |c: f64| near.iter().all(|&j| {
            let d = r[i] + r[j] + SWARM_AIR;
            let (dx, dv) = (c - off[j], v[i] - v[j]);
            dx * dx + dv * dv >= d * d - 1e-6
        });
        // The farthest touching offset always clears, so a clear one is always found.
        let best = cands.iter().copied().find(|&c| clear(c)).unwrap_or(0.0);
        let limit = (half - r[i]).max(0.0);
        if best.abs() <= limit + 1e-9 {
            off[i] = best;
            placed.push(i);
            if best > 1e-9 {
                right += 1;
            } else if best < -1e-9 {
                left += 1;
            }
        } else {
            let side = if held_right < held_left {
                1.0
            } else if held_left < held_right {
                -1.0
            } else if best >= 0.0 {
                1.0
            } else {
                -1.0
            };
            if side > 0.0 { held_right += 1 } else { held_left += 1 }
            off[i] = side * limit;
        }
    }
    // What still overlaps, counted on the ink with the air taken back off, so the
    // number names points a reader can see covering one another (`repel`'s rule).
    let hits = |i: usize, j: usize| {
        let d = (r[i] + r[j] - 0.01).max(0.0);
        let (dx, dv) = (off[i] - off[j], v[i] - v[j]);
        dx * dx + dv * dv < d * d
    };
    let crowded = (0..n).filter(|&i| {
        let before = (0..i).rev().take_while(|&j| (v[i] - v[j]).abs() < reach).any(|j| hits(i, j));
        before || (i + 1..n).take_while(|&j| (v[i] - v[j]).abs() < reach).any(|j| hits(i, j))
    }).count();
    (off, crowded)
}

/// What a swarm could not clear, summed over every panel and moment it is drawn
/// in, so the plot says it once.
#[derive(Debug, Default, Clone)]
pub(crate) struct SwarmTally {
    /// Points the swarm placed.
    pub(crate) points: usize,
    /// Of those, how many still overlap another point's ink.
    pub(crate) crowded: usize,
    /// The categories whose slots ran out of room, in axis order.
    pub(crate) slots: Vec<String>,
}

impl SwarmTally {
    pub(crate) fn absorb(&mut self, other: SwarmTally) {
        self.points += other.points;
        self.crowded += other.crowded;
        for s in other.slots {
            if !self.slots.contains(&s) {
                self.slots.push(s);
            }
        }
    }

    /// The Assumption a crowded swarm owes its reader (spec §12): the plot drew,
    /// every point is on it, here is where they overlap and what gives them room.
    /// Never spilled instead: a point pushed into the next slot would be read as
    /// the next category's row, which is `JITTER_MAX`'s ruling for the same edge.
    pub(crate) fn remark(&self) -> Option<Diagnostic> {
        if self.crowded == 0 {
            return None;
        }
        let (crowded, points) = (self.crowded, self.points);
        let place = match self.slots.as_slice() {
            [] => String::new(),
            [a] => format!(" in `{a}`"),
            [a, b] => format!(" in `{a}` and `{b}`"),
            [a, b, rest @ ..] => format!(" in `{a}`, `{b}` and {} more", rest.len()),
        };
        Some(Diagnostic {
            kind: DiagnosticKind::Assumption,
            message: format!(
                "gog: `point * dodge` has no room to set {crowded} of {points} points clear of \
                 the others{place}: the swarm there is wider than its slot, and a point moved any \
                 further would sit in the next category's. They are drawn against the edge of \
                 their own slot, over their neighbors, so every point is still on the plot. A \
                 larger plot (`theme(width =, height =)`) or a smaller `style(size = )` gives each \
                 point room. With this many rows, `ribbon * density` draws each category's \
                 distribution as a violin instead."
            ),
        })
    }
}
