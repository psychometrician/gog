//! The `bar` mark — a rectangle from the baseline to each value.
use std::collections::HashMap;
use std::fmt::Write;
use crate::data::DataFrame;
use crate::ir::{Channel, Layer, Transform};
use crate::render::palette::PALETTE_GOG;
use crate::render::encode::{opacity_at, OPACITY_DEFAULT};
use crate::render::pattern::{FillTexture, PatternMap};
use crate::render::nest::Nest;
use crate::render::polar::Polar;
use crate::render::project::Scene;
use crate::render::svg::{unit_norm, SvgRenderer, OVERLAY_FILL, OVERLAY_OUTLINE_W, PANEL_BG};
use crate::render::text::esc;
use crate::render::Layout;
use crate::scale;
use super::{bar_thickness_svg, Dodge};

impl SvgRenderer {
    // -----------------------------------------------------------------------
    // Mark: bar
    // -----------------------------------------------------------------------

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn write_bars(
        &self, svg: &mut String, layer: &Layer, df: &DataFrame,
        l: &Layout, xs: (f64, f64), ys: (f64, f64),
        x_field: &str, y_field: &str,
        cat_x: Option<&[String]>, cat_y: Option<&[String]>,
        horizontal: bool,
        // Where the bars are measured from, in scale units: zero on a linear
        // axis, negative infinity on a log one (clamped to the axis foot).
        ext_base: f64,
        color_map: &HashMap<String, String>,
        clip: &str,
        // Polar: when `Some`, the position axis is bent into a circle and every
        // bar becomes an annular sector — a wedge of the rose. Nothing above the
        // final geometry changes, which is the point: the split, the dodge, the
        // stack, the border and the hatch are all the same bar's, whichever space
        // it is drawn in (Law 2).
        polar: Option<&Polar>,
        // Nest: when `Some`, the panel is packed with regions and every bar becomes
        // one of them — its measure read as an **area** rather than as a length
        // (spec §15). This is the one space that cannot be asked where a coordinate
        // lands, so unlike `polar` it is consulted **once, above the row loop**: the
        // packing is a property of all the rows together, not of any one of them.
        nest: Option<&Nest>,
    ) {
        // Read the two axes by *role* rather than by name. Everything below is
        // written once, in terms of position and extent; only the final mapping
        // back to screen coordinates knows which is x and which is y.
        let (pos_field, ext_field) = if horizontal { (y_field, x_field) } else { (x_field, y_field) };
        let pos_cats = if horizontal { cat_y } else { cat_x };

        let Some(ext_col) = df.float_col(ext_field) else { return };

        // Positions: a numeric column directly, or a string column → category index.
        // Or none at all: a bar whose split *is* its segmentation has one slot and
        // every element stands in it (spec §15). That is the share-of-total column
        // flat, and the pie in polar, and it is the same code either way — only
        // where the slot lands on the page differs.
        let pos_vals = if pos_field.is_empty() {
            std::borrow::Cow::Owned(vec![0.0; ext_col.len()])
        } else {
            let Some(p) = super::positions(df, pos_field, pos_cats) else { return };
            p
        };
        let ext_vals = ext_col;

        let n = pos_vals.len().min(ext_vals.len());
        if n == 0 { return; }

        // A `bar * bin` is a histogram: its bars fill their bins and touch. Any
        // other bar — categorical, or a summary like `count`/`mean` — keeps the
        // gap that reads as "separate categories".
        let is_hist = layer.transforms.iter().any(|t| matches!(t, Transform::Bin));

        // The scale of whichever axis holds the positions, and its pixel length.
        let (pos_scale, pos_px) = if horizontal { (ys, l.h()) } else { (xs, l.w()) };
        // How wide a bar's slot is. In the plane that is a count of pixels; bent
        // into a circle it is a fraction of the *turn*, since there is no fixed
        // pixel width to a wedge. Same slot and the same 80%-of-it / fill-the-bin
        // rule either way — only the unit it is measured in changes, and the
        // dodge offsets below inherit whichever it is.
        let pos_span = (pos_scale.1 - pos_scale.0).max(1e-12);
        let bar_thickness = match polar {
            None => bar_thickness_svg(&pos_vals, n, pos_px, pos_scale, is_hist),
            Some(_) => bar_thickness_svg(&pos_vals, n, pos_px, pos_scale, is_hist) * pos_span / pos_px,
        };
        // Dodge sets a color split side by side within each slot (§5): the
        // position center shifts and the bar narrows to `1/G` of the slot.
        let dodge = Dodge::resolve(layer, df);
        // Stack piles that split along the *measure* axis instead (§5): each bar's
        // foot is the cumulative height of the groups below it, carried per row in
        // `stack_base` by the transform. Full-width and, like a dodged bar, solid —
        // the pile resolves the overlap, so there is no translucent fill to see through.
        let stacked = layer.transforms.iter().any(|t| matches!(t, Transform::Stack));
        let base_col = if stacked { df.float_col(crate::transform::STACK_BASE) } else { None };
        let color_labels = layer.encodings.get(&Channel::Color).and_then(|c| df.str_col(&c.field));
        let opacity_vals = layer.encodings.get(&Channel::Opacity).and_then(|c| df.float_col(&c.field));
        let op_scale = match opacity_vals {
            Some(c) => scale::ChannelScale::of(c, layer.encodings.get(&Channel::Opacity)),
            None => scale::ChannelScale::unbound(),
        };

        let st = &layer.style;
        let default_color = st.color.as_deref().map(esc).unwrap_or_else(|| PALETTE_GOG[0].to_string());
        // `pattern` on a fill is a hatch texture (spec §4/§5), two ways: the
        // `style(pattern = )` *setting* fixes one for the layer, the `pattern(col)`
        // *channel* maps one per category. `solid`/unset is the identity, so an
        // ordinary bar is byte-for-byte unchanged. The legality check refuses map
        // and set together, so at most one is present per bar.
        let mut tex = FillTexture::new();
        let pattern_map = PatternMap::resolve(layer, df);

        // Bars *overlay* when two of them share a position. Every bar of a
        // color-split histogram does, because the groups bin on shared edges: it
        // is three histograms drawn in place, one per species. Drawn opaque, the
        // last species would bury the rest; so an overlaid bar gets a translucent
        // fill and a solid outline in its own hue — the "step" silhouette that
        // stays legible where the fills pile up. A plain histogram's bars only
        // *touch* (distinct centers, no repeat), so it keeps its panel-color
        // hairline, and a bar colored by its own position (one bar per slot)
        // never triggers this and draws solid as before.
        // A dodge or a stack separates the groups — side by side, or piled up — so
        // the bar draws solid; only a true overlay (same position, neither offset)
        // needs the translucent fill. A stack shares the position axis exactly (that
        // is what it piles), so without this guard it would read as an overlay.
        // A packing separates the groups for the third time, and more completely
        // than either: a dodge and a stack move bars that would collide, and a
        // packing makes the collision impossible by giving each its own region. So
        // it joins the same guard — without it, a treemap with no domain axis would
        // read as one overlay (every row at slot 0) and paint every cell see-through.
        let overlaid = dodge.is_none() && !stacked && nest.is_none() && color_labels.is_some() && {
            let p = &pos_vals[..n];
            (1..n).any(|i| p[..i].iter().any(|&q| (q - p[i]).abs() < 1e-9))
        };

        // A set opacity answers the see-through question itself; otherwise an
        // overlaid fill is faint by default and a solid bar keeps its usual weight.
        let default_opacity = st.opacity.unwrap_or(if overlaid { OVERLAY_FILL } else { OPACITY_DEFAULT });

        // `style(border_color =, border_size =)` — the bar's outline, a setting
        // (spec §5). Either alone works; a set border draws solid and overrides
        // whichever of color/width was given, the derived edge below filling the
        // rest. Computed once here — it is the same for every bar in the layer.
        let border_edge = st.border_color.as_deref().map(esc);
        let has_border = border_edge.is_some() || st.border_size.is_some();

        // **The packing, resolved once for the whole layer** — and resolved by the
        // space rather than here, because `text` draws the same regions and the two
        // marks must not be able to disagree about where one is (`Nest::regions`).
        //
        // The measure is read raw rather than through `map_y`, because a position
        // is what this space does not have. That is also why `check_nest` refuses a
        // log scale on it: a scale that changes what a value means without changing
        // its share of the total would be accepted and then do nothing here.
        let cells = nest.map(|nst| nst.regions(&pos_vals[..n], &ext_vals[..n]));

        writeln!(svg, r##"  <g clip-path="url(#{clip})">"##).unwrap();
        for i in 0..n {
            let color: &str = if let Some(labels) = color_labels {
                let lbl = labels.get(i).map(String::as_str).unwrap_or("");
                color_map.get(lbl).map(String::as_str).unwrap_or(&default_color)
            } else {
                &default_color
            };

            let o = match opacity_vals {
                Some(col) => opacity_at(op_scale.fraction(col.get(i).copied().unwrap_or(f64::NAN))),
                None => default_opacity,
            };

            // A bar runs from the zero baseline to its value, in whichever
            // direction the value sits. `lo`/`hi` are already in screen pixels,
            // so a negative bar needs no special case.
            // Dodge shifts the position center and narrows the bar; un-dodged, the
            // offset is 0 and the thickness is the full slot.
            let d_off = dodge.as_ref().map_or(0.0, |d| d.offset_at(i, bar_thickness));
            let d_thick = bar_thickness * dodge.as_ref().map_or(1.0, Dodge::width_frac);
            // Un-stacked, every bar is measured from the shared baseline; stacked,
            // from its own foot — the cumulative height of the groups beneath it.
            let row_base = base_col.and_then(|b| b.get(i).copied()).unwrap_or(ext_base);

            // The bar as four numbers on its two axes: the edges of the slot it
            // stands in, and the two ends of what it measures. Both spaces read
            // the same four; only the shape they are turned into differs — a
            // rectangle in the plane, a wedge of the ring in polar. The offsets
            // are pixels on the flat path and scale units on the polar one (see
            // `bar_thickness` above), so each branch does its own arithmetic.
            // The packed reading comes first because it is the one that does not
            // consult the two axes at all: the cell was decided above, from every
            // row at once, and there is no slot edge or measured end left to turn
            // into anything.
            if let Some((cells, _)) = &cells {
                let c = cells[i];
                // Nothing legible: a region under half a pixel on either side. The
                // same threshold the other two branches use, and the same reason —
                // a share this small has no drawable region, and drawing it would
                // put a hairline border where there is nothing to border.
                if c.w < 0.5 || c.h < 0.5 { continue; }
                let geom = format!(
                    r#"<rect x="{:.2}" y="{:.2}" width="{:.2}" height="{:.2}""#,
                    c.x, c.y, c.w, c.h);
                // A packed region's edge is not decoration: adjacent cells are the
                // reader's only cue for where one share ends and the next begins, so
                // the derived edge is the panel color, as a touching histogram
                // bar's is, for exactly the same reason. `style(border_color =,
                // border_size = )` overrides it like anywhere else (Law 2).
                let (edge, edge_w) = if has_border {
                    (border_edge.as_deref().unwrap_or(PANEL_BG), st.border_size.unwrap_or(1.0))
                } else {
                    (PANEL_BG, 1.0)
                };
                let stroke_attrs = if edge_w > 0.0 {
                    format!(r#" stroke="{edge}" stroke-width="{edge_w}" stroke-opacity="1""#)
                } else {
                    r#" stroke="none""#.to_string()
                };
                let texture = pattern_map.as_ref().map(|pm| pm.fill_texture(pm.cat_at(i))).or(st.pattern.as_deref());
                let fill = tex.fill(svg, texture, color);
                writeln!(svg,
                    r#"    {geom} fill="{fill}" fill-opacity="{o:.3}"{stroke_attrs}/>"#
                ).unwrap();
                continue;
            }

            let geom = match polar {
                Some(p) => {
                    let (ext_scale, pos_ax_scale) = if horizontal { (xs, ys) } else { (ys, xs) };
                    let e0 = unit_norm(row_base.clamp(ext_scale.0, ext_scale.1), ext_scale);
                    let e1 = unit_norm(ext_vals[i], ext_scale);
                    if !e0.is_finite() || !e1.is_finite() { continue; }
                    let d = if p.measure_on_angle {
                        // The pie: the measure runs *round* the circle and the
                        // radius is the constant that sets the pie's size
                        // (Wilkinson's `polar.theta`). Stacked, each slice runs
                        // from the cumulative total below it to its own top, so
                        // the slices lay end to end and the last one closes the
                        // circle — the whole turn is the total, which is why a pie
                        // reads as shares without being told to.
                        if (e1 - e0).abs() * p.r_max < 0.25 { continue; }
                        p.sector(e0, e1, 0.0, 1.0)
                    } else {
                        let s0 = unit_norm(pos_vals[i] + d_off - d_thick / 2.0, pos_ax_scale);
                        let s1 = unit_norm(pos_vals[i] + d_off + d_thick / 2.0, pos_ax_scale);
                        if !s0.is_finite() || !s1.is_finite() { continue; }
                        // Nothing to show: a wedge whose measured end is under half
                        // a pixel from where it started. The radius is what that
                        // length is in pixels, whichever axis the measure sits on.
                        if (e1 - e0).abs() * p.r_max < 0.5 { continue; }
                        // A horizontal bar measures along the angle and stands on
                        // the radius; an upright one the other way about — the same
                        // swap the flat branch makes, one space further out.
                        if horizontal { p.sector(e0, e1, s0, s1) } else { p.sector(s0, s1, e0, e1) }
                    };
                    format!(r#"<path d="{d}""#)
                }
                None => {
                    let (rect_x, rect_y, w, h) = if horizontal {
                        let base = l.map_x(row_base.clamp(xs.0, xs.1), xs.0, xs.1);
                        let tip  = l.map_x(ext_vals[i], xs.0, xs.1);
                        let cy   = l.map_y(pos_vals[i], ys.0, ys.1) + d_off;
                        (base.min(tip), cy - d_thick / 2.0, (tip - base).abs(), d_thick)
                    } else {
                        let base = l.map_y(row_base.clamp(ys.0, ys.1), ys.0, ys.1);
                        let tip  = l.map_y(ext_vals[i], ys.0, ys.1);
                        let cx   = l.map_x(pos_vals[i], xs.0, xs.1) + d_off;
                        (cx - d_thick / 2.0, base.min(tip), d_thick, (base - tip).abs())
                    };
                    // A bar whose value a log scale cannot place has no height to draw.
                    if ![rect_x, rect_y, w, h].iter().all(|v| v.is_finite()) { continue; }
                    if (if horizontal { w } else { h }) < 0.5 { continue; }
                    format!(r#"<rect x="{rect_x:.2}" y="{rect_y:.2}" width="{w:.2}" height="{h:.2}""#)
                }
            };

            // The engine's derived edge for this bar kind: an overlaid bar is
            // outlined in its own color so the series stays readable through the
            // stack; a touching histogram bar needs a hairline in the panel color
            // to part it from its neighbor; a categorical bar, sitting in its own
            // gap, keeps a faint self-colored edge.
            let (derived_edge, derived_w, derived_o) = if overlaid {
                (color, OVERLAY_OUTLINE_W, 1.0)
            } else if is_hist {
                (PANEL_BG, 1.0, 1.0)
            } else {
                (color, 0.5, 0.5)
            };
            // A set border overrides color and/or width and draws solid; the
            // derived edge fills whichever half the caller left unset.
            let (edge, edge_w, edge_o) = if has_border {
                (border_edge.as_deref().unwrap_or(derived_edge), st.border_size.unwrap_or(derived_w), 1.0)
            } else {
                (derived_edge, derived_w, derived_o)
            };
            // `border_size = 0` means no outline — the fills overlap with nothing
            // drawn between them. Draw no stroke rather than a zero-width one.
            let stroke_attrs = if edge_w > 0.0 {
                format!(r#" stroke="{edge}" stroke-width="{edge_w}" stroke-opacity="{edge_o}""#)
            } else {
                r#" stroke="none""#.to_string()
            };
            // A textured bar swaps its solid fill for a hatch tile in the same
            // color (emitted once per hue into `svg` just before this rect); a
            // solid bar gets its color back unchanged. The channel picks the
            // texture by this row's category; else the setting's fixed one.
            let texture = pattern_map.as_ref().map(|pm| pm.fill_texture(pm.cat_at(i))).or(st.pattern.as_deref());
            let fill = tex.fill(svg, texture, color);
            writeln!(svg,
                r#"    {geom} fill="{fill}" fill-opacity="{o:.3}"{stroke_attrs}/>"#
            ).unwrap();
        }

        // **The outer partition, traced last so it reads as the coarser split.**
        // A two-level packing draws every cell the same weight, and a reader with
        // no axis and no strip has nothing to tell a group boundary from a member
        // boundary — so the outer regions get the same edge three times as wide,
        // which is the gutter a treemap is normally read by.
        //
        // The domain axis in this space **splits without encoding**, exactly as the
        // `group` channel does (spec §5), and that is why this is a heavier line and
        // not a label: `group` earns no guide either. A `text` layer names the
        // *cells* it has rows for, so naming these outer regions in place means
        // giving `text` a table with one row per group — the second-table route, and
        // `facet`'s strips are the other. Drawn only when there is more than one
        // region, so a one-level treemap is byte-identical to before this existed.
        if let Some((_, regions)) = &cells {
            if regions.len() > 1 {
                let (edge, w) = if has_border {
                    (border_edge.as_deref().unwrap_or(PANEL_BG), st.border_size.unwrap_or(1.0) * 3.0)
                } else {
                    (PANEL_BG, 3.0)
                };
                if w > 0.0 {
                    for r in regions.iter().filter(|r| r.w >= 0.5 && r.h >= 0.5) {
                        writeln!(svg,
                            r#"    <rect x="{:.2}" y="{:.2}" width="{:.2}" height="{:.2}" fill="none" stroke="{edge}" stroke-width="{w}"/>"#,
                            r.x, r.y, r.w, r.h).unwrap();
                    }
                }
            }
        }
        writeln!(svg, "  </g>").unwrap();
    }

    // -----------------------------------------------------------------------
    // Mark: bar, in `space` — the 3-D histogram
    // -----------------------------------------------------------------------

    /// A column standing on the floor of the cube: the same bar, with a *footprint*
    /// where the flat one has a slot width (spec §5/§15).
    ///
    /// **The four numbers became six, and nothing else changed.** The flat writer
    /// above reads a bar as the edges of the slot it stands in plus the two ends of
    /// what it measures, and turns those into a rectangle or a wedge. In the cube it
    /// is two pairs of footprint edges plus the same two ends, turned into a box —
    /// the third reading of one description, which is why this is a second geometry
    /// rather than a second mark.
    ///
    /// **Where the footprint comes from is `zone`'s question, answered `zone`'s way.**
    /// A 3-D bar's floor is a cell, so it inherits the extent descriptions §5 already
    /// names: `bin` **cuts** the edges and publishes them, a **categorical** axis
    /// bounds its own slot and publishes nothing (`build_axis` put category *k* at *k*
    /// over `-0.5 ..= n-0.5`). Read one axis at a time, so a mixed floor — cut on one,
    /// slotted on the other — falls out with no branch of its own, exactly as it did
    /// for the mixed mesh.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn write_bars_3d(
        &self, svg: &mut String, layer: &Layer, df: &DataFrame,
        xs: (f64, f64), ys: (f64, f64), zs: (f64, f64),
        x_field: &str, y_field: &str, z_field: &str,
        cat_x: Option<&[String]>, cat_y: Option<&[String]>,
        color_map: &HashMap<String, String>,
        clip: &str,
        scene: &Scene,
    ) {
        let Some(z_col) = df.float_col(z_field) else { return };

        // Each axis's two edges, in data units. A cut mesh published them; a
        // category owns `[k-½, k+½]` and the scale already holds it. Asked per axis
        // rather than per layer — the sentence the mixed mesh turned into a rule.
        //
        // **A cut axis touches and a slotted one leaves air**, which is the flat rule
        // read one dimension up rather than a new one. `bar_thickness_svg` fills a
        // whole bin (a histogram's bins are adjacent intervals — Wilkinson: "there
        // cannot be gaps between bars") and four fifths of a category's slot, where
        // the empty fifth is what *says* the categories are separate rather than a
        // divided continuum. A `zone` takes the whole slot in either case because its
        // extent is constitutive — the region a category owns — and a bar's is not:
        // it merely stands there. Per axis, so a mixed floor is contiguous along the
        // cut and gapped along the slots, which is what each axis separately means.
        // The footprint is `cell_edges`, shared with the other two slot marks so one
        // floor cannot be computed three ways (see `marks::cell_edges`).
        const SLOT_FILL: f64 = 0.80;
        let Some((x0s, x1s)) = super::cell_edges(
            df, (crate::transform::CELL_START, crate::transform::CELL_END),
            x_field, cat_x, SLOT_FILL) else { return };
        let Some((y0s, y1s)) = super::cell_edges(
            df, (crate::transform::CELL_LOWER, crate::transform::CELL_UPPER),
            y_field, cat_y, SLOT_FILL) else { return };

        let n = z_col.len().min(x0s.len()).min(y0s.len());
        if n == 0 { return; }

        let color_labels = layer.encodings.get(&Channel::Color).and_then(|c| df.str_col(&c.field));
        let st = &layer.style;
        let default_color = st.color.as_deref().map(esc).unwrap_or_else(|| PALETTE_GOG[0].to_string());
        let o = st.opacity.unwrap_or(1.0);

        // The floor the columns stand on, in scale units.
        let base = unit_norm(0.0_f64.clamp(zs.0, zs.1), zs);

        // Far foot first — `floor_order`, shared with the other two slot marks for
        // the reason the footprint is (see `marks::floor_order`).
        let order = super::floor_order(n, &x0s, &x1s, &y0s, &y1s, xs, ys, base, scene);

        writeln!(svg, r##"  <g clip-path="url(#{clip})">"##).unwrap();
        for i in order {
            let color: &str = match color_labels {
                Some(labels) => {
                    let lbl = labels.get(i).map(String::as_str).unwrap_or("");
                    color_map.get(lbl).map(String::as_str).unwrap_or(&default_color)
                }
                None => &default_color,
            };
            let top = unit_norm(z_col[i], zs);
            let (nx0, nx1) = (unit_norm(x0s[i], xs), unit_norm(x1s[i], xs));
            let (ny0, ny1) = (unit_norm(y0s[i], ys), unit_norm(y1s[i], ys));
            // The column itself is `write_solid`, shared with `box`'s body: the two
            // are the same shape and differ only in where their two ends come from —
            // a baseline and a value here, two quartiles there.
            super::write_solid(svg, scene, (nx0, nx1), (ny0, ny1), (base, top), color, o);
        }
        writeln!(svg, "  </g>").unwrap();
    }

    /// A `bar` on the globe: a **spike** standing at its place, measuring from
    /// the surface out along the radius — the one direction the sphere has
    /// that its flattening does not, and the cube's own `z` reading
    /// transplanted. `z` names the measure; the baseline is the surface, so a
    /// spike's length is its value against the fitted top, and a value below
    /// the surface is not drawn (the caller counts and says so). The sphere
    /// itself is the clip: a spike just behind the horizon still peeks over
    /// the limb when it is tall enough (`Globe::spike`), which is what keeps
    /// the rim honest at a turned view. Painted far to near by the base —
    /// spikes sort by their footprint, the cube's rule.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn write_bars_globe(
        &self, svg: &mut String, layer: &Layer, df: &DataFrame,
        x_field: &str, y_field: &str, z_field: &str, zs: (f64, f64),
        color_map: &HashMap<String, String>,
        ramp: &[String],
        clip: &str,
        globe: &crate::render::globe::Globe,
    ) {
        let (Some(lons), Some(lats), Some(vals)) = (
            df.float_col(x_field),
            df.float_col(y_field),
            df.float_col(z_field),
        ) else {
            return;
        };
        let n = lons.len().min(lats.len()).min(vals.len());
        // The fitted top of the measure; the baseline is the surface, so the
        // fraction is the value against the top rather than against the range's
        // own floor — a spike twice another's value is twice as long.
        let top = zs.1.max(zs.0);
        if !(top > 0.0) {
            return;
        }

        let st = &layer.style;
        let stroke_w = st.size.unwrap_or(2.5);
        let stroke_o = st.opacity.unwrap_or(0.9);
        let set_color = st.color.as_deref().map(esc);
        let color_labels = layer.encodings.get(&Channel::Color).and_then(|c| df.str_col(&c.field));
        let color_vals = layer.encodings.get(&Channel::Color).and_then(|c| df.float_col(&c.field));
        let color_scale = match color_vals {
            Some(c) => scale::ChannelScale::of(c, layer.encodings.get(&Channel::Color)),
            None => scale::ChannelScale::unbound(),
        };
        let stops: Vec<&str> = ramp.iter().map(String::as_str).collect();

        // Every visible spike, far foot first.
        let mut pieces: Vec<(f64, usize, (f64, f64), (f64, f64))> = Vec::new();
        for i in 0..n {
            let v = vals[i];
            if !(lons[i].is_finite() && lats[i].is_finite() && v.is_finite()) || v < 0.0 {
                continue;
            }
            let h = crate::render::globe::SPIKE_MAX * (v / top).min(1.0);
            if let Some((from, tip, depth)) = globe.spike(lons[i], lats[i], h) {
                pieces.push((depth, i, from, tip));
            }
        }
        pieces.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        writeln!(svg, r##"  <g clip-path="url(#{clip})">"##).unwrap();
        for (_, i, from, tip) in pieces {
            let ramped: String;
            let color: &str = if let Some(labels) = color_labels {
                let lbl = labels.get(i).map(String::as_str).unwrap_or("");
                color_map.get(lbl).map(String::as_str).unwrap_or_else(|| PALETTE_GOG[0])
            } else if let Some(cv) = color_vals {
                let f = color_scale.fraction(cv.get(i).copied().unwrap_or(f64::NAN));
                ramped = crate::render::palette::ramp_at(&stops, f);
                &ramped
            } else if let Some(c) = &set_color {
                c
            } else {
                PALETTE_GOG[0]
            };
            writeln!(svg,
                r##"    <line x1="{:.2}" y1="{:.2}" x2="{:.2}" y2="{:.2}" stroke="{color}" stroke-width="{stroke_w}" stroke-opacity="{stroke_o:.3}" stroke-linecap="round"/>"##,
                from.0, from.1, tip.0, tip.1
            ).unwrap();
        }
        writeln!(svg, "  </g>").unwrap();
    }
}
