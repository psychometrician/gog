//! The frame of a space that is not flat: the cube's box, the polar disc and the
//! globe's rim, each with its ticks and names.

use super::*;

/// The gap between a polar plot's rim and the ring of angular tick labels around
/// it — the circular counterpart of the gap between a flat axis and its labels.
/// Named once so the frame that reserves the room and the labels that fill it
/// cannot drift apart.
pub(super) const POLAR_RIM_GAP: f64 = 9.0;

impl SvgRenderer {
    // -----------------------------------------------------------------------
    // The polar frame — background, gridlines and rim
    //
    // Every guide a flat panel draws along two straight edges, a polar panel
    // draws around a circle: the background is a disc, the gridlines are rings at
    // the radial ticks and spokes at the angular ones, and the axis line is the
    // rim itself. Same ticks, same ranges, same colors — only the geometry the
    // positions are put through differs, which is the whole claim a coordinate
    // space makes (spec §9).
    // -----------------------------------------------------------------------

    #[allow(clippy::too_many_arguments)]
    pub(super) fn write_polar_frame(
        &self, svg: &mut String, p: &Polar,
        x_ticks: &TickSpec, xs: (f64, f64),
        y_ticks: &TickSpec, ys: (f64, f64),
        clip: &str,
        theme: &ThemeSpec,
        // Where the spokes start, as a fraction of the radial range: the cells'
        // inner edge, or `0.0` for a plot whose marks reach the center.
        inner_edge: f64,
    ) {
        writeln!(svg,
            r##"  <circle cx="{cx:.2}" cy="{cy:.2}" r="{r:.2}" fill="{bg}"/>"##,
            cx = p.cx, cy = p.cy, r = p.r_max, bg = theme.background_or(PANEL_BG)
        ).unwrap();
        // A hair wider than the disc: the clip is a safety net for a value the
        // scale could not place, not an edge to shave the glyphs sitting on the rim.
        writeln!(svg,
            r#"  <clipPath id="{clip}"><circle cx="{cx:.2}" cy="{cy:.2}" r="{r:.2}"/></clipPath>"#,
            cx = p.cx, cy = p.cy, r = p.r_max + 8.0
        ).unwrap();

        // A pie has no radial scale to ring and no angular categories to spoke: its
        // whole surface is the measure, and its key is the legend. Drawing the flat
        // plot's guides here would be furniture decoding nothing.
        if p.measure_on_angle {
            if theme.frame_drawn() {
                writeln!(svg,
                    r##"  <circle cx="{cx:.2}" cy="{cy:.2}" r="{r:.2}" fill="none" stroke="{c}" stroke-width="1.5"/>"##,
                    cx = p.cx, cy = p.cy, r = p.r_max, c = crate::ir::THEME_FRAME_COLOR
                ).unwrap();
            }
            return;
        }

        // `theme(grid = )` reaches the circle too, and naming the sets by their
        // *axis* rather than by their direction is what lets it: the y axis's
        // gridlines are rings here and horizontal lines in the flat space, but
        // they are the y axis's in both. A `grid = "x"` that meant "vertical"
        // would have no reading at all inside a circle — the Law 2 exception this
        // naming avoids rather than special-cases.
        writeln!(svg, r##"  <g stroke="#d2d2da" stroke-width="1" fill="none">"##).unwrap();
        if theme.grid_y() {
            for &v in &y_ticks.values {
                let r = p.radius(unit_norm(v, ys));
                if r > 0.5 && r <= p.r_max + 0.5 {
                    writeln!(svg, r#"    <circle cx="{cx:.2}" cy="{cy:.2}" r="{r:.2}"/>"#,
                        cx = p.cx, cy = p.cy).unwrap();
                }
            }
        }
        if theme.grid_x() {
            for (u, _) in polar_angles(x_ticks, xs) {
                let (x0, y0) = p.at(u, inner_edge);
                let (x, y) = p.at(u, 1.0);
                writeln!(svg, r#"    <line x1="{x0:.2}" y1="{y0:.2}" x2="{x:.2}" y2="{y:.2}"/>"#)
                    .unwrap();
            }
        }
        writeln!(svg, "  </g>").unwrap();

        // The rim is the axis line: in polar the two straight edges of a flat
        // panel are one closed curve, so there is one of it and not two.
        // The rim is the axis line, and in polar the flat panel's two straight
        // edges are one closed curve — so `frame = "full"` and `"axes"` draw the
        // same picture here and only `"none"` differs. The setting reaches the
        // circle rather than stopping at the rectangle, which is Law 2.
        if theme.frame_drawn() {
            writeln!(svg,
                r##"  <circle cx="{cx:.2}" cy="{cy:.2}" r="{r:.2}" fill="none" stroke="{c}" stroke-width="1.5"/>"##,
                cx = p.cx, cy = p.cy, r = p.r_max, c = crate::ir::THEME_FRAME_COLOR
            ).unwrap();
        }
    }

    /// The globe's panel: the disk, its graticule, and the limb.
    ///
    /// The disk is the panel background bent onto the sphere — `write_polar_frame`'s
    /// circular panel, one space over — and **the graticule is the panel grid**
    /// (the user's ruling): every space draws gridlines, `polar` bends them into
    /// rings and spokes, and this space bends them into meridians and parallels.
    /// `theme(grid = )` names the sets by *axis*, polar's own rule: `x`'s
    /// gridlines are the meridians here (lines of constant longitude, where `x`
    /// is bound) and `y`'s the parallels. An *emphasized* meridian or parallel
    /// is a `rule` mark, not this furniture. The limb is the axis line — one
    /// closed curve, like polar's rim, so `frame = "full"` and `"axes"` draw the
    /// same picture and only `"none"` differs.
    pub(super) fn write_globe_frame(
        &self, svg: &mut String, g: &crate::render::globe::Globe,
        clip: &str, theme: &ThemeSpec,
    ) {
        writeln!(svg,
            r##"  <circle cx="{cx:.2}" cy="{cy:.2}" r="{r:.2}" fill="{bg}"/>"##,
            cx = g.cx, cy = g.cy, r = g.r, bg = theme.background_or(PANEL_BG)
        ).unwrap();
        // A hair wider than the disk — or than the spike ceiling, when the plot
        // has one — polar's own margin: the clip is a safety net, not an edge
        // to shave a glyph on the limb or a spike standing past it.
        writeln!(svg,
            r#"  <clipPath id="{clip}"><circle cx="{cx:.2}" cy="{cy:.2}" r="{r:.2}"/></clipPath>"#,
            cx = g.cx, cy = g.cy, r = g.r * (1.0 + g.headroom) + 8.0
        ).unwrap();

        writeln!(svg, r##"  <g stroke="#d2d2da" stroke-width="1" fill="none">"##).unwrap();
        let mut lines: Vec<Vec<(f64, f64)>> = Vec::new();
        if theme.grid_x() {
            lines.extend(g.meridians());
        }
        if theme.grid_y() {
            lines.extend(g.parallels());
        }
        for run in lines {
            let pts: String = run.iter()
                .map(|(x, y)| format!("{x:.2},{y:.2}"))
                .collect::<Vec<_>>()
                .join(" ");
            writeln!(svg, r#"    <polyline points="{pts}"/>"#).unwrap();
        }
        writeln!(svg, "  </g>").unwrap();

        if theme.frame_drawn() {
            writeln!(svg,
                r##"  <circle cx="{cx:.2}" cy="{cy:.2}" r="{r:.2}" fill="none" stroke="{c}" stroke-width="1.5"/>"##,
                cx = g.cx, cy = g.cy, r = g.r, c = crate::ir::THEME_FRAME_COLOR
            ).unwrap();
        }
    }

    /// The polar tick labels: the angular names ringed outside the circle, the
    /// radial numbers running out along the spoke the circle starts on. Drawn
    /// after the marks, like the flat tick labels, so they stay readable over
    /// whatever was painted under them.
    pub(super) fn write_polar_ticks(
        &self, svg: &mut String, p: &Polar,
        x_ticks: &TickSpec, xs: (f64, f64),
        y_ticks: &TickSpec, ys: (f64, f64),
    ) {
        if p.measure_on_angle {
            return; // a pie is decoded by its legend, not by ticks (see the frame)
        }
        let cap = estimate_cap_height(self.font_sm);
        writeln!(svg,
            r##"  <g font-family="system-ui,sans-serif" font-size="{}" fill="#3c3c46">"##,
            self.font_sm
        ).unwrap();
        for (u, label) in polar_angles(x_ticks, xs) {
            if label.is_empty() { continue }
            let (x, y, anchor) = p.rim_label(u, POLAR_RIM_GAP, cap);
            writeln!(svg,
                r#"    <text x="{x:.2}" y="{y:.2}" text-anchor="{anchor}">{}</text>"#, esc(label)
            ).unwrap();
        }
        writeln!(svg, "  </g>").unwrap();

        writeln!(svg,
            r##"  <g font-family="system-ui,sans-serif" font-size="{}" fill="#5a5a64" text-anchor="start">"##,
            self.font_sm
        ).unwrap();
        for (i, &v) in y_ticks.values.iter().enumerate() {
            let frac = unit_norm(v, ys);
            let r = p.radius(frac);
            if !(r > 0.5) || r > p.r_max + 0.5 { continue }
            let (x, y) = p.at(0.0, frac);
            let label = y_ticks.labels.get(i).map(String::as_str).unwrap_or("");
            if label.is_empty() { continue }
            writeln!(svg, r#"    <text x="{:.2}" y="{:.2}">{}</text>"#,
                x + 4.0, y + cap * 0.35, esc(label)).unwrap();
        }
        writeln!(svg, "  </g>").unwrap();
    }

    // -----------------------------------------------------------------------
    // 3-D frame — the glass box that makes a projected cloud readable
    // -----------------------------------------------------------------------

    /// The **box** of a `space` plot's guides: the projected unit cube as a faint
    /// wireframe, plus the three edges its axes are numbered along drawn darker
    /// (`FrameEdge::choose`, the edges `write_space_labels` writes along). Drawn
    /// before the marks so the box sits *behind* the cloud — a draw-order stand-in
    /// for true occlusion, which with front-edge culling is still M8b (spec §16).
    ///
    /// The **labels** are not here. They are guides rather than scene elements, so
    /// they go on after the marks (`write_space_labels`), which is the rule the
    /// flat and polar frames already follow.
    pub(super) fn write_space_box(&self, svg: &mut String, scene: &project::Scene) {
        let corners: Vec<Screen> = project::CUBE_CORNERS.iter()
            .map(|&(x, y, z)| scene.to_screen(x, y, z))
            .collect();

        // The whole cube as a faint wireframe — the depth cue the cloud reads against.
        writeln!(svg, r##"  <g stroke="#d8d8de" stroke-width="1" fill="none">"##).unwrap();
        for &(a, b) in &project::CUBE_EDGES {
            writeln!(svg, r#"    <line x1="{:.2}" y1="{:.2}" x2="{:.2}" y2="{:.2}"/>"#,
                corners[a].x, corners[a].y, corners[b].x, corners[b].y).unwrap();
        }
        writeln!(svg, "  </g>").unwrap();

        // The three edges the numbers are written along, a touch darker, chosen by
        // the rule `write_space_labels` places them by. These were the three edges
        // from the origin corner (0,0,0), which at every ordinary angle is the
        // corner farthest from the camera: the dark edges met at the back of the
        // floor and carried no number, and the numbered edges were the faint ones
        // (ruled 2026-09-28: emphasize the numbered edges).
        writeln!(svg, r##"  <g stroke="#9a9aa4" stroke-width="1.4" fill="none">"##).unwrap();
        for axis in [FrameAxis::X, FrameAxis::Y, FrameAxis::Z] {
            let edge = FrameEdge::choose(scene, axis);
            let (a, b) = (edge.at(0.0), edge.at(1.0));
            let (pa, pb) = (scene.to_screen(a.0, a.1, a.2), scene.to_screen(b.0, b.1, b.2));
            writeln!(svg, r#"    <line x1="{:.2}" y1="{:.2}" x2="{:.2}" y2="{:.2}"/>"#,
                pa.x, pa.y, pb.x, pb.y).unwrap();
        }
        writeln!(svg, "  </g>").unwrap();
    }

    /// The **labels** of a `space` plot's guides: tick numbers along the three
    /// edges `FrameEdge::choose` picks for this view (the ones `write_space_box`
    /// darkens), and each axis named at its far end. Billboarded (always
    /// horizontal) and drawn *after* the marks, so a solid mesh cannot paint over
    /// the numbers that say what it measures.
    ///
    /// Placement is **measured**, which is the whole difference from the version
    /// that shipped with M8a. That one computed a position per label and wrote it,
    /// so nothing stopped two labels landing in the same place: at the origin
    /// corner three axes meet and their near-origin numbers piled up, and an
    /// axis's name printed through its own last tick (`S8pal Length` in the
    /// book's first 3-D plot). Every other guide in the engine measures its text —
    /// a flat tick label anchors inward off `estimate_text_width`, a margin is
    /// sized from it — and this was the one that did not.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn write_space_labels(
        &self, svg: &mut String, scene: &project::Scene, l: &Layout, theme: &ThemeSpec,
        view: SpaceView,
        // What each axis's binding *asked* for, where it asked — `None` is the
        // engine's default and stays silent if the frame has to thin it.
        asked: [Option<usize>; 3],
        x_ticks: &TickSpec, xs: (f64, f64), x_label: &str,
        y_ticks: &TickSpec, ys: (f64, f64), y_label: &str,
        z_ticks: &TickSpec, zs: (f64, f64), z_label: &str,
        // Which axes' labels are category names rather than numbers, `[x, y, z]`.
        // A number that meets its own axis's neighbor is dropped, and a name is
        // nudged instead (below).
        named: [bool; 3],
        // Where the thinning report goes. **Not `eprintln!`**, which is what it
        // was: this routine runs once per panel, so a faceted cube said the same
        // sentence once per cube. `remarks` is the list that already dedupes by
        // message (see `draw`), on the rule that a fact true of two panels is one
        // fact said twice. Nobody could see it before, because until the facet
        // gate came off a cube could not be faceted.
        remarks: &mut Vec<Diagnostic>,
    ) {
        let asked_for = |axis: FrameAxis| match axis {
            FrameAxis::X => asked[0],
            FrameAxis::Y => asked[1],
            FrameAxis::Z => asked[2],
        };
        let center = scene.to_screen(0.5, 0.5, 0.5);
        let axes = [
            (FrameAxis::X, x_ticks, xs, x_label),
            (FrameAxis::Y, y_ticks, ys, y_label),
            (FrameAxis::Z, z_ticks, zs, z_label),
        ];

        // Candidates in priority order, and the order is **pinned before free**.
        // A tick label is fixed to a value on an edge and can only be dropped; an
        // axis name is free to slide along its edge and away from it, so it is the
        // one that should give way. Placing names first — the intuitive reading,
        // since a nameless measurement is the worse loss — instead spent a *middle*
        // tick to seat each name, which reads as a bug in the sequence (`500 400
        // 200 100 0`) rather than as a choice, and costs a number that could not
        // have gone anywhere else. Within an axis the ticks are walked from the far
        // end *inward*, so what thins is the crowd where edges meet rather than the
        // labels that had room.
        // Measured first, placed second. A name's distance from its edge is read
        // off its own numbers, so the numbers are built before any of them is put
        // anywhere — which is also why this cannot be one pass per axis.
        //
        // **An axis seen end-on is not labeled.** Looking straight down the cube
        // (`tilt = 90`), the vertical axis projects to a point, so every one of its
        // numbers lands on one spot; the same-axis rule dropped all but the first,
        // and a lone "4" stood where the axis had been. An axis whose edge spans
        // less than a label's height on the page has no length to be read along, so
        // it draws neither its numbers nor its name.
        let built: Vec<(FrameEdge, Vec<FrameLabel>, &str)> = axes.iter()
            .map(|(axis, ticks, range, label)| {
                let edge = FrameEdge::choose(scene, *axis);
                let (a, b) = (edge.at(0.0), edge.at(1.0));
                let (pa, pb) = (scene.to_screen(a.0, a.1, a.2), scene.to_screen(b.0, b.1, b.2));
                let span = (pb.x - pa.x).hypot(pb.y - pa.y);
                match span < estimate_cap_height(self.font_sm) {
                    true => (edge, Vec::new(), ""),
                    false => (edge, self.frame_tick_labels(scene, center, ticks, *range, edge), *label),
                }
            })
            .collect();

        let mut queue: Vec<FrameLabel> = Vec::new();
        for (_, ticks, _) in &built {
            queue.extend(ticks.iter().cloned());
        }
        for (edge, ticks, label) in &built {
            if label.is_empty() { continue }
            queue.push(self.frame_name_label(scene, center, l, *edge, ticks, label));
        }

        // Place them, and the two kinds of collision get different answers.
        //
        // Two *different* axes landing together is a coincidence of the projection:
        // their edges meet at a corner and their numbers are unrelated, so pushing
        // one further out separates them and both survive.
        //
        // One axis colliding with **itself** is the foreshortened axis, and a nudge
        // is exactly the wrong answer there. Its perpendicular runs across the axis,
        // so nudging alternate numbers staggers the column into two zigzagging
        // ranks, which a reader reads as broken rather than as tight — measured on
        // `bar * mean` at `tilt = 85`, where it produced `80 70 50 40` at two
        // different x positions. So a number that collides with its own axis is
        // **dropped**, which thins the column and leaves the surviving numbers where
        // they were. Tick *selection* is untouched either way: a coarser step climbs
        // the scale ceiling and leaves the cube half empty, which is the recorded
        // reason a narrow flat panel is not thinned either (see `build_axis`).
        //
        // A *name* is free to move whatever it meets, having no fixed value to sit
        // at — which is why the rule is about two numbers of one axis rather than
        // about one axis's labels.
        let offered: Vec<(FrameAxis, usize)> = built.iter()
            .map(|(edge, ticks, _)| (edge.axis, ticks.len()))
            .collect();
        //
        // **A category name is not a number, and is nudged like a name.** Dropping
        // a number thins a scale the reader can still read; dropping a category
        // loses which slot is which. The floor of `bar * bin(12) + y(continent) +
        // space()` lost "Americas" in silence, 3.6 px from "Africa" at the default
        // view, so an axis of names takes the nudges a free label takes, and a name
        // that still finds no room is reported below.
        let is_named = |axis: FrameAxis| match axis {
            FrameAxis::X => named[0],
            FrameAxis::Y => named[1],
            FrameAxis::Z => named[2],
        };
        let mut placed: Vec<FrameLabel> = Vec::new();
        let mut unplaced: Vec<(FrameAxis, String)> = Vec::new();
        for mut cand in queue {
            let mut fits = false;
            for step in 0..=FRAME_LABEL_NUDGES {
                cand.nudge(step as f64 * FRAME_LABEL_NUDGE);
                let blockers: Vec<&FrameLabel> = placed.iter()
                    .filter(|p| !cand.clears(p, TICK_GAP))
                    .collect();
                if blockers.is_empty() {
                    fits = true;
                    break;
                }
                if !is_named(cand.axis)
                    && blockers.iter().any(|p| p.tick && cand.tick && p.axis == cand.axis)
                {
                    break;
                }
            }
            if fits {
                placed.push(cand);
            } else if cand.tick && is_named(cand.axis) {
                unplaced.push((cand.axis, cand.text.clone()));
            }
        }
        for axis in [FrameAxis::X, FrameAxis::Y, FrameAxis::Z] {
            let names: Vec<String> = unplaced.iter().filter(|(a, _)| *a == axis)
                .map(|(_, t)| format!("\"{t}\"")).collect();
            if names.is_empty() { continue }
            let (list, verb) = match names.as_slice() {
                [one] => (one.clone(), "has"),
                [init @ .., last] => (format!("{} and {last}", init.join(", ")), "have"),
                [] => unreachable!(),
            };
            remarks.push(Diagnostic {
                kind: crate::legality::DiagnosticKind::Assumption,
                message: format!(
                    "gog: at `turn = {turn:.0}, tilt = {tilt:.0}` the cube's `{c}` axis {verb} no room \
                     for {list}, so {} not labeled. Turn the view to bring {} back, as in \
                     `space(turn = {other:.0})`, or give the plot more room.",
                    if names.len() == 1 { "it is" } else { "they are" },
                    if names.len() == 1 { "it" } else { "them" },
                    c = frame_channel_name(axis), turn = view.turn, tilt = view.tilt,
                    other = (view.turn + 15.0).rem_euclid(360.0),
                ),
            });
        }

        // A count the caller *stated* and the frame could not draw is said out
        // loud (spec §12): thinning is the right answer to a foreshortened axis,
        // but doing it silently to an explicit `tick_count` is accepting a binding
        // and dropping it. The engine's own default stays silent, which is §12's
        // omission rule — an unambiguous default needs no warning, and only the
        // renderer knows how many labels fit, exactly as with a dot plot's piles.
        for (axis, wanted) in offered {
            let Some(asked) = asked_for(axis) else { continue };
            let drawn = placed.iter().filter(|l| l.tick && l.axis == axis).count();
            if drawn >= wanted { continue }
            remarks.push(Diagnostic {
                kind: crate::legality::DiagnosticKind::Assumption,
                message: format!(
                    "gog: `{c}(…, tick_count = {asked})` asked for {asked} ticks and {drawn} fit — \
                     this axis projects too short at `tilt = {tilt:.0}` for the rest, so the labels \
                     were thinned rather than overlapped. The scale is unchanged (the ones drawn \
                     are the ones you asked for, at the same spacing); lower the tilt, ask for \
                     fewer, or leave `tick_count` off.",
                    c = frame_channel_name(axis), tilt = view.tilt,
                ),
            });
        }

        // A halo in the panel's own color, because these labels now sit *over*
        // the marks. A flat tick label reads against a clean margin; the cube has
        // no margin to sit in (`FRAME_INSET` is inside the panel), so the clear
        // ground a guide needs is drawn rather than reserved.
        let bg = theme.background_or(PANEL_BG);
        writeln!(svg,
            r##"  <g font-family="system-ui,sans-serif" text-anchor="middle" paint-order="stroke" stroke="{bg}" stroke-width="3" stroke-linejoin="round">"##
        ).unwrap();
        for l in &placed {
            // On a panel dark enough that the dark default would disappear, the
            // light ink: the facet strip's rule (`strip_ink`), one ground over.
            // Written dark on dark, a night cube's numbers read as nothing.
            let fill = crate::color::better_ink(&bg, l.fill, STRIP_INK_LIGHT).unwrap_or(l.fill);
            writeln!(svg,
                r#"    <text x="{:.2}" y="{:.2}" font-size="{}" fill="{}">{}</text>"#,
                l.x, l.y, l.font, fill, esc(&l.text)).unwrap();
        }
        writeln!(svg, "  </g>").unwrap();
    }

    /// The tick labels of one 3-D axis, far end first — candidates, not yet
    /// placed. Each is offset *perpendicular to its own edge*, on the side away
    /// from the cube center: the three edges leave their corner in three screen
    /// directions, so a radial nudge would stack their labels on top of each other
    /// where a per-axis perpendicular fans them apart.
    pub(super) fn frame_tick_labels(
        &self, scene: &project::Scene, center: Screen,
        ticks: &TickSpec, range: (f64, f64), edge: FrameEdge,
    ) -> Vec<FrameLabel> {
        let (lo, hi) = (range.0.min(range.1), range.0.max(range.1));
        let cap = estimate_cap_height(self.font_sm);
        let perp = frame_perp(scene, center, edge);
        let mut out = Vec::new();
        for (v, lab) in ticks.values.iter().zip(&ticks.labels).rev() {
            if *v < lo - 1e-9 || *v > hi + 1e-9 { continue }
            if lab.is_empty() { continue }
            let (dx, dy, dz) = edge.at(unit_norm(*v, range));
            let p = scene.to_screen(dx, dy, dz);
            out.push(FrameLabel::new(
                lab.clone(), p.x, p.y + cap / 2.0, perp, FRAME_TICK_OFFSET,
                self.font_sm, "#3c3c46", edge.axis, true,
            ));
        }
        out
    }

    /// The name of one 3-D axis, placed **outside its own numbers** — a candidate,
    /// not yet placed.
    ///
    /// Centered on its edge rather than at the far end, and that is structural
    /// rather than cosmetic: the two domain edges are the two that meet at the lowest
    /// corner, so their far ends are *the same corner* and M8a's two names arrived
    /// on top of each other there. The midpoint is also where a reader looks for an
    /// axis's name, and what the flat frame does — `write_labels` centers the x name
    /// under the panel.
    ///
    /// How far out is **measured from the axis's own tick labels**, the rule
    /// `PanelGrid` already uses to size a flat y-axis margin: a name clears the
    /// widest number beside it. Both extents are taken *along the perpendicular*,
    /// which is what makes one rule serve three axes with very different needs — a
    /// floor edge's perpendicular is mostly vertical, so a domain name needs only
    /// its height cleared however long the words are, while the vertical strut's is
    /// horizontal and a long name there needs its full width.
    ///
    /// And when that does not fit the panel, the name goes past the **end** of its
    /// edge instead. Two placements tried in order, first one that fits — still one
    /// rule rather than a case per axis: a name sits beside its numbers when there
    /// is room beside them, and beyond its axis when there is not. `Elevation (m)`
    /// against the cube's left strut is the second case (the frame's inset leaves
    /// ~84px and the name wants ~90px), which puts it above the strut, where M8a
    /// put every name and where M8a happened to be right.
    pub(super) fn frame_name_label(
        &self, scene: &project::Scene, center: Screen, l: &Layout,
        edge: FrameEdge, ticks: &[FrameLabel], label: &str,
    ) -> FrameLabel {
        let cap = estimate_cap_height(self.font_sm);
        let perp = frame_perp(scene, center, edge);
        let name = |x: f64, y: f64, dir: (f64, f64), off: f64| FrameLabel::new(
            label.to_string(), x, y, dir, off, self.font_md, "#28283a", edge.axis, false,
        );
        // How much room the name itself and the widest number take, each measured
        // across the direction the name is being pushed along.
        let across = |dir: (f64, f64), half_w: f64, cap: f64|
            dir.0.abs() * half_w + dir.1.abs() * cap / 2.0;
        let mine = across(perp, crate::render::text::estimate_text_width(label, self.font_md) / 2.0,
                          estimate_cap_height(self.font_md));
        let widest = ticks.iter()
            .map(|t| across(perp, t.half_w, t.cap))
            .fold(0.0_f64, f64::max);

        let (mx, my, mz) = edge.at(0.5);
        let m = scene.to_screen(mx, my, mz);
        let beside = name(m.x, m.y + cap / 2.0, perp,
                          FRAME_TICK_OFFSET + widest + TICK_GAP + mine);
        if beside.within(l) {
            return beside;
        }

        // Past the end of the edge, along the edge's own screen direction. For the
        // strut that is straight up, above the topmost number.
        let (a, b) = (edge.at(0.0), edge.at(1.0));
        let (o, f) = (scene.to_screen(a.0, a.1, a.2), scene.to_screen(b.0, b.1, b.2));
        let (adx, ady) = (f.x - o.x, f.y - o.y);
        let alen = (adx * adx + ady * ady).sqrt().max(1e-6);
        let along = (adx / alen, ady / alen);
        let mine_along = across(along, crate::render::text::estimate_text_width(label, self.font_md) / 2.0,
                                estimate_cap_height(self.font_md));
        name(f.x, f.y + cap / 2.0, along, TICK_GAP + mine_along)
    }
}

/// Fraction of a panel rectangle left clear around the projected cube, so the
/// frame's tick labels and axis names have room to sit outside the box.
pub(super) const FRAME_INSET: f64 = 0.14;

/// The fraction of the panel's shorter side kept clear around the globe's limb.
/// Far smaller than the cube's inset because nothing is written out there — a
/// globe has no tick labels — and the margin only keeps the limb off the panel
/// edge.
pub(super) const GLOBE_INSET: f64 = 0.04;

/// Which axis of the 3-D frame a tick run belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FrameAxis {
    X,
    Y,
    Z,
}

/// The channel name a 3-D frame axis carries, for a diagnostic that has to quote
/// the caller's own word back to them.
pub(super) fn frame_channel_name(axis: FrameAxis) -> &'static str {
    match axis {
        FrameAxis::X => "x",
        FrameAxis::Y => "y",
        FrameAxis::Z => "z",
    }
}

/// Which of the cube's **four** parallel edges an axis is ticked along, and where
/// on it a value sits.
///
/// The cube offers four edges per axis and M8a always took the three radiating
/// from corner `(0,0,0)`. That corner is the one *farthest* from the camera at
/// every ordinary viewing angle (at the book's default `turn = 30, tilt = 25` its
/// depth is `+0.83`, the maximum of the eight), so the frame ticked its three back
/// edges and every label lay across the middle of the picture. Draw order hid it:
/// the labels went on before the marks, so the stretch of each edge that ran
/// behind the data was simply invisible, and what showed was the stub that reached
/// past the silhouette. Drawing the labels on top — which is what a guide needs —
/// exposes it at once, so the edge choice is part of the same fix rather than a
/// later polish item.
///
/// The rule is measured, per axis, per view: **the edge on the outside of the
/// projected silhouette.** A domain axis stays on the floor (`z = 0`), which is
/// where the plane the data sits on is, and takes the floor edge **lowest on the
/// screen** — the two of them make the V at the bottom of the projection, below
/// anything standing on the floor. From above that is the edge nearest the camera,
/// from below the farthest (`choose`). The measure axis takes the vertical strut
/// **furthest from the projected center**, which is the cube's left or right
/// silhouette and so is never inside the data either.
#[derive(Clone, Copy)]
pub(super) struct FrameEdge {
    pub(super) axis: FrameAxis,
    /// The two unit-cube coordinates held fixed while this axis runs 0 → 1, in
    /// `(x, y, z)` order with the axis's own coordinate removed.
    pub(super) fixed: (f64, f64),
}

impl FrameEdge {
    /// The unit-cube point a fraction of the way along this edge.
    fn at(self, t: f64) -> (f64, f64, f64) {
        match self.axis {
            FrameAxis::X => (t, self.fixed.0, self.fixed.1),
            FrameAxis::Y => (self.fixed.0, t, self.fixed.1),
            FrameAxis::Z => (self.fixed.0, self.fixed.1, t),
        }
    }

    /// Choose the edge to tick this axis along, for this view.
    ///
    /// **The ties are the interesting part, and they are not rare — they are the
    /// default.** At `turn = 30` the two outermost struts sit at exactly ±0.683 of
    /// the projected center, mirror images, so which is "furthest" is a coin flip
    /// that floating-point noise was calling: two plots at the same angle picked
    /// opposite sides. Every symmetric view has this property, and the default view
    /// is symmetric. So each comparison needs a real margin before it counts as a
    /// difference, and a stated preference when it does not.
    pub(super) fn choose(scene: &project::Scene, axis: FrameAxis) -> Self {
        // Generous next to projected pixels and tiny next to any real difference:
        // this only has to swallow the noise of two sin/cos paths to one number.
        const EPS: f64 = 1e-6;
        let edge = |fixed| FrameEdge { axis, fixed };
        match axis {
            // A domain axis: on the floor, along whichever of its two floor edges
            // sits **lower on the screen**, which is the one on the outline. Seen
            // from above that is the edge nearer the camera, the V at the bottom of
            // the picture; seen from below the floor faces the camera and its near
            // edges run across the middle of the picture, so the lower edge is the
            // farther one. The rule was "nearer the camera", which held only from
            // above: at `tilt = -25` it numbered the floor across the data. A tie
            // (the floor seen edge-on, `tilt = 0`) falls back to the nearer edge,
            // and a tie in that to the low side.
            FrameAxis::X | FrameAxis::Y => {
                let (a, b) = (edge((1.0, 0.0)), edge((0.0, 0.0)));
                let (ya, yb) = (a.mid(scene).y, b.mid(scene).y);
                if ya > yb + EPS {
                    a
                } else if yb > ya + EPS {
                    b
                } else if a.mid_depth(scene) < b.mid_depth(scene) - EPS {
                    a
                } else {
                    b
                }
            }
            // The measure axis: the strut whose projected midpoint sits furthest
            // from the cube's center across the screen — the cube's left or right
            // silhouette, where the data can never be in front of it. A tie goes to
            // the **left**, the side a reader looks for a vertical scale on.
            FrameAxis::Z => {
                let cx = scene.to_screen(0.5, 0.5, 0.5).x;
                let mut best = edge((0.0, 0.0));
                for (fx, fy) in [(1.0, 0.0), (1.0, 1.0), (0.0, 1.0)] {
                    let cand = edge((fx, fy));
                    let (bx, kx) = (best.mid(scene).x - cx, cand.mid(scene).x - cx);
                    let further = kx.abs() > bx.abs() + EPS;
                    // **Both halves need the margin, and only the first had it.**
                    // `kx < bx` was exact, so once the magnitudes tied *within* EPS
                    // the direction was decided by whatever noise separated them —
                    // the very thing EPS is here to swallow, applied to half the
                    // comparison. The case that exposed it was `space(turn = -360)`,
                    // the same view as `turn = 0`: bit-identical marks and frame
                    // lines, `sin_az` of 2.4e-16 rather than exactly 0, this choice
                    // flipped, and on the other edge two tick labels collided and
                    // were dropped after their nudges. Eighteen labels became
                    // sixteen, with every mark in place and nothing on stderr.
                    //
                    // `Scene::new` also folds `turn` into one lap now, which removes
                    // that *particular* noise at its source. The two fixes are not
                    // one fix twice: normalizing makes equal views equal, and this
                    // margin makes the choice steady whatever the numbers are — a
                    // genuinely different angle can land two struts a hair apart and
                    // would flip here for the same reason. Either alone hides the
                    // symptom, so each is pinned by its own test.
                    let tied = (kx.abs() - bx.abs()).abs() <= EPS;
                    let tied_and_lefter = tied && kx < bx - EPS;
                    if further || tied_and_lefter {
                        best = cand;
                    }
                }
                best
            }
        }
    }

    /// The projected midpoint of this edge.
    pub(super) fn mid(self, scene: &project::Scene) -> Screen {
        let (x, y, z) = self.at(0.5);
        scene.to_screen(x, y, z)
    }

    /// How far this edge's midpoint is from the camera — larger is farther.
    fn mid_depth(self, scene: &project::Scene) -> f64 {
        self.mid(scene).depth
    }
}

/// How far a 3-D tick label sits off its own axis, before any nudge.
pub(super) const FRAME_TICK_OFFSET: f64 = 9.0;
/// How much further out a colliding frame label is pushed per try.
pub(super) const FRAME_LABEL_NUDGE: f64 = 6.0;
/// How many nudges a frame label gets before it is dropped instead. Three tries
/// carry a label ~18px clear of where it started, which separates the three axes
/// meeting at the origin corner; a label crowded *along* its own axis is not
/// helped by any of them and is what this bound exists to give up on.
pub(super) const FRAME_LABEL_NUDGES: usize = 3;

/// One label of the 3-D frame, measured: where its ink sits and how much room it
/// takes, so the frame can ask whether two of them fit before drawing both.
///
/// Billboarded, so the box is axis-aligned on screen whatever the viewing angle:
/// `x` is the center of the text (`text-anchor="middle"`), `y` its baseline, and
/// the ink runs from `y - cap` up to `y`.
#[derive(Clone)]
pub(super) struct FrameLabel {
    text: String,
    x: f64,
    y: f64,
    /// Where the label started, before any nudge — nudges are absolute from here
    /// rather than cumulative, so retrying a placement cannot drift.
    ox: f64,
    oy: f64,
    /// The screen direction a collision pushes this label along.
    dir: (f64, f64),
    half_w: f64,
    cap: f64,
    font: f64,
    fill: &'static str,
    /// Which axis this label belongs to, and whether it is one of its numbers.
    /// Together they decide whether a collision may be nudged out of or has to be
    /// given up on — see the placement loop in `write_space_labels`.
    axis: FrameAxis,
    tick: bool,
}

impl FrameLabel {
    #[allow(clippy::too_many_arguments)]
    fn new(
        text: String, x: f64, y: f64, dir: (f64, f64), offset: f64,
        font: f64, fill: &'static str, axis: FrameAxis, tick: bool,
    ) -> Self {
        let half_w = crate::render::text::estimate_text_width(&text, font) / 2.0;
        let (ox, oy) = (x + dir.0 * offset, y + dir.1 * offset);
        FrameLabel {
            text, x: ox, y: oy, ox, oy, dir, half_w,
            cap: estimate_cap_height(font), font, fill, axis, tick,
        }
    }

    /// Push this label `d` pixels further along its own direction, from where it
    /// started.
    fn nudge(&mut self, d: f64) {
        self.x = self.ox + self.dir.0 * d;
        self.y = self.oy + self.dir.1 * d;
    }

    /// Does this label's ink clear `other`'s by `pad` pixels? Two boxes clear if
    /// they are apart on *either* screen axis, which is the same test a flat tick
    /// label makes against its panel edge before anchoring inward — one dimension
    /// up, and against another label rather than a boundary.
    fn clears(&self, other: &FrameLabel, pad: f64) -> bool {
        let dx = (self.x - other.x).abs() - (self.half_w + other.half_w);
        let dy = (self.y - other.y).abs() - (self.cap + other.cap) / 2.0;
        dx >= pad || dy >= pad
    }

    /// Does this label's ink fall inside a panel? The 3-D frame's labels live
    /// *within* the panel rather than in a reserved margin, so this is the check
    /// that a placement has anywhere to go.
    fn within(&self, l: &Layout) -> bool {
        self.x - self.half_w >= l.x0 && self.x + self.half_w <= l.x1
            && self.y - self.cap >= l.y0 && self.y <= l.y1
    }
}

/// The screen direction perpendicular to one 3-D frame edge, pointing away from
/// the cube center — the side a tick label sits on.
pub(super) fn frame_perp(scene: &project::Scene, center: Screen, edge: FrameEdge) -> (f64, f64) {
    let a = edge.at(0.0);
    let b = edge.at(1.0);
    let o = scene.to_screen(a.0, a.1, a.2);
    let f = scene.to_screen(b.0, b.1, b.2);
    let (adx, ady) = (f.x - o.x, f.y - o.y);
    let alen = (adx * adx + ady * ady).sqrt().max(1e-6);
    let mut perp = (-ady / alen, adx / alen);
    let (mx, my) = ((o.x + f.x) / 2.0 - center.x, (o.y + f.y) / 2.0 - center.y);
    if mx * perp.0 + my * perp.1 < 0.0 {
        perp = (-perp.0, -perp.1);
    }
    perp
}

/// Normalize a value into `[0, 1]` across a range, guarding a zero span — how a
/// data coordinate becomes a unit-cube coordinate before projection.
/// The angular tick positions of a polar plot, as fractions of one turn, paired
/// with their labels.
///
/// The axis is periodic, so its two ends are the *same* spoke: a month axis
/// running 0–12 has a tick at each, and drawn naively it would print "0" and "12"
/// on top of each other at twelve o'clock and stroke the same gridline twice.
/// The second one is dropped. A categorical axis never hits this — its slots are
/// interior to the turn — which is why the de-duplication compares positions on
/// the circle rather than special-casing a type.
pub(super) fn polar_angles<'a>(ticks: &'a TickSpec, xs: (f64, f64)) -> Vec<(f64, &'a str)> {
    let mut out: Vec<(f64, &str)> = Vec::new();
    for (i, &v) in ticks.values.iter().enumerate() {
        let u = unit_norm(v, xs);
        if !u.is_finite() { continue }
        let coincides = out.iter().any(|&(w, _)| {
            let d = (u - w).abs() % 1.0;
            d < 1e-9 || (1.0 - d) < 1e-9
        });
        if coincides { continue }
        out.push((u, ticks.labels.get(i).map(String::as_str).unwrap_or("")));
    }
    out
}
