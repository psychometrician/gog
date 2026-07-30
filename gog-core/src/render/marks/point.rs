//! The `point` mark — a glyph at each (x, y); the `jitter` spread lives here too.
use std::collections::HashMap;
use std::fmt::Write;
use crate::data::DataFrame;
use crate::ir::{Channel, Layer, Transform};
use crate::render::palette::{ramp_at, PALETTE_GOG};
use crate::render::polar::Polar;
use crate::render::project;
use crate::render::shape::{shape_at_index, shape_by_name, write_shape, ShapeKind};
use crate::render::encode::{opacity_at, radius_at, OPACITY_DEFAULT};
use crate::render::svg::{unit_norm, SvgRenderer};
use crate::render::text::esc;
use crate::render::Layout;
use crate::scale;
use super::bar_thickness_svg;

impl SvgRenderer {
    // -----------------------------------------------------------------------
    // Mark: point
    // -----------------------------------------------------------------------

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn write_points(
        &self, svg: &mut String, layer: &Layer, df: &DataFrame,
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
    ) {
        // A point places against either axis the same way, so both positions go
        // through the one resolution (`super::positions`): a numeric column as it
        // stands, a string column as its category index. A category on x is a
        // strip plot, on y a horizontal one — no per-mark and no per-axis
        // exception, which is exactly what sharing the resolver enforces.
        let Some(x_vals) = super::positions(df, x_field, cat_x) else { return };
        let Some(y_vals) = super::positions(df, y_field, cat_y) else { return };

        let color_labels = layer.encodings.get(&Channel::Color).and_then(|c| df.str_col(&c.field));
        // A numeric color column takes the sequential ramp instead of the
        // categorical palette. Which one applies is decided by the column, not
        // by a second atom.
        let color_vals   = layer.encodings.get(&Channel::Color).and_then(|c| df.float_col(&c.field));
        let shape_labels = layer.encodings.get(&Channel::Shape).and_then(|c| df.str_col(&c.field));
        let size_vals    = layer.encodings.get(&Channel::Size).and_then(|c| df.float_col(&c.field));
        let opacity_vals = layer.encodings.get(&Channel::Opacity).and_then(|c| df.float_col(&c.field));

        // One scale object per continuous channel. Each knows whether it runs
        // logarithmically, so `color`, `size` and `opacity` all inherit the log
        // scale from the same place instead of three parallel copies.
        let color_scale = match color_vals {
            Some(c) => scale::ChannelScale::of(c, layer.encodings.get(&Channel::Color)),
            None => scale::ChannelScale::unbound(),
        };

        // A set value replaces the renderer's built-in default. It never
        // competes with a mapped channel — `legality::check_style` refuses a
        // layer that both maps and sets the same feature, so at most one of the
        // two is present here.
        let st = &layer.style;
        let default_color = st.color.as_deref().map(esc).unwrap_or_else(|| PALETTE_GOG[0].to_string());
        let default_radius = st.size.unwrap_or(self.point_radius);
        let default_opacity = st.opacity.unwrap_or(OPACITY_DEFAULT);
        let default_shape = st.shape.as_deref().map(shape_by_name).unwrap_or(ShapeKind::Circle);

        // Build shape lookup: unique string → ShapeKind (in first-appearance order).
        // Same ordering as the shape legend, from the same function — a glyph
        // assigned in one order and decoded in another is worse than either.
        let shape_order: Vec<String> = match layer.encodings.get(&Channel::Shape) {
            Some(cd) => crate::data::categories_across(&[df], &cd.field),
            None => Vec::new(),
        };
        let shape_map: Vec<(&str, ShapeKind)> = shape_order.iter().enumerate()
            .map(|(i, s)| (s.as_str(), shape_at_index(i)))
            .collect();

        // Size scale: data range → [SIZE_MIN_R, SIZE_MAX_R].
        let size_scale = match size_vals {
            Some(c) => scale::ChannelScale::of(c, layer.encodings.get(&Channel::Size)),
            None => scale::ChannelScale::unbound(),
        };
        // Opacity scale: data range → [OPACITY_MIN, OPACITY_MAX].
        let op_scale = match opacity_vals {
            Some(c) => scale::ChannelScale::of(c, layer.encodings.get(&Channel::Opacity)),
            None => scale::ChannelScale::unbound(),
        };

        // Resolve z the same way x and y were resolved above — a numeric column
        // directly, or a category mapped to its index. Read only in 3-D.
        let owned_z: Vec<f64>;
        let z_vals: &[f64] = if scene.is_none() {
            &[]
        } else if let Some(vals) = df.float_col(z_field) {
            vals
        } else {
            let cats = crate::data::categories_across(&[df], z_field);
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

        let coords: Vec<(f64, f64, f64)> = (0..n).map(|i| match scene {
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
        }).collect();
        let mut order: Vec<usize> = (0..n).collect();
        if scene.is_some() {
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

            let radius = if let Some(col) = size_vals {
                radius_at(size_scale.fraction(col.get(i).copied().unwrap_or(f64::NAN)))
            } else {
                default_radius
            };

            let opacity = match opacity_vals {
                Some(col) => opacity_at(op_scale.fraction(col.get(i).copied().unwrap_or(f64::NAN))),
                None => default_opacity,
            };

            write_shape(svg, shape, cx, cy, radius, color, opacity, border);
        }
        writeln!(svg, "  </g>").unwrap();
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

/// A deterministic `u64 → [0, 1)` hash (SplitMix64's finalizer). Pure and
/// dependency-free — the seeded-from-data rule the whole engine follows for
/// anything that must look arbitrary yet render identically every run.
fn hash01(mut z: u64) -> f64 {
    z = z.wrapping_add(0x9E3779B97F4A7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
    z ^= z >> 31;
    // Top 53 bits → the same resolution an f64 mantissa carries.
    (z >> 11) as f64 / ((1u64 << 53) as f64)
}
