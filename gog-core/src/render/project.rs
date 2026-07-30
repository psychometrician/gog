//! Orthographic projection for the `space` (3-D) coordinate.
//!
//! Static 3-D needs no GPU (spec §15): a scatter at a fixed viewing angle is a
//! rotation of normalized coordinates, an orthographic drop to 2-D, and a depth
//! sort so near points paint over far ones. The marks are the cheap part
//! (spec §16); the guides — the projected frame — cost more and live in `svg.rs`,
//! but they read their geometry from here so a tick and a point cannot disagree.
//!
//! Convention: data `x` and `y` are the floor, data `z` is up. `turn` swings the
//! floor around the vertical (z) axis; `tilt` lifts the eye above
//! it. A point arrives in the unit cube `[0,1]³` — each axis already normalized
//! to its fitted data range — and leaves as panel pixels plus a depth, where a
//! larger depth is farther from the camera (paint descending for back-to-front).

use crate::ir::SpaceView;

/// One point after projection: panel pixels `(x, y)` and a depth (larger =
/// farther from the camera).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Screen {
    pub x: f64,
    pub y: f64,
    pub depth: f64,
}

/// A projected scene: the viewing angle plus the affine fit that centers the
/// projected unit cube inside one panel rectangle, aspect preserved so the cube
/// is never sheared. Built once per panel — the frame and every mark share it,
/// so they cannot disagree about where a coordinate lands.
pub struct Scene {
    sin_az: f64,
    cos_az: f64,
    sin_el: f64,
    cos_el: f64,
    // Affine fit from device (u, v) to pixels: x = ox + scale·u, y = oy − scale·v.
    scale: f64,
    ox: f64,
    oy: f64,
}

impl Scene {
    /// Build the scene for a panel rectangle `(x0, y0, x1, y1)` in SVG pixels
    /// (y downward). `inset` is the fraction of the rectangle to keep clear
    /// around the cube, leaving room for the frame's tick labels.
    pub fn new(view: SpaceView, x0: f64, y0: f64, x1: f64, y1: f64, inset: f64) -> Self {
        // `az`/`el` are the turn and tilt angles in radians — the standard
        // azimuth/elevation names of the projection math, one layer below the
        // plain `turn`/`tilt` the API speaks.
        let az = view.turn.to_radians();
        let el = view.tilt.to_radians();
        let mut s = Scene {
            sin_az: az.sin(),
            cos_az: az.cos(),
            sin_el: el.sin(),
            cos_el: el.cos(),
            scale: 1.0,
            ox: 0.0,
            oy: 0.0,
        };
        // Fit the projected unit cube into the rectangle: project all 8 corners
        // to device space, bound them, and choose the one uniform scale that fits
        // — the same scale on both axes is what keeps the cube square, not sheared.
        let (mut umin, mut umax, mut vmin, mut vmax) =
            (f64::INFINITY, f64::NEG_INFINITY, f64::INFINITY, f64::NEG_INFINITY);
        for &(cx, cy, cz) in &CUBE_CORNERS {
            let (u, v) = s.device(cx, cy, cz);
            umin = umin.min(u);
            umax = umax.max(u);
            vmin = vmin.min(v);
            vmax = vmax.max(v);
        }
        let uspan = (umax - umin).max(1e-9);
        let vspan = (vmax - vmin).max(1e-9);
        let w = (x1 - x0) * (1.0 - 2.0 * inset);
        let h = (y1 - y0) * (1.0 - 2.0 * inset);
        s.scale = (w / uspan).min(h / vspan);
        // Center the cube's bounding box on the rectangle's center. `+scale·v_c`
        // on oy because device v is up-positive while SVG pixels grow downward.
        let (cxr, cyr) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
        s.ox = cxr - s.scale * (umin + umax) / 2.0;
        s.oy = cyr + s.scale * (vmin + vmax) / 2.0;
        s
    }

    /// Project a unit-cube point to device `(u, v)`, v up-positive, before the
    /// panel fit. Orthographic: center the cube, rotate, read the screen-right
    /// and screen-up components of the rotated point.
    fn device(&self, nx: f64, ny: f64, nz: f64) -> (f64, f64) {
        let (px, py, pz) = (nx - 0.5, ny - 0.5, nz - 0.5);
        // Screen basis for the turn/tilt angles (az, el):
        //   right = (−sin_az,          cos_az,          0)
        //   up    = (−cos_az·sin_el,  −sin_az·sin_el,  cos_el)
        let u = -px * self.sin_az + py * self.cos_az;
        let v = -px * self.cos_az * self.sin_el - py * self.sin_az * self.sin_el + pz * self.cos_el;
        (u, v)
    }

    /// Depth of a unit-cube point along the view axis, larger = farther. The
    /// view vector `(cos_az·cos_el, sin_az·cos_el, sin_el)` points from the
    /// origin toward the camera, so negating the projection onto it grows with
    /// distance.
    fn depth(&self, nx: f64, ny: f64, nz: f64) -> f64 {
        let (px, py, pz) = (nx - 0.5, ny - 0.5, nz - 0.5);
        -(px * self.cos_az * self.cos_el + py * self.sin_az * self.cos_el + pz * self.sin_el)
    }

    /// Project a unit-cube point all the way to panel pixels plus depth.
    pub fn to_screen(&self, nx: f64, ny: f64, nz: f64) -> Screen {
        let (u, v) = self.device(nx, ny, nz);
        Screen {
            x: self.ox + self.scale * u,
            y: self.oy - self.scale * v,
            depth: self.depth(nx, ny, nz),
        }
    }
}

/// The 8 corners of the unit cube, in `(x, y, z)`.
pub const CUBE_CORNERS: [(f64, f64, f64); 8] = [
    (0.0, 0.0, 0.0),
    (1.0, 0.0, 0.0),
    (1.0, 1.0, 0.0),
    (0.0, 1.0, 0.0),
    (0.0, 0.0, 1.0),
    (1.0, 0.0, 1.0),
    (1.0, 1.0, 1.0),
    (0.0, 1.0, 1.0),
];

/// The 12 edges of the unit cube, as index pairs into `CUBE_CORNERS`.
pub const CUBE_EDGES: [(usize, usize); 12] = [
    (0, 1), (1, 2), (2, 3), (3, 0), // floor (z = 0)
    (4, 5), (5, 6), (6, 7), (7, 4), // ceiling (z = 1)
    (0, 4), (1, 5), (2, 6), (3, 7), // vertical struts
];

#[cfg(test)]
mod tests {
    use super::*;

    fn scene(az: f64, el: f64) -> Scene {
        Scene::new(SpaceView { turn: az, tilt: el }, 0.0, 0.0, 100.0, 100.0, 0.0)
    }

    #[test]
    fn a_constant_z_only_shifts_the_picture_it_adds_no_structure() {
        // The spec's founding claim: "2-D is the degenerate case of 3-D
        // (z = constant)" (§15). If z carries no variation, the projected cloud
        // must be an (x, y) picture translated bodily by the constant — never
        // reshaped. So two datasets equal in (x, y) but at different constant z
        // differ on screen by one rigid shift and nothing else.
        let s = scene(30.0, 25.0);
        let xy = [(0.1, 0.2), (0.7, 0.4), (0.5, 0.9), (0.3, 0.6)];
        let low: Vec<Screen> = xy.iter().map(|&(x, y)| s.to_screen(x, y, 0.2)).collect();
        let high: Vec<Screen> = xy.iter().map(|&(x, y)| s.to_screen(x, y, 0.8)).collect();
        let dx0 = high[0].x - low[0].x;
        let dy0 = high[0].y - low[0].y;
        for i in 0..xy.len() {
            // Horizontal position never depends on z at all.
            assert!((low[i].x - high[i].x).abs() < 1e-9, "x moved with z at {i}");
            // Vertical shift is the same constant for every point — a translation.
            assert!((high[i].x - low[i].x - dx0).abs() < 1e-9, "non-uniform dx at {i}");
            assert!((high[i].y - low[i].y - dy0).abs() < 1e-9, "non-uniform dy at {i}");
        }
        // And it genuinely moved — otherwise the test proves nothing.
        assert!(dy0.abs() > 1e-6);
    }

    #[test]
    fn looking_down_the_x_axis_maps_y_across_and_z_up() {
        // A known angle pins the math. At turn 0, tilt 0 the camera sits
        // on the x-axis: screen-across is data y, screen-up is data z, and x runs
        // straight into the screen (it becomes depth, not position).
        let s = scene(0.0, 0.0);
        let (u_mid, v_mid) = s.device(0.5, 0.5, 0.5);
        assert!(u_mid.abs() < 1e-12 && v_mid.abs() < 1e-12, "center projects to origin");
        let (u, _) = s.device(0.5, 1.0, 0.5);
        assert!((u - 0.5).abs() < 1e-12, "data y drives screen-across");
        let (_, v) = s.device(0.5, 0.5, 1.0);
        assert!((v - 0.5).abs() < 1e-12, "data z drives screen-up");
    }

    #[test]
    fn nearer_points_have_smaller_depth() {
        // The default view lifts the camera above the floor (positive tilt),
        // so a higher z is nearer the camera and must sort in front (smaller
        // depth), or the depth sort would paint the scene inside-out.
        let s = scene(30.0, 25.0);
        let top = s.to_screen(0.5, 0.5, 1.0).depth;
        let bottom = s.to_screen(0.5, 0.5, 0.0).depth;
        assert!(top < bottom, "raising z should bring a point nearer, not push it back");
    }

    #[test]
    fn the_cube_fits_inside_its_panel() {
        // Every corner of the projected cube lands within the panel rectangle,
        // so the frame never spills past the panel it belongs to.
        let s = Scene::new(SpaceView::default(), 10.0, 20.0, 210.0, 220.0, 0.1);
        for &(cx, cy, cz) in &CUBE_CORNERS {
            let p = s.to_screen(cx, cy, cz);
            assert!(p.x >= 10.0 - 1e-6 && p.x <= 210.0 + 1e-6, "corner x out of panel: {}", p.x);
            assert!(p.y >= 20.0 - 1e-6 && p.y <= 220.0 + 1e-6, "corner y out of panel: {}", p.y);
        }
    }
}
