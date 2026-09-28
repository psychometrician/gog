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
    /// The spacing the requested count asked for, when that spacing would have
    /// put more than [`MAX_TICKS`] on the axis and these ticks are the coarser
    /// ones that fit. `None` on every other axis, which is every axis at the
    /// default count.
    ///
    /// Carried on the ticks rather than beside them because an axis can have its
    /// ticks replaced after the fit — a map labels degrees it chose itself, a
    /// displaced pile and a ring index draw no numbers — and the fact has to go
    /// wherever the ticks go. A flag beside them would have to be cleared at each
    /// of those places, and the next one added would be the one that forgot.
    pub widened: Option<Widening>,
}

/// A stated count the axis could not honor under [`MAX_TICKS`]: the spacing it
/// asked for and the spacing drawn, each as the phrase a message is completed
/// with — `a tick every 1K`, `a tick every 2 days`, `a tick at each power of 10`.
///
/// Phrases rather than numbers because the three kinds of axis space their ticks
/// in three different units. A linear step is a number, a calendar stride is a
/// count of months or days, and a log axis steps by powers, or fills each power
/// with 1, 2 and 5; only the generator that chose the ticks knows which, so it
/// writes the words and the renderer passes them on.
#[derive(Clone, Debug, PartialEq)]
pub struct Widening {
    pub asked: String,
    pub drawn: String,
}

/// The most ticks one axis draws, whatever count was asked for.
///
/// Past it the step widens, to the next round number (1, 2, 5, 10, …) until the
/// ticks fit, so the axis is still labeled from one end to the other. It used to
/// cut the list instead, and a cut keeps the *first* ticks:
/// `x(gdp, tick_count = 40)` labeled 0K to 25K on an axis that runs to 49K and
/// left the right half bare, with no message. A widened step is a count the
/// caller wrote and did not get, so the renderer says so
/// ([`TickSpec::widened`]); the default count never reaches this. A calendar
/// axis and a log axis keep the same ceiling in their own steps.
pub const MAX_TICKS: usize = 26;

impl TickSpec {
    /// An axis with nothing to label: the one a chart that has no such axis
    /// draws, which is several of them (a pie's missing position, a ring index,
    /// the cube's frame drawn elsewhere).
    pub fn empty() -> TickSpec {
        TickSpec { values: Vec::new(), labels: Vec::new(), step: 1.0, widened: None }
    }

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
    nice_ticks_within(data_min, data_max, target_count, None)
}

/// [`nice_ticks`], with [`MAX_TICKS`] counted over `window`: the range the axis
/// will show, where the ticks drawn are the ones inside it. `None` counts every
/// tick that brackets the data, which is never fewer.
///
/// The two differ by the bracketing ticks at the ends, and it matters where an
/// end is stated, since an axis then draws exactly to it. `limits = c(1000,
/// 51000)` at a step of 2K brackets 27 values from 0 to 52K and draws the 25
/// from 2K to 50K; counted over the bracket it widened to 5K, and the message
/// said "more than 26 ticks on this axis" of an axis that would have drawn 25.
pub fn nice_ticks_within(
    data_min: f64,
    data_max: f64,
    target_count: usize,
    window: Option<(f64, f64)>,
) -> TickSpec {
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
            widened: None,
        };
    }

    let range = nice_number((data_max - data_min).abs(), false);
    let asked = nice_number(range / (target - 1) as f64, true);
    // Too many ticks at the step the count chose: take the next round step until
    // they fit. Each one is at least twice the last, so the count at least halves
    // per turn, and the bound is only a net for a step that is not a number.
    let mut step = asked;
    for _ in 0..128 {
        if !(step > 0.0 && step.is_finite())
            || ticks_at_step(data_min, data_max, step, window) <= MAX_TICKS
        {
            break;
        }
        step = next_nice_step(step);
    }
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
        // Not a cap any more (the step above already fits, and the two bracketing
        // ticks may sit outside the window): a net for a range so far from zero
        // that `v += step` stops moving `v`.
        if values.len() >= MAX_TICKS + 2 {
            break;
        }
    }

    let labels = values.iter().map(|&t| format_tick(t, step)).collect();
    let widened = (step != asked).then(|| Widening {
        asked: format!("a tick every {}", step_label(asked)),
        drawn: format!("a tick every {}", step_label(step)),
    });
    TickSpec { values, labels, step, widened }
}

/// How many ticks `step` draws: the round numbers bracketing `lo..=hi`, and of
/// those only the ones inside `window` when there is one, with the tolerance
/// the fit uses to keep a tick that sits on an end.
fn ticks_at_step(lo: f64, hi: f64, step: f64, window: Option<(f64, f64)>) -> usize {
    let (mut first, mut last) = ((lo / step).floor(), (hi / step).ceil());
    if let Some((from, to)) = window {
        let eps = (to - from).abs() * 1e-9;
        first = first.max(((from - eps) / step).ceil());
        last = last.min(((to + eps) / step).floor());
    }
    if last < first { 0 } else { (last - first) as usize + 1 }
}

/// The round step after `step`, at any power of ten: 1 → 2 → 5 → 10.
fn next_nice_step(step: f64) -> f64 {
    let exp = step.log10().floor();
    // The mantissa is 1, 2 or 5 up to float error, or 10 when `log10` landed a
    // hair below a whole power, which is a 1 one power up.
    let (mantissa, exp) = match (step / 10_f64.powf(exp)).round() as i64 {
        10 => (1, exp + 1.0),
        m => (m, exp),
    };
    let next = match mantissa {
        1 => 2.0,
        2 => 5.0,
        _ => 10.0,
    };
    next * 10_f64.powf(exp)
}

/// A step written the way an axis writes its ticks, so a message and the axis
/// it describes say `2K` alike.
pub fn step_label(step: f64) -> String {
    format_tick(step, step)
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
    TickSpec { values, labels, step: 1.0, widened: None }
}

/// The kinds of tick a log axis can space, sparsest first: every `k`-th power,
/// or (base 10) each power filled with 1, 2 and 5.
#[derive(Clone, Copy, Debug, PartialEq)]
enum LogStride {
    Powers(i64),
    Fill125,
}

/// [`log_ticks`] for a count the caller stated (spec §10).
///
/// The default takes every power, thins them past eight, and fills an axis of
/// one or two powers with 1, 2 and 5. A stated count chooses among the same kinds
/// of tick, which are the only round numbers a log axis has: every power, every
/// second or third power and so on, or (base 10 only) each power filled with 1,
/// 2 and 5. [`pick_counted`] decides, counting over `window`, the range the axis
/// shows. Before this the count was read by nothing on a log axis:
/// `x(gdp, scale = "log", tick_count = 12)` drew the same two ticks as the
/// default, byte for byte, and said nothing.
pub fn log_ticks_counted(
    log_min: f64, log_max: f64, base: f64, count: usize, window: (f64, f64),
) -> TickSpec {
    let lo = if log_min.is_finite() { log_min.floor() } else { 0.0 };
    let hi = match log_max.is_finite() {
        true if log_max.ceil() > lo => log_max.ceil(),
        _ => lo + 1.0,
    };
    let powers = ((hi - lo).round() as i64).max(1);
    let mut strides: Vec<LogStride> = (1..=powers).rev().map(LogStride::Powers).collect();
    if base == 10.0 {
        strides.push(LogStride::Fill125);
    }
    let values_of = |s: LogStride| -> Vec<f64> {
        match s {
            LogStride::Powers(k) => {
                // Aligned to multiples of the stride, so every third power of ten
                // is 1, 1K, 1M rather than wherever the data happened to start.
                let first = (lo / k as f64).floor() as i64 * k;
                let last = (hi / k as f64).ceil() as i64 * k;
                (0..).map(|i| first + i * k).take_while(|&p| p <= last).map(|p| p as f64).collect()
            }
            LogStride::Fill125 => {
                let mut v: Vec<f64> = ((lo as i64)..(hi as i64))
                    .flat_map(|d| [1.0, 2.0, 5.0].map(|m: f64| (m * 10f64.powi(d as i32)).log10()))
                    .collect();
                v.push(hi);
                v
            }
        }
    };
    let b = if (base - std::f64::consts::E).abs() < 1e-9 {
        "e".to_string()
    } else {
        short_decimal(base).unwrap_or_else(|| format!("{base:.3}"))
    };
    let phrase = |s: LogStride| match s {
        LogStride::Powers(1) => format!("a tick at each power of {b}"),
        LogStride::Powers(k) => format!("a tick every {k} powers of {b}"),
        LogStride::Fill125 => "ticks at 1, 2 and 5 times each power of 10".to_string(),
    };
    let sets: Vec<Vec<f64>> = strides.iter().map(|&s| values_of(s)).collect();
    let inside: Vec<usize> = sets.iter().map(|v| count_within(v, window)).collect();
    let Some((fits, free)) = pick_counted(&inside, count) else {
        return log_ticks(log_min, log_max, base);
    };
    let values = sets[fits].clone();
    let labels = values.iter().map(|&p| format_log_tick(base.powf(p), base, p)).collect();
    let widened = (fits != free).then(|| Widening {
        asked: phrase(strides[free]),
        drawn: phrase(strides[fits]),
    });
    TickSpec { values, labels, step: 1.0, widened }
}

/// How many of `values` fall inside `window`, with the tolerance the fit uses to
/// keep a tick that sits exactly on an end.
fn count_within(values: &[f64], (from, to): (f64, f64)) -> usize {
    let eps = (to - from).abs() * 1e-9;
    values.iter().filter(|&&v| v >= from - eps && v <= to + eps).count()
}

/// Which of several candidate tick sets a stated count picks, given how many
/// ticks each puts inside the axis, sparsest first.
///
/// **Nearest the count, the denser on a tie, among those that put at least two
/// on the axis.** Nearest rather than "the most that stay under it", which is
/// the default's rule, because the steps between candidates are large: a log
/// axis goes from every power to 1-2-5 fills by tripling, and a calendar from
/// every week to every 2 days by more than tripling. A reader asking for 20
/// ticks on six weeks wants the 21 a two-day step gives, not the six weekly
/// ticks the rule "stay under 20" would leave them with, which the default draws
/// anyway. The denser wins a tie so that asking for more never gives fewer, and
/// two is the floor for the reason `tick_count = 1` is refused.
///
/// Returns the pick under [`MAX_TICKS`] and the pick without it; they differ
/// exactly when the ceiling moved the answer, which is the widening a message
/// reports. `None` when no candidate puts two ticks on the axis, where the
/// caller falls back to its default.
fn pick_counted(inside: &[usize], count: usize) -> Option<(usize, usize)> {
    let nearest = |allowed: &dyn Fn(usize) -> bool| -> Option<usize> {
        (0..inside.len())
            .filter(|&i| inside[i] >= 2 && allowed(inside[i]))
            .min_by(|&a, &b| {
                inside[a].abs_diff(count).cmp(&inside[b].abs_diff(count))
                    .then(inside[b].cmp(&inside[a]))
            })
    };
    let free = nearest(&|_| true)?;
    let fits = nearest(&|n| n <= MAX_TICKS)?;
    Some((fits, free))
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
    TickSpec { values, labels, step, widened: None }
}

/// [`time_ticks`] for a count the caller stated (spec §10).
///
/// The default takes the finest calendar stride that keeps about eight ticks. A
/// stated count chooses from the same strides, the calendar's own (1, 2, 7 and 14
/// days, 1, 2, 3 and 6 months, 1, 2 and 5 years at every power of ten, and the
/// clock's below a day), and [`pick_counted`] decides, counting over `window`.
/// Before this the count was read by nothing on a calendar axis: six weeks of
/// days drew the same weekly ticks at `tick_count = 3` and at `tick_count = 20`,
/// byte for byte, and said nothing.
pub fn time_ticks_counted(
    min_s: f64, max_s: f64, unit: crate::time::TimeUnit, count: usize, window: (f64, f64),
) -> TickSpec {
    use crate::time::SECS_PER_DAY;
    const YEAR: f64 = 365.2425 * SECS_PER_DAY;
    // A single moment, or no span at all: the default gives it one interval.
    let span = max_s - min_s;
    if !span.is_finite() || span.abs() < 1e-9 {
        return time_ticks(min_s, max_s, unit);
    }
    // Coarsest first, which is sparsest first.
    let mut strides: Vec<TimeInterval> = Vec::new();
    let mut years = Vec::new();
    let mut mag = 1i64;
    'years: loop {
        for m in [1, 2, 5] {
            let k = m * mag;
            years.push(TimeInterval::Years(k));
            // A stride longer than the span puts at most two ticks on the axis;
            // one past it is as sparse as the ladder needs to reach.
            if k as f64 * YEAR > max_s - min_s {
                break 'years;
            }
        }
        match mag.checked_mul(10) {
            Some(next) => mag = next,
            None => break,
        }
    }
    strides.extend(years.into_iter().rev());
    strides.extend([6, 3, 2, 1].map(TimeInterval::Months));
    strides.extend([14, 7, 2, 1].map(TimeInterval::Days));
    if unit == crate::time::TimeUnit::Second {
        strides.extend([12, 6, 3, 1].map(TimeInterval::Hours));
        strides.extend([30, 15, 5, 1].map(TimeInterval::Minutes));
        strides.extend([30, 15, 5, 1].map(TimeInterval::Seconds));
    }
    let sets: Vec<Vec<f64>> = strides.iter().map(|&s| time_tick_values(min_s, max_s, s)).collect();
    let inside: Vec<usize> = strides.iter().zip(&sets)
        .map(|(&s, v)| match v.last().is_some_and(|&l| l >= max_s) {
            true => count_within(v, window),
            // `time_tick_values` stops at 41, a guard for a stride far too fine
            // for the span. Counted from the list, a stride of one second over
            // a week would look like 41 ticks and could be picked as nearest to
            // a count of 40; counted from its length it is 604,801, which is
            // all the choice needs to know about it.
            false => ((window.1 - window.0) / nominal_secs(s)).floor().max(0.0) as usize + 1,
        })
        .collect();
    let Some((fits, free)) = pick_counted(&inside, count) else {
        return time_ticks(min_s, max_s, unit);
    };
    let interval = strides[fits];
    let values = sets[fits].clone();
    let labels = values.iter().map(|&v| time_tick_label(v, interval)).collect();
    let step = if values.len() > 1 { values[1] - values[0] } else { SECS_PER_DAY };
    let widened = (fits != free).then(|| Widening {
        asked: stride_phrase(strides[free]),
        drawn: stride_phrase(interval),
    });
    TickSpec { values, labels, step, widened }
}

/// A calendar stride's length in seconds, with the average month and year: for
/// counting how many ticks a stride would make, never for placing one.
fn nominal_secs(interval: TimeInterval) -> f64 {
    use crate::time::SECS_PER_DAY;
    const YEAR: f64 = 365.2425 * SECS_PER_DAY;
    match interval {
        TimeInterval::Years(k) => k as f64 * YEAR,
        TimeInterval::Months(k) => k as f64 * YEAR / 12.0,
        TimeInterval::Days(k) => k as f64 * SECS_PER_DAY,
        TimeInterval::Hours(k) => k as f64 * 3600.0,
        TimeInterval::Minutes(k) => k as f64 * 60.0,
        TimeInterval::Seconds(k) => k as f64,
    }
}

/// A calendar stride in words, completing a message the way a linear step's
/// label does: `a tick every 2 days`, `a tick every month`.
fn stride_phrase(interval: TimeInterval) -> String {
    let (k, one, many) = match interval {
        TimeInterval::Years(k) => (k, "year", "years"),
        TimeInterval::Months(k) => (k, "month", "months"),
        TimeInterval::Days(7) => (1, "week", "weeks"),
        TimeInterval::Days(14) => (2, "week", "weeks"),
        TimeInterval::Days(k) => (k, "day", "days"),
        TimeInterval::Hours(k) => (k, "hour", "hours"),
        TimeInterval::Minutes(k) => (k, "minute", "minutes"),
        TimeInterval::Seconds(k) => (k, "second", "seconds"),
    };
    match k {
        1 => format!("a tick every {one}"),
        _ => format!("a tick every {k} {many}"),
    }
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
    //
    // And only while the number in front of the letter stays under a thousand,
    // which is what a suffix is for: each one covers three digits. Past `999B`
    // there is no letter left, and the suffix grew instead — `1000B`, then
    // `1000000B` at 10^15, one digit longer every power. That is the same width
    // the seven-character test below refuses a plain number, so it takes the same
    // way out: the power.
    if a >= 1.0 && v.fract() == 0.0 {
        for (mag, suffix) in [(1e9, "B"), (1e6, "M"), (1e3, "K")] {
            if a >= mag && a < mag * 1e3 && (v % mag) == 0.0 {
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
    // A fill tick (base 10's 2 and 5) has no whole power, and it reaches here
    // whenever its quantity does not read cleanly: past the suffixes (2×10¹² is
    // `2000000000000`) and below three decimal places (2×10⁻⁵ printed `0.000`,
    // a tick labeled zero on an axis that has none). It is named as its multiple
    // of the power beneath it, which is how the whole power beside it reads.
    if (exponent - exponent.round()).abs() > 1e-9 {
        let whole = exponent.floor();
        let multiple = base.powf(exponent - whole);
        return match short_decimal(multiple).filter(|m| m.len() <= 3) {
            Some(m) if whole == 0.0 => m,
            Some(m) => format!("{m}×{}", power_label(base, whole)),
            None => format!("{:.3}", base.powf(exponent)),
        };
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
///
/// **Zero is written `0` whatever the tier.** A suffix names a unit, and zero is
/// zero in every unit, so `0K` and `0M` read as a quantity where there is none;
/// a value a hair below zero printed `-0M`. Only the suffixed tiers are affected:
/// a decimal axis's `0.0` matches the precision of the ticks beside it.
fn format_tick(v: f64, step: f64) -> String {
    let abs_step = step.abs();
    if abs_step >= 1_000.0 && v.abs() < abs_step * 1e-9 {
        return "0".to_string();
    }
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
    TickSpec { values, labels, step, widened: None }
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
    TickSpec { values, labels, step, widened: None }
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
    fn a_suffix_covers_three_digits_and_the_power_takes_over_past_them() {
        // The suffix grew once the letters ran out: 10^12 read `1000B` and 10^21
        // `1000000000000B`. A letter stands for three digits, so past 999B the
        // label is the power, the way a plain number past seven characters is.
        let t = log_ticks(0.0, 24.0, 10.0);
        assert_eq!(t.labels, vec!["1", "1K", "1M", "1B", "10¹²", "10¹⁵", "10¹⁸", "10²¹", "10²⁴"]);
        assert_eq!(format_log_tick(1e11, 10.0, 11.0), "100B");
        assert!(t.labels.iter().all(|l| l.chars().count() <= 5), "{:?}", t.labels);
    }

    #[test]
    fn a_fill_tick_with_no_clean_quantity_is_named_against_its_power() {
        // Base 10's 2 and 5 fill a short axis. Past the suffixes and below three
        // decimal places their quantity is not clean: 2×10¹² printed as thirteen
        // digits, and 2×10⁻⁵ as `0.000`, a zero on an axis that cannot have one.
        let big = log_ticks(12.0, 13.0, 10.0);
        assert_eq!(big.labels, vec!["10¹²", "2×10¹²", "5×10¹²", "10¹³"]);
        let small = log_ticks(-5.0, -4.0, 10.0);
        assert_eq!(small.labels, vec!["10⁻⁵", "2×10⁻⁵", "5×10⁻⁵", "10⁻⁴"]);
        // Where the quantity is clean it still reads as one.
        assert_eq!(log_ticks(9.0, 10.0, 10.0).labels, vec!["1B", "2B", "5B", "10B"]);
        assert_eq!(log_ticks(-3.0, -2.0, 10.0).labels, vec!["0.001", "0.002", "0.005", "0.01"]);
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

    // -- the ceiling on the count ---------------------------------------------

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() <= 1e-9 * a.abs().max(b.abs())
    }

    #[test]
    fn a_count_past_the_ceiling_widens_the_step_rather_than_cutting_the_axis() {
        // gapminder 2007's gdp, the case that reported this. Forty ticks round to a
        // step of 1K, which is 51 ticks from 0K to 50K. The cut kept the first 26
        // and stopped at 25K on an axis that runs to 49K.
        let t = nice_ticks(277.55, 49357.19, 40);
        assert!(close(t.step, 2000.0), "stepped by {}", t.step);
        let w = t.widened.as_ref().expect("a count past the ceiling is marked");
        assert_eq!((w.asked.as_str(), w.drawn.as_str()), ("a tick every 1K", "a tick every 2K"));
        assert!(t.values.len() <= MAX_TICKS, "got {} ticks", t.values.len());
        assert!(t.scale_min() <= 277.55 && t.scale_max() >= 49357.19,
            "the ticks stop short of the data: {}..{}", t.scale_min(), t.scale_max());
    }

    #[test]
    fn the_ceiling_counts_what_the_axis_shows_not_the_bracket() {
        // `limits = c(1000, 51000)` at 40 ticks. A step of 2K brackets 27 values,
        // 0K to 52K, and draws the 25 from 2K to 50K that sit inside the stated
        // ends, so 2K fits and the step stays there.
        let t = nice_ticks_within(1000.0, 51000.0, 40, Some((1000.0, 51000.0)));
        assert!(close(t.step, 2000.0), "stepped by {}", t.step);
        // Counted over the bracket, the same range widened past 2K.
        assert!(nice_ticks(1000.0, 51000.0, 40).step > 2000.0);
    }

    #[test]
    fn no_count_puts_more_than_the_ceiling_on_an_axis() {
        for (lo, hi) in [(277.55, 49357.19), (0.0, 30.0), (-4.2, 17.9), (1e9, 1e9 + 7.0)] {
            for n in [27, 40, 100, 10_000, 1_000_000_000] {
                let t = nice_ticks(lo, hi, n);
                assert!(t.values.len() <= MAX_TICKS, "{lo}..{hi} at {n}: {} ticks", t.values.len());
                assert!(t.scale_min() <= lo && t.scale_max() >= hi,
                    "{lo}..{hi} at {n}: an end is bare ({}..{})", t.scale_min(), t.scale_max());
            }
        }
    }

    #[test]
    fn asking_for_more_never_gives_a_coarser_step() {
        // §10's promise, which the ceiling has to keep: past it every count gets
        // the same step, never a coarser one than a smaller count got.
        for (lo, hi) in [(277.55, 49357.19), (0.0, 30.0), (1952.0, 2007.0), (-4.2, 17.9)] {
            let steps: Vec<f64> = (2..=120).map(|n| nice_ticks(lo, hi, n).step).collect();
            assert!(steps.windows(2).all(|w| w[1] <= w[0] * (1.0 + 1e-12)),
                "{lo}..{hi}: {steps:?}");
        }
    }

    #[test]
    fn the_default_count_never_reaches_the_ceiling() {
        // The renderer reports a widened step as a count the caller wrote and did
        // not get, so the default must never widen, whatever the span or offset.
        for lo in [-1e6, -3.7, 0.0, 0.1, 277.55, 1952.0, 1e9] {
            for span in [1e-6, 0.3, 1.0, 7.0, 55.0, 999.0, 49079.64, 3.3e7] {
                let t = nice_ticks(lo, lo + span, 5);
                assert!(t.widened.is_none(), "{lo} + {span} widened at the default");
            }
        }
    }

    #[test]
    fn the_next_round_step_walks_one_two_five() {
        let walk: Vec<f64> = std::iter::successors(Some(0.001), |&s| Some(next_nice_step(s)))
            .take(10)
            .collect();
        let want = [0.001, 0.002, 0.005, 0.01, 0.02, 0.05, 0.1, 0.2, 0.5, 1.0];
        assert!(walk.iter().zip(want).all(|(&a, b)| close(a, b)), "{walk:?}");
    }

    // -- a stated count on a log axis and a calendar axis ------------------------

    /// What the axis draws: the labels of the ticks inside `window`.
    fn shown(t: &TickSpec, (from, to): (f64, f64)) -> Vec<String> {
        t.values.iter().zip(&t.labels)
            .filter(|(v, _)| **v >= from - 1e-9 && **v <= to + 1e-9)
            .map(|(_, l)| l.clone())
            .collect()
    }

    #[test]
    fn a_stated_count_on_a_log_axis_picks_among_the_powers_and_the_fill() {
        // gapminder 2007's gdp, in decades, and the window its fitted axis shows.
        let (lo, hi) = (277.55_f64.log10(), 49357.19_f64.log10());
        let window = (lo - 0.11, hi + 0.11);
        // The default draws the two powers inside; the count was read by nothing.
        assert_eq!(shown(&log_ticks(lo, hi, 10.0), window), vec!["1K", "10K"]);
        // Twelve is nearest the seven the 1-2-5 fill puts there.
        assert_eq!(shown(&log_ticks_counted(lo, hi, 10.0, 12, window), window),
                   vec!["500", "1K", "2K", "5K", "10K", "20K", "50K"]);
        // Three is nearest the two powers, and the default's answer is kept.
        assert_eq!(shown(&log_ticks_counted(lo, hi, 10.0, 3, window), window),
                   vec!["1K", "10K"]);
        // Asking for more never gives fewer.
        let counts: Vec<usize> = (2..=60)
            .map(|n| shown(&log_ticks_counted(lo, hi, 10.0, n, window), window).len())
            .collect();
        assert!(counts.windows(2).all(|w| w[1] >= w[0]), "{counts:?}");
        // A base with no fill steps by its powers alone.
        let t = log_ticks_counted(0.0, 12.0, 2.0, 7, (0.0, 12.0));
        assert_eq!(t.labels, vec!["1", "4", "16", "64", "256", "1024", "4096"]);
    }

    #[test]
    fn a_count_past_the_ceiling_on_a_log_axis_takes_the_powers_and_says_so() {
        // Ten decades filled with 1, 2 and 5 is 31 ticks, past the 26 an axis draws.
        let t = log_ticks_counted(0.0, 10.0, 10.0, 40, (0.0, 10.0));
        assert_eq!(t.values.len(), 11, "{:?}", t.labels);
        let w = t.widened.as_ref().expect("the ceiling moved the answer");
        assert_eq!(w.asked, "ticks at 1, 2 and 5 times each power of 10");
        assert_eq!(w.drawn, "a tick at each power of 10");
        assert!(log_ticks_counted(0.0, 10.0, 10.0, 11, (0.0, 10.0)).widened.is_none());
    }

    #[test]
    fn a_stated_count_on_a_calendar_axis_picks_among_its_strides() {
        // Six weeks of days, Mar 1 to Apr 11 2024, as a line's fitted axis shows it.
        let (lo, hi) = (s(2024, 3, 1), s(2024, 4, 11));
        let window = (lo - 2.05 * SECS_PER_DAY, hi + 2.05 * SECS_PER_DAY);
        let default = shown(&time_ticks(lo, hi, TimeUnit::Day), window);
        assert_eq!(default, vec!["Mar 4", "Mar 11", "Mar 18", "Mar 25", "Apr 1", "Apr 8"]);
        // Three is nearest the three a two-week stride puts there, and twenty the
        // 22 a two-day stride does: the default's weeks for both, before.
        assert_eq!(shown(&time_ticks_counted(lo, hi, TimeUnit::Day, 3, window), window),
                   vec!["Mar 4", "Mar 18", "Apr 1"]);
        let twenty = shown(&time_ticks_counted(lo, hi, TimeUnit::Day, 20, window), window);
        assert_eq!(twenty.len(), 22, "{twenty:?}");
        assert!(twenty.iter().all(|l| !l.contains(':')), "a date column grew clock ticks");
        let counts: Vec<usize> = (2..=60)
            .map(|n| shown(&time_ticks_counted(lo, hi, TimeUnit::Day, n, window), window).len())
            .collect();
        assert!(counts.windows(2).all(|w| w[1] >= w[0]), "{counts:?}");
    }

    #[test]
    fn a_count_past_the_ceiling_on_a_calendar_axis_steps_by_two_days_and_says_so() {
        let (lo, hi) = (s(2024, 3, 1), s(2024, 4, 11));
        let window = (lo - 2.05 * SECS_PER_DAY, hi + 2.05 * SECS_PER_DAY);
        // A tick a day is 46 in this window, past the 26 an axis draws.
        let t = time_ticks_counted(lo, hi, TimeUnit::Day, 40, window);
        let w = t.widened.as_ref().expect("the ceiling moved the answer");
        assert_eq!((w.asked.as_str(), w.drawn.as_str()), ("a tick every day", "a tick every 2 days"));
        assert!(shown(&t, window).len() <= MAX_TICKS);
        // A count the two-day stride answers anyway says nothing.
        assert!(time_ticks_counted(lo, hi, TimeUnit::Day, 22, window).widened.is_none());
    }

    #[test]
    fn a_stride_too_fine_to_list_is_counted_by_its_length() {
        // A week of seconds. The list of candidates stops at 41 values, and a
        // stride of one second read off that list would look like 41 ticks, the
        // nearest of all to a count of 40. By its length it is 604,801, so the
        // count lands on a stride the week can carry.
        let (lo, hi) = (s(2024, 3, 4), s(2024, 3, 11));
        let t = time_ticks_counted(lo, hi, TimeUnit::Second, 40, (lo, hi));
        assert!(t.step >= 3600.0, "a week stepped by {} seconds", t.step);
        assert!(shown(&t, (lo, hi)).len() <= MAX_TICKS);
    }

    #[test]
    fn a_stride_reads_as_the_calendar_says_it() {
        assert_eq!(stride_phrase(TimeInterval::Days(1)), "a tick every day");
        assert_eq!(stride_phrase(TimeInterval::Days(7)), "a tick every week");
        assert_eq!(stride_phrase(TimeInterval::Days(14)), "a tick every 2 weeks");
        assert_eq!(stride_phrase(TimeInterval::Months(3)), "a tick every 3 months");
        assert_eq!(stride_phrase(TimeInterval::Years(1)), "a tick every year");
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
