//! The `surface` mark — a sheet through the samples, and the last mark in the
//! kernel to gain a renderer (2026-07-26, spec §15).
//!
//! **A surface is a mesh of faces between its rows, where a 3-D `bar` stands a
//! column on a cell.** That one difference is the whole file. A bar reads two pairs
//! of footprint edges off the table and turns them into a solid; a surface reads its
//! rows as *nodes* and draws the quads between adjacent ones, so what it needs from
//! the data is not an extent but an **adjacency** — which `data::Lattice` recovers
//! from the two position columns rather than asking anyone to declare.
//!
//! Everything else it inherits. The projector, the depth sort and the per-face
//! shading are `bar.rs`'s, one geometry over: a bar's six faces have three distinct
//! normals and get three shades, a surface's have a continuum and get a continuum.
use std::collections::HashMap;
use std::fmt::Write;
use crate::data::{DataFrame, Lattice};
use crate::ir::{Channel, Layer};
use crate::render::palette::{ramp_at, shade, PALETTE_GOG};
use crate::render::project;
use crate::render::svg::{unit_norm, SvgRenderer};
use crate::render::text::esc;
use crate::scale;

/// How dark a vertical face gets, with a level one left alone.
///
/// **Shaded by slope, which is a property of the surface and not of the camera** —
/// the invariant `palette::shade` was written for, generalized from three normals to
/// a continuum. A light fixed in screen space would re-shade the mesh as
/// `space(turn = )` swung it, so the same sheet would change color when the reader
/// turned it; keyed to the face's own tilt, turning the cube rearranges the faces and
/// leaves every shade alone.
///
/// The value is the 3-D bar's side-face shade, so a cliff on a sheet and the side of
/// a column read the same, and a plateau matches a bar's top at `0.00`.
const SLOPE_DIM: f64 = 0.30;

/// The seam hairline a face carries when no `border_color` overrides it, in px.
/// Antialiasing leaves a pale gap between two abutting polygons; the same job the
/// 3-D bar does between its own faces, with the opposite sign to the histogram's
/// separator (there the bars must be parted, here the faces must not be).
const SEAM: f64 = 0.5;

/// One face of the sheet, in the unit cube, with the depth it sorts by and the shade
/// its slope earns.
///
/// At module scope because **both floors produce one** — a lid over a cut cell and a
/// quad between four nodes are the same thing by the time they are painted, and two
/// paint loops would be two chances to disagree about the ramp, the seam or the sort.
struct Face {
    /// Projected corners, counter-clockwise seen from above.
    pts: [project::Screen; 4],
    depth: f64,
    /// The table row whose `color` the face takes — for a lid its own cell, for a
    /// mesh quad the corner at the lattice crossing `(i, j)` that names it. Still the
    /// whole answer for a **categorical** color, which cannot be averaged: a face
    /// belongs to one series or another, never to the mean of two.
    row: usize,
    /// What a **measured** color reads instead, when the face's own value is not
    /// simply its row's. `None` for a lid and a riser, each of which owns exactly one
    /// cell's number; `Some(mean of the four corners)` for a mesh quad, which owns
    /// none of its corners and spans all of them.
    measure: Option<f64>,
    dim: f64,
}

/// Everything a face needs to choose its fill, gathered once so the paint loop can be
/// shared without a nine-argument signature.
struct Paint<'a> {
    labels: Option<&'a [String]>,
    vals: Option<&'a [f64]>,
    scale: scale::ChannelScale,
    map: &'a HashMap<String, String>,
    ramp: &'a [String],
    default_color: &'a str,
    opacity: f64,
    mesh_color: Option<&'a str>,
    mesh_width: f64,
}

/// Sort the sheet back to front and write it.
///
/// **No back-face culling, unlike the bar**, and the reason is that a sheet is not a
/// solid: a box is convex so a face turned away from the camera is hidden by one facing
/// it and drawing it is wasted bytes, while a surface has an underside a reader is
/// entitled to see from below (`space(tilt = -20)`). Painter's order is then exact
/// rather than approximate — neither a height field over a lattice nor a floor of
/// disjoint footprints has cyclic overlap to break a sort.
fn paint_faces(svg: &mut String, faces: &mut [Face], clip: &str, p: &Paint) {
    faces.sort_by(|a, b| b.depth.partial_cmp(&a.depth).unwrap_or(std::cmp::Ordering::Equal));

    let (o, mesh_width) = (p.opacity, p.mesh_width);
    writeln!(svg, r##"  <g clip-path="url(#{clip})">"##).unwrap();
    for f in faces.iter() {
        let ramped: String;
        let color: &str = if let Some(labels) = p.labels {
            let lbl = labels.get(f.row).map(String::as_str).unwrap_or("");
            p.map.get(lbl).map(String::as_str).unwrap_or(p.default_color)
        } else if let Some(vals) = p.vals {
            // **The ramp lands per face**, which is what a mesh can do and a region
            // cannot: an `area` has one interior and would need a gradient fill, a face
            // is already small enough to hold one value (spec §15).
            //
            // *Which* one value is `measure`'s question, and the answer is the face's
            // own center rather than a corner it happens to be named after.
            let v = f.measure.unwrap_or_else(|| vals.get(f.row).copied().unwrap_or(f64::NAN));
            let frac = p.scale.fraction(v);
            ramped = ramp_at(&p.ramp.iter().map(String::as_str).collect::<Vec<_>>(), frac);
            &ramped
        } else {
            p.default_color
        };
        let fill = shade(color, f.dim);
        let stroke = p.mesh_color.unwrap_or(&fill);
        let d: String = f.pts.iter().enumerate()
            .map(|(k, s)| format!("{}{:.2},{:.2}", if k == 0 { "M" } else { "L" }, s.x, s.y))
            .collect::<Vec<_>>()
            .join(" ");
        writeln!(svg,
            r#"    <path d="{d} Z" fill="{fill}" fill-opacity="{o:.3}" stroke="{stroke}" stroke-width="{mesh_width}"/>"#
        ).unwrap();
    }
    writeln!(svg, "  </g>").unwrap();
}

impl SvgRenderer {
    // -----------------------------------------------------------------------
    // Mark: surface — the sheet, in `space`
    // -----------------------------------------------------------------------

    /// The mesh, projected and painted back to front.
    ///
    /// Takes no `Layout`: like `write_bars_3d`, every number it reads is in data
    /// units and the projector turns them into pixels. It takes no `polar` either —
    /// a surface draws in exactly one space (`mark_draws_in_space`), which makes it
    /// the only mark writer with no second coordinate to carry.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn write_surface(
        &self, svg: &mut String, layer: &Layer, df: &DataFrame,
        xs: (f64, f64), ys: (f64, f64), zs: (f64, f64),
        x_field: &str, y_field: &str, z_field: &str,
        color_map: &HashMap<String, String>,
        ramp: &[String],
        clip: &str,
        scene: &project::Scene,
    ) {
        // The height, always numeric. The two floor positions are read differently
        // depending on which floor this layer was handed, below — but a categorical
        // one is refused in `rule_for` and never arrives either way, so `positions`
        // is not needed: there is no category to map to a slot.
        let Some(z_col) = df.float_col(z_field) else { return };

        let color_labels = layer.encodings.get(&Channel::Color).and_then(|c| df.str_col(&c.field));
        let color_vals = layer.encodings.get(&Channel::Color).and_then(|c| df.float_col(&c.field));
        let color_scale = match color_vals {
            Some(c) => scale::ChannelScale::of(c, layer.encodings.get(&Channel::Color)),
            None => scale::ChannelScale::unbound(),
        };
        // A `group` split colors nothing and is not read here at all: it separates
        // one sheet's faces from another's, and the depth sort already interleaves
        // every face of every series. Two sheets therefore thread through each other
        // rather than one landing wholly in front — `path`'s per-segment ruling, whose
        // reason (an element has a near end and a far end, so it cannot sort whole)
        // is a face's reason too.
        let st = &layer.style;
        let default_color = st.color.as_deref().map(esc).unwrap_or_else(|| PALETTE_GOG[0].to_string());
        let o = st.opacity.unwrap_or(1.0);
        // The mesh lines, and the reading that made `border_*` worth spanning to this
        // mark (spec §4/§15): the seam hairline each face already carried, handed to
        // the caller. `border_size = 0` draws a seamless sheet.
        let mesh_color = st.border_color.as_deref().map(esc);
        let mesh_width = st.border_size.unwrap_or(SEAM);

        // Every face is built before anything is written, because the whole sheet
        // sorts together.
        let paint = Paint {
            labels: color_labels.map(|c| &c[..]),
            vals: color_vals.map(|c| &c[..]),
            scale: color_scale,
            map: color_map,
            ramp,
            default_color: &default_color,
            opacity: o,
            mesh_color: mesh_color.as_deref(),
            mesh_width,
        };
        let mut faces: Vec<Face> = Vec::new();

        // **Which floor was this layer handed?** The one question that divides the two
        // geometries a surface draws (spec §15), and it is read off the data rather
        // than declared: a transform that *cut* the plane published the four edge
        // columns, and `cell_edges` is the same reader the three slot marks use. A bare
        // pair of numeric positions published none, so the rows are **nodes**.
        //
        // *Cut cells → a lid each.* A cut cell asserts one value across its whole
        // extent and nothing beyond it, so the honest geometry is flat across the cell
        // with a step at the boundary. The lid is exactly the top face of the 3-D
        // `bar` that would stand on the same cell — which is why it takes that face's
        // `0.00` shade and, below, that mark's footprint sort.
        //
        // *Nodes → the mesh between them.* The founding reading: a face spans each
        // block of four adjacent samples and asserts every value between them.
        let cut = super::cell_edges(
            df, (crate::transform::CELL_START, crate::transform::CELL_END), x_field, None, 1.0,
        )
        .zip(super::cell_edges(
            df, (crate::transform::CELL_LOWER, crate::transform::CELL_UPPER), y_field, None, 1.0,
        ));

        if let Some(((x0s, x1s), (y0s, y1s))) = cut {
            // The floor the footprints are sorted at. Any constant z would order them
            // the same — that is what makes it a *floor* sort — and this is the one
            // `write_bars_3d` uses, so a terraced sheet and the histogram of the same
            // table occlude in the same order.
            let base = unit_norm(0.0_f64.clamp(zs.0, zs.1), zs);
            let n = z_col.len().min(x0s.len()).min(y0s.len());
            // **Sorted by footprint, not by a face's own depth** (spec §15). A lid
            // floats at its cell's height, so its own mean depth would let a tall far
            // cell claim to be nearer than a short near one — the error the 3-D bar's
            // rule exists to prevent, and a lid inherits it because a lid *is* that
            // bar's top. Reading the depth at the floor is `floor_order` expressed as a
            // key, so the shared sort does the work for both floors at once.
            let foot = |x: f64, y: f64| {
                scene.to_screen(unit_norm(x, xs), unit_norm(y, ys), base).depth
            };
            let quad = |c: [(f64, f64, f64); 4]| -> Option<[project::Screen; 4]> {
                let n: Vec<_> = c.iter()
                    .map(|&(a, b, d)| (unit_norm(a, xs), unit_norm(b, ys), unit_norm(d, zs)))
                    .collect();
                n.iter().all(|t| t.0.is_finite() && t.1.is_finite() && t.2.is_finite()).then(|| {
                    [
                        scene.to_screen(n[0].0, n[0].1, n[0].2),
                        scene.to_screen(n[1].0, n[1].1, n[1].2),
                        scene.to_screen(n[2].0, n[2].1, n[2].2),
                        scene.to_screen(n[3].0, n[3].1, n[3].2),
                    ]
                })
            };

            // The lids: one per cell, counter-clockwise seen from above, which is
            // `write_solid`'s top face corner for corner — one shape described once.
            for i in 0..n {
                let (z, x0, x1, y0, y1) = (z_col[i], x0s[i], x1s[i], y0s[i], y1s[i]);
                let Some(pts) = quad([(x0, y0, z), (x1, y0, z), (x1, y1, z), (x0, y1, z)]) else {
                    continue;
                };
                faces.push(Face {
                    pts,
                    depth: foot((x0 + x1) / 2.0, (y0 + y1) / 2.0),
                    row: i,
                    // A lid *is* its cell, so the row's own number is exact and there
                    // is nothing to average — the cut floor never had the mesh's
                    // problem, because a cut cell owns a value where a quad only spans
                    // four of them.
                    measure: None,
                    // A lid is level by construction, so it takes the color undimmed —
                    // `SLOPE_DIM * (1 - 1)`, the plateau end of the same continuum a
                    // sloped face samples further along.
                    dim: 0.0,
                });
            }

            // **The risers, and without them this is not a sheet.** Lids alone are
            // disconnected tiles floating at their own heights: the eye gets confetti
            // rather than relief, and the claim this whole geometry rests on — that a
            // cut floor tiles without gaps — would be true only in plan view. The riser
            // is the face of the *step*, and it stands on the boundary line two cells
            // share, which has zero width and so asserts nothing about any area. It is
            // exactly what a jump discontinuity looks like.
            //
            // *Why this is still not the bar's wall.* A riser spans the **difference**
            // between two neighbors, where a column spans the whole way from the
            // baseline. On a smooth field the steps are small, so the sheet reads from
            // any angle the reader turns it to — which is the entire reason a terraced
            // surface beats a 3-D histogram for a field that varies gently.
            //
            // Cells are indexed by their lower corner, so the same `Lattice` that
            // recovers a node mesh recovers the cell grid — neighbors are lattice
            // neighbors, and a missing cell simply has none.
            if let Some(grid) = Lattice::of(&x0s[..n], &y0s[..n]) {
                let (nx, ny) = (grid.xs.len(), grid.ys.len());
                // Each shared edge is visited **once**, from the cell on its low side,
                // so a riser is never drawn twice into the same plane.
                for j in 0..ny {
                    for i in 0..nx {
                        let Some(a) = grid.at(i, j) else { continue };
                        if a >= n { continue; }
                        for (di, dj) in [(1usize, 0usize), (0, 1)] {
                            let (Some(ii), Some(jj)) = (i.checked_add(di), j.checked_add(dj))
                            else { continue };
                            if ii >= nx || jj >= ny { continue }
                            let Some(b) = grid.at(ii, jj) else { continue };
                            if b >= n { continue; }
                            let (za, zb) = (z_col[a], z_col[b]);
                            let (lo, hi) = (za.min(zb), za.max(zb));
                            if !(hi - lo).is_finite() || (hi - lo).abs() < 1e-12 { continue }
                            // The taller neighbor owns the face, exactly as a bar owns
                            // its own side wall — so `color` follows the plateau the
                            // step descends *from*, and a cutoff split colors the riser
                            // with the cell a reader reads it as belonging to.
                            let owner = if za >= zb { a } else { b };
                            let (pts, cx, cy) = if dj == 0 {
                                let xb = x1s[a];
                                (quad([(xb, y0s[a], lo), (xb, y1s[a], lo),
                                       (xb, y1s[a], hi), (xb, y0s[a], hi)]),
                                 xb, (y0s[a] + y1s[a]) / 2.0)
                            } else {
                                let yb = y1s[a];
                                (quad([(x0s[a], yb, lo), (x1s[a], yb, lo),
                                       (x1s[a], yb, hi), (x0s[a], yb, hi)]),
                                 (x0s[a] + x1s[a]) / 2.0, yb)
                            };
                            let Some(pts) = pts else { continue };
                            faces.push(Face {
                                pts,
                                depth: foot(cx, cy),
                                row: owner,
                                // The taller neighbor's own value, deliberately, for
                                // the reason just above: a riser is the step's face and
                                // belongs to the plateau it descends from. Averaging
                                // the two would paint it a height neither cell has.
                                measure: None,
                                // Vertical, so the far end of the same continuum the
                                // lid sits at zero of — a bar's side-face shade.
                                dim: SLOPE_DIM,
                            });
                        }
                    }
                }
            }
            paint_faces(svg, &mut faces, clip, &paint);
            return;
        }

        let (Some(x_col), Some(y_col)) = (df.float_col(x_field), df.float_col(y_field)) else {
            return;
        };
        let Some(lattice) = Lattice::of(x_col, y_col) else { return };

        let node = |r: usize| -> Option<(f64, f64, f64)> {
            let (nx, ny, nz) = (
                unit_norm(*x_col.get(r)?, xs),
                unit_norm(*y_col.get(r)?, ys),
                unit_norm(*z_col.get(r)?, zs),
            );
            (nx.is_finite() && ny.is_finite() && nz.is_finite()).then_some((nx, ny, nz))
        };

        for corners in lattice.faces() {
            // A corner a log scale cannot place leaves the face undrawable. Dropping
            // the face rather than the sheet is the same choice `write_points` makes
            // per glyph, and `warn_unplaceable` reports the values once.
            let Some(n) = corners.iter().map(|&r| node(r)).collect::<Option<Vec<_>>>() else {
                continue;
            };
            let pts = [
                scene.to_screen(n[0].0, n[0].1, n[0].2),
                scene.to_screen(n[1].0, n[1].1, n[1].2),
                scene.to_screen(n[2].0, n[2].1, n[2].2),
                scene.to_screen(n[3].0, n[3].1, n[3].2),
            ];
            // The face's normal from its two diagonals, which handles the ordinary
            // case of a quad whose four heights are not coplanar — the cross product
            // of the diagonals is the average normal, where picking one pair of edges
            // would report the tilt of one corner.
            let (d1, d2) = (
                (n[2].0 - n[0].0, n[2].1 - n[0].1, n[2].2 - n[0].2),
                (n[3].0 - n[1].0, n[3].1 - n[1].1, n[3].2 - n[1].2),
            );
            let cross = (
                d1.1 * d2.2 - d1.2 * d2.1,
                d1.2 * d2.0 - d1.0 * d2.2,
                d1.0 * d2.1 - d1.1 * d2.0,
            );
            let len = (cross.0 * cross.0 + cross.1 * cross.1 + cross.2 * cross.2).sqrt();
            // A degenerate face (zero area) has no normal; read it as level rather
            // than dividing by zero. It draws as a line and shows nothing either way.
            let level = if len > 0.0 { (cross.2 / len).abs() } else { 1.0 };
            // **The measured color is read at the face's center, which is the mean of
            // its four corners** — the same argument the normal above makes, one
            // attribute over: picking one pair of edges would report the tilt of one
            // corner, and picking `corners[0]` reported the *value* at one corner.
            //
            // It is the field's own value there rather than a summary of convenience:
            // the face interpolates bilinearly between its corners, and a bilinear
            // patch at its center is exactly their mean. So the ramp and the height
            // agree at every face's center, which is the whole of what "the height,
            // said twice" claims — and under `corners[0]` that claim was off by half a
            // cell everywhere.
            //
            // The old reading also *discarded data*: a face is named by its low corner,
            // so on an `nx` by `ny` lattice the last row and the last column colored
            // nothing at all. Five of nine values on a 3x3 grid, and a symmetric field
            // came out asymmetric — four congruent faces painted three colors, which is
            // a difference the data did not have. Invisible on a fine mesh, where
            // neighbors barely differ, which is why it survived the volcano.
            let measure = color_vals.map(|vals| {
                let (mut sum, mut k) = (0.0, 0.0);
                for &r in corners.iter() {
                    if let Some(v) = vals.get(r).copied().filter(|v| v.is_finite()) {
                        sum += v;
                        k += 1.0;
                    }
                }
                if k > 0.0 { sum / k } else { f64::NAN }
            });
            faces.push(Face {
                pts,
                depth: pts.iter().map(|p| p.depth).sum::<f64>() / 4.0,
                row: corners[0],
                measure,
                // Level face → the color itself, vertical → `SLOPE_DIM`. The
                // continuum a bar's three fixed shades are three samples of.
                dim: SLOPE_DIM * (1.0 - level),
            });
        }

        paint_faces(svg, &mut faces, clip, &paint);
    }
}
