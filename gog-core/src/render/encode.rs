//! How a channel *fraction* becomes a visual attribute — the opacity a value
//! maps to, the radius a value maps to.
//!
//! Shared by the marks that *draw* the attribute and the legend that *decodes*
//! it, so it lives below both: neither the renderer nor the legend owns the
//! size/opacity vocabulary. `ChannelScale` (in `scale`) answers *where* a value
//! sits on its channel (`0..1`); these turn that fraction into pixels of radius
//! or a fill opacity — the one place `size` and `opacity` agree what a fraction
//! means, so they cannot drift apart.

// ---------------------------------------------------------------------------
// Opacity scale
//
// The default when no `opacity` channel is bound. Mapped values run to
// OPACITY_MAX rather than 1.0, and start at OPACITY_MIN rather than 0.0 —
// a fully transparent mark is invisible, which is never what a reader wants.
// ---------------------------------------------------------------------------

pub(crate) const OPACITY_DEFAULT: f64 = 0.82;
const OPACITY_MIN: f64 = 0.15;
const OPACITY_MAX: f64 = 0.95;

/// How far the rows outside a selection are pushed back.
///
/// A constant rather than vocabulary, and the same class of decision as the
/// default point radius: one sensible value exists, so §12 lets it stay silent.
/// It sits here because this module already owns the question *what does a
/// visual attribute mean* for `size` and `opacity`, and a third answer belongs
/// beside the first two rather than in whichever mark first needed it.
///
/// Applied as **group** opacity over the unselected pass, never multiplied into
/// each element. That keeps it composable with whatever opacity a mark already
/// resolved, keeps overlapping marks from darkening each other, and is why no
/// mark writer has to learn that selection exists at all.
///
/// 0.15 is low enough that the selection reads at a glance and high enough that
/// the rest is still a visible cloud — the point of dimming rather than hiding
/// is that a selection is read *against* what it was taken from.
pub(crate) const SELECTION_DIM: f64 = 0.15;

// ---------------------------------------------------------------------------
// Size scale constants
// ---------------------------------------------------------------------------

pub(crate) const SIZE_MIN_R: f64 = 3.0;
pub(crate) const SIZE_MAX_R: f64 = 12.0;

/// Turn a channel fraction into an opacity.
///
/// Takes the fraction rather than the value and its range: *where* a value sits
/// on its channel is `ChannelScale`'s question, and asking it here too is how
/// `size` and `opacity` would come to disagree about what a log scale means.
pub(crate) fn opacity_at(f: f64) -> f64 {
    OPACITY_MIN + held(f) * (OPACITY_MAX - OPACITY_MIN)
}

/// Turn a channel fraction into a point radius.
pub(crate) fn radius_at(f: f64) -> f64 {
    SIZE_MIN_R + held(f) * (SIZE_MAX_R - SIZE_MIN_R)
}

/// Hold a fraction inside `0..=1`, and give a value with no place the least ink.
///
/// A fraction can arrive outside the unit interval: `limit_cut` deliberately
/// keeps the rows on the axis a transform *writes*, so a transform output past a
/// stated `limits` reaches here. Unclamped, that becomes an opacity above 1 or a
/// negative radius, and a NaN becomes `r="NaN"` — SVG nothing renders. Clamping
/// is this module's job because it is the one place `size` and `opacity` agree
/// what a fraction means.
fn held(f: f64) -> f64 {
    if f.is_nan() { 0.0 } else { f.clamp(0.0, 1.0) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fraction_outside_the_unit_interval_is_held_at_the_ends() {
        assert_eq!(opacity_at(-0.5), opacity_at(0.0));
        assert_eq!(opacity_at(1.5), opacity_at(1.0));
        assert_eq!(radius_at(-0.5), radius_at(0.0));
        assert_eq!(radius_at(1.5), radius_at(1.0));
        assert_eq!(radius_at(f64::INFINITY), radius_at(1.0));
    }

    #[test]
    fn a_nan_fraction_gets_the_least_ink_rather_than_a_nan_attribute() {
        assert_eq!(opacity_at(f64::NAN), opacity_at(0.0));
        assert_eq!(radius_at(f64::NAN), radius_at(0.0));
        assert!(radius_at(f64::NAN).is_finite());
    }
}
