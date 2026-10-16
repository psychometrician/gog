//! How a number is written as text: a label's always, and an axis's and a key's
//! when the sentence asks.
//!
//! **One writer for three places.** `label(life, decimals = 1)` printed with its
//! own rule, an axis tick with `ticks::format_tick`'s and a key row with the
//! legend's, so one column could be written three ways on one plot. The three
//! defaults stay as they are (ruled 2026-10-15: "only what is asked for", so no
//! plot drawn before moves). What a binding *states* — `decimals`, `short`,
//! `percent` — is written here, the same way wherever the column's numbers are
//! printed, which is what lets a binding's setting mean one thing.
//!
//! It depends on nothing but the IR, so the label writer, the tick writer and
//! the legend all read it from below.

use crate::ir::ChannelDef;

/// A whole number has no point; anything else keeps up to three places, with
/// the zeros it leaves dropped. A label's default.
pub(crate) fn fmt_label_num(v: f64) -> String {
    if !v.is_finite() {
        return String::new();
    }
    if v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{v:.0}")
    } else {
        let s = format!("{v:.3}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

/// `x` to `d` places, with no minus sign on a value that rounds to zero.
fn fixed(x: f64, d: u32) -> String {
    let s = format!("{:.*}", d as usize, x);
    if s.trim_start_matches('-').chars().all(|c| c == '0' || c == '.') {
        s.trim_start_matches('-').to_string()
    } else {
        s
    }
}

/// The zeros after the point dropped, and the point when nothing is left after it.
fn trimmed(s: String) -> String {
    if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.').to_string() } else { s }
}

/// The letters `short` writes, largest first, and the multiplier each stands for.
const UNITS: [(f64, &str); 5] = [(1e12, "T"), (1e9, "B"), (1e6, "M"), (1e3, "K"), (1.0, "")];

/// How a number in a label is written.
///
/// `decimals` fixes the places, so labels set side by side read as one column of
/// figures (`label(life, decimals = 1)`: 77 prints as 77.0). `short` writes a
/// large number with K, M, B or T, for thousands, millions, billions and trillions
/// (`label(population, short = TRUE)`: 3811953827 prints as 3.81B), keeping three
/// significant digits unless `decimals` gives the places after the point. With
/// neither, a whole number has no decimal point and any other keeps up to three
/// places ([`fmt_label_num`]). A value that rounds to zero keeps no minus sign,
/// and one that rounds up to a thousand of its unit is written in the next one:
/// 999,999 is 1M, not 1000K.
pub(crate) fn label_number(v: f64, places: Option<u32>, short: bool) -> String {
    if !v.is_finite() {
        return String::new();
    }
    if !short {
        return match places {
            Some(d) => fixed(v, d),
            None => fmt_label_num(v),
        };
    }
    // Three significant digits, trailing zeros dropped: 3.81, 586, 24.5, 0.0123.
    let significant = |x: f64| {
        if x == 0.0 {
            return "0".to_string();
        }
        let d = (2 - x.abs().log10().floor() as i32).clamp(0, 10) as u32;
        trimmed(fixed(x, d))
    };
    let write = |k: usize| {
        let (m, unit) = UNITS[k];
        let n = match places {
            Some(d) => fixed(v / m, d),
            None => significant(v / m),
        };
        format!("{n}{unit}")
    };
    // The largest unit that leaves at least 1 in front of its letter.
    let k = UNITS.iter().position(|(m, _)| v.abs() >= *m).unwrap_or(UNITS.len() - 1);
    let written = write(k);
    let front: f64 = written.trim_start_matches('-')
        .trim_end_matches(char::is_alphabetic).parse().unwrap_or(0.0);
    if front >= 1000.0 && k > 0 { write(k - 1) } else { written }
}

/// A number written as a percent: 0.25 is `25%`. `places` fixes the places after
/// the point. Without it a percent keeps one place, with the zero it leaves
/// dropped: a label prints a share to three places, 0.232, and the same number
/// as a percent is 23.2%, not 23.239%.
fn percent_text(v: f64, places: Option<u32>) -> String {
    if !v.is_finite() {
        return String::new();
    }
    let p = v * 100.0;
    match places {
        Some(d) => format!("{}%", fixed(p, d)),
        None => format!("{}%", trimmed(fixed(p, 1))),
    }
}

/// The places a step needs to be written exactly, so every tick on it reads
/// apart: none for 1 or 250, one for 0.5 or 2.5, two for 0.25 or 0.05, at most
/// six.
fn places_for(step: f64) -> u32 {
    if !(step.is_finite() && step > 0.0) {
        return 0;
    }
    (0..=6u32)
        .find(|&p| {
            let scaled = step * 10f64.powi(p as i32);
            (scaled - scaled.round()).abs() <= scaled.abs() * 1e-6
        })
        .unwrap_or(6)
}

/// The letter an axis's step chooses, and its multiplier: `ticks::format_tick`'s
/// tiers, so a `decimals` on an axis keeps the letter the axis would have used.
fn axis_tier(step: f64) -> (f64, &'static str) {
    let s = step.abs();
    UNITS.iter().copied().find(|(m, _)| *m > 1.0 && s >= *m).unwrap_or((1.0, ""))
}

/// The letter a key's row takes for `v`: the legend's own tiers, where a
/// thousand is written in full below ten thousand (`legend::fmt_value`).
fn key_tier(v: f64) -> (f64, &'static str) {
    let a = v.abs();
    match a {
        a if a >= 1e12 => (1e12, "T"),
        a if a >= 1e9 => (1e9, "B"),
        a if a >= 1e6 => (1e6, "M"),
        a if a >= 1e4 => (1e3, "K"),
        _ => (1.0, ""),
    }
}

/// What a binding states about how its column's numbers are written:
/// `decimals`, `short` and `percent`. Nothing stated is the default writer of
/// each place, untouched.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct NumberFormat {
    pub decimals: Option<u32>,
    pub short: bool,
    pub percent: bool,
}

impl NumberFormat {
    pub(crate) fn of(def: Option<&ChannelDef>) -> Self {
        match def {
            Some(d) => NumberFormat {
                decimals: d.decimals,
                short: d.short == Some(true),
                percent: d.percent == Some(true),
            },
            None => NumberFormat::default(),
        }
    }

    /// Did the binding state anything? A place that was told nothing keeps its
    /// own writer, so a plot without these arguments draws as it always did.
    pub(crate) fn is_stated(&self) -> bool {
        self.decimals.is_some() || self.short || self.percent
    }

    /// A label's number.
    pub(crate) fn label(&self, v: f64) -> String {
        if self.percent {
            percent_text(v, self.decimals)
        } else {
            label_number(v, self.decimals, self.short)
        }
    }

    /// A key row's number. `decimals` alone keeps the key's own letter and fixes
    /// the places; `short` and `percent` write as a label does.
    pub(crate) fn key(&self, v: f64) -> String {
        if !v.is_finite() {
            return String::new();
        }
        if self.percent {
            return percent_text(v, self.decimals);
        }
        if self.short {
            return label_number(v, self.decimals, true);
        }
        let (m, unit) = key_tier(v);
        format!("{}{unit}", fixed(v / m, self.decimals.unwrap_or(0)))
    }

    /// An axis tick's number, for a tick at `v` on an axis stepping by `step`.
    ///
    /// - `percent`: the value × 100 with `%`, places from the step unless stated,
    ///   and never a letter (no `5K%`).
    /// - `short`: each tick its own letter, with places from the step in that
    ///   unit, so 1,001,000 and 1,002,000 read 1.001M and 1.002M rather than one
    ///   `1M` twice. A lettered tick drops the zeros it leaves; a tick under a
    ///   thousand keeps the step's places, as the axis alone does.
    /// - `decimals` alone: the letter the step chose, with `d` places.
    pub(crate) fn tick(&self, v: f64, step: f64) -> String {
        if !v.is_finite() {
            return String::new();
        }
        let step = step.abs();
        if self.percent {
            let d = self.decimals.unwrap_or_else(|| places_for(step * 100.0));
            return format!("{}%", fixed(v * 100.0, d));
        }
        if self.short {
            if v.abs() < step * 1e-9 {
                return "0".to_string();
            }
            let (m, unit) = UNITS.iter().copied().find(|(m, _)| v.abs() >= *m).unwrap_or((1.0, ""));
            let n = fixed(v / m, self.decimals.unwrap_or_else(|| places_for(step / m)));
            let n = if self.decimals.is_none() && !unit.is_empty() { trimmed(n) } else { n };
            return format!("{n}{unit}");
        }
        let (m, unit) = axis_tier(step);
        if !unit.is_empty() && v.abs() < step * 1e-9 {
            return "0".to_string();
        }
        format!("{}{unit}", fixed(v / m, self.decimals.unwrap_or(0)))
    }

    /// The smallest step at which a stated `decimals` prints every tick apart, or
    /// `None` when the places come from the step and always do.
    ///
    /// `x(rate, decimals = 0)` on an axis stepping by 0.2 would print 0, 0, 0, 1,
    /// 1: a stated place count is a floor on the step, which `ticks::relabel`
    /// keeps by drawing fewer ticks rather than repeating one.
    pub(crate) fn min_step(&self, step: f64, values: &[f64]) -> Option<f64> {
        let d = self.decimals? as i32;
        let unit = if self.percent {
            0.01
        } else if self.short {
            values.iter()
                .filter_map(|v| UNITS.iter().find(|(m, _)| v.abs() >= *m).map(|(m, _)| *m))
                .fold(1.0, f64::max)
        } else {
            axis_tier(step).0
        };
        Some(unit * 10f64.powi(-d))
    }
}

#[cfg(test)]
mod tests {
    use super::NumberFormat;

    fn fmt(decimals: Option<u32>, short: bool, percent: bool) -> NumberFormat {
        NumberFormat { decimals, short, percent }
    }

    /// **A share axis written as a percent**: 0 to 0.6 at a step of 0.1 is 0% to
    /// 60%, whole percents because the step is ten of them, and a step of 0.025
    /// keeps one place.
    #[test]
    fn a_percent_axis_writes_its_shares_as_percents() {
        let pct = fmt(None, false, true);
        let ticks: Vec<String> = [0.0, 0.1, 0.2, 0.6].iter().map(|&v| pct.tick(v, 0.1)).collect();
        assert_eq!(ticks, ["0%", "10%", "20%", "60%"]);
        assert_eq!(pct.tick(0.025, 0.025), "2.5%");
        assert_eq!(fmt(Some(1), false, true).tick(0.2, 0.1), "20.0%");
        assert_eq!(pct.tick(50.0, 10.0), "5000%", "a percent never takes a letter");
        assert_eq!(pct.label(0.256), "25.6%");
        assert_eq!(pct.label(0.232394), "23.2%");
        assert_eq!(pct.label(0.25), "25%");
        assert_eq!(pct.key(0.07), "7%");
    }

    /// **`short` on an axis gives each tick its own letter**, with the places the
    /// step needs in that unit: a 250K step reads 1M, 1.25M, where the axis alone
    /// writes 1000K, 1250K. Neighbors a thousand apart near a million stay apart.
    #[test]
    fn a_short_axis_gives_each_tick_its_own_letter() {
        let short = fmt(None, true, false);
        let ticks: Vec<String> = [0.0, 250e3, 750e3, 1e6, 1.25e6, 1.5e6].iter()
            .map(|&v| short.tick(v, 250e3)).collect();
        assert_eq!(ticks, ["0", "250K", "750K", "1M", "1.25M", "1.5M"]);
        assert_ne!(short.tick(1_001_000.0, 1000.0), short.tick(1_002_000.0, 1000.0));
        assert_eq!(short.tick(1_001_000.0, 1000.0), "1.001M");
        assert_eq!(short.tick(0.3, 0.1), "0.3");
    }

    /// **`decimals` alone changes the places and keeps the letter**: an axis
    /// stepping by a million keeps its M, and a key keeps its own tiers.
    #[test]
    fn decimals_alone_keep_the_letter_and_fix_the_places() {
        let d1 = fmt(Some(1), false, false);
        assert_eq!(d1.tick(2e6, 1e6), "2.0M");
        assert_eq!(d1.tick(0.5, 0.5), "0.5");
        assert_eq!(d1.key(1_318_683_096.0), "1.3B");
        assert_eq!(d1.key(70.72848), "70.7");
        assert_eq!(fmt(Some(2), false, false).label(70.72848), "70.73");
    }

    /// **A stated place count is a floor on the step**, in the unit the ticks are
    /// written in, so `decimals = 0` never prints one whole number twice.
    #[test]
    fn a_stated_place_count_is_a_floor_on_the_step() {
        assert_eq!(fmt(Some(0), false, false).min_step(0.2, &[0.0, 0.2]), Some(1.0));
        assert_eq!(fmt(Some(0), false, true).min_step(0.001, &[0.0]), Some(0.01));
        assert_eq!(fmt(Some(0), true, false).min_step(250e3, &[0.0, 1.5e6]), Some(1e6));
        assert_eq!(fmt(None, true, false).min_step(250e3, &[1.5e6]), None);
    }
}
