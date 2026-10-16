//! The flat chrome around the marks: canvas, panel, strips, gridlines, axis lines,
//! ticks and names.

use super::*;

impl SvgRenderer {
    // -----------------------------------------------------------------------
    // SVG structure
    // -----------------------------------------------------------------------

    pub(super) fn write_header(&self, svg: &mut String, name: &str) {
        writeln!(svg,
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}" role="img" aria-label="{name}">"#,
            w = self.width, h = self.height, name = esc(name)
        ).unwrap();
    }

    pub(super) fn write_canvas(&self, svg: &mut String) {
        writeln!(svg,
            r#"  <rect width="{w}" height="{h}" fill="white"/>"#,
            w = self.width, h = self.height
        ).unwrap();
    }

    pub(super) fn write_panel_background(&self, svg: &mut String, l: &Layout, clip: &str,
                              theme: &ThemeSpec) {
        writeln!(svg,
            r##"  <rect x="{x}" y="{y}" width="{w}" height="{h}" fill="{bg}"/>"##,
            x = l.x0, y = l.y0, w = l.w(), h = l.h(),
            bg = theme.background_or(PANEL_BG)
        ).unwrap();
        writeln!(svg,
            r#"  <clipPath id="{clip}"><rect x="{x}" y="{y}" width="{w}" height="{h}"/></clipPath>"#,
            x = l.x0, y = l.y0, w = l.w(), h = l.h()
        ).unwrap();
    }

    // -----------------------------------------------------------------------
    // Facet strips — the names of the panels
    //
    // A grid-faceted plot names its columns once, above the top row, and its
    // rows once, beside the last column — repeating the name on every panel
    // would spend a text row per panel to say something the alignment already
    // says. Row-strip text turns 90° to fit its narrow band, the one place in
    // gog text runs vertically.
    //
    // A *folded* ribbon cannot do that, and the reason is the fold: alignment
    // says nothing about which level a cell holds once the line has turned, so
    // every panel carries its own name. The layout put a rectangle above each
    // panel for it (`Panel::strip`); this only fills them.
    // -----------------------------------------------------------------------

    /// The facet strips. Returns how many names had to be shortened to fit their
    /// strips (`fit_strip_names`), so the caller can say so.
    pub(super) fn write_strips(&self, svg: &mut String, grid: &PanelGrid, theme: &ThemeSpec) -> usize {
        let strip_bg = theme.strip_or(STRIP_BG);
        let strip_fg = strip_ink(theme, &strip_bg);
        use crate::render::layout::{STRIP_H, STRIP_W};
        const STRIP_GAP: f64 = 4.0; // between the strip box and its panel
        let mut shortened = 0usize;

        if !grid.wrap_values.is_empty() {
            let bands: Vec<(&Layout, &str)> = grid.panels.iter()
                .filter_map(|panel| Some((panel.strip.as_ref()?, grid.wrap_values.get(panel.slot)?.as_str())))
                .collect();
            let room = bands.iter().map(|(b, _)| b.w()).fold(f64::INFINITY, f64::min) - STRIP_PAD;
            let names: Vec<&str> = bands.iter().map(|(_, v)| *v).collect();
            let (fs, names, cut) = fit_strip_names(&names, room, self.font_sm);
            shortened += cut;
            let cap = estimate_cap_height(fs);
            writeln!(svg,
                r##"  <g font-family="system-ui,sans-serif" font-size="{fs}" fill="{strip_fg}" text-anchor="middle">"##
            ).unwrap();
            for ((band, _), value) in bands.iter().zip(names.iter()) {
                let bh = band.h() - STRIP_GAP;
                writeln!(svg,
                    r##"    <rect x="{x:.2}" y="{y:.2}" width="{w:.2}" height="{bh:.2}" fill="{strip_bg}"/>"##,
                    x = band.x0, y = band.y0, w = band.w()
                ).unwrap();
                writeln!(svg,
                    r#"    <text x="{cx:.2}" y="{ty:.2}">{v}</text>"#,
                    cx = band.x0 + band.w() / 2.0, ty = band.y0 + bh / 2.0 + cap / 2.0,
                    v = esc(value)
                ).unwrap();
            }
            writeln!(svg, "  </g>").unwrap();
            return shortened;
        }

        if !grid.col_values.is_empty() {
            // Row 0 comes first in row-major order, so panel `c` is column `c`'s top.
            let room = (0..grid.col_values.len())
                .map(|c| grid.panels[c].rect.w()).fold(f64::INFINITY, f64::min) - STRIP_PAD;
            let names: Vec<&str> = grid.col_values.iter().map(String::as_str).collect();
            let (fs, names, cut) = fit_strip_names(&names, room, self.font_sm);
            shortened += cut;
            let cap = estimate_cap_height(fs);
            writeln!(svg,
                r##"  <g font-family="system-ui,sans-serif" font-size="{fs}" fill="{strip_fg}" text-anchor="middle">"##
            ).unwrap();
            for (c, value) in names.iter().enumerate() {
                let p = &grid.panels[c].rect;
                // Below the play strip when there is one: that band names the
                // moment every panel is showing, so it sits above the names of the
                // panels themselves. Without `play` this is `outer.y0` exactly, and
                // the faceted plot is unmoved. Down by the grid's inset as well,
                // so a strip stays on top of a panel a ratio centered in its cell.
                let by = grid.play_strip.as_ref().map_or(grid.outer.y0 + grid.inset.1, |p| p.y1);
                let bh = STRIP_H - STRIP_GAP;
                writeln!(svg,
                    r##"    <rect x="{x:.2}" y="{by:.2}" width="{w:.2}" height="{bh:.2}" fill="{strip_bg}"/>"##,
                    x = p.x0, w = p.w()
                ).unwrap();
                writeln!(svg,
                    r#"    <text x="{cx:.2}" y="{ty:.2}">{v}</text>"#,
                    cx = p.x0 + p.w() / 2.0, ty = by + bh / 2.0 + cap / 2.0, v = esc(value)
                ).unwrap();
            }
            writeln!(svg, "  </g>").unwrap();
        }

        if !grid.row_values.is_empty() {
            let rows: Vec<&Layout> = (0..grid.row_values.len())
                .map(|r| &grid.panels[r * grid.ncols + grid.ncols - 1].rect).collect();
            // A row's name is turned to read downward, so its room is the panel's height.
            let room = rows.iter().map(|p| p.h()).fold(f64::INFINITY, f64::min) - STRIP_PAD;
            let names: Vec<&str> = grid.row_values.iter().map(String::as_str).collect();
            let (fs, names, cut) = fit_strip_names(&names, room, self.font_sm);
            shortened += cut;
            let cap = estimate_cap_height(fs);
            writeln!(svg,
                r##"  <g font-family="system-ui,sans-serif" font-size="{fs}" fill="{strip_fg}" text-anchor="middle">"##
            ).unwrap();
            for (p, value) in rows.iter().zip(names.iter()) {
                let bw = STRIP_W - STRIP_GAP;
                // Left by the grid's inset, so the strip stays beside a panel a
                // ratio centered in its cell.
                let bx = grid.outer.x1 - bw - grid.inset.0;
                writeln!(svg,
                    r##"    <rect x="{bx:.2}" y="{y:.2}" width="{bw:.2}" height="{h:.2}" fill="{strip_bg}"/>"##,
                    y = p.y0, h = p.h()
                ).unwrap();
                // Rotated 90° about its anchor: the glyphs then extend rightward
                // from the anchor, so backing the anchor off by half a cap height
                // centers the line of text in the strip box.
                let ax = bx + bw / 2.0 - cap / 2.0;
                let ay = p.y0 + p.h() / 2.0;
                writeln!(svg,
                    r#"    <text x="{ax:.2}" y="{ay:.2}" transform="rotate(90 {ax:.2} {ay:.2})">{v}</text>"#,
                    v = esc(value)
                ).unwrap();
            }
            writeln!(svg, "  </g>").unwrap();
        }
        shortened
    }

    pub(super) fn write_footer(&self, svg: &mut String) {
        writeln!(svg, "</svg>").unwrap();
    }

    // -----------------------------------------------------------------------
    // Grid
    // -----------------------------------------------------------------------

    pub(super) fn write_grid(
        &self, svg: &mut String, l: &Layout,
        x_ticks: &TickSpec, xs: (f64, f64),
        y_ticks: &TickSpec, ys: (f64, f64),
        // Grid lines running *along* the bars are noise — the bars already fill
        // those gaps. Which set that is depends on orientation, so the caller
        // says which axis the bars stand on.
        skip_vertical: bool,
        skip_horizontal: bool,
        // A map's meridians, in degrees, with its projection and the latitudes at
        // the panel's bottom and top: under Equal Earth each is a curve, drawn
        // through the projection between those two latitudes rather than straight
        // up from where it meets the bottom edge.
        meridians: Option<&(crate::render::geo::Geo, Vec<f64>, (f64, f64))>,
    ) {
        writeln!(svg, r##"  <g stroke="#d2d2da" stroke-width="1">"##).unwrap();
        if !skip_vertical {
            match meridians.filter(|(geo, lons, _)| !geo.straight_meridians() && lons.len() == x_ticks.values.len()) {
                Some((geo, lons, (south, north))) => {
                    const STEPS: usize = 48;
                    for &lon in lons {
                        let at = |k: usize| {
                            let lat = south + (north - south) * k as f64 / STEPS as f64;
                            let (px, py) = geo.project(lon, lat);
                            (l.map_x(px, xs.0, xs.1), l.map_y(py, ys.0, ys.1))
                        };
                        // Kept because it meets the bottom edge inside the panel,
                        // but a meridian bends, and one near a side can leave
                        // through it higher up: the curve ends where it crosses.
                        let mut points = vec![at(0)];
                        for k in 1..=STEPS {
                            let (a, b) = (points[points.len() - 1], at(k));
                            let side = if b.0 < l.x0 { Some(l.x0) } else if b.0 > l.x1 { Some(l.x1) } else { None };
                            match side {
                                Some(x) => {
                                    let t = (x - a.0) / (b.0 - a.0);
                                    points.push((x, a.1 + (b.1 - a.1) * t));
                                    break;
                                }
                                None => points.push(b),
                            }
                        }
                        let points: Vec<String> = points.iter().map(|(x, y)| format!("{x:.2},{y:.2}")).collect();
                        writeln!(svg, r#"    <polyline points="{}" fill="none"/>"#, points.join(" ")).unwrap();
                    }
                }
                None => for &v in &x_ticks.values {
                    let sx = l.map_x(v, xs.0, xs.1);
                    writeln!(svg, r#"    <line x1="{sx:.2}" y1="{y0:.2}" x2="{sx:.2}" y2="{y1:.2}"/>"#,
                        y0 = l.y0, y1 = l.y1).unwrap();
                },
            }
        }
        if !skip_horizontal {
            for &v in &y_ticks.values {
                let sy = l.map_y(v, ys.0, ys.1);
                writeln!(svg, r#"    <line x1="{x0:.2}" y1="{sy:.2}" x2="{x1:.2}" y2="{sy:.2}"/>"#,
                    x0 = l.x0, x1 = l.x1).unwrap();
            }
        }
        writeln!(svg, "  </g>").unwrap();
    }

    // -----------------------------------------------------------------------
    // Axis lines
    // -----------------------------------------------------------------------

    /// The panel's boundary. Two axis lines by default; `theme(frame = "full")`
    /// closes them into a rectangle, which is what `theme_bw` draws and what a
    /// journal figure is usually asked for.
    pub(super) fn write_axes(&self, svg: &mut String, l: &Layout, theme: &ThemeSpec) {
        if !theme.frame_drawn() {
            return;
        }
        writeln!(svg, r##"  <g stroke="{c}" stroke-width="1.5" fill="none">"##,
            c = crate::ir::THEME_FRAME_COLOR).unwrap();
        if theme.frame_is_full() {
            // One rectangle rather than four lines: the corners meet exactly, and
            // a stroked rect is what an SVG reader expects a frame to be.
            writeln!(svg,
                r#"    <rect x="{x0:.2}" y="{y0:.2}" width="{w:.2}" height="{h:.2}"/>"#,
                x0 = l.x0, y0 = l.y0, w = l.w(), h = l.h()).unwrap();
        } else {
            // Bottom (x-axis)
            writeln!(svg, r#"    <line x1="{x0:.2}" y1="{y1:.2}" x2="{x1:.2}" y2="{y1:.2}"/>"#,
                x0 = l.x0, y1 = l.y1, x1 = l.x1).unwrap();
            // Left (y-axis)
            writeln!(svg, r#"    <line x1="{x0:.2}" y1="{y0:.2}" x2="{x0:.2}" y2="{y1:.2}"/>"#,
                x0 = l.x0, y0 = l.y0, y1 = l.y1).unwrap();
        }
        writeln!(svg, "  </g>").unwrap();
    }

    // -----------------------------------------------------------------------
    // Tick marks and labels
    // -----------------------------------------------------------------------

    /// `draw_x` / `draw_y` say whether this panel's edges carry tick marks and
    /// labels at all. With fixed, shared scales one set per margin is enough,
    /// so only the bottom row draws x ticks and only the left column draws y
    /// ticks; interior panels keep the gridlines, which carry the same
    /// positions without the ink.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn write_ticks(
        &self, svg: &mut String, l: &Layout,
        x_ticks: &TickSpec, xs: (f64, f64),
        y_ticks: &TickSpec, ys: (f64, f64),
        draw_x: bool, draw_y: bool,
        // Panel columns sit a small gap apart, and a centered label on an edge
        // tick overhangs its panel — the left panel's "150K" and the right
        // panel's "0K" then meet in the gap and read as one number. When set,
        // an edge tick's label anchors *inward* instead, so no label can cross
        // a panel boundary. Off for the unfaceted plot, whose margins already
        // give centered labels all the room they need.
        snap_edge_labels: bool,
        // `theme(tick_angle = )`. Degrees, turning the x labels counterclockwise so
        // long category names stop overlapping. The y labels are left alone: they
        // are already right-anchored in a margin sized to the longest of them, so
        // they do not collide with each other and turning them would only make
        // them harder to read.
        tick_angle: Option<f64>,
    ) {
        const TICK_LEN: f64 = 5.0;

        writeln!(svg, r##"  <g stroke="#5a5a64" stroke-width="1" fill="none">"##).unwrap();
        if draw_x {
            for &v in &x_ticks.values {
                let sx = l.map_x(v, xs.0, xs.1);
                writeln!(svg, r#"    <line x1="{sx:.2}" y1="{y1:.2}" x2="{sx:.2}" y2="{y2:.2}"/>"#,
                    y1 = l.y1, y2 = l.y1 + TICK_LEN).unwrap();
            }
        }
        if draw_y {
            for &v in &y_ticks.values {
                let sy = l.map_y(v, ys.0, ys.1);
                writeln!(svg, r#"    <line x1="{x1:.2}" y1="{sy:.2}" x2="{x2:.2}" y2="{sy:.2}"/>"#,
                    x1 = l.x0 - TICK_LEN, x2 = l.x0).unwrap();
            }
        }
        writeln!(svg, "  </g>").unwrap();

        // Tick labels
        writeln!(svg,
            r##"  <g font-family="system-ui,sans-serif" font-size="{}" fill="#3c3c46" text-anchor="middle">"##,
            self.font_sm
        ).unwrap();
        if draw_x {
            let label_y = l.y1 + TICK_LEN + TICK_GAP + estimate_cap_height(self.font_sm);
            let turn = tick_angle.filter(|d| *d != 0.0);
            for (v, label) in x_ticks.values.iter().zip(&x_ticks.labels) {
                let sx = l.map_x(*v, xs.0, xs.1);
                if let Some(deg) = turn {
                    // Anchored at its *end*, one gap past the tick, and turned
                    // about that point, so the label hangs down-left from the mark
                    // it belongs to and the end nearest the axis is the end that
                    // names it. Centered across its line on the tick (`dy`, half a
                    // cap height), so in a crowded row each name points at its own
                    // tick rather than leaning into its neighbor's slot. The
                    // upright row's cap height is not added: a turned label has no
                    // cap above its anchor, and adding it hung the row one cap
                    // height below the band `layout` reserves for it. The negative
                    // angle is SVG's: its rotation is clockwise, because y is down.
                    let ty = l.y1 + TICK_LEN + TICK_GAP;
                    writeln!(svg,
                        r#"    <text x="{sx:.2}" y="{ty:.2}" dy="{dy:.2}" text-anchor="end" transform="rotate({a:.2} {sx:.2} {ty:.2})">{}</text>"#,
                        esc(label), a = -deg,
                        dy = estimate_cap_height(self.font_sm) / 2.0).unwrap();
                    continue;
                }
                let half = crate::render::text::estimate_text_width(label, self.font_sm) / 2.0;
                let anchor = if !snap_edge_labels { "" }
                    else if sx - half < l.x0 { r#" text-anchor="start""# }
                    else if sx + half > l.x1 { r#" text-anchor="end""# }
                    else { "" };
                writeln!(svg, r#"    <text x="{sx:.2}" y="{label_y:.2}"{anchor}>{}</text>"#, esc(label)).unwrap();
            }
        }
        writeln!(svg, "  </g>").unwrap();

        writeln!(svg,
            r##"  <g font-family="system-ui,sans-serif" font-size="{}" fill="#3c3c46" text-anchor="end">"##,
            self.font_sm
        ).unwrap();
        if draw_y {
            let label_x = l.x0 - TICK_LEN - TICK_GAP;
            for (v, label) in y_ticks.values.iter().zip(&y_ticks.labels) {
                let sy = l.map_y(*v, ys.0, ys.1) + estimate_cap_height(self.font_sm) / 2.0;
                writeln!(svg, r#"    <text x="{label_x:.2}" y="{sy:.2}">{}</text>"#, esc(label)).unwrap();
            }
        }
        writeln!(svg, "  </g>").unwrap();
    }

    // -----------------------------------------------------------------------
    // Axis labels and title
    // -----------------------------------------------------------------------

    /// `drew_x_ticks` is whether the x axis put a row of labels under the panel.
    /// The name has to clear that row when there is one and must not reserve it
    /// when there is not: `Layout::compute` gives an axis with no tick labels no
    /// band for them, so assuming one anyway drops the name past the space that
    /// was reserved. **Every polar plot in the book had its axis name clipped by
    /// the canvas edge** for exactly that reason — the circle draws its guides
    /// inside the panel and is handed an empty tick list, like the cube and the
    /// packing, and those two suppress their outer names for other reasons and so
    /// never showed it.
    /// `theme(axis_label = )` resolved to the one question the layout asks.
    /// **`beside` is the default**, so an unset theme draws the conventional
    /// arrangement and `"end"` is the thing a reader opts into.
    pub(super) fn axis_label_beside_theme(theme: &crate::ir::ThemeSpec) -> bool {
        !matches!(theme.axis_label.as_deref(), Some("end"))
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn write_labels(
        &self, svg: &mut String, l: &Layout,
        // How far the plot drawn sits inside `l` on each side, `(x, y)`: a panel a
        // `ratio` narrowed, or a circle in a wider panel. The names are placed
        // beside the plot, so they move in by this much.
        inset: (f64, f64),
        // How far a page moved the panel area in from the margin
        // (`PanelGrid::moved_x`); the y name moves in with it.
        moved_x: f64,
        // The legend's band (`theme(legend = )`). Above the plot it stands between
        // the title and the panel, so the title goes up by as much; on the left it
        // stands outside the y name, so the name goes right by as much.
        legend: LegendRoom,
        x_label: &str, y_label: &str, spec: &PlotSpec, drew_x_ticks: bool,
        // The height of the x tick-label row as drawn, turned or upright.
        x_band: f64,
    ) -> Option<f64> {
        // Where the turned y name went, for the page (`Drawn::y_name`).
        let mut y_name = None;
        let plot_cx = (l.x0 + l.x1) / 2.0;
        let label_h = estimate_cap_height(self.font_md);
        // `theme(axis_label = )`. One convention for both axes: `beside` centers
        // each name along its own axis, turning the y name through 90 degrees,
        // and `end` puts both at their axis's far end, horizontal. `beside` is
        // the default, so there is no third, unnamed arrangement.
        let x_at_end = spec.theme.resolved().axis_label.as_deref() == Some("end");
        let y_beside = !x_at_end;
        let drop = title_drop(spec, inset);

        // Title, and the subtitle under it.
        //
        // **Centered on the panel, then held inside the canvas.** The panel is
        // the thing the title names, so it centers there and not over the legend
        // beside it — but a legend pushes that center left, and a title wider than
        // what is left of the canvas then starts at a negative x and is cut off by
        // the edge. On a page that edge is the *cell's*, which is where it showed:
        // at full width a title has room to spare, and in a quarter-page cell
        // "Turned: the same sheet from the west" lost its first letter. Nudging
        // beats clipping, and it only ever moves a title that would otherwise be
        // unreadable. The subtitle is placed by the same rule.
        const EDGE: f64 = 4.0; // breathing room, so a nudged line is not flush
        let centered = |text: &str, font: f64| -> f64 {
            let half = estimate_text_width(text, font) / 2.0;
            let lo = half + EDGE;
            // `max(lo)` so a line wider than the whole canvas still centers rather
            // than inverting the range: it overflows both sides evenly, which reads
            // as too long instead of as misplaced.
            plot_cx.clamp(lo, (self.width - half - EDGE).max(lo))
        };
        // A y name that has moved beside its axis is no longer above the panel,
        // so the lines over the plot stop making room for it.
        let above = l.y0 + drop - y_name_above(y_label, &spec.theme, self.font_md) - legend.top;
        let sub_band = if spec.subtitle.is_some() { subtitle_band(self.font_md) } else { 0.0 };
        if let Some(title) = &spec.title {
            let ty = above - sub_band - estimate_cap_height(self.font_lg) * 0.3 - 8.0;
            writeln!(svg,
                r##"  <text x="{cx:.2}" y="{ty:.2}" font-family="system-ui,sans-serif" font-size="{fs}" font-weight="600" fill="#0f0f19" text-anchor="middle">{title}</text>"##,
                cx = centered(title, self.font_lg), fs = self.font_lg, title = esc(title)
            ).unwrap();
        }
        // **The subtitle is a second, quieter line**: the middle size and the
        // legend's ink, and never the title's weight, so the title stays the one
        // bold line over the plot.
        if let Some(subtitle) = &spec.subtitle {
            let ty = above - estimate_cap_height(self.font_md) * 0.3 - 8.0;
            writeln!(svg,
                r##"  <text x="{cx:.2}" y="{ty:.2}" font-family="system-ui,sans-serif" font-size="{fs}" fill="#3c3c46" text-anchor="middle">{subtitle}</text>"##,
                cx = centered(subtitle, self.font_md), fs = self.font_md, subtitle = esc(subtitle)
            ).unwrap();
        }
        // **The caption sits at the foot of the canvas**, in the smallest size:
        // the source of the numbers, as a printed figure carries one. It starts at
        // the plot's left edge, so it reads as a note on the plot rather than on
        // the legend, and is held inside the canvas like the title.
        if let Some(caption) = &spec.caption {
            let cy = self.height - 8.0;
            let width = estimate_text_width(caption, self.font_sm);
            // Left, chosen by the author over the right edge (2026-10-15).
            let x = (l.x0 + inset.0).min((self.width - width - EDGE).max(EDGE));
            writeln!(svg,
                r##"  <text x="{x:.2}" y="{cy:.2}" font-family="system-ui,sans-serif" font-size="{fs}" fill="#3c3c46" text-anchor="start">{caption}</text>"##,
                fs = self.font_sm, caption = esc(caption)
            ).unwrap();
        }

        // The y-axis name: at the axis's end, horizontal above the panel, or
        // turned and centered along the axis in the left margin.
        if !y_label.is_empty() {
            if y_beside {
                // `rotate(-90)` sends ascenders to the pivot's left, so the pivot
                // sits one cap height in from the canvas edge and the text stays
                // on the page. `layout` reserved exactly that band, and it moves
                // in with the plot when the plot is inset, or when a page moved
                // the panel to run under a plot on another line (`Fit`).
                let lx = self.fit.y_name_at.unwrap_or(label_h + 2.0 + inset.0 + moved_x + legend.left);
                y_name = Some(lx);
                let ly = (l.y0 + l.y1) / 2.0;
                writeln!(svg,
                    r##"  <text transform="rotate(-90 {lx:.2} {ly:.2})" x="{lx:.2}" y="{ly:.2}" font-family="system-ui,sans-serif" font-size="{fs}" fill="#28283a" text-anchor="middle">{y_label}</text>"##,
                    fs = self.font_md, y_label = esc(y_label)
                ).unwrap();
            } else {
                // Over the plot as drawn, on the title's rule (`title_drop`): above
                // the top of `l`, a panel a `ratio` centered lower left its name
                // behind, and a legend laid above the plot moves down with the
                // panel and would have met it there.
                let ty = l.y0 + drop - 6.0;
                writeln!(svg,
                    r##"  <text x="{x:.2}" y="{ty:.2}" font-family="system-ui,sans-serif" font-size="{fs}" fill="#28283a" text-anchor="start">{y_label}</text>"##,
                    x = l.x0 + inset.0, fs = self.font_md, y_label = esc(y_label)
                ).unwrap();
            }
        }

        // X-axis label (centered below ticks)
        if !x_label.is_empty() {
            let tick_row = match drew_x_ticks {
                true => 5.0 + x_band,
                false => 0.0,
            };
            let ty = l.y1 - inset.1 + tick_row + 8.0 + label_h;
            // At its end the x name sits under the axis's far end, right-aligned,
            // which is the same rule the y name follows at the top left.
            let (cx, anchor) = match x_at_end {
                true => (l.x1 - inset.0, "end"),
                false => (plot_cx, "middle"),
            };
            writeln!(svg,
                r##"  <text x="{cx:.2}" y="{ty:.2}" font-family="system-ui,sans-serif" font-size="{fs}" fill="#28283a" text-anchor="{anchor}">{x_label}</text>"##,
                fs = self.font_md, x_label = esc(x_label)
            ).unwrap();
        }
        y_name
    }
}

/// The air a tick label keeps around itself: between a flat axis's tick mark and
/// its number, and between any two labels of a 3-D frame. One constant, because
/// it answers one question — how much clearance a number needs to read as its own
/// word — in the two places that ask it.
pub(super) const TICK_GAP: f64 = 4.0;

/// Say that an axis draws fewer ticks than the `tick_count` its caller wrote,
/// and why. Only a written count is answered: the ticks gog chose itself are a
/// guide, and drawing fewer of them hides no data, while a count the caller
/// wrote is a request the drawing now departs from (Law 5).
/// The note for a legend left out because the plot is too narrow for it: the
/// height check's sentence (`legend::write_legends`), read across.
/// How far the text over the plot moves down with the plot as drawn: the title,
/// a y name at its axis's end, and a legend laid above.
///
/// **Above the plot as drawn**, as the names beside it are: a panel a `ratio`
/// narrowed, or a circle in a taller panel, sits `inset.1` below the top of the
/// area, and a title left at that top stood 78 px over a map set beside a globe.
/// Not where a facet's strips run along the top of the grid, which were placed
/// there before they moved with their panels, and which a title moved down could
/// land on.
pub(super) fn title_drop(spec: &PlotSpec, inset: (f64, f64)) -> f64 {
    let strips_on_top = spec.facet.as_ref().is_some_and(|f| f.col.is_some() || f.wrap.is_some());
    if strips_on_top { 0.0 } else { inset.1 }
}

/// The band a y name written at its axis's end takes above the panel, and that
/// the title and a legend laid above the plot stand over. A name beside its axis
/// is turned into the left margin and takes none.
pub(super) fn y_name_above(y_label: &str, theme: &ThemeSpec, font_md: f64) -> f64 {
    let at_end = theme.resolved().axis_label.as_deref() == Some("end");
    if !y_label.is_empty() && at_end { estimate_cap_height(font_md) + 6.0 } else { 0.0 }
}

pub(super) fn label_for(ctx: &RenderContext<'_>, channel: Channel, override_label: Option<&str>) -> String {
    override_label
        .map(str::to_string)
        .or_else(|| ctx.coord_field(&channel).map(auto_label))
        .unwrap_or_default()
}

/// Infer the y-axis label from a synthesizing transform when no `y()` binding
/// is present.  Synthesizing transforms (`bin`, `count`, `density`) invent their
/// own y column — the user should not need to name it.
///
/// **`proportion` is asked before the others and outranks them**, because it is a
/// normalizer: whatever made the number, the axis is reading it as a share once
/// `proportion` has divided (spec §5). Read in list order instead, `bar * bin *
/// proportion` labeled its axis `Count` over a column of fractions — the same
/// mistake `transform::cell_measure` had to stop making one dimension up, and one
/// half of the plot the relative-frequency histogram replaced.
/// Does any layer fill its piles to 1 (`stack(share = true)`)?
///
/// Asked by `axis_label`, because a filled pile changes what the measure axis is
/// *in* rather than what wrote it — see the note there.
pub(super) fn share_stacked(spec: &PlotSpec) -> bool {
    spec.layers.iter().any(|l| {
        l.transforms.contains(&Transform::Stack)
            && l.stack.as_ref().is_some_and(|s| s.share.unwrap_or(false))
    })
}

pub(super) fn synth_y_label(spec: &PlotSpec) -> Option<String> {
    // A partition's `proportion` divides the partition's own measure axis, which
    // is `x` (`transform::partition_shares`), so it has nothing to say about this
    // one: the axis a partition synthesizes here is its ring or its column share,
    // and it keeps that name below.
    if spec.layers.iter().any(|l| l.transforms.contains(&Transform::Proportion)
        && !l.transforms.contains(&Transform::Partition))
    {
        return Some("Proportion".into());
    }
    for layer in &spec.layers {
        for t in &layer.transforms {
            match t {
                Transform::Count      => return Some("Count".into()),
                Transform::Bin        => return Some("Count".into()),
                Transform::Density    => return Some("Density".into()),
                // A nested partition's measure axis is the **ring**, which is the
                // one synthesized axis here that is an index rather than a
                // quantity. It still belongs in this list: the user did not name
                // the column, so the axis has to name itself, which is the whole
                // rule. Crossed there is no ring — the second axis is apportioning
                // the same measure the first one is, one level down, so it says so
                // and the caller's own `y_label()` is what renames it.
                Transform::Partition  => return Some(match layer.partition.as_ref()
                    .map(|p| p.cross).unwrap_or(false) {
                        true  => "Share of column".into(),
                        false => "Depth".into(),
                    }),
                // The merge distance a cluster tree synthesizes — a quantity
                // in the value column's own units, named for what it is.
                Transform::Cluster    => return Some("Distance".into()),
                _ => {}
            }
        }
    }
    None
}
