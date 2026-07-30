/// Tick generation using Wilkinson's "nice numbers" algorithm.
///
/// Always produces round, human-readable tick values — e.g. 0, 10, 20, 30
/// instead of 0, 8.33, 16.67, 25. The scale range is extended to the outermost
/// tick so that all ticks align exactly with the axis.

#[derive(Clone)]
pub struct TickSpec {
    /// Tick values in ascending order (covers a slightly wider range than the data).
    pub values: Vec<f64>,
    /// Pre-formatted label for each tick value.
    pub labels: Vec<String>,
    /// The step size between ticks (used internally for formatting).
    pub step: f64,
}

impl TickSpec {
    /// First tick value — use as the scale minimum.
    pub fn scale_min(&self) -> f64 {
        self.values.first().copied().unwrap_or(0.0)
    }
    /// Last tick value — use as the scale maximum.
    pub fn scale_max(&self) -> f64 {
        self.values.last().copied().unwrap_or(1.0)
    }
}

/// Generate nice tick values for the range [data_min, data_max].
/// `target_count` is the desired number of ticks (typically 5).
pub fn nice_ticks(data_min: f64, data_max: f64, target_count: usize) -> TickSpec {
    let target = target_count.max(2);

    // Degenerate: all values equal
    if (data_max - data_min).abs() < 1e-12 {
        let v = data_min;
        let step = 1.0_f64;
        return TickSpec {
            values: vec![v - 1.0, v, v + 1.0],
            labels: vec![
                format_tick(v - 1.0, step),
                format_tick(v, step),
                format_tick(v + 1.0, step),
            ],
            step,
        };
    }

    let range = nice_number((data_max - data_min).abs(), false);
    let step = nice_number(range / (target - 1) as f64, true);
    let scale_min = (data_min / step).floor() * step;
    let scale_max = (data_max / step).ceil() * step;

    let mut values: Vec<f64> = Vec::new();
    let mut v = scale_min;
    let eps = step * 1e-9;
    while v <= scale_max + eps {
        // Snap to an exact multiple to eliminate floating-point drift.
        let snapped = (v / step).round() * step;
        values.push(snapped);
        v += step;
        if values.len() > 25 {
            break;
        }
    }

    let labels = values.iter().map(|&t| format_tick(t, step)).collect();
    TickSpec { values, labels, step }
}

/// Generate ticks for a log scale, given the range in log positions.
///
/// The values are positions (so the renderer maps them like any other number),
/// but the **labels are the original quantities**. That split is the whole point
/// of a log scale as opposed to logging the column: an axis whose ticks read
/// 0, 1, 2, 3 has moved the arithmetic into the reader's head, which is the work
/// a scale exists to do for them.
///
/// The range is widened to whole powers of `base`, so the axis begins and ends
/// on a round quantity — the log counterpart of what `nice_ticks` does with
/// 1-2-5.
pub fn log_ticks(log_min: f64, log_max: f64, base: f64) -> TickSpec {
    let lo = if log_min.is_finite() { log_min.floor() } else { 0.0 };
    let hi = match log_max.is_finite() {
        true if log_max.ceil() > lo => log_max.ceil(),
        // A single distinct value has no span; give it one power to sit in.
        _ => lo + 1.0,
    };
    let powers = (hi - lo).round() as i64;

    let mut values: Vec<f64> = Vec::new();
    if base == 10.0 && powers <= 2 {
        // One or two decades is too few for decade-only ticks — 1, 10, 100 on
        // its own leaves the axis nearly bare. Fill in with 2 and 5, the same
        // 1-2-5 progression `nice_number` walks on a linear axis.
        //
        // Base 10 only, and not as a special case: a decade is a factor of ten,
        // which is coarse enough to want subdividing. A doubling is not, and
        // there is no comparable subdivision of one — 1, 1.5, 2 is nobody's
        // idea of a gridline.
        for k in (lo as i64)..(hi as i64) {
            for m in [1.0, 2.0, 5.0] {
                values.push((m * 10f64.powi(k as i32)).log10());
            }
        }
    } else {
        // Whole powers, thinned when there are more than an axis can label.
        let step = ((powers as f64) / 8.0).ceil().max(1.0) as i64;
        let mut k = lo as i64;
        while (k as f64) < hi {
            values.push(k as f64);
            k += step;
        }
    }
    // Whatever the stride left off at, the axis ends at its maximum.
    values.push(hi);

    let labels = values.iter()
        .map(|&p| format_log_tick(base.powf(p), base, p))
        .collect();
    TickSpec { values, labels, step: 1.0 }
}

/// Generate ticks for a time scale, given the range in epoch seconds.
///
/// The values are seconds (so the renderer maps them like any other number),
/// but the ticks land on **calendar boundaries** — Jan 1, the first of a month,
/// a Monday, midnight — and the labels read as dates. Round numbers of seconds
/// are nobody's gridlines: 1.5e9 is a quantity, not a moment.
///
/// One rule for the labels, stated once:
///
/// > **A tick is labeled at its own resolution; context it shares with its
/// > neighbors is not repeated on every one of them.**
///
/// Year ticks read `1994`; month ticks `Jan 2024` (a bare `Jan` recurs every
/// year, so the year is not shared context); day ticks `Mar 4`, because a
/// day-stepped axis spans weeks and the year genuinely is shared; clock ticks
/// `14:00`, except at midnight, where the clock says nothing and the tick
/// borrows the day's name — `Mar 5`.
///
/// `unit` is the column's declared resolution: a `Date` column never grows
/// ticks at 06:00, however narrow its range.
pub fn time_ticks(min_s: f64, max_s: f64, unit: crate::time::TimeUnit) -> TickSpec {
    use crate::time::SECS_PER_DAY;

    // A single distinct moment has no span; give it one interval to sit in.
    let (min_s, max_s) = if (max_s - min_s).abs() < 1e-9 {
        match unit {
            crate::time::TimeUnit::Day => (min_s - SECS_PER_DAY, max_s + SECS_PER_DAY),
            crate::time::TimeUnit::Second => (min_s - 3600.0, max_s + 3600.0),
        }
    } else {
        (min_s, max_s)
    };

    let interval = choose_time_interval(max_s - min_s, unit);
    let values = time_tick_values(min_s, max_s, interval);
    let labels = values.iter().map(|&v| time_tick_label(v, interval)).collect();
    let step = if values.len() > 1 { values[1] - values[0] } else { SECS_PER_DAY };
    TickSpec { values, labels, step }
}

/// A calendar stride: the unit a time axis steps by, and how many of it.
#[derive(Clone, Copy, Debug, PartialEq)]
enum TimeInterval {
    Years(i64),
    Months(i64),
    Days(i64),
    Hours(i64),
    Minutes(i64),
    Seconds(i64),
}

/// Pick the finest calendar stride that keeps the axis under ~8 ticks.
///
/// The candidate steps are the calendar's own habits — quarters not fifths of
/// a year, weeks not decads — which is the whole difference from
/// `nice_number`'s 1-2-5: the calendar is not decimal, and an axis that cuts
/// it in tenths of a year reads as nothing at all.
fn choose_time_interval(span: f64, unit: crate::time::TimeUnit) -> TimeInterval {
    use crate::time::SECS_PER_DAY;
    const YEAR: f64 = 365.2425 * SECS_PER_DAY;
    let fits = |secs: f64| span / secs <= 7.0;

    if unit == crate::time::TimeUnit::Second {
        for k in [1, 5, 15, 30] {
            if fits(k as f64) { return TimeInterval::Seconds(k) }
        }
        for k in [1, 5, 15, 30] {
            if fits(k as f64 * 60.0) { return TimeInterval::Minutes(k) }
        }
        for k in [1, 3, 6, 12] {
            if fits(k as f64 * 3600.0) { return TimeInterval::Hours(k) }
        }
    }
    for k in [1, 2, 7, 14] {
        if fits(k as f64 * SECS_PER_DAY) { return TimeInterval::Days(k) }
    }
    for k in [1, 2, 3, 6] {
        if fits(k as f64 * YEAR / 12.0) { return TimeInterval::Months(k) }
    }
    // Years walk the same 1-2-5 progression a linear axis does — the calendar
    // has no unit above the year, so decimal habits resume.
    let mut mag = 1i64;
    loop {
        for m in [1, 2, 5] {
            let k = m * mag;
            if fits(k as f64 * YEAR) { return TimeInterval::Years(k) }
        }
        match mag.checked_mul(10) {
            Some(next) => mag = next,
            None => return TimeInterval::Years(mag),
        }
    }
}

/// The tick moments themselves: first tick at or before `min_s`, last at or
/// after `max_s`, every one on a calendar boundary.
fn time_tick_values(min_s: f64, max_s: f64, interval: TimeInterval) -> Vec<f64> {
    use crate::time::{civil_from_days, day_of, days_from_civil, SECS_PER_DAY};

    let mut values = Vec::new();
    match interval {
        TimeInterval::Years(k) => {
            let (y0, _, _) = civil_from_days(day_of(min_s));
            let mut y = y0.div_euclid(k) * k;
            loop {
                let v = days_from_civil(y, 1, 1) as f64 * SECS_PER_DAY;
                values.push(v);
                if v >= max_s || values.len() > 40 { break }
                y += k;
            }
        }
        TimeInterval::Months(k) => {
            let (y0, m0, _) = civil_from_days(day_of(min_s));
            // Month counter from year zero; k divides 12, so flooring to a
            // multiple keeps January a tick and quarters starting in January.
            let mut mi = (y0 * 12 + (m0 as i64 - 1)).div_euclid(k) * k;
            loop {
                let (y, m) = (mi.div_euclid(12), mi.rem_euclid(12) as u32 + 1);
                let v = days_from_civil(y, m, 1) as f64 * SECS_PER_DAY;
                values.push(v);
                if v >= max_s || values.len() > 40 { break }
                mi += k;
            }
        }
        TimeInterval::Days(k) => {
            // Weekly strides land on Mondays — the calendar's own week
            // boundary — by anchoring to 1970-01-05, the epoch's first Monday.
            let anchor = if k % 7 == 0 { 4 } else { 0 };
            let mut d = anchor + (day_of(min_s) - anchor).div_euclid(k) * k;
            loop {
                let v = d as f64 * SECS_PER_DAY;
                values.push(v);
                if v >= max_s || values.len() > 40 { break }
                d += k;
            }
        }
        TimeInterval::Hours(k) | TimeInterval::Minutes(k) | TimeInterval::Seconds(k) => {
            let len = match interval {
                TimeInterval::Hours(_) => 3600.0,
                TimeInterval::Minutes(_) => 60.0,
                _ => 1.0,
            } * k as f64;
            // Every candidate step divides a day evenly, so flooring to a
            // multiple aligns to midnight of its own accord.
            let mut v = (min_s / len).floor() * len;
            loop {
                values.push(v);
                if v >= max_s || values.len() > 40 { break }
                v += len;
            }
        }
    }
    values
}

/// One time tick's label, at the interval's own resolution.
fn time_tick_label(secs: f64, interval: TimeInterval) -> String {
    use crate::time::{civil_from_days, day_of, time_of_day, MONTHS};
    let (y, m, d) = civil_from_days(day_of(secs));
    match interval {
        TimeInterval::Years(_) => y.to_string(),
        TimeInterval::Months(_) => format!("{} {y}", MONTHS[m as usize - 1]),
        TimeInterval::Days(_) => format!("{} {d}", MONTHS[m as usize - 1]),
        TimeInterval::Hours(_) | TimeInterval::Minutes(_) => {
            let tod = time_of_day(secs).round() as i64;
            if tod == 0 {
                // Midnight's clock face says nothing; the day's name does.
                format!("{} {d}", MONTHS[m as usize - 1])
            } else {
                format!("{:02}:{:02}", tod / 3600, (tod % 3600) / 60)
            }
        }
        TimeInterval::Seconds(_) => {
            let tod = time_of_day(secs).round() as i64;
            format!("{:02}:{:02}:{:02}", tod / 3600, (tod % 3600) / 60, tod % 60)
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Format one log-scale tick.
///
/// Separate from `format_tick` because that one takes its precision from the
/// step, and a log axis has no single step: the gap from 1 to 10 and the gap
/// from 1M to 10M are one tick apart and six orders of magnitude different.
///
/// One rule decides the form, rather than a table per base:
///
/// > **Label the quantity when it reads cleanly; otherwise label the power.**
///
/// Base 10 gives 1, 10, 100, 1K — always clean. Base 2 gives 1, 2, 4 … 1048576,
/// clean until it gets too wide, then `2²⁴`. Base *e* has no clean quantities at
/// all — 2.718, 7.389, 20.09 — so it always reads `e`, `e²`, `e³`, which is what
/// somebody counting e-foldings wanted in the first place.
fn format_log_tick(v: f64, base: f64, exponent: f64) -> String {
    // base^k is not always exact in binary, and a value landing on
    // 999.9999999999999 would print as "1000" while being bucketed as though it
    // were under a thousand. Snap to the round number first.
    let rounded = v.round();
    let v = if (v - rounded).abs() < v.abs() * 1e-9 { rounded } else { v };
    let a = v.abs();

    // Thousands only when the division is exact. 2^20 is 1048576, and calling
    // it "1M" would be a lie told to save four characters.
    if a >= 1.0 && v.fract() == 0.0 {
        for (mag, suffix) in [(1e9, "B"), (1e6, "M"), (1e3, "K")] {
            if a >= mag && (v % mag) == 0.0 {
                return format!("{:.0}{suffix}", v / mag);
            }
        }
    }
    if let Some(s) = short_decimal(v) {
        // Wide enough to crowd the axis is not "clean" — `2²⁴` beats 16777216.
        if s.len() <= 7 {
            return s;
        }
    }
    power_label(base, exponent)
}

/// The shortest decimal string that round-trips to `v`, if a short one does.
///
/// This is the whole readability test. `1000` survives at zero places and
/// `0.01` at two; `2.718281828…` survives at none, which is exactly why base
/// *e* falls through to power notation.
fn short_decimal(v: f64) -> Option<String> {
    for prec in 0..=3usize {
        let s = format!("{v:.prec$}");
        if let Ok(back) = s.parse::<f64>() {
            if (back - v).abs() <= v.abs() * 1e-9 {
                return Some(s);
            }
        }
    }
    None
}

/// `e²`, `2²⁴`, `10⁻⁴` — a tick named by its power rather than its value.
fn power_label(base: f64, exponent: f64) -> String {
    // A fill tick (base 10's 2 and 5) has no whole power, but it always has a
    // clean decimal, so it never reaches here. Guard anyway.
    if (exponent - exponent.round()).abs() > 1e-9 {
        return format!("{:.3}", base.powf(exponent));
    }
    let b = if (base - std::f64::consts::E).abs() < 1e-9 {
        "e".to_string()
    } else {
        short_decimal(base).unwrap_or_else(|| format!("{base:.3}"))
    };
    match exponent.round() as i64 {
        0 => "1".to_string(),
        1 => b,
        n => format!("{b}{}", superscript(n)),
    }
}

fn superscript(n: i64) -> String {
    const SUP: [char; 10] = ['⁰', '¹', '²', '³', '⁴', '⁵', '⁶', '⁷', '⁸', '⁹'];
    let mut s = String::new();
    if n < 0 {
        s.push('⁻');
    }
    for c in n.abs().to_string().chars() {
        s.push(SUP[c.to_digit(10).unwrap_or(0) as usize]);
    }
    s
}

/// Round `x` to the nearest "nice" number: a power of 10 multiplied by 1, 2, or 5.
fn nice_number(x: f64, round: bool) -> f64 {
    if x == 0.0 {
        return 1.0;
    }
    let exp = x.abs().log10().floor();
    let f = x / 10_f64.powf(exp);
    let nf = if round {
        if f < 1.5 {
            1.0
        } else if f < 3.0 {
            2.0
        } else if f < 7.0 {
            5.0
        } else {
            10.0
        }
    } else {
        if f <= 1.0 {
            1.0
        } else if f <= 2.0 {
            2.0
        } else if f <= 5.0 {
            5.0
        } else {
            10.0
        }
    };
    nf * 10_f64.powf(exp)
}

/// Format a single tick value given the step size.
/// Uses K/M suffixes for large ranges; shows only as many decimals as the step requires.
fn format_tick(v: f64, step: f64) -> String {
    let abs_step = step.abs();
    if abs_step >= 1_000_000.0 {
        format!("{:.0}M", v / 1_000_000.0)
    } else if abs_step >= 1_000.0 {
        format!("{:.0}K", v / 1_000.0)
    } else if abs_step >= 1.0 {
        format!("{:.0}", v)
    } else {
        // Number of decimal places = magnitude of step (e.g. step=0.5 → 1 dp, step=0.05 → 2 dp)
        let decimals = (-abs_step.log10().floor()) as usize;
        format!("{:.prec$}", v, prec = decimals.min(6))
    }
}

/// Build a `TickSpec` from explicit tick positions (used for bar charts, where
/// ticks belong under each bar rather than at "nice" numbers).
/// The step is inferred from the minimum spacing; labels are formatted accordingly.
pub fn ticks_at(values: Vec<f64>) -> TickSpec {
    let step = if values.len() > 1 {
        values
            .windows(2)
            .map(|w| (w[1] - w[0]).abs())
            .filter(|&d| d > 1e-12)
            .fold(f64::INFINITY, f64::min)
    } else {
        1.0
    };
    let step = if step.is_infinite() { 1.0 } else { step };
    let labels = values.iter().map(|&v| format_tick(v, step)).collect();
    TickSpec { values, labels, step }
}

/// Build a `TickSpec` with caller-supplied string labels — used for categorical
/// (string) x-axes where each tick label is a category name, not a number.
pub fn ticks_with_labels(values: Vec<f64>, labels: Vec<String>) -> TickSpec {
    let step = if values.len() > 1 {
        values
            .windows(2)
            .map(|w| (w[1] - w[0]).abs())
            .filter(|&d| d > 1e-12)
            .fold(f64::INFINITY, f64::min)
    } else {
        1.0
    };
    let step = if step.is_infinite() { 1.0 } else { step };
    TickSpec { values, labels, step }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_ticks_are_labeled_in_the_readers_units() {
        // The point of a log scale rather than a logged column: the reader sees
        // the quantities they supplied, not their exponents.
        let t = log_ticks(2.0, 5.0, 10.0);
        assert_eq!(t.labels, vec!["100", "1K", "10K", "100K"]);
        assert_eq!(t.values, vec![2.0, 3.0, 4.0, 5.0]);
    }

    #[test]
    fn the_axis_ends_on_whole_decades() {
        let t = log_ticks(2.3, 5.7, 10.0);
        assert_eq!(t.scale_min(), 2.0);
        assert_eq!(t.scale_max(), 6.0);
    }

    #[test]
    fn a_narrow_range_is_filled_in_with_two_and_five() {
        // 1, 10 alone would leave the axis nearly bare, so a short span gets the
        // same 1-2-5 progression a linear axis would.
        let t = log_ticks(0.0, 1.0, 10.0);
        assert_eq!(t.labels, vec!["1", "2", "5", "10"]);
    }

    #[test]
    fn sub_unit_decades_keep_their_places() {
        let t = log_ticks(-3.0, 0.0, 10.0);
        assert_eq!(t.labels, vec!["0.001", "0.01", "0.1", "1"]);
    }

    #[test]
    fn a_very_wide_range_is_thinned_but_still_ends_at_its_maximum() {
        let t = log_ticks(0.0, 20.0, 10.0);
        assert!(t.values.len() <= 10, "got {} ticks", t.values.len());
        assert_eq!(t.scale_min(), 0.0);
        assert_eq!(t.scale_max(), 20.0);
        // Ascending, so the renderer can map them without sorting.
        assert!(t.values.windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn a_single_distinct_value_still_gets_an_axis() {
        let t = log_ticks(2.0, 2.0, 10.0);
        assert!(t.scale_max() > t.scale_min());
        assert!(!t.labels.is_empty());
    }

    // -- other bases ------------------------------------------------------

    #[test]
    fn base_two_ticks_are_doublings() {
        // 1 to 32. Clean integers, so they read as quantities.
        let t = log_ticks(0.0, 5.0, 2.0);
        assert_eq!(t.labels, vec!["1", "2", "4", "8", "16", "32"]);
    }

    #[test]
    fn base_two_does_not_get_the_one_two_five_fill() {
        // The fill subdivides a factor of ten. A doubling needs no subdividing,
        // and 1, 1.5, 2 is nobody's idea of a gridline.
        let t = log_ticks(0.0, 2.0, 2.0);
        assert_eq!(t.labels, vec!["1", "2", "4"]);
    }

    #[test]
    fn base_e_is_labeled_in_powers_because_its_quantities_are_not_readable() {
        // 2.718, 7.389, 20.09 are the quantities. Nobody reads those off an
        // axis — which is the whole reason `e` needs power notation to be worth
        // having at all.
        let t = log_ticks(0.0, 3.0, std::f64::consts::E);
        assert_eq!(t.labels, vec!["1", "e", "e²", "e³"]);
    }

    #[test]
    fn a_thousands_suffix_is_only_used_when_it_is_exact() {
        // 2^20 is 1048576. Calling it "1M" would be a lie told to save four
        // characters, so base 2 keeps the integer.
        assert_eq!(format_log_tick(1048576.0, 2.0, 20.0), "1048576");
        // A real million still gets the suffix.
        assert_eq!(format_log_tick(1e6, 10.0, 6.0), "1M");
    }

    #[test]
    fn a_quantity_too_wide_to_read_falls_back_to_its_power() {
        // 2^24 = 16777216 — eight digits crowding the axis.
        assert_eq!(format_log_tick(16777216.0, 2.0, 24.0), "2²⁴");
    }

    #[test]
    fn negative_powers_keep_their_sign() {
        assert_eq!(power_label(10.0, -4.0), "10⁻⁴");
        assert_eq!(power_label(std::f64::consts::E, -1.0), "e⁻¹");
    }

    #[test]
    fn the_zeroth_power_is_one_not_e_to_the_nothing() {
        assert_eq!(power_label(std::f64::consts::E, 0.0), "1");
        assert_eq!(power_label(2.0, 1.0), "2");
    }

    // -- time -------------------------------------------------------------

    use crate::time::{days_from_civil, TimeUnit, SECS_PER_DAY};

    fn s(y: i64, m: u32, d: u32) -> f64 {
        days_from_civil(y, m, d) as f64 * SECS_PER_DAY
    }

    #[test]
    fn a_span_of_decades_gets_year_ticks_on_january_first() {
        let t = time_ticks(s(1991, 3, 15), s(2019, 8, 2), TimeUnit::Day);
        assert_eq!(t.labels, vec!["1990", "1995", "2000", "2005", "2010", "2015", "2020"]);
        // Every tick is a moment, and that moment is Jan 1.
        assert_eq!(t.values[1], s(1995, 1, 1));
        // The axis begins and ends on ticks, like every other scale.
        assert_eq!(t.scale_min(), s(1990, 1, 1));
        assert_eq!(t.scale_max(), s(2020, 1, 1));
    }

    #[test]
    fn a_span_of_months_names_the_months() {
        // 2-month strides anchor to the year — January stays a tick — so a
        // range opening in February still reads Jan, Mar, May, …
        let t = time_ticks(s(2023, 2, 10), s(2023, 11, 5), TimeUnit::Day);
        assert_eq!(
            t.labels,
            vec!["Jan 2023", "Mar 2023", "May 2023", "Jul 2023", "Sep 2023", "Nov 2023", "Jan 2024"]
        );
        // First of the month, not the 10th.
        assert_eq!(t.values[0], s(2023, 1, 1));
    }

    #[test]
    fn quarters_start_in_january_not_wherever_the_data_does() {
        // 3-month strides align to the year, so they read Jan, Apr, Jul, Oct
        // whatever month the range happens to begin in.
        let t = time_ticks(s(2023, 2, 10), s(2024, 6, 5), TimeUnit::Day);
        assert_eq!(t.labels[0], "Jan 2023");
        assert!(t.labels.contains(&"Apr 2023".to_string()), "got {:?}", t.labels);
    }

    #[test]
    fn a_span_of_weeks_ticks_on_mondays() {
        // 1970-01-05 was a Monday; every 7-day stride anchors there.
        let t = time_ticks(s(2024, 3, 6), s(2024, 4, 10), TimeUnit::Day);
        for &v in &t.values {
            let days = (v / SECS_PER_DAY) as i64;
            assert_eq!((days - 4).rem_euclid(7), 0, "{v} is not a Monday");
        }
        assert_eq!(t.labels[0], "Mar 4");
    }

    #[test]
    fn a_date_column_never_gets_clock_ticks() {
        // Two days of range: a timestamp column would tick in hours, but a
        // Date column resolves no finer than the day it names.
        let t = time_ticks(s(2024, 3, 4), s(2024, 3, 6), TimeUnit::Day);
        assert_eq!(t.labels, vec!["Mar 4", "Mar 5", "Mar 6"]);

        let t = time_ticks(s(2024, 3, 4), s(2024, 3, 6), TimeUnit::Second);
        assert!(t.labels.iter().any(|l| l.contains(':')), "got {:?}", t.labels);
    }

    #[test]
    fn clock_ticks_read_as_clock_times_except_at_midnight() {
        let noon = s(2024, 3, 4) + 12.0 * 3600.0;
        let t = time_ticks(noon, noon + 20.0 * 3600.0, TimeUnit::Second);
        assert_eq!(t.labels[0], "12:00");
        // Midnight's clock face says nothing — the tick borrows the day.
        assert!(t.labels.contains(&"Mar 5".to_string()), "got {:?}", t.labels);
    }

    #[test]
    fn a_single_date_still_gets_an_axis() {
        let t = time_ticks(s(2024, 3, 4), s(2024, 3, 4), TimeUnit::Day);
        assert!(t.scale_max() > t.scale_min());
        assert!(t.labels.len() >= 2);
    }

    #[test]
    fn time_ticks_are_ascending_and_bracket_the_data() {
        for (lo, hi) in [
            (s(1971, 6, 1), s(2026, 7, 22)),
            (s(2024, 1, 31), s(2024, 2, 2)),
            (s(1999, 12, 20), s(2000, 1, 10)), // across a century boundary
        ] {
            let t = time_ticks(lo, hi, TimeUnit::Day);
            assert!(t.scale_min() <= lo && t.scale_max() >= hi);
            assert!(t.values.windows(2).all(|w| w[0] < w[1]));
            assert!(t.values.len() <= 12, "got {} ticks", t.values.len());
        }
    }
}

/// Derive a human-readable axis label from a snake_case field name.
///
/// `"life_expectancy"` → `"Life Expectancy"`
/// `"gdp"` → `"Gdp"`  (override with `.x_label("GDP")` if needed)
pub fn auto_label(field: &str) -> String {
    field
        .split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().to_string() + chars.as_str(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
