//! The `map` coordinate space — the sphere flattened onto the page.
//!
//! Wilkinson files cartography under ch. 13 §13.3.1.3, "Mapping the Sphere to 2D
//! Euclidean Space", the same section family as the nested space `nest` came
//! from. What makes this the cheapest of the four spaces to build is that it is
//! an ordinary *coordinate transform*: longitude and latitude go in, projected
//! positions come out, and everything downstream — extent fitting, ticks, the
//! marks, the legends — is the flat renderer doing exactly what it always does.
//! `polar` bends the normalized plane and `space` projects a cube, so both have
//! to be understood by the code that draws. This does not. A mark learns nothing.
//!
//! **Two projections, named by what they preserve** (spec §15). A projection
//! family behind one parameter is allowed here for the reason `scale = "log"` is
//! allowed and `smooth(method = )` is refused: each value is *one orthogonal
//! meaning* rather than a different thing wearing one name. Preserving area and
//! preserving angle are the two things a flattened sphere can do and cannot do
//! at once — Tissot's theorem — so the parameter names the choice a reader
//! actually makes, not the cartographer whose name the formula carries.
//!
//! Neither projection takes a further parameter, which is the constraint that
//! chose them. Albers is the usual equal-area answer for a single country and it
//! needs two standard parallels; a knob on a knob is the enumeration §5 exists to
//! stop, so it is out and stays out.

use crate::ir::{MapView, Preserve};

/// Degrees to radians.
const RAD: f64 = std::f64::consts::PI / 180.0;

/// The latitude where Mercator is cut off, in degrees.
///
/// Mercator sends the poles to infinity, so it must stop somewhere, and the
/// number is not arbitrary: at this latitude the projected `y` reaches ±π, which
/// is exactly half the ±π..π that longitude spans, so the world comes out square.
/// It is the same cut every web map makes, for the same reason.
///
/// A row beyond it is **clamped and reported**, never dropped and never silently
/// moved — a plot that quietly relocated Svalbard would be the silent drop §12
/// forbids, and one that dropped it would misreport *n*.
pub(crate) const MERCATOR_LIMIT: f64 = 85.051_128_779_806_59;

/// Equal Earth's four polynomial coefficients and the constant that authalically
/// squashes latitude, from Šavrič, Patterson & Jenny (2018).
const A1: f64 = 1.340_264;
const A2: f64 = -0.081_106;
const A3: f64 = 0.000_893;
const A4: f64 = 0.003_796;
/// √3⁄2 — the authalic factor, `sin θ = M sin φ`, which is what makes the
/// projection exactly equal-area before the polynomial reshapes it.
const M: f64 = 0.866_025_403_784_438_6;

/// One panel's projection: how a (longitude, latitude) pair in degrees becomes a
/// position on the flat page.
///
/// Built once per panel and shared, the way `Polar` and `project::Scene` are, so
/// a point, the path through it and a label beside it cannot disagree about where
/// a place is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Geo {
    preserve: Preserve,
}

impl Geo {
    pub(crate) fn new(view: &MapView) -> Self {
        Geo { preserve: view.preserve }
    }

    /// Where a place lands, in projected units.
    ///
    /// The units are the unit sphere's rather than the page's, and that is
    /// deliberate: the caller fits the panel to the projected extent, so only the
    /// **ratio** between the two axes has to be right, and it is. Returning
    /// pixels here would put the layout inside the projection, where two
    /// different callers could disagree about it.
    pub(crate) fn project(&self, lon: f64, lat: f64) -> (f64, f64) {
        match self.preserve {
            Preserve::Area => equal_earth(lon, lat),
            Preserve::Angle => mercator(lon, lat),
        }
    }

    /// Whether this projection has a latitude it cannot draw past. Only Mercator
    /// does; Equal Earth reaches both poles, which is one of the things being
    /// equal-area buys.
    pub(crate) fn limit(&self) -> Option<f64> {
        match self.preserve {
            Preserve::Area => None,
            Preserve::Angle => Some(MERCATOR_LIMIT),
        }
    }

    /// The latitude a projected `y` stands for, the inverse the axes need: a
    /// panel's edges are projected numbers, and a meridian is drawn and ticked
    /// between the latitudes at those edges. Both projections place `y` by
    /// latitude alone and raise it with latitude, so halving the interval finds it;
    /// fifty halvings leave it far below a pixel. Clamped to the drawable range.
    pub(crate) fn lat_at_y(&self, y: f64) -> f64 {
        let cap = self.limit().unwrap_or(90.0);
        let (mut lo, mut hi) = (-cap, cap);
        for _ in 0..50 {
            let mid = (lo + hi) / 2.0;
            if self.project(0.0, mid).1 < y { lo = mid } else { hi = mid }
        }
        (lo + hi) / 2.0
    }

    /// Whether a meridian is a straight line on the page: under Mercator a
    /// longitude has one `x` at every latitude, and under Equal Earth it bends.
    pub(crate) fn straight_meridians(&self) -> bool {
        self.preserve == Preserve::Angle
    }

    /// The projected span of a domain stated in degrees (spec §10).
    ///
    /// `limits` on a map is written in degrees and the panel is fitted in projected
    /// units, so the stated box has to be projected before it can bound the panel.
    /// It was not: `x(lon, limits = c(-12, 40))` handed -12..40 to an axis whose
    /// whole world spans about ±2.7, so every mark collapsed toward one point and
    /// the tick labels piled up on it, drawn without a word.
    ///
    /// A latitude alone places `y` in both projections, so a stated latitude is
    /// projected directly. A longitude's `x` also depends on the latitude under
    /// Equal Earth, whose meridians bend inward toward the poles, so each stated
    /// end takes its outermost `x` across the latitudes the panel covers: the
    /// stated ones, or the data's where an end is left open. An open end stays
    /// open, for the data to fill as it does on every other axis.
    pub(crate) fn project_domain(
        &self,
        lon: (Option<f64>, Option<f64>),
        lat: (Option<f64>, Option<f64>),
        data_lat: (f64, f64),
    ) -> ((Option<f64>, Option<f64>), (Option<f64>, Option<f64>)) {
        let cap = self.limit().unwrap_or(90.0);
        let south = lat.0.unwrap_or(data_lat.0).clamp(-cap, cap);
        let north = lat.1.unwrap_or(data_lat.1).clamp(-cap, cap);
        // A meridian is widest where it meets the equator, so the equator is
        // sampled whenever the band crosses it.
        let mut band: Vec<f64> =
            (0..=64).map(|i| south + (north - south) * i as f64 / 64.0).collect();
        if south < 0.0 && north > 0.0 {
            band.push(0.0);
        }
        let west = lon.0.map(|l| {
            band.iter().map(|&p| self.project(l, p).0).fold(f64::INFINITY, f64::min)
        });
        let east = lon.1.map(|l| {
            band.iter().map(|&p| self.project(l, p).0).fold(f64::NEG_INFINITY, f64::max)
        });
        let y = |l: Option<f64>| l.map(|v| self.project(0.0, v.clamp(-cap, cap)).1);
        ((west, east), (y(lat.0), y(lat.1)))
    }
}

/// **Equal Earth** (Šavrič, Patterson & Jenny 2018) — equal-area, and the default.
///
/// A choropleth is read by area: a reader compares how much ink a region has, so
/// a projection that inflates Greenland tells them something false about the
/// number in it. That is why the equal-area member is the default rather than the
/// famous one.
///
/// Equal Earth rather than Mollweide or Lambert cylindrical, which are equally
/// parameter-free and equally exact: it was designed, in 2018, specifically
/// because the older equal-area projections look wrong enough that people refuse
/// to use them and go back to a projection that lies about area. Being correct
/// and being used are not separable properties for a default.
fn equal_earth(lon: f64, lat: f64) -> (f64, f64) {
    let lam = lon * RAD;
    // The authalic latitude: this is the step that makes the map equal-area. The
    // polynomial below only redistributes what this has already made correct.
    let sin_theta = (M * (lat * RAD).sin()).clamp(-1.0, 1.0);
    let theta = sin_theta.asin();

    let t2 = theta * theta;
    let t6 = t2 * t2 * t2;

    // The x denominator is dy/dθ, which is what keeps the two axes in proportion —
    // spacing the parallels by the polynomial while spacing the meridians by its
    // slope is the whole trick, and getting it wrong would silently stop the map
    // being equal-area while still looking like one.
    let dy = A1 + 3.0 * A2 * t2 + t6 * (7.0 * A3 + 9.0 * A4 * t2);
    let x = lam * theta.cos() / (M * dy);
    let y = theta * (A1 + A2 * t2 + t6 * (A3 + A4 * t2));
    (x, y)
}

/// **Mercator** — conformal, and what `preserve = "angle"` means.
///
/// Every small shape keeps its true form, which is why it is the projection of
/// navigation and of every web map. The price is area, and it is not a small
/// price: Greenland arrives the size of Africa while being fourteen times
/// smaller. Law 8 is why this is offered at all rather than being refused as bad
/// taste — the ugly-but-legal is never forbidden — and it is why a choropleth
/// drawn in it earns a warning rather than a refusal.
fn mercator(lon: f64, lat: f64) -> (f64, f64) {
    let lam = lon * RAD;
    let phi = lat.clamp(-MERCATOR_LIMIT, MERCATOR_LIMIT) * RAD;
    let y = (std::f64::consts::FRAC_PI_4 + phi / 2.0).tan().ln();
    (lam, y)
}

/// Replace a frame's two position columns with their projected values.
///
/// **This is where the map space happens, and it is the whole of it.** Doing it
/// to the *data*, once, before anything is fitted, is what buys the rest for
/// free: the extent is then the projected extent, so the panel is the shape of
/// the map; the ticks, the marks, the brush rectangle and the legends all read
/// projected numbers and none of them has to be told. Compare `polar`, which
/// every mark has to ask where a coordinate lands, and `space`, which routes the
/// marks through a projector and depth-sorts them.
///
/// The pair is projected together rather than a column at a time, and that is not
/// an optimization: Equal Earth's `x` depends on latitude, so a longitude column
/// has no projected value of its own. Any pseudocylindrical projection is coupled
/// this way, which is why the two columns are read and written as one step.
///
/// **A frame with one of the two columns is a `rule`'s, and it is projected on the
/// axis it names.** It was returned unchanged, so a rule's raw degrees landed in a
/// projected panel: `rule + y(lat)` over a table holding only `lat = 45` stretched
/// the panel to 45 and collapsed the map to a 58-pixel sliver, in silence. Latitude
/// 0 was the one value that worked, because it projects to 0. Both projections set
/// a height from latitude alone, so a latitude alone is a parallel, placed exactly.
/// A longitude alone is read at the equator, which is where a two-column rule at
/// that longitude is placed too; under `preserve = "area"` a meridian curves, and
/// drawing it straight is one gap for both forms. A frame with neither column is
/// returned unchanged: a position that is not a number is refused in `legality`
/// before the renderer is reached, because a category has no longitude.
pub(crate) fn project_frame(
    df: &crate::data::DataFrame,
    geo: &Geo,
    x_field: &str,
    y_field: &str,
) -> crate::data::DataFrame {
    let (lon, lat) = match (df.float_col(x_field), df.float_col(y_field)) {
        (Some(lon), Some(lat)) => (lon, lat),
        (None, Some(lat)) => {
            let ys = lat.iter().map(|&l| geo.project(0.0, l).1).collect();
            return df.clone().with_float(y_field, ys);
        }
        (Some(lon), None) => {
            let xs = lon.iter().map(|&l| geo.project(l, 0.0).0).collect();
            return df.clone().with_float(x_field, xs);
        }
        (None, None) => return df.clone(),
    };
    let n = lon.len().min(lat.len());
    let mut xs = Vec::with_capacity(n);
    let mut ys = Vec::with_capacity(n);
    for i in 0..n {
        let (x, y) = geo.project(lon[i], lat[i]);
        xs.push(x);
        ys.push(y);
    }
    df.clone().with_float(x_field, xs).with_float(y_field, ys)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area() -> Geo {
        Geo::new(&MapView { preserve: Preserve::Area })
    }
    fn angle() -> Geo {
        Geo::new(&MapView { preserve: Preserve::Angle })
    }

    /// Both projections send the origin to the origin. Trivial, and it is the one
    /// case where a sign error in either formula shows up immediately.
    #[test]
    fn null_island_is_the_origin_in_both_projections() {
        for g in [area(), angle()] {
            let (x, y) = g.project(0.0, 0.0);
            assert!(x.abs() < 1e-12 && y.abs() < 1e-12, "{x} {y}");
        }
    }

    /// A frame with one position column is a rule's, and it is projected on the axis
    /// it names: a latitude to the height its parallel has at any longitude, and a
    /// longitude to where the equator crosses it. It kept its raw degrees before.
    #[test]
    fn a_rule_frame_with_one_column_is_projected_on_its_axis() {
        use crate::data::DataFrame;
        for g in [area(), angle()] {
            let lats = DataFrame::new().with_float("lat", vec![45.0, -30.0]);
            let out = project_frame(&lats, &g, "lon", "lat");
            let ys = out.float_col("lat").unwrap();
            for (i, &l) in [45.0, -30.0].iter().enumerate() {
                assert_eq!(ys[i], g.project(0.0, l).1);
                assert_eq!(ys[i], g.project(120.0, l).1, "a parallel's height is the same at every longitude");
            }
            let lons = DataFrame::new().with_float("lon", vec![90.0]);
            let out = project_frame(&lons, &g, "lon", "lat");
            assert_eq!(out.float_col("lon").unwrap()[0], g.project(90.0, 0.0).0);
        }
    }

    /// Both are symmetric about the equator and the prime meridian: the northern
    /// hemisphere is the southern one flipped, and east is west mirrored. A map
    /// that failed this would be visibly wrong and the failure would be easy to
    /// mistake for the data.
    #[test]
    fn the_projections_are_symmetric_about_both_axes() {
        for g in [area(), angle()] {
            for (lon, lat) in [(30.0, 45.0), (120.0, 10.0), (170.0, 75.0)] {
                let (x, y) = g.project(lon, lat);
                let (xs, ys) = g.project(-lon, -lat);
                assert!((x + xs).abs() < 1e-12, "east/west: {x} {xs}");
                assert!((y + ys).abs() < 1e-12, "north/south: {y} {ys}");
            }
        }
    }

    /// **The property the default is named for**, checked rather than asserted.
    ///
    /// A projection is equal-area when the determinant of its Jacobian is the same
    /// everywhere. Measuring it by finite differences over a small cell at several
    /// latitudes is the honest test: it would catch a wrong coefficient or a
    /// dropped term in the `x` denominator, which is exactly the mistake that
    /// leaves a map looking plausible while no longer being equal-area.
    #[test]
    fn equal_earth_gives_every_cell_on_the_globe_the_same_area() {
        let g = area();
        let d = 0.01;
        // The area a cell *should* have shrinks as cos(latitude); the projected
        // cell's area divided by that is the constant we are checking.
        let ratio_at = |lat: f64| {
            let (x0, y0) = g.project(0.0, lat);
            let (x1, _) = g.project(d, lat);
            let (_, y1) = g.project(0.0, lat + d);
            ((x1 - x0) * (y1 - y0)).abs() / (lat * RAD).cos()
        };
        let base = ratio_at(0.0);
        for lat in [15.0, 30.0, 45.0, 60.0, 75.0] {
            let r = ratio_at(lat);
            assert!(
                (r / base - 1.0).abs() < 2e-3,
                "a cell at {lat}° covers {:.4} of what one at the equator does",
                r / base
            );
        }
    }

    /// **The property the other value is named for.** Conformal means the scale
    /// factor at a place is the same in both directions, so a small circle on the
    /// globe stays a circle instead of becoming an ellipse.
    ///
    /// The comparison has to be against distance *on the sphere*, not against
    /// degrees: a degree of longitude is only `cos(latitude)` as long as a degree
    /// of latitude, and forgetting that is the obvious way to write this test and
    /// have it fail against a correct projection — which is what happened here
    /// first.
    #[test]
    fn mercator_stretches_both_directions_equally_at_every_latitude() {
        let g = angle();
        let d = 0.001;
        for lat in [0.0, 15.0, 30.0, 45.0, 60.0, 75.0] {
            let (x0, y0) = g.project(0.0, lat);
            let (x1, _) = g.project(d, lat);
            let (_, y1) = g.project(0.0, lat + d);
            // Each projected step divided by the true distance it covers.
            let east = (x1 - x0) / (d * RAD * (lat * RAD).cos());
            let north = (y1 - y0) / (d * RAD);
            assert!(
                (east / north - 1.0).abs() < 1e-3,
                "at {lat}° the scale is {east:.6} east and {north:.6} north"
            );
        }
    }

    /// Mercator's cut is where the world becomes square: `y` reaches ±π exactly as
    /// longitude spans ±π. If this drifts, the projection is still conformal and
    /// the world is no longer square, which is the kind of change nothing else
    /// would notice.
    #[test]
    fn the_mercator_limit_is_where_the_world_comes_out_square() {
        let (_, y) = angle().project(0.0, MERCATOR_LIMIT);
        assert!((y - std::f64::consts::PI).abs() < 1e-9, "{y}");
    }

    /// Past the cut, latitude is **clamped rather than sent to infinity**. The
    /// caller reports it; what matters here is that the number stays finite, since
    /// an infinite coordinate would poison the extent and blank the whole panel.
    #[test]
    fn a_pole_in_mercator_clamps_instead_of_reaching_infinity() {
        let (_, y) = angle().project(0.0, 90.0);
        assert!(y.is_finite(), "the north pole projected to {y}");
        assert_eq!(angle().limit(), Some(MERCATOR_LIMIT));
        assert_eq!(area().limit(), None, "equal earth reaches the poles");
    }

    /// Equal Earth reaches the poles, and each is a **line rather than a point** —
    /// it is built on Putniņš P4′, which has pole lines by design. The line is
    /// about 59% of the equator, which is what gives the map its rounded shape
    /// without the hard shearing an ellipse has at its edges.
    ///
    /// Worth pinning, because "the poles close to a point" is the natural guess
    /// and it is wrong: a projection that did close them would be a different
    /// projection wearing this one's name.
    #[test]
    fn equal_earth_draws_the_poles_as_a_line_shorter_than_the_equator() {
        let g = area();
        let (equator, _) = g.project(180.0, 0.0);
        let (pole, _) = g.project(180.0, 90.0);
        assert!(pole > 0.0, "the pole closed to a point: {pole}");
        let share = pole / equator;
        assert!(
            (share - 0.592).abs() < 0.01,
            "the pole line is {share:.4} of the equator"
        );
    }

    /// The world is wider than it is tall, in both projections, and Equal Earth's
    /// ratio is the published 2.05:1. The panel's shape is taken from this, so a
    /// wrong ratio here is a map that is stretched on the page.
    #[test]
    fn equal_earth_lays_the_world_out_at_its_published_proportions() {
        let g = area();
        let (x, _) = g.project(180.0, 0.0);
        let (_, y) = g.project(0.0, 90.0);
        let ratio = (2.0 * x) / (2.0 * y);
        assert!((ratio - 2.05).abs() < 0.01, "the world came out {ratio:.4} : 1");
    }

    /// A domain stated in degrees is projected before it bounds the panel. Handed
    /// over as degrees, -12..40 was read as projected units, about ten times the
    /// width of the whole world, and the map collapsed toward one point.
    #[test]
    fn a_stated_box_is_projected_before_it_bounds_the_panel() {
        for g in [area(), angle()] {
            let ((west, east), (south, north)) =
                g.project_domain((Some(-12.0), Some(40.0)), (Some(34.0), Some(60.0)), (34.0, 60.0));
            let (west, east, south, north) =
                (west.unwrap(), east.unwrap(), south.unwrap(), north.unwrap());
            assert!(west < east && south < north, "the box turned over: {west} {east} {south} {north}");
            assert!(west.abs() < 1.0 && east.abs() < 1.0, "x was left in degrees: {west}..{east}");
            assert_eq!(south, g.project(0.0, 34.0).1);
            assert_eq!(north, g.project(0.0, 60.0).1);
            // Meridians bend toward the poles under Equal Earth, so the box is
            // widest along its edge nearest the equator, here its southern one.
            assert!((west - g.project(-12.0, 34.0).0).abs() < 1e-12, "west edge {west}");
            assert!((east - g.project(40.0, 34.0).0).abs() < 1e-12, "east edge {east}");
        }
    }

    /// A band that crosses the equator is widest on it, and an end left open stays
    /// open for the data to fill.
    #[test]
    fn a_stated_longitude_is_widest_where_its_band_crosses_the_equator() {
        let g = area();
        let ((west, east), (south, north)) =
            g.project_domain((None, Some(40.0)), (None, None), (-30.0, 30.0));
        assert_eq!(west, None);
        assert_eq!((south, north), (None, None));
        assert!((east.unwrap() - g.project(40.0, 0.0).0).abs() < 1e-12, "east edge {east:?}");
    }
}
