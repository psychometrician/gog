//! The `polar` coordinate space — the plane bent into a circle.
//!
//! Wilkinson's chapter 9 is *Coordinates*, and the polar transform is its most
//! used member: "the polar transformation is useful whenever data lend themselves
//! to circular arrangements" (§9.1.6) — compass bearings, hours of the day, months
//! of the year, proportions of a whole. It is not a chart type. A rose is `bar`
//! seen in this space, a radar is `line` seen in it, and the marks themselves
//! learn nothing new: they ask this module where a coordinate lands, the way the
//! flat marks ask `Layout::map_x`/`map_y`.
//!
//! **Which axis bends.** `x` becomes the angle and `y` the radius, because
//! Wilkinson's own argument order says so: "the first dimension is taken to be the
//! domain, which is assigned to θ. The second dimension is taken to be the range,
//! which is assigned to ρ" (§9.1.6). So the axis a bar chart stands its categories
//! on is the axis a rose wraps around the circle, and nothing about the sentence
//! changes when the space does.
//!
//! **The angular axis is periodic.** One turn spans exactly the fitted data range
//! — Wilkinson aligns 0 radians with the scale minimum and 2π with the maximum
//! (§9.1.6) — so the circle closes with no seam and no dead wedge. Categories
//! divide the turn into equal slots the way they divide a flat axis into equal
//! slots; the arithmetic is identical, because the *normalized* coordinate is what
//! this module reads.
//!
//! Angles run **clockwise from twelve o'clock**, and `polar(start = )` rotates
//! where the circle begins (`ir::PolarView`).

use crate::ir::PolarView;
use crate::render::Layout;

use std::f64::consts::TAU;

/// The polar frame for one panel: where the center is, how long the longest
/// radius is, and where the circle starts. Built once per panel — the guides and
/// every mark share it, so a tick ring and a bar's tip cannot disagree about
/// where a value sits. (The same reason `project::Scene` is built once.)
pub(crate) struct Polar {
    pub(crate) cx: f64,
    pub(crate) cy: f64,
    pub(crate) r_max: f64,
    /// Radians clockwise from twelve o'clock where the angular axis begins.
    start: f64,
    /// Whether the *measure* is on the angle rather than the radius: Wilkinson's
    /// one-argument `polar.theta`, which "assigns its only argument to θ, and
    /// assigns the radius to a constant that determines the size of the pie"
    /// (§9.1.6.1). This is the pie, and it is read off the bindings rather than
    /// asked for: a plot with one bound position has nothing to choose between,
    /// so the position it has is the angle. Two positions is the rose.
    pub(crate) measure_on_angle: bool,
    /// Whether the angular domain **wraps with no repeated endpoint** — that is,
    /// whether the angular axis is categorical. See [`Polar::wraps`].
    wraps: bool,
}

/// Extra room a rim label needs *sideways* beyond its height — a name at three
/// o'clock runs outward from its tick, where one at the top only stands above it.
/// Half a typical label's width, so the widest names still clear the panel.
const RIM_LABEL_REACH: f64 = 24.0;

impl Polar {
    /// Inscribe the circle in a panel rectangle, leaving `label_room` pixels
    /// outside it for the ring of angular names.
    ///
    /// The radius is bounded by the *shorter* side, so the plot is a circle in a
    /// wide panel rather than an ellipse — the polar counterpart of `Scene`'s
    /// one-uniform-scale rule, and for the same reason: a squashed circle
    /// misreports every angle. The room is a count of pixels rather than a
    /// fraction of the panel because that is what a label costs: a fraction would
    /// leave a big plot wastefully small and shrink a facet panel's circle to
    /// nothing at exactly the size where the labels need the room most.
    ///
    /// `angle_slots` is how many equal slots divide the turn when the angular axis
    /// is **categorical**, and it decides what the start angle points *at*. A
    /// categorical scale runs from `-0.5` to `n - 0.5`, so category `i` sits at
    /// `(i + 0.5)/n` of the turn and the scale's own origin is half a slot *before*
    /// the first category — a padding artifact, not a place in the data. Pointing
    /// `start` there put north at 22.5° on an eight-point compass and made the
    /// reader do `-180/n` arithmetic to get it back. So the space is rotated back
    /// by half a slot: **`start` points at the first category itself**, the way a
    /// flat categorical axis puts its tick at the category's center. `None` for a
    /// measured angle, where the scale minimum is a real value and stays put.
    pub(crate) fn new(
        l: &Layout,
        view: PolarView,
        label_room: f64,
        measure_on_angle: bool,
        angle_slots: Option<usize>,
    ) -> Self {
        // A pie carries no ring of angular names (its key is the legend, and its
        // slice labels are a `text` layer, not a guide — Wilkinson §9.1.6.1), so it
        // keeps the room those names would have taken and draws bigger.
        let room = if measure_on_angle { 0.0 } else { label_room };
        let reach = if measure_on_angle { 0.0 } else { RIM_LABEL_REACH };
        let r = (l.w() / 2.0 - room - reach).min(l.h() / 2.0 - room);
        // Half a slot back, so `start` lands on the first category rather than on
        // the padding before it. Everything rides on this one number: the wedges,
        // the spokes and the rim labels all rotate together, and the scale's origin
        // (`u = 0`) stays the slot *boundary* — which is where the radial tick
        // numbers are drawn, so they keep to the gap between wedges instead of
        // landing on the first one.
        let half_slot = match angle_slots {
            Some(n) if n > 0 => std::f64::consts::PI / n as f64,
            _ => 0.0,
        };
        Polar {
            cx: (l.x0 + l.x1) / 2.0,
            cy: (l.y0 + l.y1) / 2.0,
            r_max: r.max(1.0),
            start: view.start.to_radians() - half_slot,
            measure_on_angle,
            wraps: angle_slots.is_some(),
        }
    }

    /// Does the angular domain come back to its first value with nothing repeated?
    ///
    /// The angular axis is always periodic (one turn spans the fitted range), but
    /// *who closes the curve* differs by what the axis carries, and the difference
    /// is the data's, not the renderer's. A **measured** angle closes itself: a
    /// cyclical variable includes both ends of its cycle (hours 0 **and** 24), the
    /// scale puts them at the same place, and the last vertex lands on the first
    /// with no help. A **categorical** angle cannot do that — each category appears
    /// exactly once, by definition, so there is no repeated endpoint to land on and
    /// a path would stop one slot short of where it began, leaving a wedge of
    /// missing curve. The categories exhaust the turn between them, which makes the
    /// last→first adjacency real, so a path across them closes.
    ///
    /// Asked once here rather than decided in each mark: `line` and `area` are the
    /// two path/region marks drawn in polar today, and a rule they answered
    /// separately is the per-mark gap Law 2 exists to catch.
    pub(crate) fn wraps(&self) -> bool {
        self.wraps
    }

    /// The angle a normalized angular coordinate lands on. `u` is the fraction of
    /// the fitted x-range, so `0` is the scale minimum and `1` the maximum — one
    /// whole turn between them.
    pub(crate) fn angle(&self, u: f64) -> f64 {
        self.start + u * TAU
    }

    /// The radius a normalized radial coordinate lands on. Clamped at the center:
    /// a negative radius would reflect the point through the origin and draw it
    /// half a turn away from where its angle says it belongs.
    pub(crate) fn radius(&self, v: f64) -> f64 {
        (v * self.r_max).max(0.0)
    }

    /// Panel pixels for a normalized `(angle, radius)` pair.
    pub(crate) fn at(&self, u: f64, v: f64) -> (f64, f64) {
        self.polar_px(self.angle(u), self.radius(v))
    }

    /// Panel pixels for an angle in radians and a radius in pixels. Clockwise from
    /// twelve o'clock: `sin` on x and `-cos` on y, with SVG's y growing downward.
    pub(crate) fn polar_px(&self, theta: f64, r: f64) -> (f64, f64) {
        (self.cx + r * theta.sin(), self.cy - r * theta.cos())
    }

    /// The `d` of an annular sector: the wedge between two angles and two radii,
    /// which is what a bar becomes here. A bar measured from the center (`v0 = 0`)
    /// degenerates to a pie slice, and the inner arc collapses to the center point
    /// rather than being drawn as a zero-radius arc, which renderers disagree about.
    pub(crate) fn sector(&self, u0: f64, u1: f64, v0: f64, v1: f64) -> String {
        let (t0, t1) = (self.angle(u0), self.angle(u1));
        let (r0, r1) = (self.radius(v0), self.radius(v1));
        let (r_in, r_out) = if r0 <= r1 { (r0, r1) } else { (r1, r0) };
        let span = (t1 - t0).abs();
        let sweep = if t1 >= t0 { 1 } else { 0 };
        let large = if span > std::f64::consts::PI { 1 } else { 0 };

        let (ax, ay) = self.polar_px(t0, r_out);
        let (bx, by) = self.polar_px(t1, r_out);
        let mut d = format!("M {ax:.2} {ay:.2} ");
        arc_to(&mut d, r_out, large, sweep, bx, by, self.cx, self.cy, span);
        if r_in <= 1e-9 {
            // A pie slice: two straight radii meeting at the center.
            d.push_str(&format!("L {:.2} {:.2} Z", self.cx, self.cy));
        } else {
            let (dx, dy) = self.polar_px(t1, r_in);
            let (ex, ey) = self.polar_px(t0, r_in);
            d.push_str(&format!("L {dx:.2} {dy:.2} "));
            arc_to(&mut d, r_in, large, 1 - sweep, ex, ey, self.cx, self.cy, span);
            d.push('Z');
        }
        d
    }

    /// Start a stroke path at `(u, v)`.
    ///
    /// The three builders below are the whole of what the span marks needed, and
    /// the distinction they draw is the one the *marks* know and the geometry does
    /// not: whether a segment **holds** a value across a span or **joins** two
    /// vertices. Deciding it by comparing the two radii would be wrong in both
    /// directions — a `line` between two rows that happen to share a `y` would
    /// bow into an arc it never asserted, and a tread whose two ends round to
    /// different pixels would cut a chord through the value it is holding.
    pub(crate) fn move_to(&self, d: &mut String, u: f64, v: f64) {
        let (x, y) = self.at(u, v);
        d.push_str(&format!("M {x:.2} {y:.2} "));
    }

    /// Append a **joined** segment: a straight edge from wherever the path is to
    /// `(u, v)`. This is the chord `line`/`area`/`path` already draw between two
    /// vertices, and — when the angle does not change — it is exactly the radius,
    /// which is why a whisker's span and a stair's riser need nothing else.
    pub(crate) fn line_to(&self, d: &mut String, u: f64, v: f64) {
        let (x, y) = self.at(u, v);
        d.push_str(&format!("L {x:.2} {y:.2} "));
    }

    /// Append a **held** segment: the value is constant from `u0` to `u1`, so the
    /// path follows the ring at radius `v` instead of cutting across it.
    ///
    /// A stair's tread and a whisker's cap are the two callers, and both *assert*
    /// the value between their ends rather than interpolating to it — which is
    /// what makes this an arc and a `line`'s segment a chord. Half of one drawn
    /// straight puts the mark where the data is not (§12), which is the whole
    /// reason these marks were refused in this space.
    pub(crate) fn hold_to(&self, d: &mut String, u0: f64, u1: f64, v: f64) {
        let (t0, t1) = (self.angle(u0), self.angle(u1));
        let r = self.radius(v);
        let span = (t1 - t0).abs();
        // A held span of no width is a point, and `A` to where the pen already is
        // draws nothing at all — so say nothing rather than emitting a no-op.
        if span < 1e-9 {
            return;
        }
        let sweep = if t1 >= t0 { 1 } else { 0 };
        let large = if span > std::f64::consts::PI { 1 } else { 0 };
        let (x, y) = self.polar_px(t1, r);
        arc_to(d, r, large, sweep, x, y, self.cx, self.cy, span);
    }

    /// The angular half-width, in turns, that `half_px` of arc length subtends at
    /// radius `v` — how wide a **pixel-sized** ornament is when it is bent.
    ///
    /// An interval's end caps and a box's median are stroke ornaments: they mark
    /// where the span ends and where the middle is, and their width carries no
    /// quantity. So they keep the pixel width they have flat, which is §18's
    /// standing rule that a stroke's width is pixels — the same rule that refuses
    /// `rule` a thickness in data units. A cap held at a fixed *angle* instead
    /// would grow with the radius and read as a measurement of something.
    ///
    /// Guarded at the center, where a fixed arc length subtends an unbounded
    /// angle: an ornament may not wrap the circle, so it is capped at a sixth of
    /// a turn and shrinks with the radius below that.
    pub(crate) fn px_as_turns(&self, v: f64, half_px: f64) -> f64 {
        let r = self.radius(v);
        if r < 1e-9 {
            return 0.0;
        }
        (half_px / r / TAU).min(1.0 / 6.0)
    }

    /// Where a tick label sits around the rim, and how it must be anchored there.
    /// A name at three o'clock hangs off the right of its tick, one at nine
    /// o'clock off the left, and one at the top or bottom is centered — otherwise
    /// every label would collide with the circle on one side of the plot.
    pub(crate) fn rim_label(&self, u: f64, gap: f64, cap: f64) -> (f64, f64, &'static str) {
        let t = self.angle(u);
        let (x, y) = self.polar_px(t, self.r_max + gap);
        // `sin` is the horizontal component of the direction the label points.
        let s = t.sin();
        let anchor = if s > 0.2 {
            "start"
        } else if s < -0.2 {
            "end"
        } else {
            "middle"
        };
        // Nudge the baseline so a label at the top clears the rim and one at the
        // bottom does not sit on it: text hangs below its baseline by a cap height.
        let dy = -t.cos() * cap * 0.5 + cap * 0.35;
        (x, y + dy, anchor)
    }
}

/// Append one SVG elliptical-arc command. A span of a full turn has no arc: its
/// two endpoints coincide, and `A` would draw nothing at all, so it is split in
/// half at the antipode.
#[allow(clippy::too_many_arguments)]
fn arc_to(d: &mut String, r: f64, large: i32, sweep: i32, x: f64, y: f64, cx: f64, cy: f64, span: f64) {
    if span >= TAU - 1e-9 {
        let (mx, my) = (2.0 * cx - x, 2.0 * cy - y);
        d.push_str(&format!("A {r:.2} {r:.2} 0 1 {sweep} {mx:.2} {my:.2} "));
        d.push_str(&format!("A {r:.2} {r:.2} 0 1 {sweep} {x:.2} {y:.2} "));
    } else {
        d.push_str(&format!("A {r:.2} {r:.2} 0 {large} {sweep} {x:.2} {y:.2} "));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame() -> Polar {
        Polar::new(&Layout { x0: 0.0, y0: 0.0, x1: 200.0, y1: 200.0 }, PolarView { start: 0.0 }, 20.0, false, None)
    }

    /// The angular axis is periodic: one whole turn between the scale's minimum
    /// and its maximum, so the two ends of the axis land on the same spot. This is
    /// Wilkinson's alignment rule (§9.1.6), and it is what closes the circle.
    #[test]
    fn the_angular_axis_closes_on_itself() {
        let p = frame();
        let (x0, y0) = p.at(0.0, 1.0);
        let (x1, y1) = p.at(1.0, 1.0);
        assert!((x0 - x1).abs() < 1e-6, "{x0} vs {x1}");
        assert!((y0 - y1).abs() < 1e-6, "{y0} vs {y1}");
    }

    /// Zero is twelve o'clock and the travel is clockwise — a quarter turn lands
    /// at three o'clock, not at nine. Pins the convention the whole space is read
    /// against; flipping it would silently mirror every plot.
    #[test]
    fn the_circle_starts_at_the_top_and_runs_clockwise() {
        let p = frame();
        let (x_top, y_top) = p.at(0.0, 1.0);
        assert!((x_top - p.cx).abs() < 1e-6);
        assert!(y_top < p.cy, "0 must be above the center");

        let (x_q, y_q) = p.at(0.25, 1.0);
        assert!(x_q > p.cx, "a quarter turn is to the right, not the left");
        assert!((y_q - p.cy).abs() < 1e-6);
    }

    /// `start` rotates the whole space and nothing else: the same normalized
    /// coordinate, a quarter turn on, lands where the next quarter used to.
    #[test]
    fn start_rotates_the_whole_circle() {
        let p = Polar::new(&Layout { x0: 0.0, y0: 0.0, x1: 200.0, y1: 200.0 }, PolarView { start: 90.0 }, 20.0, false, None);
        let (x, y) = p.at(0.0, 1.0);
        assert!(x > p.cx, "start = 90 puts the origin at three o'clock");
        assert!((y - p.cy).abs() < 1e-6);
    }

    /// A circle in a wide panel, not an ellipse: the radius is bounded by the
    /// shorter side, so every angle keeps its length.
    #[test]
    fn a_wide_panel_still_holds_a_circle() {
        let p = Polar::new(&Layout { x0: 0.0, y0: 0.0, x1: 400.0, y1: 100.0 }, PolarView::default(), 20.0, false, None);
        let (x_r, _) = p.at(0.25, 1.0);
        let (_, y_t) = p.at(0.0, 1.0);
        assert!((x_r - p.cx - (p.cy - y_t)).abs() < 1e-6, "horizontal and vertical radii must match");
        assert!(p.r_max <= 50.0);
    }

    /// A bar measured from the center is a pie slice: its path closes through the
    /// center rather than around a zero-radius arc.
    #[test]
    fn a_sector_from_the_center_closes_through_it() {
        let p = frame();
        let d = p.sector(0.0, 0.25, 0.0, 1.0);
        assert!(d.contains(&format!("L {:.2} {:.2} Z", p.cx, p.cy)), "{d}");
        assert_eq!(d.matches('A').count(), 1, "one outer arc, no inner one: {d}");
    }

    /// A bar that does not reach the center is an annulus: two arcs, the inner one
    /// swept back the other way so the ring closes.
    #[test]
    fn a_sector_off_the_center_is_an_annulus() {
        let p = frame();
        let d = p.sector(0.0, 0.25, 0.5, 1.0);
        assert_eq!(d.matches('A').count(), 2, "{d}");
        assert!(d.contains("0 0 1 "), "outer arc sweeps forward: {d}");
        assert!(d.contains("0 0 0 "), "inner arc sweeps back: {d}");
    }

    /// A wedge wider than a half turn must set the large-arc flag, or SVG draws
    /// the *minor* arc and the sector comes out inside-out.
    #[test]
    fn a_wedge_past_the_half_turn_takes_the_long_way() {
        let p = frame();
        let small = p.sector(0.0, 0.2, 0.0, 1.0);
        let big = p.sector(0.0, 0.8, 0.0, 1.0);
        assert!(small.contains("0 0 1 "), "minor arc: {small}");
        assert!(big.contains("0 1 1 "), "major arc: {big}");
    }

    /// A single slot covering the whole turn has coincident endpoints, so one `A`
    /// would draw nothing. It is split at the antipode instead.
    #[test]
    fn a_full_turn_is_drawn_as_two_arcs() {
        let p = frame();
        let d = p.sector(0.0, 1.0, 0.0, 1.0);
        assert_eq!(d.matches('A').count(), 2, "{d}");
    }

    /// The radius never goes negative: a value below the scale floor is drawn at
    /// the center, not reflected to the far side of the circle.
    #[test]
    fn a_radius_below_the_floor_stops_at_the_center() {
        let p = frame();
        let (x, y) = p.at(0.25, -0.5);
        assert!((x - p.cx).abs() < 1e-6 && (y - p.cy).abs() < 1e-6);
    }

    /// Rim labels lean away from the circle: right of a tick on the right side,
    /// left of one on the left, centered top and bottom.
    #[test]
    fn rim_labels_are_anchored_away_from_the_circle() {
        let p = frame();
        assert_eq!(p.rim_label(0.0, 8.0, 8.0).2, "middle");
        assert_eq!(p.rim_label(0.25, 8.0, 8.0).2, "start");
        assert_eq!(p.rim_label(0.5, 8.0, 8.0).2, "middle");
        assert_eq!(p.rim_label(0.75, 8.0, 8.0).2, "end");
    }
}
