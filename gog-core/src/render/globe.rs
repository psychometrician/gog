//! The `globe` coordinate space — the sphere itself, viewed.
//!
//! Wilkinson separates this space from `map` on meaning: *"This is not a
//! cartographic map. It is a statistical distribution measured in geographic
//! coordinates"* (§9.2.4.3). `map` flattens the sphere onto the page; this space
//! keeps it round and chooses which half faces the reader. `x` is longitude and
//! `y` latitude, both in degrees, and a mark stands at its place on the surface.
//!
//! **The projection is the cube's, verbatim.** The screen basis `project::Scene`
//! uses for `space` — right = (−sin az, cos az, 0), up = (−cos az·sin el,
//! −sin az·sin el, cos el) — applied to the lon/lat unit vector
//! p = (cos φ cos λ, cos φ sin λ, sin φ) *is* the textbook orthographic
//! projection centered on (turn, tilt): p·right = cos φ·sin(λ−turn) and
//! p·up = cos(tilt)·sin φ − sin(tilt)·cos φ·cos(λ−turn). A test below pins the
//! two term for term, so the formulas here cannot drift from the cube's.
//!
//! What the sphere adds to the cube's math is small and closed-form:
//! - **The cull is one inequality.** A surface point is visible iff its dot with
//!   the view vector is not negative — the front hemisphere — so the far side
//!   never enters the painter's sort. No occlusion engine is involved; the
//!   caller counts what a view hides and says so, never dropping it in silence.
//! - **The fit is `polar`'s.** An orthographic view of a sphere is a disk
//!   whatever the angles, so the panel fit is a circle bounded by the shorter
//!   side — the same rule that keeps a polar plot a circle in a wide panel and
//!   the cube square in `Scene::new`.
//! - **The graticule is the panel grid** (the user's ruling): every space draws
//!   gridlines, `polar` bends them into rings and spokes, and this space bends
//!   them into meridians and parallels. An *emphasized* meridian or parallel is
//!   a `rule` mark, not furniture.

use crate::ir::GlobeView;
use crate::render::project::Screen;
use crate::render::Layout;

/// Degrees to radians.
const RAD: f64 = std::f64::consts::PI / 180.0;

/// The graticule's step, in degrees — meridians every 30°, parallels every 30°
/// between ±60. Thirty rather than d3's ten because the disk is panel-sized, not
/// page-sized: at book width a 10° grid is ink, a 30° one is a reference.
const GRATICULE_STEP: f64 = 30.0;
/// How finely a graticule line is sampled before projection, in degrees. Two
/// degrees renders as a smooth curve at panel size (measured: a 79° great-circle
/// arc at this step is 41 vertices and reads as a clean curve).
const GRATICULE_SAMPLE: f64 = 2.0;
/// Parallels run from −60 to 60: the next 30° step is the pole itself, a point.
const PARALLEL_REACH: f64 = 60.0;

/// The spike ceiling: how far the largest value stands off the surface, in
/// sphere radii. The fitted top of the measure reaches exactly this; the disk
/// shrinks by the same headroom so the tallest spike stays on the panel.
/// Two fifths reads as drama without the spikes dwarfing the earth they stand
/// on — the proportion the WebGL population globes settled on by eye.
pub(crate) const SPIKE_MAX: f64 = 0.4;

/// One panel's globe: the viewing angles plus the affine fit that centers the
/// disk in the panel rectangle. Built once per panel and shared — the frame,
/// every mark and the graticule read it, so a point, the label beside it and
/// the meridian under it cannot disagree about where a place is.
pub(crate) struct Globe {
    sin_az: f64,
    cos_az: f64,
    sin_el: f64,
    cos_el: f64,
    /// The disk's center, in SVG pixels.
    pub(crate) cx: f64,
    pub(crate) cy: f64,
    /// The disk's radius — the limb — in pixels.
    pub(crate) r: f64,
    /// How far past the limb a spike may reach, in sphere radii. Zero for a
    /// plot with no `bar`; the spike ceiling otherwise, so the sphere shrinks
    /// to leave its spikes room instead of having them shaved by the clip.
    pub(crate) headroom: f64,
}

impl Globe {
    /// Build the globe for a panel rectangle. `inset` is the fraction of the
    /// shorter side kept clear around the limb, the way `Scene::new` keeps room
    /// around the cube; `headroom` is the spike ceiling in sphere radii, and it
    /// scales the disk down so the tallest spike still lands inside the panel.
    pub(crate) fn new(l: &Layout, view: GlobeView, inset: f64, headroom: f64) -> Self {
        // `turn` is a bearing and folds into one lap before the trig, for
        // `Scene::new`'s recorded reason: `sin(0)` is exactly 0 while `sin(-2π)`
        // is 2.4e-16, and equal views must be equal bit for bit. `tilt` is an
        // elevation, refused outside ±90 by `legality::check_globe`, so there is
        // nothing left to fold — folding it would turn a refused angle into a
        // silently different picture.
        let az = view.turn.rem_euclid(360.0).to_radians();
        let el = view.tilt.to_radians();
        let headroom = headroom.max(0.0);
        let half = (l.w().min(l.h()) / 2.0) * (1.0 - 2.0 * inset) / (1.0 + headroom);
        Globe {
            sin_az: az.sin(),
            cos_az: az.cos(),
            sin_el: el.sin(),
            cos_el: el.cos(),
            cx: (l.x0 + l.x1) / 2.0,
            cy: (l.y0 + l.y1) / 2.0,
            r: half.max(1.0),
            headroom,
        }
    }

    /// A place's device coordinates: `u` right, `v` up, both in [−1, 1] on the
    /// unit disk, and `w` the dot with the view vector — positive on the facing
    /// hemisphere, negative on the far one, zero exactly on the limb.
    fn device(&self, lon: f64, lat: f64) -> (f64, f64, f64) {
        let (lam, phi) = (lon * RAD, lat * RAD);
        let (px, py, pz) = (phi.cos() * lam.cos(), phi.cos() * lam.sin(), phi.sin());
        let u = -px * self.sin_az + py * self.cos_az;
        let v = -px * self.cos_az * self.sin_el - py * self.sin_az * self.sin_el
            + pz * self.cos_el;
        let w = px * self.cos_az * self.cos_el + py * self.sin_az * self.cos_el
            + pz * self.sin_el;
        (u, v, w)
    }

    /// Where a place lands on the page, or `None` when this view cannot see it.
    ///
    /// `None` is the far hemisphere — and a coordinate that is not a number,
    /// which has no place on the sphere at all. The caller counts what it could
    /// not draw and says so; per-row silence is the drop §12 forbids.
    pub(crate) fn place(&self, lon: f64, lat: f64) -> Option<Screen> {
        let (u, v, w) = self.device(lon, lat);
        if !(w >= 0.0) {
            return None;
        }
        Some(Screen {
            x: self.cx + self.r * u,
            y: self.cy - self.r * v,
            depth: -w,
        })
    }

    /// Does this view see the place? The same test `place` makes, exposed so a
    /// caller can count a layer's hidden rows without building screen points.
    pub(crate) fn front(&self, lon: f64, lat: f64) -> bool {
        self.device(lon, lat).2 >= 0.0
    }

    /// A **spike**: the visible stretch of the radial segment standing at a
    /// place, from the surface out to `1 + h` sphere radii. What a `bar` draws
    /// here — the place spends both positions, and the radius is the one
    /// direction the sphere has that its flattening does not, so the measure
    /// stands along it, exactly as `z` stands a bar up in the cube.
    ///
    /// The clip is the sphere's own, in closed form. A spike on the facing
    /// hemisphere is visible whole. One standing just *behind* the horizon can
    /// still peek over the limb: a point at `ρ·p` clears the silhouette when
    /// its lateral distance `ρ·sqrt(1 − w²)` reaches 1, so the visible stretch
    /// starts at `ρ = 1 / sqrt(1 − w²)` and exists only when the spike is tall
    /// enough to get there. Returns the on-page segment (from, tip) and the
    /// base's depth — spikes sort by their **footprint**, the cube's own rule —
    /// or `None` when the sphere hides the whole of it.
    pub(crate) fn spike(
        &self,
        lon: f64,
        lat: f64,
        h: f64,
    ) -> Option<((f64, f64), (f64, f64), f64)> {
        if !(h >= 0.0) {
            return None;
        }
        let (u, v, w) = self.device(lon, lat);
        if !w.is_finite() {
            return None;
        }
        let at = |rho: f64| (self.cx + self.r * rho * u, self.cy - self.r * rho * v);
        let from_rho = if w >= 0.0 {
            1.0
        } else {
            let lateral = (1.0 - w * w).sqrt();
            if lateral <= 1e-9 {
                // Straight behind the sphere's center: nothing clears the limb.
                return None;
            }
            let rho_min = 1.0 / lateral;
            if 1.0 + h < rho_min {
                return None;
            }
            rho_min
        };
        Some((at(from_rho), at(1.0 + h), -w))
    }

    /// One held longitude, whole: the meridian at `lon`, pole to pole, as pixel
    /// polylines split at the limb. This is what a `rule` on `x` draws — a rule
    /// *holds* one coordinate and spans the axis it does not name, and holding a
    /// longitude across every latitude is a great semicircle. Polar's spoke, one
    /// space over, from the same sentence.
    pub(crate) fn held_meridian(&self, lon: f64) -> Vec<Vec<(f64, f64)>> {
        self.visible_runs(sample(-90.0, 90.0).into_iter().map(move |lat| (lon, lat)))
    }

    /// One held latitude, whole: the parallel at `lat`, all the way round, as
    /// pixel polylines split at the limb — a small circle, polar's ring. What a
    /// `rule` on `y` draws.
    pub(crate) fn held_parallel(&self, lat: f64) -> Vec<Vec<(f64, f64)>> {
        self.visible_runs(sample(-180.0, 180.0).into_iter().map(move |lon| (lon, lat)))
    }

    /// The graticule's meridians: one pixel polyline per visible run of each
    /// 30° line of longitude, pole to pole.
    pub(crate) fn meridians(&self) -> Vec<Vec<(f64, f64)>> {
        let mut out = Vec::new();
        let mut lon = -180.0;
        while lon < 180.0 - GRATICULE_STEP / 2.0 {
            out.extend(self.held_meridian(lon));
            lon += GRATICULE_STEP;
        }
        out
    }

    /// The graticule's parallels: one pixel polyline per visible run of each 30°
    /// line of latitude between ±60 — the next step is the pole, a point.
    pub(crate) fn parallels(&self) -> Vec<Vec<(f64, f64)>> {
        let mut out = Vec::new();
        let mut lat = -PARALLEL_REACH;
        while lat <= PARALLEL_REACH + 1e-9 {
            out.extend(self.held_parallel(lat));
            lat += GRATICULE_STEP;
        }
        out
    }

    /// The place on the limb at angle `theta` (measured counterclockwise from
    /// screen-right in device coordinates), as pixels.
    fn limb_px(&self, theta: f64) -> (f64, f64) {
        (self.cx + self.r * theta.cos(), self.cy - self.r * theta.sin())
    }

    /// The same limb point as a longitude and latitude, for asking whether a
    /// stretch of the horizon lies inside a region. The horizon's 3-D point is
    /// `cos θ · right + sin θ · up`, read back through the view's basis.
    fn limb_lonlat(&self, theta: f64) -> (f64, f64) {
        let right = [-self.sin_az, self.cos_az, 0.0];
        let up = [
            -self.cos_az * self.sin_el,
            -self.sin_az * self.sin_el,
            self.cos_el,
        ];
        let (c, s) = (theta.cos(), theta.sin());
        let p = [
            c * right[0] + s * up[0],
            c * right[1] + s * up[1],
            c * right[2] + s * up[2],
        ];
        (p[1].atan2(p[0]) / RAD, p[2].clamp(-1.0, 1.0).asin() / RAD)
    }

    /// The point the view faces, as a longitude and latitude.
    fn center_lonlat(&self) -> (f64, f64) {
        let p = [
            self.cos_az * self.cos_el,
            self.sin_az * self.cos_el,
            self.sin_el,
        ];
        (p[1].atan2(p[0]) / RAD, p[2].clamp(-1.0, 1.0).asin() / RAD)
    }

    /// The whole disk as one closed subpath — what an invisible ring that
    /// surrounds the view contributes, for even-odd to carve the others from.
    pub(crate) fn disk_subpath(&self) -> String {
        let mut d = String::new();
        for i in 0..=180 {
            let (x, y) = self.limb_px(i as f64 * 2.0 * RAD);
            let cmd = if i == 0 { 'M' } else { 'L' };
            d.push_str(&format!("{cmd}{x:.2},{y:.2} "));
        }
        d.push_str("Z ");
        d
    }

    /// **A zone's ring against the facing hemisphere** — the one genuinely new
    /// piece of geometry this space asked for. A ring cut by the horizon must be
    /// **re-closed along the limb arc** to stay fillable, or a half-visible
    /// country is an open stroke rather than a region.
    ///
    /// Returns the closed pixel loops this ring contributes, plus whether it
    /// surrounds the view entirely (an invisible ring whose interior holds the
    /// view center — its visible extent is the whole disk). Everything is
    /// decided **even-odd**, matching the flat choropleth's fill rule, so no
    /// winding convention is ever consulted: whether a stretch of horizon lies
    /// inside the region is asked of the ring itself, point by point.
    ///
    /// The walk: resample every edge along its geodesic, cull to the facing
    /// hemisphere collecting visible chains with their exact horizon crossings,
    /// then link chain ends along the limb through the horizon stretches the
    /// region covers. Each ring crosses the horizon an even number of times, and
    /// the covered stretches alternate with the uncovered around the circle, so
    /// one containment test anchors them all.
    pub(crate) fn clip_ring(&self, ring: &[(f64, f64)]) -> (Vec<Vec<(f64, f64)>>, bool) {
        if ring.len() < 3 {
            return (Vec::new(), false);
        }
        // The cyclic vertex list: the closing duplicate off, each edge resampled
        // so the boundary bends with the sphere.
        let closed = ring.first() == ring.last();
        let m = if closed { ring.len() - 1 } else { ring.len() };
        if m < 3 {
            return (Vec::new(), false);
        }
        let mut cycle: Vec<(f64, f64)> = vec![ring[0]];
        for k in 0..m {
            cycle.extend(geodesic(ring[k], ring[(k + 1) % m]));
        }
        // The last geodesic lands back on the first vertex; drop the duplicate.
        cycle.pop();
        let n = cycle.len();
        if n < 3 {
            return (Vec::new(), false);
        }
        let w: Vec<f64> = cycle.iter().map(|&(lon, lat)| self.device(lon, lat).2).collect();
        let front = |i: usize| w[i] >= 0.0;

        if (0..n).all(front) {
            let lp: Vec<(f64, f64)> = cycle
                .iter()
                .filter_map(|&(lon, lat)| self.place(lon, lat).map(|s| (s.x, s.y)))
                .collect();
            return (vec![lp], false);
        }
        if !(0..n).any(front) {
            return (Vec::new(), ring_contains(&cycle, self.center_lonlat()));
        }

        // Mixed: walk the cycle from a hidden-to-visible transition so no chain
        // wraps the seam. A crossing is found by interpolating the two samples'
        // unit vectors to where the view dot is zero — at the sampling step that
        // is exact to well under a pixel.
        let start = (0..n)
            .find(|&i| !front((i + n - 1) % n) && front(i))
            .unwrap_or(0);
        let unit = |(lon, lat): (f64, f64)| {
            let (lam, phi) = (lon * RAD, lat * RAD);
            [phi.cos() * lam.cos(), phi.cos() * lam.sin(), phi.sin()]
        };
        // The crossing between samples i (one side) and j (the other), as a limb
        // angle theta and its pixels.
        let crossing = |i: usize, j: usize| -> (f64, (f64, f64)) {
            let (a, b) = (unit(cycle[i]), unit(cycle[j]));
            let (wa, wb) = (w[i], w[j]);
            let t = if (wa - wb).abs() > 1e-12 { wa / (wa - wb) } else { 0.5 };
            let v = [
                a[0] + t * (b[0] - a[0]),
                a[1] + t * (b[1] - a[1]),
                a[2] + t * (b[2] - a[2]),
            ];
            let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-12);
            let p = [v[0] / len, v[1] / len, v[2] / len];
            let u = -p[0] * self.sin_az + p[1] * self.cos_az;
            let vv = -p[0] * self.cos_az * self.sin_el - p[1] * self.sin_az * self.sin_el
                + p[2] * self.cos_el;
            let theta = vv.atan2(u).rem_euclid(std::f64::consts::TAU);
            (theta, (self.cx + self.r * u, self.cy - self.r * vv))
        };

        // Chains of visible pixels, each opening and closing on an exact
        // horizon crossing.
        struct Chain {
            pts: Vec<(f64, f64)>,
            entry: f64,
            exit: f64,
        }
        let mut chains: Vec<Chain> = Vec::new();
        let mut cur: Option<Chain> = None;
        for step in 0..n {
            let i = (start + step) % n;
            let prev = (i + n - 1) % n;
            if front(i) {
                if cur.is_none() {
                    let (theta, px) = crossing(prev, i);
                    cur = Some(Chain { pts: vec![px], entry: theta, exit: 0.0 });
                }
                if let (Some(c), Some(s)) = (cur.as_mut(), self.place(cycle[i].0, cycle[i].1)) {
                    c.pts.push((s.x, s.y));
                }
            } else if let Some(mut c) = cur.take() {
                let (theta, px) = crossing(prev, i);
                c.pts.push(px);
                c.exit = theta;
                chains.push(c);
            }
        }
        if let Some(mut c) = cur.take() {
            // The walk started on a visible run's first sample, so the cycle
            // ends visible only by returning to that run; close it at the
            // start's own entry crossing.
            let prev = (start + n - 1) % n;
            let (theta, px) = crossing(prev, start);
            c.pts.push(px);
            c.exit = theta;
            chains.push(c);
        }
        if chains.is_empty() {
            return (Vec::new(), false);
        }

        // Every crossing on the horizon circle, sorted by angle. The stretches
        // between consecutive crossings alternate between inside the region and
        // outside; one even-odd test anchors the alternation.
        #[derive(Clone, Copy)]
        struct Cross {
            theta: f64,
            chain: usize,
            is_exit: bool,
        }
        let mut crossings: Vec<Cross> = Vec::new();
        for (ci, c) in chains.iter().enumerate() {
            crossings.push(Cross { theta: c.entry, chain: ci, is_exit: false });
            crossings.push(Cross { theta: c.exit, chain: ci, is_exit: true });
        }
        crossings.sort_by(|a, b| a.theta.partial_cmp(&b.theta).unwrap_or(std::cmp::Ordering::Equal));
        let k = crossings.len();
        // Arc `i` runs from crossing `i` to the next; the last wraps. Widths
        // come from the sorted order and must sum to one turn, which is what
        // keeps a *degenerate* pair honest: a region closed through a pole edge
        // reaches the horizon twice at one angle, and that pair is a zero-width
        // arc and a full-turn one — never two ambiguous zeros.
        let mut width = vec![0.0_f64; k];
        for i in 0..k - 1 {
            width[i] = crossings[i + 1].theta - crossings[i].theta;
        }
        width[k - 1] = (std::f64::consts::TAU - width[..k - 1].iter().sum::<f64>()).max(0.0);
        // The covered and uncovered stretches alternate around the circle, so
        // one containment test anchors them all — taken at the *widest* arc's
        // midpoint, the farthest any test point can sit from a boundary; a
        // degenerate arc's midpoint sits exactly on one.
        let widest = (0..k)
            .max_by(|&a, &b| width[a].partial_cmp(&width[b]).unwrap_or(std::cmp::Ordering::Equal))
            .unwrap_or(0);
        let mid = crossings[widest].theta + width[widest] / 2.0;
        let widest_inside = ring_contains(&cycle, self.limb_lonlat(mid));
        let arc_inside =
            |i: usize| if (i + k - widest) % 2 == 0 { widest_inside } else { !widest_inside };

        // Link the chains through the covered stretches into closed loops.
        let mut used = vec![false; chains.len()];
        let mut loops: Vec<Vec<(f64, f64)>> = Vec::new();
        let pos_of = |chain: usize, is_exit: bool| {
            crossings.iter().position(|c| c.chain == chain && c.is_exit == is_exit)
        };
        for first in 0..chains.len() {
            if used[first] {
                continue;
            }
            let mut lp: Vec<(f64, f64)> = Vec::new();
            let mut at = first;
            for _ in 0..=chains.len() {
                used[at] = true;
                lp.extend(chains[at].pts.iter().copied());
                let Some(xi) = pos_of(at, true) else { break };
                // The covered stretch adjacent to this exit: exactly one of the
                // two arcs around it is inside, by alternation.
                let (arc, fwd) = if arc_inside(xi) {
                    (xi, true)
                } else {
                    ((xi + k - 1) % k, false)
                };
                // Walk the limb from the exit to the stretch's other end,
                // sampling the arc so it stays an arc on the page.
                let from = crossings[xi].theta;
                let span = width[arc];
                let steps = (span / (GRATICULE_SAMPLE * RAD)).ceil().max(1.0) as usize;
                for si in 1..=steps {
                    let t = from + if fwd { 1.0 } else { -1.0 } * span * si as f64 / steps as f64;
                    lp.push(self.limb_px(t));
                }
                let next = if fwd { (xi + 1) % k } else { (xi + k - 1) % k };
                let nc = crossings[next];
                if nc.is_exit {
                    // Numerically tangled crossings; close what we have rather
                    // than looping forever. The shape degrades by one limb arc,
                    // never by a bridge across the page.
                    break;
                }
                if nc.chain == first {
                    break;
                }
                at = nc.chain;
            }
            if lp.len() >= 3 {
                loops.push(lp);
            }
        }
        (loops, false)
    }

    /// Project a sampled line and split it where it leaves the facing
    /// hemisphere, so no stroke bridges across the limb. A great or small circle
    /// crosses the horizon at most twice, so each line yields at most two runs —
    /// and a parallel's two runs abut at ±180°, which is one place.
    fn visible_runs(&self, pts: impl Iterator<Item = (f64, f64)>) -> Vec<Vec<(f64, f64)>> {
        let mut runs: Vec<Vec<(f64, f64)>> = Vec::new();
        let mut open = false;
        for (lon, lat) in pts {
            match self.place(lon, lat) {
                Some(s) => {
                    if !open {
                        runs.push(Vec::new());
                        open = true;
                    }
                    runs.last_mut().unwrap().push((s.x, s.y));
                }
                None => open = false,
            }
        }
        runs.retain(|r| r.len() >= 2);
        runs
    }
}

/// Even-odd containment in longitude/latitude — the flat choropleth's own fill
/// rule, asked of the ring directly, which is what keeps the whole spherical
/// clip free of any winding convention. The test point's longitude is first
/// shifted by whole turns into the ring's own frame, so the two longitude
/// conventions meet here the way they meet everywhere else. A ring itself must
/// not straddle a frame seam, which is the convention boundary data ships
/// with: a ring crossing the antimeridian arrives already cut.
fn ring_contains(ring: &[(f64, f64)], pt: (f64, f64)) -> bool {
    if ring.len() < 3 {
        return false;
    }
    let mean: f64 = ring.iter().map(|p| p.0).sum::<f64>() / ring.len() as f64;
    let mut x = pt.0;
    while x < mean - 180.0 {
        x += 360.0;
    }
    while x > mean + 180.0 {
        x -= 360.0;
    }
    let y = pt.1;
    let mut inside = false;
    let n = ring.len();
    for i in 0..n {
        let (x1, y1) = ring[i];
        let (x2, y2) = ring[(i + 1) % n];
        if (y1 > y) != (y2 > y) {
            let t = (y - y1) / (y2 - y1);
            if x < x1 + t * (x2 - x1) {
                inside = !inside;
            }
        }
    }
    inside
}

/// Inclusive samples from `lo` to `hi` at the graticule's sampling step.
fn sample(lo: f64, hi: f64) -> Vec<f64> {
    let n = ((hi - lo) / GRATICULE_SAMPLE).round() as usize;
    (0..=n).map(|i| lo + i as f64 * GRATICULE_SAMPLE).collect()
}

/// The geodesic from `a` to `b` (each `(lon, lat)` in degrees), sampled at
/// roughly the graticule's step: the intermediate places, then `b` itself —
/// `a` is left out so a route's pairs chain without repeating their joints.
///
/// **A joined segment on a sphere follows a great circle** — the geodesic is
/// what two endpoints assert lies between them (Wilkinson §13.1.7), the
/// hold-versus-join principle `polar` recorded, answered here by citation. The
/// interpolation is spherical, by unit vectors, which is also what makes the
/// two longitude conventions (−180..180 and 0..360) meet without any wrap
/// arithmetic: the vectors never know which convention named them.
///
/// Antipodal endpoints have no unique geodesic. The route goes deterministically
/// through the north pole (or, from a pole, through the equator's origin) —
/// shortest-arc-among-equals, the recorded residue, and the same answer every
/// render gives so the picture cannot flicker between equals.
pub(crate) fn geodesic(a: (f64, f64), b: (f64, f64)) -> Vec<(f64, f64)> {
    let unit = |(lon, lat): (f64, f64)| {
        let (lam, phi) = (lon * RAD, lat * RAD);
        [phi.cos() * lam.cos(), phi.cos() * lam.sin(), phi.sin()]
    };
    let (ua, ub) = (unit(a), unit(b));
    let dot = (ua[0] * ub[0] + ua[1] * ub[1] + ua[2] * ub[2]).clamp(-1.0, 1.0);
    let omega = dot.acos();
    if !omega.is_finite() {
        return vec![b];
    }
    if omega < GRATICULE_SAMPLE * RAD {
        // Closer than one sample: the chord and the arc are the same pixels.
        return vec![b];
    }
    if (std::f64::consts::PI - omega) < 1e-9 {
        // Antipodes: route through a fixed waypoint, in two halves.
        let via = if a.1.abs() > 89.9 { (0.0, 0.0) } else { (0.0, 90.0) };
        let mut out = geodesic(a, via);
        out.extend(geodesic(via, b));
        return out;
    }
    let n = (omega / (GRATICULE_SAMPLE * RAD)).ceil() as usize;
    let sin_o = omega.sin();
    (1..=n)
        .map(|i| {
            let t = i as f64 / n as f64;
            let (fa, fb) = (((1.0 - t) * omega).sin() / sin_o, (t * omega).sin() / sin_o);
            let v = [
                fa * ua[0] + fb * ub[0],
                fa * ua[1] + fb * ub[1],
                fa * ua[2] + fb * ub[2],
            ];
            (v[1].atan2(v[0]) / RAD, (v[2].clamp(-1.0, 1.0)).asin() / RAD)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{GlobeView, SpaceView};
    use crate::render::project::Scene;
    use crate::render::Layout;

    fn panel() -> Layout {
        Layout { x0: 100.0, y0: 50.0, x1: 500.0, y1: 450.0 }
    }

    fn fiji() -> Globe {
        Globe::new(&panel(), GlobeView { turn: 178.0, tilt: -18.0 }, 0.04, 0.0)
    }

    /// **The projection is the cube's, term for term.** Three landmarks go
    /// through `Scene` as sphere points in the unit cube and through `Globe` as
    /// degrees; the two triangles must agree as one uniform scale with no
    /// rotation. This is the identity the whole design leans on — if it breaks,
    /// the two spaces have stopped sharing a camera and one of them is lying.
    #[test]
    fn the_screen_basis_is_the_cubes() {
        let view = SpaceView { turn: 178.0, tilt: -18.0 };
        let l = panel();
        let scene = Scene::new(view, l.x0, l.y0, l.x1, l.y1, 0.04);
        let g = fiji();

        // Fiji (the view center), Tokyo, the south pole — all facing this view.
        let marks = [(178.0, -18.0), (139.69, 35.69), (0.0, -90.0)];
        let sphere = |lon: f64, lat: f64| {
            let (lam, phi) = (lon * RAD, lat * RAD);
            (
                0.5 + 0.5 * phi.cos() * lam.cos(),
                0.5 + 0.5 * phi.cos() * lam.sin(),
                0.5 + 0.5 * phi.sin(),
            )
        };
        let cube: Vec<Screen> = marks
            .iter()
            .map(|&(lon, lat)| {
                let (x, y, z) = sphere(lon, lat);
                scene.to_screen(x, y, z)
            })
            .collect();
        let disk: Vec<Screen> =
            marks.iter().map(|&(lon, lat)| g.place(lon, lat).expect("front")).collect();

        let side = |p: &[Screen], i: usize, j: usize| (p[j].x - p[i].x, p[j].y - p[i].y);
        for (i, j) in [(0, 1), (0, 2)] {
            let (ax, ay) = side(&cube, i, j);
            let (bx, by) = side(&disk, i, j);
            let ratio = (ax * ax + ay * ay).sqrt() / (bx * bx + by * by).sqrt();
            // The cube fits a whole unit cube where the disk fits the sphere, so
            // the scale differs; the *shape* may not. Cross product zero means no
            // rotation and no reflection between the two.
            let cross = ax * by - ay * bx;
            let dot = ax * bx + ay * by;
            assert!(ratio.is_finite() && ratio > 0.0);
            assert!(
                cross.abs() / dot.abs() < 1e-9,
                "the two projections disagree by a rotation: cross/dot = {}",
                cross / dot
            );
            assert!(dot > 0.0, "the two projections point opposite ways");
        }
        // One uniform scale, not two: both sides must shrink by the same factor.
        let r = |i: usize, j: usize| {
            let (ax, ay) = side(&cube, i, j);
            let (bx, by) = side(&disk, i, j);
            (ax * ax + ay * ay).sqrt() / (bx * bx + by * by).sqrt()
        };
        assert!(
            (r(0, 1) / r(0, 2) - 1.0).abs() < 1e-9,
            "the scale is not uniform: {} vs {}",
            r(0, 1),
            r(0, 2)
        );
    }

    /// The view faces (turn, tilt): that place lands at the disk's center, its
    /// antipode is culled, and a coordinate that is not a number is culled too
    /// rather than emitted as `NaN` pixels.
    #[test]
    fn the_faced_place_is_the_center_and_its_antipode_is_hidden() {
        let g = fiji();
        let s = g.place(178.0, -18.0).expect("the faced place is visible");
        assert!((s.x - g.cx).abs() < 1e-9 && (s.y - g.cy).abs() < 1e-9);
        assert!(g.place(-2.0, 18.0).is_none(), "the antipode drew");
        assert!(g.place(f64::NAN, 0.0).is_none(), "NaN drew");
        assert!(!g.front(f64::NAN, 0.0));
    }

    /// A place a quarter turn from the view center sits exactly on the limb —
    /// distance `r` from the disk's center. The limb is the frame, so a mark
    /// past it would stand outside its own panel's world.
    #[test]
    fn a_quarter_turn_away_is_the_limb() {
        let g = Globe::new(&panel(), GlobeView { turn: 0.0, tilt: 0.0 }, 0.04, 0.0);
        for (lon, lat) in [(90.0, 0.0), (-90.0, 0.0), (0.0, 90.0), (0.0, -90.0)] {
            let s = g.place(lon, lat).expect("the limb is visible");
            let d = ((s.x - g.cx).powi(2) + (s.y - g.cy).powi(2)).sqrt();
            assert!((d - g.r).abs() < 1e-6, "({lon},{lat}) sits {d} from center, r = {}", g.r);
        }
    }

    /// Equal bearings are equal bit for bit — `turn = -360`, `0` and `720` name
    /// one view (`Scene::new`'s rule, inherited with its reason).
    #[test]
    fn equal_bearings_are_equal_views() {
        let l = panel();
        let a = Globe::new(&l, GlobeView { turn: 0.0, tilt: 10.0 }, 0.04, 0.0);
        for turn in [-360.0, 360.0, 720.0] {
            let b = Globe::new(&l, GlobeView { turn, tilt: 10.0 }, 0.04, 0.0);
            let (sa, sb) = (a.place(30.0, 40.0).unwrap(), b.place(30.0, 40.0).unwrap());
            assert_eq!(sa.x.to_bits(), sb.x.to_bits());
            assert_eq!(sa.y.to_bits(), sb.y.to_bits());
        }
    }

    /// A geodesic lands on its endpoint, stays on the sphere, and steps at
    /// roughly the sampling grain; endpoints closer than a step return just the
    /// far one, so a dense boundary is not inflated.
    #[test]
    fn a_geodesic_ends_where_it_was_asked_to() {
        let pts = geodesic((139.69, 35.69), (-118.24, 34.05));
        let last = *pts.last().unwrap();
        assert!((last.0 - -118.24).abs() < 1e-9 && (last.1 - 34.05).abs() < 1e-9);
        // 79.3° of sphere at a ~2° step.
        assert!((35..=45).contains(&pts.len()), "{} samples", pts.len());
        assert_eq!(geodesic((10.0, 10.0), (10.0, 10.5)), vec![(10.0, 10.5)]);
    }

    /// Antipodes have no unique geodesic; the detour is fixed and the same
    /// every time, so the picture cannot flicker between equals.
    #[test]
    fn antipodes_take_the_recorded_detour() {
        let a = geodesic((0.0, 0.0), (180.0, 0.0));
        let b = geodesic((0.0, 0.0), (180.0, 0.0));
        assert_eq!(a, b);
        // Through the north pole: some sample sits above 89°.
        assert!(a.iter().any(|p| p.1 > 89.0), "the detour did not go over the pole");
        // From a pole, the antipode is the other pole; the detour goes through
        // the equator's origin instead and still terminates.
        let c = geodesic((0.0, -90.0), (0.0, 90.0));
        assert!(c.iter().any(|p| p.1.abs() < 1.0));
    }

    /// **The pole-encircling ring — the named test.** A cap over the south pole
    /// (a synthetic Antarctica: shipped `world_borders` carries none), checked
    /// at the three views that decide the clip: facing it, it is one whole
    /// loop; facing away, it contributes nothing — the view center is not in
    /// it, decided even-odd with no winding convention consulted; and side-on
    /// it is cut at the horizon and re-closed along the limb, with the filled
    /// side the south.
    #[test]
    fn a_pole_cap_fills_its_own_side_of_the_limb_and_never_the_other() {
        // The cap, NE-style: the coast at −70, closed through the pole edge.
        let mut cap: Vec<(f64, f64)> = (-180..=180).step_by(5)
            .map(|lon| (lon as f64, -70.0)).collect();
        cap.push((180.0, -90.0));
        cap.push((-180.0, -90.0));
        cap.push(cap[0]);

        let l = panel();
        // Facing it: one loop, nothing else.
        let south = Globe::new(&l, GlobeView { turn: 0.0, tilt: -90.0 }, 0.04, 0.0);
        let (loops, disk) = south.clip_ring(&cap);
        assert_eq!(loops.len(), 1, "facing the cap it is one whole loop");
        assert!(!disk);

        // Facing away: nothing at all.
        let north = Globe::new(&l, GlobeView { turn: 0.0, tilt: 90.0 }, 0.04, 0.0);
        let (loops, disk) = north.clip_ring(&cap);
        assert!(loops.is_empty() && !disk, "the far side drew");

        // Side-on: cut and re-closed. The filled side must be the south — a
        // point over the cap lands inside a loop, one over the equator does not.
        let side = Globe::new(&l, GlobeView { turn: 0.0, tilt: 0.0 }, 0.04, 0.0);
        let (loops, disk) = side.clip_ring(&cap);
        assert!(!disk);
        assert!(!loops.is_empty(), "the side view lost the cap");
        let px_inside = |lp: &[(f64, f64)], p: (f64, f64)| {
            let mut inside = false;
            for i in 0..lp.len() {
                let (x1, y1) = lp[i];
                let (x2, y2) = lp[(i + 1) % lp.len()];
                if (y1 > p.1) != (y2 > p.1)
                    && p.0 < x1 + (p.1 - y1) / (y2 - y1) * (x2 - x1)
                {
                    inside = !inside;
                }
            }
            inside
        };
        let at = |lon: f64, lat: f64| {
            let s = side.place(lon, lat).unwrap();
            (s.x, s.y)
        };
        assert!(
            loops.iter().any(|lp| px_inside(lp, at(0.0, -80.0))),
            "a place over the cap fell outside every loop"
        );
        assert!(
            !loops.iter().any(|lp| px_inside(lp, at(0.0, 0.0))),
            "the equator was painted into the cap"
        );
        // Every loop vertex stays on the disk.
        for lp in &loops {
            for &(x, y) in lp {
                let d = ((x - side.cx).powi(2) + (y - side.cy).powi(2)).sqrt();
                assert!(d <= side.r + 1e-6, "a loop vertex sits {d} out, past {}", side.r);
            }
        }
    }

    /// A region that surrounds the view fills what the view sees. Cut boundary
    /// data closes such a region through a pole edge, so its ring reaches the
    /// front as a zero-width sliver down one meridian; the clip then traces the
    /// slit disk, and even-odd fills it whole. (A ring with **no** front vertex
    /// cannot surround a view in cut data — the pole edge it would need is the
    /// front vertex — so the full-disk marker is a safety net, not the path.)
    #[test]
    fn a_region_that_surrounds_the_view_fills_what_it_sees() {
        // Everything except the north cap: the boundary at +70, closed through
        // the SOUTH pole edge, NE-style.
        let mut wide: Vec<(f64, f64)> = (-180..=180).step_by(5)
            .map(|lon| (lon as f64, 70.0)).collect();
        wide.push((180.0, -90.0));
        wide.push((-180.0, -90.0));
        wide.push(wide[0]);

        let l = panel();
        let south = Globe::new(&l, GlobeView { turn: 0.0, tilt: -90.0 }, 0.04, 0.0);
        let (loops, _) = south.clip_ring(&wide);
        assert!(!loops.is_empty(), "the surrounding region vanished");
        // The view center must sit inside an odd number of loops: the region
        // covers everything this view sees.
        let inside = |lp: &[(f64, f64)], p: (f64, f64)| {
            let mut hit = false;
            for i in 0..lp.len() {
                let (x1, y1) = lp[i];
                let (x2, y2) = lp[(i + 1) % lp.len()];
                if (y1 > p.1) != (y2 > p.1)
                    && p.0 < x1 + (p.1 - y1) / (y2 - y1) * (x2 - x1)
                {
                    hit = !hit;
                }
            }
            hit
        };
        let crossings = loops.iter().filter(|lp| inside(lp, (south.cx, south.cy))).count();
        assert!(crossings % 2 == 1, "the view center fell outside the surrounding region");
    }

    /// **The spike, at the three places that decide it.** Facing the view it
    /// stands whole from the surface; just behind the horizon it peeks over the
    /// limb exactly when it is tall enough to clear the silhouette, entering at
    /// the limb itself; straight behind the sphere nothing clears. And a spike
    /// leaves the disk radially: its tip sits `(1 + h)` times the limb's
    /// distance from center, which is what lets the sphere shrink by the same
    /// headroom and keep every tip on the panel.
    #[test]
    fn a_spike_stands_faces_and_peeks_by_the_spheres_own_clip() {
        let g = Globe::new(&panel(), GlobeView { turn: 0.0, tilt: 0.0 }, 0.04, 0.5);
        // Facing: base on the surface, tip radially out at 1 + h.
        let (from, tip, depth) = g.spike(30.0, 20.0, 0.4).expect("a facing spike draws");
        let d = |p: (f64, f64)| ((p.0 - g.cx).powi(2) + (p.1 - g.cy).powi(2)).sqrt();
        let base = g.place(30.0, 20.0).unwrap();
        assert!((from.0 - base.x).abs() < 1e-9 && (from.1 - base.y).abs() < 1e-9);
        assert!((d(tip) - 1.4 * d(from)).abs() < 1e-6, "the tip is not radial");
        assert!(depth <= 0.0, "a facing base is on the viewer's side");

        // Behind the horizon at 130° from center, the silhouette is cleared at
        // 1.305 radii: a short spike stays hidden, a tall one peeks, entering
        // exactly at the limb.
        assert!(g.spike(130.0, 0.0, 0.1).is_none(), "a short back spike drew");
        let (from, tip, _) = g.spike(130.0, 0.0, 0.4).expect("a tall back spike peeks");
        assert!((d(from) - g.r).abs() < 1e-6, "the peek does not enter at the limb");
        assert!(d(tip) > g.r, "the peek does not clear the limb");

        // Straight behind: no height clears the silhouette's center.
        assert!(g.spike(180.0, 0.0, 10.0).is_none());
        // A negative height is not a spike; the caller counts and reports it.
        assert!(g.spike(30.0, 20.0, -0.1).is_none());
    }

    /// The graticule stays inside the limb and splits rather than bridging it:
    /// every vertex of every run is within the disk, and no line yields more
    /// than two visible runs (a circle crosses the horizon at most twice).
    #[test]
    fn the_graticule_is_clipped_to_the_disk_and_never_bridges_the_limb() {
        let g = fiji();
        let all: Vec<Vec<(f64, f64)>> =
            g.meridians().into_iter().chain(g.parallels()).collect();
        assert!(!all.is_empty(), "no graticule drew at all");
        for run in &all {
            for &(x, y) in run {
                let d = ((x - g.cx).powi(2) + (y - g.cy).powi(2)).sqrt();
                assert!(d <= g.r + 1e-6, "a graticule vertex sits {d} out, past the limb {}", g.r);
            }
        }
        // Twelve meridians and five parallels; at most two runs each.
        assert!(g.meridians().len() <= 24, "{} meridian runs", g.meridians().len());
        assert!(g.parallels().len() <= 10, "{} parallel runs", g.parallels().len());
    }
}
