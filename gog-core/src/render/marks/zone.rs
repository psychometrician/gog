//! The `zone` mark — a filled rectangle in data space, the highlighted area.
//!
//! `rule`'s sibling one dimension up, and the file reads like it: a rule takes
//! one position and spans the axis it does not name, and a zone takes a *pair*
//! and spans the axis it is not given a pair for. Both ask the panel for what the
//! data did not say, which is the one thing `ribbon * bounds` cannot do — a
//! ribbon is bounded by its data, so it stops at the numbers given.
//!
//! **One row is one rectangle.** No pair-rows, no grouping pass: a zone's four
//! sides are four columns on a single row, so the whole mark is a loop over rows
//! emitting a `<rect>` — or, in polar, the annular sector a rectangle bends into.
//! That is why `rule`'s payoff carries over — one table of recessions draws every
//! band at once.
use std::collections::HashMap;
use std::fmt::Write;
use crate::data::DataFrame;
use crate::ir::{Channel, Layer};
use crate::render::palette::{ramp_at, PALETTE_GOG};
use crate::render::pattern::{FillTexture, PatternMap};
use crate::scale;
use crate::render::polar::Polar;
use crate::render::svg::{unit_norm, SvgRenderer};
use crate::render::text::esc;
use crate::render::Layout;

/// A zone is background: it is drawn under the data, so it defaults translucent
/// rather than making the caller remember to say so. Matched to the overlaid-fill
/// convention the split bar and ribbon already use.
const ZONE_OPACITY: f64 = 0.20;

/// `style(border_color =, border_size =)` as an SVG stroke — the frame round each
/// region this mark fills.
///
/// The mark joined the **closed-glyph fills** on 2026-07-27 (spec §4, the settable
/// rule), reversing a ruling the treemap entry had recorded as settled. What forced
/// it was the mosaic: `partition` is `zone`-only, so unlike the packing there was no
/// `bar` to fall back on, and a mosaic whose cells have no edges is one blob
/// wherever two neighbors share a color.
///
/// One function so all four writers — rectangle, sector, hexagon and the filled
/// contour's band — cannot disagree about what a border is (Law 2). Either half
/// works alone, on `bar`'s precedent: a color with no width takes a hairline, a
/// width with no color takes the panel background, which is the white gutter a
/// mosaic is read by. `border_size = 0` is how a caller says *no* edge.
fn border_edge(st: &crate::ir::StyleSpec) -> String {
    match (st.border_color.as_deref(), st.border_size) {
        (None, None) | (_, Some(0.0)) => r#"stroke="none""#.to_string(),
        (c, w) => format!(
            r#"stroke="{}" stroke-width="{:.2}""#,
            c.map(esc).unwrap_or_else(|| crate::render::svg::PANEL_BG.to_string()),
            w.unwrap_or(1.0),
        ),
    }
}

impl SvgRenderer {
    // -----------------------------------------------------------------------
    // Mark: zone
    // -----------------------------------------------------------------------

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn write_zone(
        &self, svg: &mut String, layer: &Layer, df: &DataFrame,
        l: &Layout, xs: (f64, f64), ys: (f64, f64),
        // The plot's own position columns. A *bounded* zone never reads them — its
        // sides are its own four columns — but a **level set** publishes its boundary
        // as vertices, and vertices ride the position columns like any other mark's.
        x_field: &str, y_field: &str,
        cat_x: Option<&[String]>,
        cat_y: Option<&[String]>,
        color_map: &HashMap<String, String>,
        // The sequential stops a numeric `color` reads. A zone is the one fill
        // with enough area to decode a continuous scale from — which is exactly
        // what a heatmap does with it.
        ramp: &[String],
        clip: &str,
        // Polar: a rectangle bent is an **annular sector**, two arcs joined by two
        // radii, which `Polar::sector` has drawn since `bar` became a rose. A zone
        // that spans an axis then reaches the whole turn, and a sector spanning the
        // whole turn is an annulus — handled where the arc is written, because an
        // `A` across a full turn has coincident ends and draws nothing.
        polar: Option<&Polar>,
    ) {
        // **Two questions, not one** — the distinction the tile plot forced, and the
        // one this file used to conflate under a single `binned` flag.
        //
        // *Was my measurement made for me?* `bin`, `density`, `count` and
        // `proportion` all invent their own measure, which on this mark goes to
        // `color`. Asked below, where the fill is chosen, by `cell_measure`.
        //
        // *Were my sides cut for me?* Only `bin` and `density` cut them — they hand
        // over an extent description in synthesized columns (four edges, or a center
        // and a half-extent, or a ring's vertices). A **tally** cuts nothing: its
        // cells are the categories, and a category's extent is its slot, which the
        // *axis* holds and no column carries. That is this flag, and every use of it
        // below is a place that used to be wrong about the tally.
        //
        // Both asked of `transform`/`legality` rather than of `Bin` by name, for the
        // reason the refusal lists are generated — a hand-written list beside a
        // generated one always loses, and this one already lost once when `zone`
        // learned to `bin`.
        // `Flat` is this mark's answer in every space, not a stand-in for one we
        // could not reach: a zone measures by color wherever it stands, so it cuts
        // both positions whatever the coordinate. (It still refuses the cube; since
        // 2026-07-26 it draws in polar, where the answer is the same one again.)
        // `publishes_cells` rather than `reads_a_field`, since 2026-07-27: a
        // **partition** publishes the same four edges without cutting a plane, so
        // the narrower predicate answered `false` and this mark went looking for
        // `bounds` columns that were never named. Nothing else here changed — the
        // sides arrive as `cell_bounds()` exactly as a mesh's do, which is the
        // evidence that a partition really is the rectangular extent description
        // and not a fifth kind of cell.
        let cut = crate::legality::publishes_cells(
            &layer.mark, &layer.transforms, crate::legality::SpaceKind::Flat);

        // **The third extent description**, and the contract §5 wrote holding a third
        // time. A rectangular mesh publishes four edges; a hexagonal one publishes a
        // center and a half-extent; a *level set* publishes neither, because its
        // boundary is a curve — so it publishes the curve, as one row per vertex
        // tagged with the ring it belongs to. Rectangularity was never this mark's
        // identity (§5): `zone` is the region mark, and here the region is whatever
        // shape the density turned out to have.
        //
        // Drawn **outermost band first**, which is what makes plain filled polygons
        // enough where a general filled contour would need holes: a density's level
        // sets are *nested* by construction, so each inner band simply paints over the
        // one containing it. The exception is a crater — an annular mode, points
        // scattered on a circle — where a level set really is a ring with a hole and
        // this fills its middle. Recorded in §5 rather than guarded against, because
        // the guard is the polygon-with-holes geometry the grammar has no mark for.
        if let Some(rings) = cut.then(|| df.float_col(crate::transform::FIELD_RING)).flatten() {
            self.write_zone_bands(svg, layer, df, l, xs, ys, x_field, y_field, rings, ramp, clip, polar);
            return;
        }

        let synthesized;
        let spec = match layer.bounds.as_ref() {
            Some(s) => Some(s),
            None if cut => {
                synthesized = crate::transform::cell_bounds();
                Some(&synthesized)
            }
            None => None,
        };

        // **The mark asks the tiling what its cells look like**, rather than
        // assuming four sides — the whole reason the 2-D bin's output was split
        // into a center plus an extent. A hexagon has no edges to name, so it
        // arrives as a center (already on `x`/`y`) and a half-width/half-height
        // pair, and the six vertices follow from those.
        let hex = cut
            .then(|| Some((
                df.float_col(crate::transform::CELL_X)?,
                df.float_col(crate::transform::CELL_Y)?,
                df.float_col(crate::transform::CELL_DX)?,
                df.float_col(crate::transform::CELL_DY)?,
            )))
            .flatten();

        // Each side, in data units, or `None` where the panel supplies it. The
        // two axes are resolved by the same closure with different mappers, so
        // "bounded on x, panel on y" and its mirror cannot drift apart.
        let side = |pair: Option<(&str, &str)>, cats: Option<&[String]>| {
            let (a, b) = pair?;
            let lo = super::positions(df, a, cats)?.into_owned();
            let hi = super::positions(df, b, cats)?.into_owned();
            Some((lo, hi))
        };

        // **The fourth extent description, and it publishes no columns.** A
        // categorical position bounds its axis all by itself: `build_axis` puts
        // category *k* at *k* over a range of `-0.5 ..= n-0.5`, so the slot *k* owns
        // is exactly `[k-½, k+½]` and every fact about it is already in the scale.
        // A rectangular mesh publishes four edges and a hexagonal one a center and a
        // half-extent because nothing else knows where those cells are; a slot needs
        // no such column, and asking the transform to emit one would be storing a
        // number the axis had already fixed.
        //
        // The **whole** slot, where a categorical `bar` takes 80% of it — and the
        // difference is not a style choice. A bar's thickness is arbitrary, so the
        // 20% of air is free to say "these categories are separate". A zone's extent
        // is *constitutive*: the region a category owns is its slot, and drawing four
        // fifths of it would draw a rectangle that is not the thing it names. So the
        // cells touch, exactly as a cut mesh's do, and for the same reason.
        let slot = |field: &str, cats: Option<&[String]>| -> Option<(Vec<f64>, Vec<f64>)> {
            // A *number* is a point and owns no slot. Tested on the column rather
            // than on the axis, because a categorical axis may still be handed a
            // numeric column by another layer.
            df.str_col(field)?;
            let p = super::positions(df, field, cats)?;
            Some((p.iter().map(|v| v - 0.5).collect(), p.iter().map(|v| v + 0.5).collect()))
        };

        // A named pair wins over the slot on the same axis: it is the more specific
        // request, and the slot was never one (Law 5). `zone * bounds(start = "Mar",
        // end = "Jun")` shades from one named category to another, and always did.
        //
        // **The extent description is per axis**, and that sentence is the whole of
        // the mixed mesh. This used to suppress the slot for the entire layer
        // whenever anything had been cut (`&& !cut`), which stated an assumption
        // nobody had made a rule of: that a layer's two axes describe their extents
        // the *same* way. They need not — `zone * bin + x(<number>) + y(<category>)`
        // cuts the axis with a width to cut and leaves the other to its slots, so an
        // axis the mesh did not publish falls through here exactly as an unbinned one
        // does. What keeps that honest rather than lax is `slot` itself: it refuses a
        // *numeric* column, so a continuous axis the mesh failed to cut cannot quietly
        // come out one category wide.
        // **Which axis each named pair bounds is read off the bindings**, never
        // assumed — `legality::zone_orient`, and the same question `bar`, `box` and
        // `interval` ask (§6, and why there is no `flip` atom). `bounds` names a
        // *measure* pair and a *domain* pair, so on a categorical `y` the measure is
        // `x` and the two pairs change places. Assuming otherwise is what drew
        // `zone * bounds(lo, hi) + y(stage)` off the panel; the whole story is on
        // `zone_orient`, together with the second place that had to agree with this
        // one (`build_axis`'s `zone_sides`).
        //
        // A **mesh's** synthesized sides are exempt and stay where the transform put
        // them: `cell_bounds()` publishes the edges each axis was *cut* on, already
        // assigned to an axis, where a `bounds` the sentence wrote is a pair of
        // semantic pairs that says nothing about the screen. So the swap is asked of
        // `layer.bounds` rather than of `spec`, which is the two of them merged.
        let turned = layer.bounds.is_some()
            && crate::legality::zone_orient(cat_x.is_some(), cat_y.is_some())
                == crate::legality::Orient::Horizontal;
        let (x_pair, y_pair) = if turned {
            (spec.and_then(|s| s.measure()), spec.and_then(|s| s.domain()))
        } else {
            (spec.and_then(|s| s.domain()), spec.and_then(|s| s.measure()))
        };
        let x_ext = side(x_pair, cat_x).or_else(|| slot(x_field, cat_x));
        let y_ext = side(y_pair, cat_y).or_else(|| slot(y_field, cat_y));
        if hex.is_none() && x_ext.is_none() && y_ext.is_none() {
            return; // refused by `check_zone_extent`; nothing to draw
        }

        // **Does this zone tile the panel, or sit on top of it?** The question the
        // opacity and the antialiasing both turn on, and the honest form of it is
        // *did every axis get its extent from a mesh* — cut out of the data, or the
        // slot a category owns. A rectangle someone **named** is background whatever
        // else is true of it, which is why a highlight band colored by category
        // stays translucent and a confusion matrix does not; and an axis with no
        // extent at all is spanning the panel, which is a highlight too.
        //
        // Asked of the two axes rather than of the layer, for the reason above: a
        // mixed mesh is cut on one and slotted on the other, and both of those are a
        // mesh. The one thing still asked of the layer is whether the sides were
        // *named*, since a bounded zone is a rectangle someone chose however
        // completely its columns cover the panel.
        let tiles = layer.bounds.is_none() && (hex.is_some() || (x_ext.is_some() && y_ext.is_some()));

        let n = match hex {
            Some((cxs, ..)) => cxs.len(),
            None => [x_ext.as_ref().map(|(v, _)| v.len()), y_ext.as_ref().map(|(v, _)| v.len())]
                .into_iter().flatten().min().unwrap_or(0),
        };

        let st = &layer.style;
        let set_color = st.color.as_deref().map(esc);
        // A zone is translucent because it is *background* — drawn under the data,
        // so the data has to show through. A **tiling** zone *is* the data, and there
        // is nothing behind it to see; 20% would only wash the ramp out. Read off
        // the bindings like everything else, and stated once here.
        let opacity = st.opacity.unwrap_or(if tiles { 1.0 } else { ZONE_OPACITY });
        // `style(border_color =, border_size =)` — the frame round each region. The
        // mark joined the closed-glyph fills for the mosaic (spec §4, the settable
        // rule), and this is the whole of what that cost: one attribute, computed
        // once for the layer as a `bar` computes its own, and written by all three
        // geometries so `rect`, `sector` and `hexagon` cannot disagree (Law 2).
        //
        // Either alone works, on `bar`'s precedent: a color with no width takes a
        // hairline, and a width with no color takes the panel background, which is
        // the white gutter a mosaic and a treemap are both read by. `border_size = 0`
        // is how you say *no* edge after asking for one.
        let edge = border_edge(st);
        let pattern_map = PatternMap::resolve(layer, df);
        // The cells' measure is what the transform just made — a count or a density —
        // so `color` needs no binding, the same courtesy `bar * bin` does for `y`.
        // Naming it out loud (`color(count)`, `color(density)`) resolves to the
        // identical column; `check_field` refuses any other, so nothing else arrives.
        let color_field = layer.encodings.get(&Channel::Color).map(|c| c.field.as_str())
            .or(crate::transform::cell_measure(&layer.transforms));
        let color_vals = color_field.and_then(|f| df.str_col(f));
        let color_nums = color_field.and_then(|f| df.float_col(f));
        let cat_order = color_field.map(|f| crate::data::categories_across(&[df], f));
        let color_scale = match color_nums {
            Some(c) => scale::ChannelScale::of(c, layer.encodings.get(&Channel::Color)),
            None => scale::ChannelScale::unbound(),
        };
        let mut tex = FillTexture::new();

        // **A tiled cell turns antialiasing off**, and that is a correctness fix rather
        // than a polish one. Two abutting rectangles are antialiased independently, so
        // the pixel their shared edge falls in is covered partly by each and keeps a
        // few percent of the panel showing through — which at 63 cells across drew a
        // visible lattice over an estimated heatmap. That is the eye reading the mesh's
        // own alignment as structure in the data, precisely the artifact Wilkinson
        // gives as the reason hexagonal binning exists, and it is invisible on a pale
        // ramp but obvious on a dark one like `viridis`. Snapping the edges to whole
        // pixels removes it without moving anything: a cell is still exactly where the
        // mesh cut it, and the alternative (growing each cell so neighbors lap) buys
        // the same result by making the geometry lie.
        //
        // Only where cells abut, and only for *rectangles*. A bounded `zone` is a
        // rectangle someone chose, whose edge is a place in the data and should be
        // drawn as smoothly as any other; a hexagon's edges are diagonal, where
        // snapping to pixels is what *causes* jaggedness rather than curing it. The
        // tile plot is in scope for the same reason the cut mesh is — its cells share
        // every internal edge — which is why this asks `tiles` and not `cut`.
        let crisp = if tiles { r#" shape-rendering="crispEdges""# } else { "" };

        writeln!(svg, r##"  <g clip-path="url(#{clip})">"##).unwrap();

        // A hexagon's outline, in page pixels, from a center and a half-extent.
        // **Pointy-top**: a vertex straight up and straight down, shoulders at
        // half that height on each side — Carr's orientation, and every hexbin
        // since. Written once here rather than inline so the six vertices and
        // the `dy/2` shoulder appear in exactly one place.
        let hexagon = |cx: f64, cy: f64, dx: f64, dy: f64| -> String {
            let (px, py) = (l.map_x(cx, xs.0, xs.1), l.map_y(cy, ys.0, ys.1));
            // Half-extents become pixel offsets through the same mapping, as a
            // *difference* — the axis may be log-scaled, where a length in data
            // units is not a length in pixels anywhere but at its own position.
            let ex = (l.map_x(cx + dx, xs.0, xs.1) - px).abs();
            let ey = (l.map_y(cy + dy, ys.0, ys.1) - py).abs();
            [(0.0, -ey), (ex, -ey / 2.0), (ex, ey / 2.0),
             (0.0, ey), (-ex, ey / 2.0), (-ex, -ey / 2.0)]
                .iter()
                .map(|(ox, oy)| format!("{:.2},{:.2}", px + ox, py + oy))
                .collect::<Vec<_>>()
                .join(" ")
        };

        for row in 0..n {
            // Where the data does not bound an axis, the panel does — the whole
            // point of the mark. `l.x0`/`l.y0` are the panel's own edges, so the
            // rectangle reaches them exactly rather than to a padded data value.
            let (x0, x1) = match &x_ext {
                Some((lo, hi)) => (
                    l.map_x(lo[row], xs.0, xs.1),
                    l.map_x(hi[row], xs.0, xs.1),
                ),
                None => (l.x0, l.x1),
            };
            let (y0, y1) = match &y_ext {
                Some((lo, hi)) => (
                    l.map_y(lo[row], ys.0, ys.1),
                    l.map_y(hi[row], ys.0, ys.1),
                ),
                None => (l.y1, l.y0),
            };
            // The same two extents in scale units rather than pixels, which is what
            // a sector is described by. An unbounded axis spans it whole: the turn
            // for the angle, center-to-rim for the radius — the polar reading of
            // "the panel bounds what the data does not".
            let (u0, u1) = match &x_ext {
                Some((lo, hi)) => (unit_norm(lo[row], xs), unit_norm(hi[row], xs)),
                None => (0.0, 1.0),
            };
            let (v0, v1) = match &y_ext {
                Some((lo, hi)) => (unit_norm(lo[row], ys), unit_norm(hi[row], ys)),
                None => (0.0, 1.0),
            };
            if hex.is_none() && polar.is_none() && ![x0, x1, y0, y1].iter().all(|v| v.is_finite()) {
                continue;
            }
            if hex.is_none() && polar.is_some() && ![u0, u1, v0, v1].iter().all(|v| v.is_finite()) {
                continue;
            }
            // A rectangle given its corners in either order is the same
            // rectangle: SVG needs a non-negative width, so normalize rather than
            // demand the caller put the smaller number first.
            let (rx, rw) = (x0.min(x1), (x1 - x0).abs());
            let (ry, rh) = (y0.min(y1), (y1 - y0).abs());

            let base = match (&set_color, &color_vals, &cat_order, &color_nums) {
                (Some(c), _, _, _) => c.clone(),
                (None, Some(cv), Some(order), _) => {
                    let key = cv.get(row).map(String::as_str).unwrap_or("");
                    color_map.get(key).cloned().unwrap_or_else(|| {
                        let i = order.iter().position(|c| c == key).unwrap_or(0);
                        PALETTE_GOG[i % PALETTE_GOG.len()].to_string()
                    })
                }
                // A numeric `color` reads off the ramp, the way `point`'s does.
                (None, None, _, Some(nums)) => {
                    let f = color_scale.fraction(nums.get(row).copied().unwrap_or(f64::NAN));
                    ramp_at(&ramp.iter().map(String::as_str).collect::<Vec<_>>(), f)
                }
                _ => PALETTE_GOG[0].to_string(),
            };
            let texture = pattern_map.as_ref().map(|pm| pm.fill_texture(pm.cat_at(row)))
                .or(st.pattern.as_deref());
            let fill = tex.fill(svg, texture, &base);

            match (hex, polar) {
                // A hexagonal mesh has no polar reading, and `check_polar` refuses
                // the sentence before this is reached — so this arm exists to make
                // the refusal's promise true in code rather than to draw anything.
                (Some(_), Some(_)) => continue,
                (Some((cxs, cys, dxs, dys)), None) => {
                    let pts = hexagon(cxs[row], cys[row], dxs[row], dys[row]);
                    if pts.contains("NaN") || pts.contains("inf") {
                        continue;
                    }
                    writeln!(svg,
                        r##"    <polygon points="{pts}" fill="{fill}" fill-opacity="{opacity:.3}" {edge}/>"##
                    ).unwrap();
                }
                // The wedge. No `crisp` here: `shape-rendering="crispEdges"` snaps
                // to the pixel grid, which is a rectilinear hint — it would leave a
                // curved seam between two abutting cells more ragged, not less.
                (None, Some(p)) => {
                    let d = p.sector(u0, u1, v0, v1);
                    if d.contains("NaN") || d.contains("inf") {
                        continue;
                    }
                    writeln!(svg,
                        r##"    <path d="{d}" fill="{fill}" fill-opacity="{opacity:.3}" {edge}/>"##
                    ).unwrap();
                }
                (None, None) => {
                    writeln!(svg,
                        r##"    <rect x="{rx:.2}" y="{ry:.2}" width="{rw:.2}" height="{rh:.2}" fill="{fill}" fill-opacity="{opacity:.3}" {edge}{crisp}/>"##
                    ).unwrap();
                }
            }
        }

        writeln!(svg, "  </g>").unwrap();
    }

    /// Fill the traced level sets — the filled contour, and what a `zone` draws a
    /// banded field as.
    ///
    /// `path * density(levels = k)` and `zone * density(levels = k)` run the **same**
    /// transform and differ only here: one strokes each ring, the other fills it. So a
    /// band's edge is exactly the curve the contour would have drawn, to the pixel,
    /// which is the property that makes the two readings worth having as one sentence
    /// with two marks rather than two unrelated features.
    ///
    /// The rows arrive in **ascending level order**, so emitting them in order paints
    /// the outermost band first and each inner one over it. See the note at the call
    /// site for why nesting makes that sound, and for the one topology it cannot draw.
    #[allow(clippy::too_many_arguments)]
    fn write_zone_bands(
        &self, svg: &mut String, layer: &Layer, df: &DataFrame,
        l: &Layout, xs: (f64, f64), ys: (f64, f64),
        x_field: &str, y_field: &str,
        rings: &[f64],
        ramp: &[String],
        clip: &str,
        // A band's boundary is a *curve through the data's own vertices*, so it
        // bends the way `path * density` does — the same rows, stroked there and
        // filled here, must not part company about where the ring runs.
        polar: Option<&Polar>,
    ) {
        let (Some(vx), Some(vy)) = (df.float_col(x_field), df.float_col(y_field)) else { return };
        let levels = df.float_col(crate::transform::FIELD_LEVEL);
        let n = vx.len().min(vy.len()).min(rings.len());
        if n < 3 { return; }

        let st = &layer.style;
        let set_color = st.color.as_deref().map(esc);
        let opacity = st.opacity.unwrap_or(1.0);
        // The same border the cells take, on the same terms. A filled contour's
        // region is as closed as a cell's, and Law 2 will not have a setting mean
        // something on one of this mark's readings and nothing on another.
        let edge = border_edge(st);
        // The band's color is the level it was cut at, off the same sequential ramp
        // the cells read — unbound, the courtesy every two-dimensional reading does
        // for `color` (`check_field` refuses any other field).
        let scale = match levels {
            Some(v) => scale::ChannelScale::of(v, layer.encodings.get(&Channel::Color)),
            None => scale::ChannelScale::unbound(),
        };
        let stops: Vec<&str> = ramp.iter().map(String::as_str).collect();

        writeln!(svg, r##"  <g clip-path="url(#{clip})">"##).unwrap();

        // One polygon per run of a ring id. Runs rather than a grouping, for
        // `write_path`'s reason: the transform emits each ring's vertices consecutively,
        // and a `group` split leaves each group's rows contiguous while restarting the
        // numbering, so a run can never straddle two groups.
        let mut start = 0usize;
        for i in 0..=n {
            let ends = i == n || (i > start && rings[i] != rings[start]);
            if !ends { continue; }
            let pts: Vec<(f64, f64)> = (start..i)
                .filter(|&r| vx[r].is_finite() && vy[r].is_finite())
                .map(|r| super::place(l, polar, vx[r], vy[r], xs, ys))
                .collect();
            start = i;
            if pts.len() < 3 { continue; }

            let fill = match &set_color {
                Some(c) => c.clone(),
                None => {
                    let f = levels
                        .map(|v| scale.fraction(v.get(i.saturating_sub(1)).copied().unwrap_or(f64::NAN)))
                        .unwrap_or(0.5);
                    ramp_at(&stops, f)
                }
            };
            let points: String = pts.iter()
                .map(|(x, y)| format!("{x:.2},{y:.2}"))
                .collect::<Vec<_>>()
                .join(" ");
            if points.contains("NaN") || points.contains("inf") { continue; }
            writeln!(svg,
                r##"    <polygon points="{points}" fill="{fill}" fill-opacity="{opacity:.3}" {edge}/>"##
            ).unwrap();
        }

        writeln!(svg, "  </g>").unwrap();
    }
}
