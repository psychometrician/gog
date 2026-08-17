//! The `text` mark — a string glyph (the `label` channel) at each (x, y), and the
//! `repel` placement that moves those glyphs off one another.
use std::collections::HashMap;
use std::fmt::Write;
use crate::data::DataFrame;
use crate::ir::{Channel, Layer, Transform};
use crate::legality::{Diagnostic, DiagnosticKind};
use crate::render::nest::Nest;
use crate::render::polar::Polar;
use crate::render::svg::{fmt_label_num, SvgRenderer, TEXT_FILL};
use crate::render::text::{esc, estimate_cap_height, estimate_text_width};
use crate::render::{hash01, Layout};

impl SvgRenderer {
    // -----------------------------------------------------------------------
    // Mark: text
    // -----------------------------------------------------------------------

    /// A glyph mark whose glyph is a *string*: at each (x, y) it draws the value
    /// of the `label` column as text. `point`'s sibling — same per-row placement,
    /// but the glyph is data (the `label` channel) rather than one of the five
    /// shapes. `label` is the mark's minimum syllable, so `check` guarantees the
    /// encoding is present before this runs.
    ///
    /// Color resolves as a point's categorical color does: a set color wins,
    /// else the group's palette hue. A string label is drawn as-is; a numeric one
    /// is formatted. The baseline drops half a cap-height so the glyph sits
    /// centered on its point rather than resting its feet there.
    pub(crate) fn write_text(
        &self, svg: &mut String, layer: &Layer, df: &DataFrame,
        l: &Layout, xs: (f64, f64), ys: (f64, f64),
        x_field: &str, y_field: &str,
        cat_x: Option<&[String]>, cat_y: Option<&[String]>,
        color_map: &HashMap<String, String>,
        clip: &str,
        // Polar: the label sits at an angle and a radius. `style(nudge = )` still
        // moves it in page directions ("up" is up the page, not outward from the
        // center) — a nudge is a constant offset on the finished glyph, and the
        // page is where the reader sees the collision it exists to fix.
        polar: Option<&Polar>,
        // Nest: when `Some`, the label names a **region** rather than sitting at a
        // coordinate, and the two positions stop being positions — `y` is the
        // measure that sizes the region and `x`, if bound at all, is the outer
        // partition. Consulted once above the row loop, as `bar`'s is and for the
        // same reason: a packing is a property of all the rows together.
        nest: Option<&Nest>,
        // Globe: the name sits at its place on the sphere's facing hemisphere,
        // `point`'s own reading one mark over. A row the view cannot see is
        // skipped here and counted by the caller; the far half of the earth is
        // what a globe is, not a drop.
        globe: Option<&crate::render::globe::Globe>,
        // Where a label that could not be drawn is reported. A packing of many
        // shares has more regions than legible ones, so leaving the unfitted ones
        // out in silence would let a reader take the labeled cells for all of them
        // (§12). Empty in every other space, where a label always draws.
        remarks: &mut Vec<Diagnostic>,
    ) {
        // The strings: the label column — a string drawn as-is, a number
        // formatted (the "value on each point" case). Its presence is the mark's
        // minimum syllable, guaranteed by legality.
        let Some(label_field) = layer.encodings.get(&Channel::Label).map(|c| c.field.as_str()) else { return };
        let owned_labels: Vec<String>;
        let labels: &[String] = if let Some(s) = df.str_col(label_field) {
            s
        } else if let Some(nums) = df.float_col(label_field) {
            owned_labels = nums.iter().map(|v| fmt_label_num(*v)).collect();
            &owned_labels
        } else { return };

        // **The packed reading comes first, because it does not consult the axes
        // at all.** Everything below this branch is written in terms of a place on
        // two scales, and a packing has neither (spec §15) — so rather than
        // threading a third meaning through the position resolution, the region
        // case answers itself and returns.
        if let Some(nst) = nest {
            self.write_text_nest(svg, layer, df, x_field, y_field, cat_x, labels,
                                 color_map, clip, nst, remarks);
            return;
        }

        // Position: the one resolution every mark shares — `text` is `point`'s
        // sibling and places its glyph the same way, so no per-mark exception.
        let Some(x_vals) = super::positions(df, x_field, cat_x) else { return };
        let Some(y_vals) = super::positions(df, y_field, cat_y) else { return };

        // Color: a set color wins; else the group's palette hue (category →
        // color via the shared map). A numeric ramp on text is not drawn yet —
        // legality renders `color` as discrete here, so there is no ramp path.
        let st = &layer.style;
        let set_color = st.color.as_deref().map(esc);
        let group_vals = layer.encodings.get(&Channel::Color).and_then(|c| df.str_col(&c.field));

        let fs = st.size.unwrap_or(self.font_md);
        let opacity = st.opacity.unwrap_or(1.0);
        let dy = estimate_cap_height(fs) / 2.0; // center the glyph on its point

        // A nudge moves the label off its point, so a superposed dot is not
        // covered. The distance is derived from the font size — enough to clear a
        // default dot — never a parameter. A vertical nudge keeps the centered
        // anchor and shifts the baseline; a horizontal one anchors the glyph's
        // near edge to the point and shifts sideways. `check_nudge` has already
        // rejected any direction but these four, so `_` is the no-nudge default.
        let (anchor, ndx, ndy) = match st.nudge.as_deref() {
            Some("up")    => ("middle", 0.0, -fs),
            Some("down")  => ("middle", 0.0,  fs),
            Some("left")  => ("end",   -fs * 0.6, 0.0),
            Some("right") => ("start",  fs * 0.6, 0.0),
            _             => ("middle", 0.0, 0.0),
        };

        let n = x_vals.len().min(y_vals.len()).min(labels.len());

        // Every row that will draw, and the page position of the point it names.
        // Collected before anything is written because `repel` places the labels
        // *against one another*, so it has to see all of them before it can place
        // the first (spec §5). Without it this list is just the loop's filter.
        let mut rows: Vec<(usize, f64, f64)> = Vec::with_capacity(n);
        for i in 0..n {
            if !(x_vals[i].is_finite() && y_vals[i].is_finite()) {
                continue;
            }
            let (bx, by) = match globe {
                Some(g) => match g.place(x_vals[i], y_vals[i]) {
                    Some(s) => (s.x, s.y),
                    // The far hemisphere — counted and reported by the caller.
                    None => continue,
                },
                None => super::place(l, polar, x_vals[i], y_vals[i], xs, ys),
            };
            rows.push((i, bx, by));
        }

        // The fourth collision modifier. A dot's overlap is its position and a
        // label's is its *ink*, so this is the one offset that cannot be computed
        // before the glyphs have a size — and this is where they get one.
        let repelled = layer.transforms.contains(&Transform::Repel).then(|| {
            place_repelled(&rows, labels, fs, st.nudge.as_deref(), l, self.point_radius + 1.0, remarks)
        });

        let fill_for = |i: usize| -> String {
            if let Some(sc) = &set_color {
                sc.clone()
            } else if let Some(gv) = group_vals {
                gv.get(i).and_then(|g| color_map.get(g).cloned()).unwrap_or_else(|| TEXT_FILL.to_string())
            } else {
                TEXT_FILL.to_string()
            }
        };

        // The leaders first, as one group, so that no connector is drawn over a
        // word. A label crosses nothing of its own — its leader stops at its own
        // border — but it can be crossed by a *neighbor's*, and the reader is
        // reading the words.
        if let Some(boxes) = &repelled {
            let mut leaders = String::new();
            for (k, &(i, _, _)) in rows.iter().enumerate() {
                if let Some((fx, fy, tx, ty)) = boxes[k].leader(fs) {
                    writeln!(&mut leaders,
                        r#"    <line x1="{fx:.2}" y1="{fy:.2}" x2="{tx:.2}" y2="{ty:.2}" stroke="{}" stroke-width="0.7" stroke-opacity="{:.3}"/>"#,
                        fill_for(i), opacity * 0.55
                    ).unwrap();
                }
            }
            if !leaders.is_empty() {
                writeln!(svg, r##"  <g clip-path="url(#{clip})">"##).unwrap();
                svg.push_str(&leaders);
                writeln!(svg, "  </g>").unwrap();
            }
        }

        // A repelled label is placed by its **center**, since a box is what was
        // moved, so the group's anchor is `middle` whatever the nudge asked for —
        // the nudge is already spent, as the offset the placement started from.
        let anchor = if repelled.is_some() { "middle" } else { anchor };
        writeln!(svg,
            r##"  <g clip-path="url(#{clip})" font-family="system-ui,sans-serif" font-size="{fs}" text-anchor="{anchor}">"##
        ).unwrap();
        for (k, &(i, bx, by)) in rows.iter().enumerate() {
            let fill = fill_for(i);
            let (px, py) = match &repelled {
                Some(boxes) => (boxes[k].cx, boxes[k].cy + dy),
                None => (bx + ndx, by + dy + ndy),
            };
            writeln!(svg,
                r#"    <text x="{px:.2}" y="{py:.2}" fill="{fill}" fill-opacity="{opacity:.3}">{}</text>"#,
                esc(&labels[i])
            ).unwrap();
        }
        writeln!(svg, "  </g>").unwrap();
    }

    // -----------------------------------------------------------------------
    // Mark: text, in `nest` — the name inside its own region
    // -----------------------------------------------------------------------

    /// A label at the centroid of the region its row was packed into — what makes
    /// a treemap readable once the split is too wide for a legend to decode (spec
    /// §15).
    ///
    /// **The cells come from the space, not from here.** `Nest::regions` is the
    /// one answer to *which rectangle does this row get*, and `bar` asks it the
    /// same question with the same two columns, so a label cannot land in a cell
    /// its bar did not draw. That is the whole reason the packing moved down into
    /// `nest.rs`: two marks computing it separately is two marks that can disagree,
    /// and a name sitting a pixel outside its own rectangle is worse than no name,
    /// because nothing on the page says which region it belongs to.
    ///
    /// **A label that does not fit is not drawn, and is counted.** A packing puts
    /// many shares on one panel and most of them are smaller than a word; printing
    /// the ones that fit and saying nothing about the rest would let a reader take
    /// the labeled cells for all of them, which is §12's silent drop wearing a
    /// typographic excuse. So the layer reports its tally, once, as an Assumption:
    /// the plot drew, and the reader is told what is missing from it. This is the
    /// rule `repel` already carries for the same failure in the plane (spec §5).
    #[allow(clippy::too_many_arguments)]
    fn write_text_nest(
        &self, svg: &mut String, layer: &Layer, df: &DataFrame,
        x_field: &str, y_field: &str, cat_x: Option<&[String]>,
        labels: &[String],
        color_map: &HashMap<String, String>,
        clip: &str,
        nest: &Nest,
        remarks: &mut Vec<Diagnostic>,
    ) {
        // The measure is read raw, exactly as the bar's is: this space has no
        // position to map through, and the share a region reports is arithmetic on
        // the values themselves (which is why `check_nest` refuses a log scale).
        let Some(weights) = df.float_col(y_field) else { return };
        // The outer partition, when one is bound. A one-level packing has a single
        // slot and every row stands in it — the same degenerate reading `bar` takes,
        // and what Law 7's third relaxation exists to allow (`legality.rs`).
        let slots = if x_field.is_empty() {
            std::borrow::Cow::Owned(vec![0.0; weights.len()])
        } else {
            let Some(p) = super::positions(df, x_field, cat_x) else { return };
            p
        };

        let n = slots.len().min(weights.len()).min(labels.len());
        if n == 0 { return; }
        let (cells, _) = nest.regions(&slots[..n], &weights[..n]);

        let st = &layer.style;
        let set_color = st.color.as_deref().map(esc);
        let group_vals = layer.encodings.get(&Channel::Color).and_then(|c| df.str_col(&c.field));
        let fs = st.size.unwrap_or(self.font_md);
        let opacity = st.opacity.unwrap_or(1.0);
        let dy = estimate_cap_height(fs) / 2.0;

        // `style(nudge = )` is not read here, and the anchor is always `middle`. A
        // nudge steps a label off the point it would otherwise cover, and a region
        // has no dot underneath to clear — so honoring one would shove the name at
        // its own border for no reason. It is **refused** in `check_nest` rather
        // than ignored here, which is what keeps this line from being the
        // accept-and-drop §12 forbids: nothing reaches this function carrying one.
        writeln!(svg,
            r##"  <g clip-path="url(#{clip})" font-family="system-ui,sans-serif" font-size="{fs}" text-anchor="middle">"##
        ).unwrap();
        let mut unfitted = 0usize;
        for i in 0..n {
            let c = cells[i];
            let label = &labels[i];
            // Does the name fit the rectangle that carries it? Width against the
            // ink the string will actually take, height against the cap height —
            // the two numbers the glyph is placed by, asked of the region rather
            // than of the panel. The one-pixel margin keeps a label off its own
            // border, which is where the reader looks to find the region's edge.
            let w = estimate_text_width(label, fs);
            if !(c.w >= w + 2.0 && c.h >= estimate_cap_height(fs) + 2.0) {
                // A region with no area at all is not an unfitted label — it is a
                // share too small to have a region, which the bar does not draw
                // either. Counting it would report a name the plot never had room
                // to consider.
                if c.w >= 0.5 && c.h >= 0.5 { unfitted += 1; }
                continue;
            }
            let fill: String = if let Some(sc) = &set_color {
                sc.clone()
            } else if let Some(gv) = group_vals {
                gv.get(i).and_then(|g| color_map.get(g).cloned()).unwrap_or_else(|| TEXT_FILL.to_string())
            } else {
                TEXT_FILL.to_string()
            };
            writeln!(svg,
                r#"    <text x="{:.2}" y="{:.2}" fill="{fill}" fill-opacity="{opacity:.3}">{}</text>"#,
                c.x + c.w / 2.0, c.y + c.h / 2.0 + dy, esc(label)
            ).unwrap();
        }
        writeln!(svg, "  </g>").unwrap();

        if unfitted > 0 {
            remarks.push(Diagnostic {
                kind: DiagnosticKind::Assumption,
                message: format!(
                    "gog: {unfitted} of {n} labels are wider than the region they name, and \
                     were left out — the packing drew every share, the names are what is \
                     missing. Fewer categories, a larger plot (`theme(width =, height =)`) or \
                     a smaller `style(size = )` fits more of them in."
                ),
            });
        }
    }
}

// ---------------------------------------------------------------------------
// `repel` — the placement that moves labels off one another
//
// The fourth collision modifier (spec §5), and the only one that runs here rather
// than in `transform.rs`. `dodge`, `stack` and `jitter` all resolve marks that
// share a *position*, which is a fact about the data; a label is as wide as the
// word it draws, so two labels collide at positions their points never shared, and
// the collision does not exist until the glyphs have a size. This file is where
// they get one, so this is where the offset is computed.
// ---------------------------------------------------------------------------

/// The air left around a label's ink, as a fraction of the font size: what keeps
/// two separated labels from ending up shoulder to shoulder with no gap at all.
/// Derived from the size, the way the nudge's distance is — a knob for it would be
/// a second, quieter way of saying `style(size = )`.
const REPEL_PAD_X: f64 = 0.30;
const REPEL_PAD_Y: f64 = 0.40;

/// How much of an overlap one pass undoes. Less than all of it, because a label in
/// a crowd is pushed by several neighbors at once and each one is answering a
/// picture that is about to change; taking half the correction each time lets the
/// whole panel settle instead of two labels trading places forever.
const REPEL_RELAX: f64 = 0.5;

/// How hard a label is pulled back toward its own point while the crowd is being
/// sorted out. It fades to nothing over the first part of the run, so the last
/// passes are pure separation and the result is settled rather than mid-argument.
const REPEL_PULL: f64 = 0.08;

/// How far a label that is still colliding is shaken each early pass, as a fraction
/// of the font size, fading out with the pull.
///
/// Separation alone gets stuck: labels part along the axis they overlap least on,
/// which for a word is up and down, so a knot of them stacks into a column and the
/// two in the middle have nowhere left to go even though there is space to the
/// side. The shake is what lets one of them get around another. It is a *hashed*
/// direction, seeded from the row and the pass — the plot is shaken the same way
/// every time it is drawn, which is the whole difference between annealing and a
/// picture that changes when nobody changed anything.
const REPEL_KICK: f64 = 0.25;

/// One label's ink, as the rectangle the placement moves.
struct LabelBox {
    /// Half the padded ink — what two labels are kept out of each other by.
    hw: f64,
    hh: f64,
    /// Where the label is now: the center of that rectangle.
    cx: f64,
    cy: f64,
    /// The point the label names: the foot of its leader line, and an obstacle to
    /// every label, its own included. What a label rests *beside* rather than on.
    ax: f64,
    ay: f64,
    /// Where it would sit if nothing collided: its point, plus the nudge. The
    /// placement pulls each label back toward this as it separates them, so a label
    /// ends up as near its point as the crowd allows rather than wherever the first
    /// push happened to send it.
    ix: f64,
    iy: f64,
}

impl LabelBox {
    /// Does this box overlap another? Two rectangles, so both axes have to.
    ///
    /// `shrink` takes the air back off first. The placement works with the padded
    /// box, because two words a hair apart are two words nobody can read; the
    /// *report* at the end is about ink, because a reader told that labels overlap
    /// will look for the ones that touch and has been misled if none does.
    fn hits(&self, o: &LabelBox, shrink: (f64, f64)) -> bool {
        (o.cx - self.cx).abs() < self.hw + o.hw - shrink.0
            && (o.cy - self.cy).abs() < self.hh + o.hh - shrink.1
    }

    /// The connector back to the point, when the label has left it: from the point
    /// to the nearest place on the label's own border.
    ///
    /// A label that only took its resting step off the dot needs none — the eye
    /// pairs a word with the dot beside it, and a line from every label is a panel
    /// full of lines. So it draws once the label is more than a line of text from
    /// its point, which is the distance at which a word stops looking attached to
    /// anything.
    fn leader(&self, fs: f64) -> Option<(f64, f64, f64, f64)> {
        let ex = self.ax.clamp(self.cx - self.hw, self.cx + self.hw);
        let ey = self.ay.clamp(self.cy - self.hh, self.cy + self.hh);
        let gap = ((ex - self.ax).powi(2) + (ey - self.ay).powi(2)).sqrt();
        (gap > fs).then_some((self.ax, self.ay, ex, ey))
    }
}

/// Which way two things part when nothing in their positions says.
///
/// Almost always the sign of the distance between them. Two labels at the
/// *identical* pixel have no direction at all, and that is the one place the
/// placement could have reached for a random number: it hashes the two rows and
/// their coordinates instead, `jitter`'s rule, so a tie parts the same way every
/// time the same table is drawn. One specification is one picture (spec §5).
fn parting_sign(d: f64, i: usize, j: usize, a: f64, b: f64) -> f64 {
    if d.abs() > 1e-9 {
        return d.signum();
    }
    let seed = (i as u64)
        .wrapping_mul(0x9E3779B97F4A7C15)
        ^ (j as u64).rotate_left(23)
        ^ a.to_bits().rotate_left(17)
        ^ b.to_bits().rotate_left(43);
    if hash01(seed) < 0.5 { -1.0 } else { 1.0 }
}

/// Move the labels that overlap until they do not — `text * repel`, whole.
///
/// **What it moves them off.** Two things: the other labels' ink, and the points.
/// The second is what makes the ordinary `point + text` sentence work with no
/// plumbing between the two layers — the labels share `x` and `y` with the dots, so
/// a label that clears every anchor has cleared every dot the point layer drew.
///
/// **How.** Each pass nudges every overlapping pair apart along the axis they
/// overlap *least* on, which is the smallest move that resolves them — for a word,
/// wider than it is tall, that is usually upward or downward, so a crowd resolves
/// into the stacked column a person would have written. Half the correction is
/// taken at a time ([`REPEL_RELAX`]) because each label is being pushed by several
/// neighbors at once. A pull toward home ([`REPEL_PULL`]) runs alongside and fades
/// out, so the run ends in pure separation and settles.
///
/// **Nothing is dropped, and nothing is silent** (§12). Every label draws: the
/// panel is clamped against, so none is pushed out of the picture and clipped away.
/// Past some density there is no overlap-free arrangement at all, and then the
/// layer says how many labels are still touching rather than leaving the reader to
/// notice. That the placement gets a fixed budget rather than unlimited passes
/// changes nothing about this — whatever the budget did not finish is counted by
/// the same line.
fn place_repelled(
    rows: &[(usize, f64, f64)],
    labels: &[String],
    fs: f64,
    nudge: Option<&str>,
    l: &Layout,
    dot: f64,
    remarks: &mut Vec<Diagnostic>,
) -> Vec<LabelBox> {
    let mut bs: Vec<LabelBox> = rows
        .iter()
        .map(|&(i, ax, ay)| {
            let w = estimate_text_width(&labels[i], fs);
            let hh = (estimate_cap_height(fs) + fs * REPEL_PAD_Y) / 2.0;
            // The nudge, restated as an offset of the label's *center*. The glyph
            // itself is anchored by its near edge when nudged sideways, so the
            // conversion carries the half-width — the same label in the same place,
            // described the way a box has to be described.
            //
            // **With no nudge the label still steps off its dot**, and this is
            // where a repelled label parts company with a plain one. A plain label
            // is centered on its point, dot and all, because it was put there
            // deliberately; a repelled label is being placed *for* the reader, and
            // a word with a dot in the middle of it is the thing the reader was
            // trying to see. So the resting place is just clear of the dot — the
            // step `style(nudge = "up")` takes by hand, taken by default and
            // measured off the dot rather than off the font.
            let (nx, ny) = match nudge {
                Some("up") => (0.0, -fs),
                Some("down") => (0.0, fs),
                Some("left") => (-(fs * 0.6 + w / 2.0), 0.0),
                Some("right") => (fs * 0.6 + w / 2.0, 0.0),
                _ => (0.0, -(dot + hh)),
            };
            LabelBox {
                hw: (w + fs * REPEL_PAD_X) / 2.0,
                hh,
                cx: ax + nx, cy: ay + ny,
                ax, ay,
                ix: ax + nx, iy: ay + ny,
            }
        })
        .collect();

    let n = bs.len();
    // The placement is quadratic in the labels and iterative, so it is given a
    // budget rather than a fixed number of passes: a panel of twenty names is
    // solved thoroughly, and one of two thousand is not allowed to cost a reader a
    // second of waiting. The budget can only cost quality, never honesty — what it
    // leaves touching is reported below exactly as an impossible packing is.
    let iters = if n < 2 { 0 } else { (40_000_000.0 / (n * n) as f64) as usize };
    let iters = iters.clamp(if n < 2 { 0 } else { 24 }, 240);
    let anneal = (iters * 3 / 5).max(1);

    // Which labels ran into something on the pass before. Only those are shaken:
    // a label standing on its own is where it should be, and moving it would be
    // annealing the part of the picture that was already right.
    let mut bumped = vec![false; n];

    for it in 0..iters {
        if it < anneal {
            let cool = 1.0 - it as f64 / anneal as f64;
            let (pull, kick) = (REPEL_PULL * cool, fs * REPEL_KICK * cool);
            for (i, b) in bs.iter_mut().enumerate() {
                b.cx += (b.ix - b.cx) * pull;
                b.cy += (b.iy - b.cy) * pull;
                if bumped[i] {
                    let seed = (i as u64)
                        .wrapping_mul(0x9E3779B97F4A7C15)
                        ^ (it as u64).rotate_left(31)
                        ^ b.ax.to_bits().rotate_left(17)
                        ^ b.ay.to_bits().rotate_left(43);
                    let angle = hash01(seed) * std::f64::consts::TAU;
                    b.cx += angle.cos() * kick;
                    b.cy += angle.sin() * kick;
                }
            }
        }
        bumped.fill(false);
        let mut moved = false;

        // Label against label.
        for i in 0..n {
            for j in (i + 1)..n {
                let (dx, dy) = (bs[j].cx - bs[i].cx, bs[j].cy - bs[i].cy);
                let ox = (bs[i].hw + bs[j].hw) - dx.abs();
                let oy = (bs[i].hh + bs[j].hh) - dy.abs();
                if ox <= 0.0 || oy <= 0.0 {
                    continue;
                }
                moved = true;
                bumped[i] = true;
                bumped[j] = true;
                if ox < oy {
                    let s = parting_sign(dx, i, j, bs[i].ax, bs[j].ax) * ox * 0.5 * REPEL_RELAX;
                    bs[i].cx -= s;
                    bs[j].cx += s;
                } else {
                    let s = parting_sign(dy, i, j, bs[i].ay, bs[j].ay) * oy * 0.5 * REPEL_RELAX;
                    bs[i].cy -= s;
                    bs[j].cy += s;
                }
            }
        }

        // Label against the points — every one of them, its own included. At rest a
        // label is exactly clear of its own dot, so this does nothing there; what it
        // does is stop a crowd from shoving a label back down onto the point it
        // names. The point does not move, so the label takes the whole correction
        // rather than half of it.
        //
        // **Only while there is still room to look for.** Once the pull has faded
        // the placement stops asking labels to clear the dots, because the two
        // constraints can deadlock: a word with another word above it and its own
        // dot below is pushed both ways at once and stays where it is, overlapping.
        // When it comes to a choice the words win, since a word over a dot is
        // readable and two words over each other are not.
        for i in 0..(if it < anneal { n } else { 0 }) {
            for k in 0..n {
                let (ax, ay) = (bs[k].ax, bs[k].ay);
                let (dx, dy) = (bs[i].cx - ax, bs[i].cy - ay);
                let ox = (bs[i].hw + dot) - dx.abs();
                let oy = (bs[i].hh + dot) - dy.abs();
                if ox <= 0.0 || oy <= 0.0 {
                    continue;
                }
                moved = true;
                if ox < oy {
                    bs[i].cx += parting_sign(dx, i, k, bs[i].ax, ax) * ox * REPEL_RELAX;
                } else {
                    bs[i].cy += parting_sign(dy, i, k, bs[i].ay, ay) * oy * REPEL_RELAX;
                }
            }
        }

        clamp_into_panel(&mut bs, l);
        // Settled: the pull is spent and the last pass moved nothing.
        if !moved && it >= anneal {
            break;
        }
    }
    clamp_into_panel(&mut bs, l);

    // What is still touching — the *ink*, with the air taken back off, so the count
    // names labels a reader can see running into each other. Reported once for the
    // layer, as an Assumption: the plot drew, and every label is on it (§12).
    let shrink = (fs * REPEL_PAD_X, fs * REPEL_PAD_Y);
    let stuck = (0..n).filter(|&i| (0..n).any(|j| j != i && bs[i].hits(&bs[j], shrink))).count();
    if stuck > 0 {
        remarks.push(Diagnostic {
            kind: DiagnosticKind::Assumption,
            message: format!(
                "gog: {stuck} of {n} labels still overlap another one after `repel` moved them — \
                 there is no arrangement of this many words that fits this panel. Every label was \
                 drawn, so none is missing; they are crowded. A larger plot (`theme(width =, \
                 height =)`), a smaller `style(size = )`, or fewer rows in the layer separates them."
            ),
        });
    }
    bs
}

/// Hold every label inside the panel. The glyphs are clipped to it, so a label
/// pushed past the edge would be cut in half or vanish — which is the silent drop
/// (§12) arriving by way of the placement rather than the data. A label too wide
/// for the panel it is in has nowhere to be but the middle.
fn clamp_into_panel(bs: &mut [LabelBox], l: &Layout) {
    for b in bs.iter_mut() {
        let (lo, hi) = (l.x0 + b.hw, l.x1 - b.hw);
        b.cx = if lo <= hi { b.cx.clamp(lo, hi) } else { (l.x0 + l.x1) / 2.0 };
        let (lo, hi) = (l.y0 + b.hh, l.y1 - b.hh);
        b.cy = if lo <= hi { b.cy.clamp(lo, hi) } else { (l.y0 + l.y1) / 2.0 };
    }
}
