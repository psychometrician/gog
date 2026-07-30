//! How a number becomes a date.
//!
//! The engine's temporal unit is **seconds since 1970-01-01**, always. An R
//! `Date` is days since that epoch and a `POSIXct` is seconds; the binding
//! multiplies days out before the wire, so by the time a value reaches this
//! crate there is exactly one unit and no code path ever asks which. What a
//! column *keeps* is its declared resolution — [`TimeUnit`] — because a column
//! of dates must never grow ticks at 06:00: the resolution says how finely the
//! calendar may be cut, not how the number is stored.
//!
//! The engine is **timezone-naive** on purpose. A timestamp arrives as the
//! civil time the user saw in R — the binding converts before serializing —
//! and is formatted back with no offset arithmetic. Time zones are a property
//! of the *reading* of a moment, and the reading already happened on the R
//! side; carrying a zone through the engine would mean two places could
//! disagree about what the clock said.
//!
//! Calendar arithmetic is the standard civil-calendar algorithm (proleptic
//! Gregorian), implemented here rather than imported: the workspace depends on
//! `serde` and nothing else, and twenty lines of well-tested integer math do
//! not justify a date crate.

use serde::{Deserialize, Serialize};

/// Seconds in one civil day.
pub const SECS_PER_DAY: f64 = 86_400.0;

/// The resolution a temporal column was declared at.
///
/// This is the second piece of the missing type layer (`levels` on a text
/// column was the first): "this number is a moment in time" is exactly what
/// the absent temporal type would carry, arriving against the concrete bug
/// that a `Date` used to reach the engine as the category string
/// `"2026-01-02"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TimeUnit {
    /// A calendar date — `as.Date` in R. Ticks never subdivide a day.
    Day,
    /// A timestamp — `POSIXct` in R. Ticks may run down to seconds.
    Second,
}

// ---------------------------------------------------------------------------
// Civil calendar — days since epoch ↔ (year, month, day)
// ---------------------------------------------------------------------------

/// (year, month 1–12, day 1–31) of a count of days since 1970-01-01.
///
/// Howard Hinnant's `civil_from_days`, exact over the whole proleptic
/// Gregorian calendar — leap years, century rules and all.
pub fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097; // day of era     [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // day of year, Mar-based
    let mp = (5 * doy + 2) / 153; // Mar = 0
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Days since 1970-01-01 of a civil (year, month 1–12, day 1–31).
pub fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400; // [0, 399]
    let mp = ((m + 9) % 12) as i64; // Mar = 0
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Whole days since epoch of a moment in seconds, rounding toward −∞ so that
/// 23:59 belongs to its own day, not the next one.
pub fn day_of(secs: f64) -> i64 {
    (secs / SECS_PER_DAY).floor() as i64
}

/// Seconds past midnight of a moment, in `[0, 86400)`.
pub fn time_of_day(secs: f64) -> f64 {
    secs - day_of(secs) as f64 * SECS_PER_DAY
}

/// Three-letter English month names, January first.
///
/// English and only English, for the same reason the color vocabulary is
/// CSS-only: one set of words that every binding shares, rather than a locale
/// negotiation between R, the engine, and the reader.
pub const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun",
    "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

// ---------------------------------------------------------------------------
// Formatting — a moment as a reader sees it
// ---------------------------------------------------------------------------

/// A moment as a self-contained label: `2024-03-04`, or with a clock when the
/// column resolves below a day: `2024-03-04 14:05`.
///
/// ISO order, because this form is for labels that stand *alone* — a legend
/// row, a diagnostic. An axis tick has neighbors to borrow context from, so
/// it can afford the friendlier `Mar 4`; a label with no neighbors has to
/// carry its whole date, and `2024-03-04` does that in the fewest characters
/// that cannot be misread.
pub fn fmt_moment(secs: f64, unit: TimeUnit) -> String {
    let (y, m, d) = civil_from_days(day_of(secs));
    match unit {
        TimeUnit::Day => format!("{y:04}-{m:02}-{d:02}"),
        TimeUnit::Second => {
            let tod = time_of_day(secs).round() as i64;
            let (h, min, s) = (tod / 3600, (tod % 3600) / 60, tod % 60);
            if s == 0 {
                format!("{y:04}-{m:02}-{d:02} {h:02}:{min:02}")
            } else {
                format!("{y:04}-{m:02}-{d:02} {h:02}:{min:02}:{s:02}")
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_epoch_is_day_zero() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(civil_from_days(0), (1970, 1, 1));
    }

    #[test]
    fn civil_round_trips_across_leap_rules() {
        // 2000 is a leap year (divisible by 400), 1900 is not (century rule),
        // 2024 is (plain fourth year). The round trip has to survive all three.
        for (y, m, d) in [
            (2000, 2, 29),
            (1900, 2, 28),
            (1900, 3, 1),
            (2024, 2, 29),
            (2024, 12, 31),
            (1969, 12, 31), // the day before the epoch is day -1
            (1600, 7, 15),
            (2100, 1, 1),
        ] {
            let days = days_from_civil(y, m, d);
            assert_eq!(civil_from_days(days), (y, m, d), "{y}-{m}-{d}");
        }
        assert_eq!(days_from_civil(1969, 12, 31), -1);
    }

    #[test]
    fn known_dates_land_on_known_days() {
        // 2026-07-22 is 20,656 days after the epoch — checked against R's
        // as.numeric(as.Date("2026-07-22")).
        assert_eq!(days_from_civil(2026, 7, 22), 20_656);
        // 1970-01-05 was the first Monday of the epoch.
        assert_eq!(days_from_civil(1970, 1, 5), 4);
    }

    #[test]
    fn a_moment_before_midnight_belongs_to_its_own_day() {
        let secs = days_from_civil(2024, 3, 4) as f64 * SECS_PER_DAY - 60.0; // 23:59 on Mar 3
        assert_eq!(civil_from_days(day_of(secs)), (2024, 3, 3));
        assert_eq!(time_of_day(secs), 86_340.0);
    }

    #[test]
    fn a_date_formats_without_a_clock_and_a_timestamp_with_one() {
        let noon = days_from_civil(2024, 3, 4) as f64 * SECS_PER_DAY + 12.0 * 3600.0;
        assert_eq!(fmt_moment(noon, TimeUnit::Day), "2024-03-04");
        assert_eq!(fmt_moment(noon, TimeUnit::Second), "2024-03-04 12:00");
        // Seconds appear only when they carry information.
        assert_eq!(fmt_moment(noon + 90.0, TimeUnit::Second), "2024-03-04 12:01:30");
    }
}
