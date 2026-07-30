//! Scale transformation — how a number becomes a position.
//!
//! A scale is **not** a transform. `bin` and `mean` are statistics: they read the
//! data and produce different data. A scale produces no new values at all — it
//! only decides how far along the axis a number lands, and what the ticks say.
//! That is why it is a property of a channel binding, `x(gdp, scale = "log")`,
//! rather than an atom of its own: there is nothing to combine it with.
//!
//! It is also not the same thing as logging the column. `x(log_gdp)` runs the
//! axis 2, 3, 4, 5 and leaves the reader to exponentiate; a log *scale* leaves
//! the data in its own units and labels the ticks 100, 1K, 10K, 100K. The
//! numbers a reader sees are the numbers they were given.
//!
//! # Where a scale sits in the pipeline
//!
//! Wilkinson runs scales *before* statistics — Ch. 6 before Ch. 7 — and the
//! ordering is invisible until a transform is in the plot, at which point it is
//! very visible. gog follows one rule, with no per-transform exceptions:
//!
//! > **A scale applies before the transform on the axis it groups by, and after
//! > it on the axis the transform writes.**
//!
//! Grouping has to happen in the space the reader will see. `bar * bin` binned
//! in linear space and merely *drawn* on a log axis bunches every bar towards
//! the top of the range — measured on four decades of gapminder income, the
//! gaps between bars run 176, 82, 54, 40, 32, 27 px and the left half of the
//! plot is empty. Binning in log space gives each bin one constant ratio, so the
//! bars land at a constant spacing and cover the axis.
//!
//! (The *widths* stay equal either way, because `bar_thickness_svg` hands every
//! bar one thickness taken from the smallest gap. That is why the renderer test
//! for this rule asserts spacing: width cannot tell the two orders apart, and a
//! test that cannot fail proves nothing.)
//!
//! The measured value is the opposite case: it is computed in the data's own
//! units and *then* displayed, so `bar * sum` stays a sum. Logging first would
//! make it the log of a product, which is not a quantity anyone asked for. This
//! is where ggplot2 surprises people — `stat_summary(fun = mean)` under
//! `scale_y_log10()` quietly returns a geometric mean — and the divergence is
//! deliberate.
//!
//! `transform::apply` already names its axes by role, `key_field` and
//! `out_field`, so the rule needs no new distinction: it reuses one the code was
//! making already.
//!
//! # Non-positive values
//!
//! A logarithm is undefined at zero and below, so those rows have no position.
//! `to_log` maps them to `NaN` rather than to some clamped stand-in, because a
//! stand-in would place a mark somewhere it does not belong. `legality` refuses
//! the plot outright when the *source* data cannot be placed; the renderer skips
//! `NaN` coordinates and says how many it dropped, which covers the case
//! legality cannot see — a transform whose *output* goes non-positive.

use crate::data::DataFrame;
use crate::ir::{ChannelDef, ScaleType};

/// The base a log scale uses when the binding does not name one.
///
/// Ten, because it is the base a reader decodes without arithmetic: a tick
/// marked 1000 sits three steps from one marked 1, and nobody has to be told so.
pub const DEFAULT_LOG_BASE: f64 = 10.0;

/// Euler's number, to the precision a base comparison needs.
pub const E: f64 = std::f64::consts::E;

/// Is this binding on a log scale? `None` (no binding) is not.
pub fn is_log(def: Option<&ChannelDef>) -> bool {
    matches!(def.and_then(|d| d.scale.as_ref()), Some(ScaleType::Log))
}

/// The base this binding's log scale runs on.
///
/// **The base is very nearly cosmetic**, which is worth knowing before reaching
/// for it. `log_b(x)` differs from `log10(x)` by the constant factor
/// `1 / log10(b)`, and the renderer normalizes every axis by its own range — so
/// the factor cancels and *every base draws the same picture*. It survives into
/// the transforms too: bins of equal width in log₂ space are equal width in
/// log₁₀ space, so `bar * bin` cuts at the same values whatever the base.
///
/// What the base actually changes is the **ticks**: where the gridlines fall and
/// how they are labeled. That is why base 2 earns its keep — 1, 2, 4, 8, 16
/// are the gridlines for octaves, bits and doubling times — and why base *e*
/// only makes sense with exponent labels, since 2.718 and 7.389 are not numbers
/// anyone wants to read off an axis.
pub fn log_base(def: Option<&ChannelDef>) -> f64 {
    def.and_then(|d| d.base)
        .filter(|b| *b > 1.0 && b.is_finite())
        .unwrap_or(DEFAULT_LOG_BASE)
}

/// A value's position on a log scale, or `NaN` where it has none.
pub fn to_log(v: f64, base: f64) -> f64 {
    if v <= 0.0 {
        return f64::NAN;
    }
    // `v.log(b)` is `ln v / ln b`, which loses the exactness that matters most:
    // it can put `1000` at 2.9999999999999996 and cost the axis a whole decade.
    // The two bases anyone actually asks for have exact intrinsics.
    if base == 10.0 { v.log10() }
    else if base == 2.0 { v.log2() }
    else { v.log(base) }
}

/// The quantity at a log-scale position — used to label a tick in the reader's
/// units rather than in exponents.
pub fn from_log(position: f64, base: f64) -> f64 {
    base.powf(position)
}

/// Replace one column with its log positions, leaving every other column alone.
///
/// Returns a new frame rather than mutating, so the raw data stays available to
/// the guides: a legend labels what the reader was given, not what the axis
/// happens to be doing.
pub fn log_column(df: &DataFrame, field: &str, base: f64) -> DataFrame {
    let names: Vec<String> = df.column_names().map(str::to_string).collect();
    let mut out = DataFrame::new();
    for name in names {
        if let Some(vals) = df.float_col(&name) {
            out = if name == field {
                out.with_float(name, vals.iter().map(|&v| to_log(v, base)).collect())
            } else {
                out.with_float(name, vals.clone())
            };
        } else if let Some(vals) = df.str_col(&name) {
            out = out.with_str(name, vals.clone());
        }
    }
    out
}

/// `Some(base)` when this binding asks for a log scale, `None` otherwise.
pub fn log_of(def: Option<&ChannelDef>) -> Option<f64> {
    if is_log(def) { Some(log_base(def)) } else { None }
}

/// The stated domain for this binding, in the data's own units (spec §10).
///
/// Either end is `None` on its own, which leaves that end to the data:
/// `limits = c(0, NA)` pins a baseline and lets the top follow.
pub fn limits_of(def: Option<&ChannelDef>) -> (Option<f64>, Option<f64>) {
    match def.and_then(|d| d.limits) {
        Some([lo, hi]) => (lo, hi),
        None => (None, None),
    }
}

/// The domain to actually *use* — [`limits_of`], less the ones that are not
/// domains.
///
/// A backwards or zero-width pair is a typo, and `check_limits` refuses it by
/// name. Nothing downstream may act on it in the meantime: filtering rows
/// against `c(20, 5)` empties the frame, which would then be reported as "your
/// limits leave no rows" — a fact about the data standing in for a fact about
/// the sentence, and the second, wronger diagnostic §12 says not to print. So
/// the consumers read the domain through here and the reporters read it raw.
///
/// The visible effect is under `GOG_STRICT=0`, where the refusal is a warning
/// and something still has to draw: an unlimited plot, which is the picture the
/// caller had before the typo, rather than an empty panel.
pub fn domain_of(def: Option<&ChannelDef>) -> (Option<f64>, Option<f64>) {
    match limits_of(def) {
        (Some(l), Some(h)) if !(l < h) => (None, None),
        (l, h) => (l.filter(|v| v.is_finite()), h.filter(|v| v.is_finite())),
    }
}

/// Does this binding state a domain worth acting on? Cheap enough to ask per row.
pub fn has_limits(def: Option<&ChannelDef>) -> bool {
    matches!(domain_of(def), (Some(_), _) | (_, Some(_)))
}

/// How many ticks this binding asks for, if it asks (spec §10). Read through here
/// rather than off the field for the reason `domain_of` exists: a count of 0 or 1
/// cannot describe an axis (two ticks are the fewest that show a direction), and
/// `nice_ticks` would silently clamp it. `legality` reports the bad count; the
/// consumers read it through here and get the default instead, so a rejected count
/// under `GOG_STRICT=0` draws the axis the caller had before the typo.
pub fn tick_count_of(def: Option<&ChannelDef>) -> Option<usize> {
    def?.tick_count.filter(|n| *n >= 2)
}

/// Is `v` inside the domain this binding states? True when it states none.
///
/// The one place the question is answered, so the row filter, the legality
/// count and any future caller cannot disagree about which side of the boundary
/// a value on it falls (inclusive, both ends — `limits = c(0, 24)` keeps a
/// midnight reading of exactly 24).
pub fn within_limits(def: Option<&ChannelDef>, v: f64) -> bool {
    let (lo, hi) = domain_of(def);
    if !v.is_finite() {
        // Not a value the domain excludes — whatever drops a NaN, it is not this.
        return true;
    }
    lo.is_none_or(|l| v >= l) && hi.is_none_or(|h| v <= h)
}

// ---------------------------------------------------------------------------
// ChannelScale — the same question the axes ask, for every other channel
// ---------------------------------------------------------------------------

/// Where a value sits along a continuous channel, as a fraction in `0.0..=1.0`.
///
/// The positional axes have `Layout::map_x`. Every *other* continuous channel —
/// a color ramp, a radius, an opacity — needs the identical question answered,
/// and used to answer it inline in four places with `(v - mn) / span`. Giving it
/// one owner is what let the log scale reach all of them at once instead of
/// three times: `color`, `size` and `opacity` differ in what they do with the
/// fraction, not in how they compute it.
pub struct ChannelScale {
    /// Range ends, already in scale space — log positions when `base` is set.
    lo: f64,
    hi: f64,
    base: Option<f64>,
}

impl ChannelScale {
    /// Build the scale a binding asks for: its log base and its stated domain.
    ///
    /// Takes the whole `ChannelDef` rather than a base, because the two
    /// questions travel together and a caller that answered one and forgot the
    /// other would give a ramp a different domain from the axis beside it. That
    /// was a real risk with eight call sites — the compiler now asks both at
    /// once.
    pub fn of(col: &[f64], def: Option<&ChannelDef>) -> Self {
        Self::of_parts(col, log_of(def), domain_of(def))
    }

    /// The mechanism under [`of`](Self::of): a column's own range, then the
    /// stated ends laid over whichever of them were given.
    ///
    /// Values a log scale cannot place are skipped when finding the ends, the
    /// same way `channel_range_eff` skips them on an axis — one unplaceable row
    /// must not drag a whole ramp to infinity.
    ///
    /// The stated ends arrive in the **data's own units** and are converted into
    /// scale space here, which is what makes `color(gdp, scale = "log", limits =
    /// c(100, 1e5))` mean what it reads as: dollars, on a decade ramp.
    pub fn of_parts(col: &[f64], base: Option<f64>, limits: (Option<f64>, Option<f64>)) -> Self {
        let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
        for &v in col {
            let x = match base {
                Some(b) => to_log(v, b),
                None => v,
            };
            if x.is_finite() {
                if x < lo { lo = x }
                if x > hi { hi = x }
            }
        }
        if !lo.is_finite() {
            lo = 0.0;
            hi = 1.0;
        }
        // A stated end wins outright — that is the whole point of stating it —
        // and each end is independent, so a half-stated domain leaves the other
        // to the data.
        let to_scale = |v: f64| match base {
            Some(b) => to_log(v, b),
            None => v,
        };
        if let Some(l) = limits.0 {
            let l = to_scale(l);
            if l.is_finite() { lo = l }
        }
        if let Some(h) = limits.1 {
            let h = to_scale(h);
            if h.is_finite() { hi = h }
        }
        Self { lo, hi, base }
    }

    /// A scale for a channel that is not bound — everything sits at the bottom.
    pub fn unbound() -> Self {
        Self { lo: 0.0, hi: 1.0, base: None }
    }

    /// Where `v` sits between the ends. `NaN` when a log scale cannot place it,
    /// which the caller must skip rather than draw.
    pub fn fraction(&self, v: f64) -> f64 {
        let x = match self.base {
            Some(b) => to_log(v, b),
            None => v,
        };
        (x - self.lo) / (self.hi - self.lo).max(1e-12)
    }

    /// The value a given fraction of the way along — the inverse of `fraction`,
    /// used by legends to label what a swatch actually stands for.
    pub fn value_at(&self, f: f64) -> f64 {
        let x = self.lo + f * (self.hi - self.lo);
        match self.base {
            Some(b) => from_log(x, b),
            None => x,
        }
    }

    pub fn min(&self) -> f64 { self.value_at(0.0) }
    pub fn max(&self) -> f64 { self.value_at(1.0) }

    /// The value at the halfway point.
    ///
    /// On a log scale this is the **geometric** mean, not the arithmetic one —
    /// which is the whole reason it is a method rather than `(min + max) / 2`
    /// at each call site. A legend's middle label has to name the color painted
    /// at the middle of the strip, and on a log ramp that is √(min·max).
    pub fn mid(&self) -> f64 { self.value_at(0.5) }
}

/// Rows a log scale has no position for, when there are any.
pub struct Unplaceable {
    /// How many values are zero or negative.
    pub count: usize,
    /// How many values there are in total, so the message can say "3 of 142".
    pub total: usize,
    /// The smallest offending value — the concrete thing to go and look at.
    pub smallest: f64,
}

/// Count the values in `field` that a log scale cannot place.
///
/// `None` means every value is placeable, which is the answer a caller wants to
/// branch on. A missing or non-numeric column is also `None`: it is a different
/// defect, reported by a different rule.
pub fn unplaceable(df: &DataFrame, field: &str) -> Option<Unplaceable> {
    let vals = df.float_col(field)?;
    let bad: Vec<f64> = vals.iter().copied().filter(|v| *v <= 0.0 || !v.is_finite()).collect();
    if bad.is_empty() {
        return None;
    }
    Some(Unplaceable {
        count: bad.len(),
        total: vals.len(),
        smallest: bad.iter().copied().fold(f64::INFINITY, f64::min),
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::ChannelDef;

    #[test]
    fn log_positions_are_decades() {
        assert_eq!(to_log(1.0, 10.0), 0.0);
        assert_eq!(to_log(10.0, 10.0), 1.0);
        // Exactly 3.0, not 2.9999999999999996 — `ln v / ln b` would lose this,
        // and the axis would round out to a whole extra decade.
        assert_eq!(to_log(1000.0, 10.0), 3.0);
        assert!((to_log(0.01, 10.0) - -2.0).abs() < 1e-12);
    }

    #[test]
    fn base_two_counts_doublings() {
        assert_eq!(to_log(1.0, 2.0), 0.0);
        assert_eq!(to_log(8.0, 2.0), 3.0);
        assert_eq!(to_log(1024.0, 2.0), 10.0);
    }

    #[test]
    fn base_e_counts_e_foldings() {
        assert!((to_log(E, E) - 1.0).abs() < 1e-12);
        assert!((to_log(E * E, E) - 2.0).abs() < 1e-12);
    }

    #[test]
    fn a_logarithm_has_no_value_at_zero_or_below() {
        for b in [10.0, 2.0, E] {
            assert!(to_log(0.0, b).is_nan());
            assert!(to_log(-5.0, b).is_nan());
        }
    }

    #[test]
    fn from_log_undoes_to_log_in_any_base() {
        for b in [10.0, 2.0, E] {
            for v in [1.0_f64, 2.5, 10.0, 999.0, 1e6] {
                assert!((from_log(to_log(v, b), b) - v).abs() < v * 1e-9,
                        "base {b} value {v}");
            }
        }
    }

    #[test]
    fn every_base_puts_the_data_in_the_same_relative_place() {
        // The claim that makes the base cosmetic: log_b differs from log10 by a
        // constant factor, and the renderer normalizes by the axis range, so the
        // factor cancels. Checked here rather than asserted in prose.
        let vals = [1.0_f64, 7.0, 50.0, 900.0];
        let frac = |b: f64| {
            let p: Vec<f64> = vals.iter().map(|&v| to_log(v, b)).collect();
            let (lo, hi) = (p[0], p[p.len() - 1]);
            p.iter().map(|x| (x - lo) / (hi - lo)).collect::<Vec<_>>()
        };
        let ten = frac(10.0);
        for b in [2.0, E, 1.5] {
            for (a, c) in ten.iter().zip(frac(b).iter()) {
                assert!((a - c).abs() < 1e-12, "base {b} moved a point");
            }
        }
    }

    #[test]
    fn an_unusable_base_falls_back_rather_than_dividing_by_zero() {
        // log base 1 is undefined and log base 0 worse. `legality` refuses both,
        // but the renderer must not produce infinities if one slips through.
        for bad in [1.0, 0.0, -2.0, f64::NAN] {
            let def = ChannelDef::field("v").with_base(bad);
            assert_eq!(log_base(Some(&def)), DEFAULT_LOG_BASE);
        }
    }

    #[test]
    fn is_log_reads_the_binding() {
        let plain = ChannelDef::field("gdp");
        let logged = ChannelDef::field("gdp").with_scale(ScaleType::Log);
        let timed = ChannelDef::field("gdp").with_scale(ScaleType::Time);
        assert!(!is_log(None));
        assert!(!is_log(Some(&plain)));
        assert!(!is_log(Some(&timed)));
        assert!(is_log(Some(&logged)));
    }

    #[test]
    fn log_column_leaves_the_other_columns_alone() {
        let df = DataFrame::new()
            .with_float("gdp", vec![1.0, 100.0])
            .with_float("life", vec![50.0, 80.0])
            .with_str("country", vec!["A".into(), "B".into()]);

        let out = log_column(&df, "gdp", 10.0);
        assert_eq!(out.float_col("gdp").unwrap(), &vec![0.0, 2.0]);
        assert_eq!(out.float_col("life").unwrap(), &vec![50.0, 80.0]);
        assert_eq!(out.str_col("country").unwrap().len(), 2);
    }

    #[test]
    fn a_linear_scale_washes_out_a_column_spread_over_decades() {
        // The complaint the manual carried for two sessions, as a number. Thirty
        // values spread evenly over four decades: on a linear scale the median
        // one sits in the bottom 2% of the range, so nearly every mark gets the
        // same color, radius or opacity. On a log scale it sits in the middle,
        // which is what the reader needs to see a difference.
        let col: Vec<f64> = (0..30).map(|i| 10f64.powf(5.0 + i as f64 * 4.0 / 29.0)).collect();
        let mid = col[col.len() / 2];

        assert!(ChannelScale::of_parts(&col, None, (None, None)).fraction(mid) < 0.02);
        assert!((ChannelScale::of_parts(&col, Some(10.0), (None, None)).fraction(mid) - 0.5).abs() < 0.05);
    }

    #[test]
    fn the_middle_of_a_log_scale_is_the_geometric_mean() {
        // Not `(min + max) / 2`. A legend's middle label has to name the color
        // painted half way along the strip, and on a log ramp that is √(min·max).
        let sc = ChannelScale::of_parts(&[100.0, 10_000.0], Some(10.0), (None, None));
        assert!((sc.mid() - 1000.0).abs() < 1e-6, "got {}", sc.mid());
        assert_eq!(ChannelScale::of_parts(&[100.0, 10_000.0], None, (None, None)).mid(), 5050.0);
    }

    #[test]
    fn a_value_a_log_channel_cannot_place_yields_no_fraction() {
        let sc = ChannelScale::of_parts(&[1.0, 100.0], Some(10.0), (None, None));
        assert!(sc.fraction(0.0).is_nan());
        assert!(sc.fraction(-1.0).is_nan());
        // And one unplaceable row must not drag the ends to infinity.
        let sc = ChannelScale::of_parts(&[1.0, 0.0, 100.0], Some(10.0), (None, None));
        assert_eq!(sc.min(), 1.0);
        assert!((sc.max() - 100.0).abs() < 1e-9);
    }

    #[test]
    fn unplaceable_counts_and_names_the_worst_offender() {
        let df = DataFrame::new().with_float("v", vec![1.0, 0.0, -7.0, 10.0]);
        let u = unplaceable(&df, "v").expect("two values cannot be placed");
        assert_eq!(u.count, 2);
        assert_eq!(u.total, 4);
        assert_eq!(u.smallest, -7.0);
    }

    #[test]
    fn all_positive_is_placeable() {
        let df = DataFrame::new().with_float("v", vec![0.5, 1.0, 1e9]);
        assert!(unplaceable(&df, "v").is_none());
    }

    // -- limits: the stated domain (spec §10) --------------------------------

    fn limited(lo: Option<f64>, hi: Option<f64>) -> ChannelDef {
        ChannelDef::field("v").with_limits(lo, hi)
    }

    #[test]
    fn a_stated_end_replaces_the_data_and_the_other_end_survives() {
        // The half-stated domain: `c(0, NA)` pins the baseline and leaves the
        // top to the data, which is the shape a proportion chart wants.
        let col = vec![3.0, 7.0, 9.0];
        let sc = ChannelScale::of(&col, Some(&limited(Some(0.0), None)));
        assert_eq!(sc.min(), 0.0, "the stated end wins");
        assert_eq!(sc.max(), 9.0, "the unstated end is still the data's");
    }

    #[test]
    fn a_stated_domain_extends_as_readily_as_it_restricts() {
        // The forcing case's direction: hours observed 1..22 span the whole day
        // once the day is stated, and nothing is excluded to do it.
        let col = vec![1.0, 10.0, 22.0];
        let sc = ChannelScale::of(&col, Some(&limited(Some(0.0), Some(24.0))));
        assert_eq!((sc.min(), sc.max()), (0.0, 24.0));
        assert!(col.iter().all(|&v| within_limits(Some(&limited(Some(0.0), Some(24.0))), v)));
    }

    #[test]
    fn a_stated_domain_on_a_log_channel_is_read_in_the_datas_units() {
        // `limits = c(100, 100000)`, not `c(2, 5)` — the ticks read in data
        // units and the domain has to agree with them or the two disagree about
        // the same axis.
        let def = ChannelDef::field("v").with_scale(ScaleType::Log)
            .with_limits(Some(100.0), Some(100_000.0));
        let sc = ChannelScale::of(&[500.0, 5000.0], Some(&def));
        assert!((sc.min() - 100.0).abs() < 1e-9);
        assert!((sc.max() - 100_000.0).abs() < 1e-6);
        // And the middle of that ramp is the geometric mean, as it is without limits.
        assert!((sc.mid() - 3162.27766).abs() < 1e-3, "got {}", sc.mid());
    }

    #[test]
    fn both_ends_are_inclusive() {
        let d = limited(Some(0.0), Some(24.0));
        assert!(within_limits(Some(&d), 0.0), "midnight at the low end is in");
        assert!(within_limits(Some(&d), 24.0), "midnight at the high end is in");
        assert!(!within_limits(Some(&d), 24.5));
        assert!(!within_limits(Some(&d), -0.5));
    }

    #[test]
    fn a_backwards_domain_is_not_a_domain_and_nothing_acts_on_it() {
        // `check_limits` refuses `c(20, 5)` by name. Until it does, nothing may
        // filter against it — an emptied frame would then be reported as "your
        // limits leave no rows", a fact about the data standing in for a typo.
        let d = limited(Some(20.0), Some(5.0));
        assert_eq!(domain_of(Some(&d)), (None, None));
        assert!(!has_limits(Some(&d)));
        assert!(within_limits(Some(&d), 1.0), "nothing is excluded by a non-domain");
        // The raw read still reports what was written, which is what the
        // refusal needs to quote back.
        assert_eq!(limits_of(Some(&d)), (Some(20.0), Some(5.0)));
        // Zero width is the same failure.
        assert_eq!(domain_of(Some(&limited(Some(5.0), Some(5.0)))), (None, None));
    }

    #[test]
    fn saying_nothing_states_no_domain() {
        assert!(!has_limits(None));
        assert!(!has_limits(Some(&ChannelDef::field("v"))));
        assert!(!has_limits(Some(&limited(None, None))), "`c(NA, NA)` says nothing");
    }
}
