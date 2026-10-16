//! The axes: the domain each position is fit to, and the ticks it carries.

use super::*;

/// Collect the categories on one axis, in the order they should be drawn.
///
/// Any mark with a string column on this axis defines the categorical scale,
/// not just `bar` — a `point` on a categorical axis is a strip plot, and gating
/// that to one mark was a per-mark special case.
///
/// `reverse` flips the result for the y axis. Screen y grows downward while the
/// scale grows upward, so without it the first category would land at the bottom
/// and `order(gold, desc = TRUE)` would put the largest bar at the bottom of a
/// horizontal chart and the leftmost of a vertical one. Reversing keeps
/// `order` meaning one thing: first in sort order, first in reading order.
///
/// This doc comment spent several sessions attached to `FRAME_INSET`, 280 lines
/// up: a second `///` block was added directly beneath it with no blank line
/// between, so rustdoc glued both onto the const and this function had none.
pub(super) fn detect_categories(
    eff: &[&DataFrame],
    // The `order()` this axis takes, which the caller decides: the plot's, or
    // none when it names the column of the other categorical axis.
    order: Option<&crate::ir::OrderSpec>,
    field: &str,
    reverse: bool,
    // Each category's rank when `order()` names another column, read from the
    // layer's statistic over the category's unsplit rows; `None` falls back to
    // the first drawn row, for a transform with no one statistic to rank by.
    ranked: Option<&[(String, f64)]>,
    // The order the unsplit tables give this column, when the plot is split into
    // panels or frames; `None` when it is not, or when a transform decides the
    // order itself.
    natural: Option<&[String]>,
) -> Option<Vec<String>> {
    let mut labels = crate::data::categories_across(eff, field);
    if labels.is_empty() {
        return None;
    }
    // **A split does not reorder the axis it does not name.** `eff` is every panel's
    // and every frame's frames chained, so read in that order the first panel's
    // categories came first: `zone + x(exporter) + y(importer) + play(exporter)`
    // listed the importers China exports to before the rest, where the same plot
    // unsplit lists them in row order. Found 2026-10-13 walking the mark × channel
    // grid (the author: "yes do what you recommended"); `order()` reaching an axis
    // it did not name was the same shape, fixed the same day. Declared levels
    // already decide the order and are left alone, and `order()` sorts below.
    if let Some(natural) = natural.filter(|_| eff.iter().all(|df| df.levels(field).is_none())) {
        let at = |c: &String| natural.iter().position(|n| n == c).unwrap_or(usize::MAX);
        labels.sort_by_key(at);
    }

    if let Some(s) = order {
        if s.field == field {
            // Ordering an axis **by itself** means "in this column's own order",
            // and which order that is depends on what the column carries. Plain
            // text carries nothing but its spelling, so alphabetical is the only
            // answer there is. A **factor carries declared levels**, and those are
            // the column's own order — `categories_across` has already applied
            // them, so ascending is the list we are holding and descending is that
            // list reversed.
            //
            // Sorting the labels alphabetically in both cases is what this did
            // until 2026-07-26, and it silently threw the levels away: five-year
            // age bands ordered `0 10 15 5 50 55` and a population pyramid came out
            // with its floors shuffled. Nothing failed, because a scrambled
            // categorical axis is still a well-formed axis. Note the asymmetry it
            // created, which is the Law-2 smell that names the bug: the levels were
            // honored by *every* other reader of this column (the facet strips say
            // so at `layout_panels`, and the axis itself when no `order` is present)
            // and discarded only by the one atom whose whole job is ordering.
            //
            // `order` by *another* column still overrides the levels, in the branch
            // below. That is the documented power and it is untouched: the levels
            // are what the column says about itself, and a second column outranks
            // them. Nothing outranks the column's own order but itself.
            match eff.iter().find_map(|df| df.levels(field)) {
                Some(_) if s.descending => labels.reverse(),
                Some(_) => {}
                None if s.descending => labels.sort_by(|a, b| b.cmp(a)),
                None => labels.sort(),
            }
        } else {
            // By another column: the layer's statistic over each category's
            // rows when there is one, else category → value from the first
            // layer that carries both.
            let mut sort_map: Vec<(String, f64)> = ranked.map(<[_]>::to_vec).unwrap_or_default();
            for df in eff.iter().filter(|_| ranked.is_none()) {
                let Some(keys) = df.str_col(field)        else { continue };
                let Some(vals) = df.float_col(&s.field)   else { continue };
                for (k, v) in keys.iter().zip(vals.iter()) {
                    if !sort_map.iter().any(|(e, _)| e == k) {
                        sort_map.push((k.clone(), *v));
                    }
                }
                break;
            }
            if !sort_map.is_empty() {
                let value_of = |k: &String| {
                    sort_map.iter().find(|(e, _)| e == k).map(|(_, v)| *v).unwrap_or(0.0)
                };
                labels.sort_by(|a, b| {
                    let (va, vb) = (value_of(a), value_of(b));
                    if s.descending {
                        vb.partial_cmp(&va).unwrap_or(std::cmp::Ordering::Equal)
                    } else {
                        va.partial_cmp(&vb).unwrap_or(std::cmp::Ordering::Equal)
                    }
                });
            }
        }
    }

    if reverse { labels.reverse(); }
    Some(labels)
}

/// Build the ticks and scale for one axis.
///
/// `bar_position` — the axis the bars sit on, when their positions are numeric:
/// ticks land on the bars rather than at round numbers.
/// `bar_extent` — the axis the bars measure along: the range is stretched to
/// include zero, because a bar is read from the baseline.
///
/// `eff` is every panel's effective frames together: a facet shares one scale
/// across its panels, so the axis is built from all of them at once.
/// `bar_frames` is the subset the bar layers drew — the frames bar-aligned
/// ticks read their positions from.
#[allow(clippy::too_many_arguments)]
/// A fitted range widened by half a slot at each end — the *support* of the
/// slots rather than the span between their centers.
///
/// The slot width is read the same way [`super::marks::bar_thickness_svg`] reads
/// it, as the smallest gap between distinct positions, and from the same frames
/// the bars are drawn from; the two have to agree or the wedges will not tile
/// whatever the range says. Returns the range untouched when there is no second
/// position to measure a gap against.
/// Fit an axis to a **tiling's outer edges**, when the plot has one.
///
/// `build_axis` reads the position columns, and a 2-D `bin` fills those with cell
/// *centers* — so the outer half of every edge cell hangs past the fitted range
/// and is clipped away by the panel, visibly, as a border of short cells. The
/// tiling's own edge columns are the honest range: they are where the data
/// actually stops.
///
/// Flush by construction, and that is the point rather than an omission. A
/// breathing margin would leave a band of panel no cell covers, which on a
/// heatmap does not read as margin — it reads as a region that was measured and
/// found empty.
///
/// A plot with no binned zone passes its range and ticks through untouched.
/// `center`/`half` is how a **hexagonal** mesh describes the same extent — one
/// center column and one half-extent — since a hexagon has no edges to name. The
/// two forms are asked in that order and the first that answers wins, which is
/// the mark's own rule ("ask the tiling what its cells look like") applied to the
/// axis.
///
/// **A stated end is not fitted away** (spec §10, §12). The mesh's edges are a
/// *derivation* — where the data stopped — and `stated` is the caller's own
/// domain, so each end that was stated survives this pass and each end that was
/// not is taken from the cells. Overriding both was a live silent drop:
/// `y(depth, limits = c(0, 4))` on a sunburst reached the ticks and nothing else,
/// so the innermost ring still ran to the center and the hole the book promised
/// was never drawn — while the tick labels moved, which is what made it look as
/// though the domain had landed. Which end came from where is why the fix is
/// per-end rather than a flag: `c(0, NA)` opens the hole and still lets the rim
/// grow to the tree.
pub(super) fn fit_to_cells(
    frames: &[&DataFrame], lo_field: &str, hi_field: &str,
    center: &str, half: &str,
    t: TickSpec, range: (f64, f64),
    stated: (Option<f64>, Option<f64>),
    // Whether to choose the ticks again over the cells themselves, and with what
    // count ([`Retick`]).
    retick: Retick,
) -> (TickSpec, (f64, f64)) {
    let fresh = |n: usize, lo: f64, hi: f64| nice_ticks_within(lo, hi, n, Some((lo, hi)));
    let reduce = |field: &str, pick: fn(f64, f64) -> f64, seed: f64| -> Option<f64> {
        let v = frames.iter()
            .filter_map(|d| d.float_col(field))
            .flat_map(|c| c.iter().copied())
            .filter(|v| v.is_finite())
            .fold(seed, pick);
        v.is_finite().then_some(v)
    };
    // A hexagon's outermost point on each axis is its center plus its own half-
    // extent, so the mesh's bounds are the extreme centers pushed out by one.
    // Read as a pair rather than per row because every cell in a mesh is the same
    // size — that is what makes it a mesh.
    let span = match (reduce(center, f64::min, f64::INFINITY),
                      reduce(center, f64::max, f64::NEG_INFINITY),
                      reduce(half, f64::max, f64::NEG_INFINITY)) {
        (Some(lo), Some(hi), Some(h)) => Some((lo - h, hi + h)),
        _ => match (reduce(lo_field, f64::min, f64::INFINITY),
                    reduce(hi_field, f64::max, f64::NEG_INFINITY)) {
            (Some(lo), Some(hi)) => Some((lo, hi)),
            _ => None,
        },
    };
    match span {
        Some((lo, hi)) => {
            let (lo, hi) = (stated.0.unwrap_or(lo), stated.1.unwrap_or(hi));
            match (hi > lo, stated.0.is_some() || stated.1.is_some()) {
                // `clip_ticks`'s give-up guard is for a range gog *derived* on the
                // caller's behalf; a range the caller had a hand in is `adopt_range`'s,
                // which is the same distinction those two functions already draw.
                (true, true) => match retick {
                    Retick::Always(n) => adopt_range(fresh(n, lo, hi), lo, hi),
                    _ => adopt_range(t, lo, hi),
                },
                // `t` was chosen over the cell *centers*, and those can miss the
                // cells: every center of a one-level crossed partition sits at 0.5,
                // so the ticks were the degenerate triple −0.5, 0.5 and 1.5, one of
                // them fell inside `0 .. 1`, and `clip_ticks` gave the fit up. The
                // spine plot drew its bands across the middle half of the panel,
                // under an axis reading `-0`, `0` and `2`. The cells are where the
                // data is, so the ticks are chosen again over them.
                (true, false) => match retick {
                    Retick::Always(n) => clip_ticks(fresh(n, lo, hi), lo, hi),
                    Retick::WhenMissed(n) if ticks_inside(&t, lo, hi).1 < 2 =>
                        clip_ticks(fresh(n, lo, hi), lo, hi),
                    _ => clip_ticks(t, lo, hi),
                },
                _ => (t, range),
            }
        }
        None => (t, range),
    }
}

/// When [`fit_to_cells`] may choose an axis's ticks again, over the cells.
///
/// **A partition's axes always are** (spec §15, ruled 2026-09-24). Its cells are
/// laid end to end from 0, so its measure axis runs from 0 to the total and its
/// share axis from 0 to 1, and those ends are what the axis means; ticked over
/// the cells' centers instead, a mosaic's shares read 0.2 to 0.8 and a donut's
/// amounts started at 1000, wherever the centers fell, where every other measure
/// axis labels its zero. A sunburst's angle then ticks 0 and its total on one
/// spoke, and `polar_angles` keeps the first. Every other mesh keeps the ticks it
/// chose over its centers unless they miss the cells, which is the spine plot's
/// rescue and moves nothing that drew.
#[derive(Clone, Copy)]
pub(super) enum Retick {
    /// A log, calendar or categorical axis keeps the ticks it chose.
    Never,
    /// Choose again only when fewer than two of the ticks land on the cells.
    WhenMissed(usize),
    /// Choose again over the cells, always: a partition's axes.
    Always(usize),
}

/// A stated domain (spec §10) in the **axis's own units**.
///
/// `limits` arrives in the data's units, like the ticks, so a log axis converts —
/// its columns already hold log positions. Shared by the two readers of a stated
/// domain rather than written twice: `build_axis`, which adopts it, and
/// `fit_to_cells`, which must not fit it away. Two copies is how the second one
/// came to disagree with the first.
pub(super) fn stated_domain(
    limits: (Option<f64>, Option<f64>), is_log: bool, base: f64,
) -> (Option<f64>, Option<f64>) {
    let to_axis = |v: f64| if is_log { scale::to_log(v, base) } else { v };
    (
        limits.0.map(to_axis).filter(|v| v.is_finite()),
        limits.1.map(to_axis).filter(|v| v.is_finite()),
    )
}

/// The slots' own support: from half a slot below the first center to half a slot
/// above the last, where the slot is the smallest gap between centers. `None`
/// when fewer than two centers leave no gap to read a slot from.
pub(super) fn slot_support(bar_frames: &[&DataFrame], field: &str) -> Option<(f64, f64)> {
    let mut vals: Vec<f64> = bar_frames
        .iter()
        .filter_map(|d| d.float_col(field))
        .flat_map(|c| c.iter().copied())
        .filter(|v| v.is_finite())
        .collect();
    vals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let slot = vals.windows(2).map(|w| w[1] - w[0]).filter(|&d| d > 1e-12)
        .fold(f64::INFINITY, f64::min);
    (vals.len() >= 2 && slot.is_finite())
        .then(|| (vals[0] - slot / 2.0, vals[vals.len() - 1] + slot / 2.0))
}

pub(super) fn widen_to_slot_support(bar_frames: &[&DataFrame], field: &str, xs: (f64, f64)) -> (f64, f64) {
    let mut vals: Vec<f64> = bar_frames
        .iter()
        .filter_map(|d| d.float_col(field))
        .flat_map(|c| c.iter().copied())
        .filter(|v| v.is_finite())
        .collect();
    if vals.len() < 2 {
        return xs;
    }
    vals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let slot = vals
        .windows(2)
        .map(|w| w[1] - w[0])
        .filter(|&d| d > 1e-12)
        .fold(f64::INFINITY, f64::min);
    if !slot.is_finite() {
        return xs;
    }
    (xs.0 - slot / 2.0, xs.1 + slot / 2.0)
}

/// What a set of panels implies about the axes — the whole answer, so that the
/// shared fit and a freed one differ only in *which panels were asked*.
///
/// Built by the `fit_axes` closure in `render`. A default plot builds one of
/// these over every panel at once; a binding that says `free = TRUE` builds
/// another per panel and takes that axis's fields from it (spec §11).
#[derive(Clone)]
pub(super) struct PanelAxes {
    pub(super) x_ticks: TickSpec,
    pub(super) xs: (f64, f64),
    pub(super) y_ticks: TickSpec,
    pub(super) ys: (f64, f64),
    pub(super) z_ticks: TickSpec,
    pub(super) zs: (f64, f64),
    pub(super) cat_x: Option<Vec<String>>,
    pub(super) cat_y: Option<Vec<String>>,
    /// Where a polar plot's spokes start, as a fraction of the radial range —
    /// read off the cells' inner edge, so it follows whichever fit produced `ys`.
    pub(super) inner_edge: f64,
    /// What `x`'s and `y`'s ticks were chosen over, which a page reads back to
    /// choose a shared axis's ticks (`build_axis`'s third value).
    pub(super) x_extent: (f64, f64),
    pub(super) y_extent: (f64, f64),
    /// Whether a stated `decimals` would leave `x` or `y` fewer than two ticks
    /// (`ticks::relabel`), which the drawing refuses.
    pub(super) too_few: (bool, bool),
}

impl PanelAxes {
    /// One position's ticks, for the questions asked of all three alike.
    pub(super) fn ticks(&self, channel: &Channel) -> &TickSpec {
        match channel {
            Channel::X => &self.x_ticks,
            Channel::Y => &self.y_ticks,
            _ => &self.z_ticks,
        }
    }
}

pub(super) fn build_axis(
    eff: &[&DataFrame],
    bar_frames: &[&DataFrame],
    // The `(frame, column)` pairs that place marks on this axis *without* going
    // through `field` — a bounded `zone`'s sides, and nothing else today. See
    // where they are gathered for why this mark is the exception.
    sides: &[(&DataFrame, &str)],
    field: &str,
    cats: Option<&[String]>,
    bar_position: bool,
    bar_extent: bool,
    // Suppress the free-end breathing margin: the axis an `area` fills along
    // sits flush to its data rather than leaving an empty band each side.
    flush: bool,
    tick_count: Option<usize>,
    is_log: bool,
    base: f64,
    time: Option<crate::time::TimeUnit>,
    // The domain the binding states, in the data's own units (spec §10). Each
    // end is independent: `c(0, NA)` pins the baseline and leaves the top to the
    // data.
    limits: (Option<f64>, Option<f64>),
    // How far, **in slots**, the marks on a *categorical* axis reach below the
    // first category and above the last (`legality::slot_reach`). `(0.0, 0.0)` for
    // every axis nothing overhangs, which is every axis but a violin's.
    slot_reach: (f64, f64),
    // The span a **page** chose this axis's ticks over, when it shares the axis
    // with another plot (`render::page`): the union of what each plot's ticks were
    // chosen over alone. `None` for every plot drawn by itself, which then chooses
    // over its own data. See the third value returned.
    tick_extent: Option<(f64, f64)>,
    // Where each end has to reach for the labels' letters to fit the panel
    // (`room_for_labels`), from a first drawing; `(None, None)` on the first. A
    // stated end and a bar's baseline keep their place, so it widens only an end
    // the data chose.
    label_room: (Option<f64>, Option<f64>),
) -> (TickSpec, (f64, f64), (f64, f64)) {
    if let Some(cats) = cats {
        let vals: Vec<f64> = (0..cats.len()).map(|i| i as f64).collect();
        // Half a slot each side is exactly right for everything that *stands in* its
        // slot — a bar, a box, a whisker. A violin does not have to: `density(reach =
        // )` is measured in slots and a ridgeline's whole point is reaching past its
        // own, so the axis is told how far and grows to hold it. Without this the top
        // ridge was clipped by the frame — the mark drawing outside the panel, which
        // is the same defect as an area whose baseline fell off the scale, and just
        // as invisible to every check because the plot still rendered.
        //
        // Only the end the shapes actually reach toward grows: an `area` or a stroke
        // reaches one way, so the other keeps its half slot rather than opening a
        // matching band of nothing.
        let (lo, hi) = slot_reach;
        let range =
            ((-0.5f64).min(-lo), (cats.len() as f64 - 0.5).max(cats.len() as f64 - 1.0 + hi));
        return (ticks_with_labels(vals, cats.to_vec()), range, range);
    }

    // The axis spans everything placed on it: the values of its own column, and
    // the columns a bounded `zone` draws its sides from. Unioned rather than used
    // as a fallback, because the two coexist — a zone highlighting part of a line
    // chart must not shrink the axis to itself, and one reaching past the line's
    // last point must widen it.
    let (mut mn, mut mx) = std::iter::once(channel_range_eff(eff, field))
        .chain(sides.iter().map(|(d, c)| channel_range_eff(&[d], c)))
        .flatten()
        .reduce(|a, b| (a.0.min(b.0), a.1.max(b.1)))
        .unwrap_or((0.0, 1.0));

    // A stated end replaces the data's, before anything downstream reads the
    // range — the ticks, the baseline stretch, the bar slots and the polar wrap
    // all have to agree about where the axis ends, and they do that by there
    // being one range rather than by each being told.
    //
    // In log space the column already holds log positions, so a stated end —
    // which arrives in the data's own units, like the ticks — is converted here.
    let stated = stated_domain(limits, is_log, base);
    if let Some(l) = stated.0 { mn = l }
    if let Some(h) = stated.1 { mx = h }

    // Every branch below picks its ticks against the range above, and then the
    // one closing step puts the stated ends back — because each branch widens
    // the range its own way (a calendar to its boundaries, a log axis to whole
    // powers, a fitted axis by 5%) and a stated end must survive all three
    // identically. Doing it once here is also what gives `limits` the **flush**
    // rule for free: replacing a breathed end with the exact stated value *is*
    // suppressing `AXIS_EXPAND` on that end, and only on that end, so
    // `c(0, NA)` pins the baseline while the top still breathes (spec §10).
    let close = |(t, range): (TickSpec, (f64, f64))| -> (TickSpec, (f64, f64)) {
        if stated.0.is_none() && stated.1.is_none() {
            return (t, range);
        }
        adopt_range(t, stated.0.unwrap_or(range.0), stated.1.unwrap_or(range.1))
    };

    // **What the ticks are chosen over, which on a page is not the range.** A page
    // shares an axis by stating the union of its plots' *fitted* ranges as their
    // domain, margins included, and a tick step chosen over that is chosen over a
    // wider span than any of the plots has: gdp alone spans 49K, which rounds to
    // 50K and gives a step of 5K at `tick_count = 12`; the shared range spans 54K,
    // which rounds to 100K and doubles the step to 10K. So every composed page drew
    // coarser ticks than its plots drew alone, and a chapter putting 3 ticks beside
    // 12 drew 2 beside 6. The page therefore also hands down the span the plots'
    // ticks were chosen over alone, and the ticks are chosen over that while the
    // range stays the shared one. A plot by itself has no such span and chooses
    // over its own data, exactly as it always did.
    let over = |mn: f64, mx: f64| tick_extent.unwrap_or((mn, mx));

    // **Room for the labels' letters**, on an end the data chose. A stated end is
    // the author's and a baseline is where a bar stands, so neither moves (`close`
    // would put a stated end back in any case). Joined before the ticks are
    // clipped, so a round number the wider range now holds is ticked.
    let room = |(lo, hi): (f64, f64), pinned: (bool, bool)| -> (f64, f64) {
        let lo = match label_room.0 {
            Some(v) if stated.0.is_none() && !pinned.0 => lo.min(v),
            _ => lo,
        };
        let hi = match label_room.1 {
            Some(v) if stated.1.is_none() && !pinned.1 => hi.max(v),
            _ => hi,
        };
        (lo, hi)
    };

    // A temporal axis ticks at calendar boundaries, whatever else is going on.
    // Deliberately ahead of the bar paths: bars at dates get calendar
    // gridlines, not a tick per bar — daily bars would label every one — and
    // `bar_extent`'s stretch-to-zero would put the baseline at 1970.
    //
    // **The ticks come from the calendar; the range comes from the data.** That
    // is the section's own rule (§10) and the calendar is not exempt from it —
    // it used to be, taking `(first tick, last tick)` as the range, which is
    // precisely the bracket-outward failure the rule exists to undo. Six weeks
    // of daily orders ticked on Mondays drew a 49-day axis for 41 days of data,
    // four dead days at each end, because the first Monday at or before Mar 1 is
    // Feb 26 and the first at or after Apr 11 is Apr 15. An axis is allowed to
    // end between ticks; that is the whole point of fitting.
    if let Some(unit) = time {
        // A bar owns a slot, so the range has to hold the half-slot beyond the
        // first and last one or the end bars are sliced by the frame. Everything
        // else fits with the ordinary free-end breathing, and an `area` fills
        // flush — the same three answers the linear path gives, reached here
        // rather than skipped.
        let (lo, hi) = room(if bar_position {
            widen_to_slot_support(bar_frames, field, (mn, mx))
        } else {
            fitted_range(mn, mx, false, flush)
        }, (false, false));
        // A stated count picks among the calendar's strides, counted over what
        // the axis shows; the default keeps its own rule, so no plot that did not
        // ask moves.
        let (tlo, thi) = over(mn, mx);
        let t = match tick_count {
            Some(n) => time_ticks_counted(
                tlo, thi, unit, n, (stated.0.unwrap_or(lo), stated.1.unwrap_or(hi))),
            None => time_ticks(tlo, thi, unit),
        };
        let (t, range) = close(clip_ticks(t, lo, hi));
        return (t, range, (tlo, thi));
    }

    // The column already holds log positions, so the range is in decades and the
    // ticks have to be chosen there too. `bar_extent` is deliberately not
    // honored: stretching to include zero is meaningless when zero is
    // infinitely far down the axis — the bars measure from its foot instead.
    //
    // **The ticks come from the powers; the range comes from the data** — §10's
    // rule, and the log axis was the last branch still exempt from it. It took
    // `(first tick, last tick)` as its range, which is the bracket-outward failure
    // the calendar branch above was fixed for, arriving through the one door left
    // open. On a histogram it did not merely waste space, it **clipped data**: a
    // bar's range is read off its *centers*, and a slot reaches half a bin past the
    // last of them, so `bar * bin(20) + x(gdp, scale = "log")` over gapminder ended
    // its axis at 10^5 while the last bar reached 10^5.055 — 41% of that bar sliced
    // off by the frame — and opened 2.9 empty bar-widths at the other end, where
    // the first center sat well above 10^2. Both are the one mistake, and the
    // asymmetry is why it read as two: bracketing outward is invisible below the
    // data and fatal above it.
    if is_log {
        // A bar owns a slot, so the range has to hold the half-slot beyond the
        // first and last one or the end bars are sliced by the frame. Everything
        // else fits with the ordinary free-end breathing, and an `area` fills
        // flush — the same three answers the linear and calendar paths give,
        // reached here rather than skipped.
        let (lo, hi) = room(if bar_position {
            widen_to_slot_support(bar_frames, field, (mn, mx))
        } else {
            fitted_range(mn, mx, false, flush)
        }, (false, false));
        // A stated count picks among the powers and the 1-2-5 fill, counted over
        // what the axis shows; the default keeps its own rule.
        let (tlo, thi) = over(mn, mx);
        let t = match tick_count {
            Some(n) => log_ticks_counted(
                tlo, thi, base, n, (stated.0.unwrap_or(lo), stated.1.unwrap_or(hi))),
            None => log_ticks(tlo, thi, base),
        };
        let (t, range) = close(clip_ticks(t, lo, hi));
        return (t, range, (tlo, thi));
    }

    // A stated end is a real coordinate, so the stretch-to-baseline does not
    // reach it: `y(v, limits = c(10, 50))` on bars means the axis starts at 10,
    // and pulling it back to 0 would draw the baseline where the caller said it
    // is not. The *unstated* end still stretches, which is what keeps
    // `c(NA, 50)` a bar chart rather than a floating one.
    if bar_extent {
        if stated.0.is_none() { mn = mn.min(0.0) }
        if stated.1.is_none() { mx = mx.max(0.0) }
    }

    if bar_position {
        // What the axis will show, which is where the ceiling on the count is
        // counted: the fit `bar_x_ticks_eff` makes, with a stated end put back.
        let (lo, hi) = fitted_range(mn, mx, false, false);
        let shown = (stated.0.unwrap_or(lo), stated.1.unwrap_or(hi));
        let chosen_over = over(mn, mx);
        let (t, range) = close(bar_x_ticks_eff(
            bar_frames, field, mn, mx, chosen_over, tick_count, shown));
        return (t, range, chosen_over);
    }
    // Nice numbers for the *ticks*; the data for the *range*. `nice_ticks`
    // brackets the data outward with round numbers — that is the right set of
    // tick values, but the wrong scale bounds (§6.2.2). `fit_axis` keeps the
    // values and pins the range to the data instead.
    //
    // **A stated count is refined until the fit survives it; a derived one is
    // not.** `nice_ticks` picks a *step* from the target and cannot know how many
    // of the round numbers it lands on will fall inside the fitted range — and
    // when fewer than two do, `clip_ticks` gives the fit up and widens the axis to
    // the bracketing numbers instead. That trade was made when this parameter was
    // reachable from no binding, so only odd data could reach it. A caller asking
    // for two ticks reaches it on purpose: `x(year, tick_count = 2)` over
    // 1952–2007 takes a step of 100, lands on 1900/2000/2100, keeps one, and drew
    // a **1900–2100 axis for 55 years of data** — the bracket-outward failure §10
    // exists to undo, arriving through the new door.
    //
    // So a stated count asks for a finer step until at least two of its ticks land
    // inside the data's own range. It is the distinction `adopt_range` already
    // draws one line over: the guard exists because a *derivation* was the lesser
    // thing to give up, and a count the caller wrote is not a derivation.
    //
    // **A derived count is refined too, but only where it leaves two ticks.** The
    // step comes from the range rounded *up* first (Heckbert's loose labeling),
    // which assumes the axis runs out to the bracketing ticks; this axis is fitted
    // to the data instead, so a range just above a round number keeps only the
    // two ticks inside it. Orders from 13 to 34 round to a range of 50, a step of
    // 10, and an axis ticked only at 20 and 30, which shows a place and a
    // direction and nothing to read a value between. A finer step is taken only
    // while fewer than three ticks land inside what the axis shows, and only when
    // one reaches three, so every axis with three ticks or more is left exactly
    // as it was (ruled 2026-09-28: the narrow change, which moves only the
    // two-tick axes).
    let default_target = 5;
    // The ceiling on the count (`ticks::MAX_TICKS`) is counted over what the axis
    // will show: this fit, with a stated end put back by `close`.
    let (lo, hi) = room(fitted_range(mn, mx, bar_extent, flush),
                        (bar_extent && mn == 0.0, bar_extent && mx == 0.0));
    let (show_lo, show_hi) = (stated.0.unwrap_or(lo), stated.1.unwrap_or(hi));
    let shown = Some((show_lo, show_hi));
    let (tlo, thi) = over(mn, mx);
    let mut t = nice_ticks_within(tlo, thi, tick_count.unwrap_or(default_target), shown);
    if tick_count.is_some() {
        let mut target = tick_count.unwrap_or(default_target);
        // Bounded: each step is at least a factor 2 finer, so a handful of tries
        // covers any range a nice number can straddle.
        for _ in 0..8 {
            if ticks_inside(&t, lo, hi).1 >= 2 { break }
            target += 1;
            t = nice_ticks_within(tlo, thi, target, shown);
        }
    } else if ticks_inside(&t, show_lo, show_hi).1 < 3 {
        // Raising the target by one does not always change the step, so the
        // bound is two turns of the 1-2-5 ladder rather than one.
        let mut target = default_target;
        for _ in 0..16 {
            target += 1;
            let finer = nice_ticks_within(tlo, thi, target, shown);
            if ticks_inside(&finer, show_lo, show_hi).1 >= 3 {
                t = finer;
                break;
            }
        }
    }
    // `fit_axis`'s two steps, with the labels' room between them.
    let (t, range) = close(clip_ticks(t, lo, hi));
    (t, range, (tlo, thi))
}

/// Where each end of an axis has to reach for every label's letters to fit the
/// panel, or `None` for an end that is already far enough or may not move.
///
/// `ink` is one entry per label: the value it names on this axis, its letters'
/// reach past that value toward the low end and toward the high end in pixels,
/// and the panel's length along the axis in pixels. A reach in pixels is a reach
/// in the scale's units only for a given range, and widening the range shrinks
/// every pixel's worth of data, so the ends are found by repeating the fit until
/// it holds. It settles quickly, since no label is given more than
/// [`LABEL_ROOM_MAX`] of the panel.
///
/// **A label wider than that keeps today's cut** rather than squeezing the data
/// into a corner of the panel for one word: a smaller `style(size = )`, a shorter
/// label, or `limits` gives it room.
pub(super) fn room_for_labels(
    range: (f64, f64),
    ink: &[(f64, f64, f64, f64)],
    free: (bool, bool),
) -> (Option<f64>, Option<f64>) {
    let (mut lo, mut hi) = range;
    if !(hi > lo) || !(free.0 || free.1) {
        return (None, None);
    }
    let tol = (hi - lo) * 1e-9;
    for _ in 0..64 {
        let mut need = (lo, hi);
        for &(v, low, high, len) in ink {
            if !v.is_finite() || !(len > 0.0) {
                continue;
            }
            let per = (hi - lo) / len;
            if free.0 && low / len <= LABEL_ROOM_MAX {
                need.0 = need.0.min(v - low * per);
            }
            if free.1 && high / len <= LABEL_ROOM_MAX {
                need.1 = need.1.max(v + high * per);
            }
        }
        if need.0 >= lo - tol && need.1 <= hi + tol {
            break;
        }
        (lo, hi) = need;
    }
    (
        (lo < range.0 - tol).then_some(lo),
        (hi > range.1 + tol).then_some(hi),
    )
}

/// The most of a panel's length one label's letters may take past its point and
/// still be given room ([`room_for_labels`]).
pub(super) const LABEL_ROOM_MAX: f64 = 0.25;

/// The display range for a linear axis: fit the data closely, with a small
/// breathing margin on each *free* end and none on a baseline end.
///
/// `mn`/`mx` arrive with the baseline (0) already folded in when `baseline` is
/// set, so an end sitting exactly at 0 is the baseline and is pinned; every
/// other end breathes by `AXIS_EXPAND`.
pub(super) fn fitted_range(mn: f64, mx: f64, baseline: bool, flush: bool) -> (f64, f64) {
    let span = mx - mn;
    if !(span > 0.0) {
        // A single distinct value: give it a unit of room either side so the
        // degenerate tick triple (v-1, v, v+1) has somewhere to land.
        return (mn - 1.0, mx + 1.0);
    }
    // A flush axis fits its data exactly; otherwise a free end breathes.
    let margin = if flush { 0.0 } else { span * AXIS_EXPAND };
    let mut lo = mn - margin;
    let mut hi = mx + margin;
    if baseline {
        if mn == 0.0 { lo = 0.0; } // data was all ≥ 0 — bottom is the baseline
        if mx == 0.0 { hi = 0.0; } // data was all ≤ 0 — top is the baseline
    }
    (lo, hi)
}

/// Pin an axis's range to its data and drop the ticks that fall outside it.
///
/// `nice_ticks` returns round tick values that bracket the data *outward*; this
/// keeps their values and labels but replaces the loose range with a close fit.
/// A tick beyond the fitted range is simply not drawn — the axis can end between
/// ticks, which is what lets the range follow the data.
///
/// The guard: if fewer than two ticks survive, the data span is narrower than a
/// single nice step, and fitting would leave a bare axis (Wilkinson's "too few
/// tick marks" failure, the twin of the one this fixes). There the loose
/// bracketing range is the lesser evil, so it is kept.
pub(super) fn fit_axis(t: TickSpec, mn: f64, mx: f64, baseline: bool, flush: bool) -> (TickSpec, (f64, f64)) {
    let (lo, hi) = fitted_range(mn, mx, baseline, flush);
    clip_ticks(t, lo, hi)
}

/// Adopt `lo..=hi` as the axis range and drop the ticks that fall outside it.
///
/// Split out of [`fit_axis`] because a tiling arrives at its range a different
/// way — from its own cell edges rather than from a fitted spread — but has the
/// identical duty afterwards. The guard below is why it must be shared: an axis
/// whose surviving ticks number fewer than two is bare, and both callers would
/// otherwise have to remember that.
pub(super) fn clip_ticks(t: TickSpec, lo: f64, hi: f64) -> (TickSpec, (f64, f64)) {
    let (clipped, kept) = ticks_inside(&t, lo, hi);
    if kept < 2 {
        let s = (t.scale_min(), t.scale_max());
        return (t, s);
    }
    (clipped, (lo, hi))
}

/// Adopt `lo..=hi` because the caller **stated** it — no guard.
///
/// [`clip_ticks`]'s guard exists for a range gog *derived*: when fitting would
/// bare the axis, the derivation was the lesser thing and is given up. A stated
/// domain (spec §10) is not a derivation to second-guess. Dropping back to the
/// loose bracketing range here would draw an axis over a range the caller did
/// not ask for, having accepted the binding — the silent override §12 forbids,
/// and it would take the polar cycle with it, since `limits = c(0, 24)` closing
/// the circle is the whole point of stating it.
///
/// A bare axis is possible in return and is the honest outcome: the ticks are
/// chosen against the stated range upstream, so it takes a deliberately odd
/// domain to get one.
pub(super) fn adopt_range(t: TickSpec, lo: f64, hi: f64) -> (TickSpec, (f64, f64)) {
    (ticks_inside(&t, lo, hi).0, (lo, hi))
}

/// The ticks that fall within `lo..=hi`, and how many there were.
pub(super) fn ticks_inside(t: &TickSpec, lo: f64, hi: f64) -> (TickSpec, usize) {
    let eps = (hi - lo) * 1e-9;
    let keep: Vec<usize> = (0..t.values.len())
        .filter(|&i| t.values[i] >= lo - eps && t.values[i] <= hi + eps)
        .collect();
    let values = keep.iter().map(|&i| t.values[i]).collect();
    let labels = keep.iter().map(|&i| t.labels[i].clone()).collect();
    (TickSpec { values, labels, step: t.step, widened: t.widened.clone() }, keep.len())
}

/// The declared time resolution of this field, wherever it is bound.
///
/// Read from the *source* tables, not the transformed frames: a transform
/// rebuilds its output with plain floats, but grouping by a date column keeps
/// the key values as epoch seconds, so the declaration on the source is still
/// the truth about what the axis is showing.
pub(super) fn detect_time(ctx: &RenderContext<'_>, field: &str) -> Option<crate::time::TimeUnit> {
    ctx.spec.layers.iter()
        .filter_map(|layer| ctx.resolve_data(&layer.data))
        .find_map(|df| df.time_unit(field))
}

/// Say that a stated tick count put more ticks on an axis than it carries, when
/// it did (spec §12).
///
/// The spacing the count asked for and the spacing drawn are the generator's own
/// words ([`crate::render::ticks::Widening`]), written the way the axis writes its
/// ticks, so the message and a linear axis say `2K` alike, and a calendar axis
/// says days. The direction is the two ways to a finer spacing: fewer ticks asked
/// for, or a shorter axis, since `limits` narrows the range the same number of
/// ticks has to cover.
pub(super) fn widened_ticks(c: &str, field: &str, asked: usize, ticks: &TickSpec) -> Option<Diagnostic> {
    let w = ticks.widened.as_ref()?;
    let field = if field.is_empty() { "<column>" } else { field };
    Some(Diagnostic {
        kind: crate::legality::DiagnosticKind::Assumption,
        message: format!(
            "gog: `{c}({field}, tick_count = {asked})` rounds to {from}, which puts more \
             than {max} ticks on this axis, the most one axis draws. The axis draws {to} \
             instead, the finest spacing that fits, so it is still labeled from end to end. \
             Ask for fewer ticks, or leave `tick_count` off; for {from}, narrow the axis \
             with `limits`.",
            from = w.asked,
            to = w.drawn,
            max = crate::render::ticks::MAX_TICKS,
        ),
    })
}

/// A position axis with a domain its author stated: judged in the axis's own
/// units, a log axis's in decades, and quoted back in the data's.
pub(super) struct StatedAxis<'a> {
    pub(super) channel: Channel,
    pub(super) field: &'a str,
    pub(super) judged: (Option<f64>, Option<f64>),
    pub(super) quoted: (Option<f64>, Option<f64>),
}

/// **A value a transform wrote outside a stated domain has no position** (spec
/// §10), which is what a logarithm meets at zero, and it gets the same mechanism:
/// the value is blanked, every mark skips it as it skips one a log axis cannot
/// place, and the count is said aloud. A table's own rows outside a domain are cut
/// before any transform reads them (`legality::limit_cut`), and `bin` cuts inside
/// the domain, so a value left outside here is one a statistic computed: a count
/// taller than `y(count, limits = c(0, 20))`, a ring past `y(depth, limits = c(0,
/// 3))`. Flat, the panel's clip cut them off at its edge, so a bar of 52 read as
/// one of 20; in `polar()` the ring was drawn past the circle.
///
/// Judged per drawn element: its value on the axis, and a cell's two edges along
/// it, since a ring that starts inside the domain and ends past it has no place
/// either. Returns, per axis, how many values were left out and of how many.
///
/// **Not for a mark that joins its rows into one path** (`line`, `area`, `step`,
/// `ribbon`, `path`). Each of those skips a value it cannot place by joining the
/// values either side of it, which would draw a straight chord where the curve
/// left the domain. Until a blank breaks the path there, such a curve is drawn as
/// before: cut at a flat panel's edge, and past the circle in `polar()`.
pub(super) fn unplace_outside_domains(
    spec: &PlotSpec, frames: &mut [DataFrame], axes: &[StatedAxis<'_>],
) -> Vec<(usize, usize)> {
    let mut counts = vec![(0usize, 0usize); axes.len()];
    for (layer, df) in spec.layers.iter().zip(frames.iter_mut()) {
        let joins = matches!(layer.mark, Mark::Line | Mark::Area | Mark::Step | Mark::Ribbon | Mark::Path);
        if layer.transforms.is_empty() || joins { continue }
        for (k, axis) in axes.iter().enumerate() {
            let (lo, hi) = axis.judged;
            let edges = match axis.channel {
                Channel::X => [crate::transform::CELL_START, crate::transform::CELL_END],
                _ => [crate::transform::CELL_LOWER, crate::transform::CELL_UPPER],
            };
            let cols: Vec<&str> = std::iter::once(axis.field).chain(edges)
                .filter(|c| df.float_col(c).is_some())
                .collect();
            if cols.is_empty() { continue }
            let tol = 1e-9 * (hi.unwrap_or(0.0) - lo.unwrap_or(0.0)).abs().max(1.0);
            let outside = |v: f64| v.is_finite()
                && (lo.is_some_and(|l| v < l - tol) || hi.is_some_and(|h| v > h + tol));
            let mut gone = vec![false; df.len()];
            let mut placed = vec![false; df.len()];
            for c in &cols {
                for (i, &v) in df.float_col(c).into_iter().flatten().enumerate() {
                    placed[i] |= v.is_finite();
                    gone[i] |= outside(v);
                }
            }
            counts[k].1 += placed.iter().filter(|&&p| p).count();
            let left_out = gone.iter().filter(|&&g| g).count();
            if left_out == 0 { continue }
            counts[k].0 += left_out;
            for c in &cols {
                let blanked: Vec<f64> = df.float_col(c).into_iter().flatten().zip(&gone)
                    .map(|(&v, &g)| if g { f64::NAN } else { v })
                    .collect();
                *df = match df.time_unit(c) {
                    Some(u) => std::mem::take(df).with_time(*c, blanked, u),
                    None => std::mem::take(df).with_float(*c, blanked),
                };
            }
        }
    }
    counts
}

/// What [`unplace_outside_domains`] left out, said the way a table's own rows are
/// (`legality::check_limit_rows`), with the same direction.
pub(super) fn warn_outside_domains(out: &mut Vec<Diagnostic>, axes: &[StatedAxis<'_>], counts: &[(usize, usize)]) {
    for (axis, &(left_out, total)) in axes.iter().zip(counts) {
        if left_out == 0 { continue }
        let c = if axis.channel == Channel::X { "x" } else { "y" };
        let end = |v: Option<f64>| v.map_or("…".to_string(), |v| format!("{v}"));
        out.push(Diagnostic {
            kind: crate::legality::DiagnosticKind::Assumption,
            message: format!(
                "gog: `{c}({field}, limits = …)` leaves out {left_out} of {total} computed \
                 values, which fall outside [{}, {}]. Stating a domain is what removes them — \
                 widen the limits if they should be drawn.",
                end(axis.quoted.0), end(axis.quoted.1), field = axis.field,
            ),
        });
    }
}

/// Say how many rows a log axis could not place, when any survive to this point.
///
/// `legality` refuses the plot outright when the *source* data has values a
/// logarithm is undefined at, so in the normal path this never fires. It covers
/// the two cases the check cannot see: `GOG_STRICT=0`, where the caller has
/// asked for best effort, and a transform whose *output* goes non-positive —
/// `bar * sum` over a column that nets out at zero. Dropping a row without
/// saying so is the one outcome gog forbids (§12) — which is why
/// this lands in `remarks` rather than on stderr: a browser has no stderr, and
/// a dropped-rows report only the CLI can hear is the silent drop one hop out.
pub(super) fn warn_unplaceable(out: &mut Vec<Diagnostic>, eff: &[&DataFrame], field: &str, is_log: bool, axis: &str) {
    if !is_log || field.is_empty() { return }
    let (mut bad, mut total) = (0usize, 0usize);
    for df in eff {
        let Some(vals) = df.float_col(field) else { continue };
        total += vals.len();
        bad += vals.iter().filter(|v| !v.is_finite()).count();
    }
    if bad == 0 { return }
    out.push(Diagnostic {
        kind: crate::legality::DiagnosticKind::Assumption,
        message: format!(
            "gog: {bad} of {total} rows have no place on the log `{axis}` axis — a \
             logarithm is undefined at zero and below, so they are not drawn. Filter \
             those rows, or drop `scale = \"log\"` from `{axis}({field})`."
        ),
    });
}

pub(super) fn channel_range_eff(eff: &[&DataFrame], field: &str) -> Option<(f64, f64)> {
    let mut mn = f64::INFINITY;
    let mut mx = f64::NEG_INFINITY;
    for df in eff {
        let Some(vals) = df.float_col(field) else { continue };
        for &v in vals { if v < mn { mn = v; } if v > mx { mx = v; } }
    }
    if mn.is_finite() { Some((mn, mx)) } else { None }
}

// **The missing-binding warnings were deleted 2026-08-17, and the deletion is the
// fourth narrowing of the same function.** It printed three messages -- a reading
// transform with no `y`, one with no `x`, and a mark with no `x` -- each ending
// "Rendering empty chart".
//
// Every case it named is refused by `legality::check` first, measured across all
// twelve marks on both axes, so the warning was never the only voice and never the
// deciding one. Its sentence was false in both switch positions besides: under
// `GOG_STRICT=1` nothing is rendered at all, and under `GOG_STRICT=0` a chart *is*
// rendered, so "Rendering empty chart" described neither.
//
// The history is the argument. The comments removed with it recorded three earlier
// narrowings, each after this function libeled a plot the gate had already blessed:
// a `partition`, a `text` in `nest` drawing its labels perfectly, and
// `interval * bounds(lo, hi) + y(term)` -- a good forest plot told it was empty.
// Each fix taught it one more of `legality`'s exceptions. A second copy of a
// judgment that belongs to the gate has to be taught every exception the gate
// knows, forever, and it is wrong in the window before it is. So it is gone rather
// than corrected a fourth time: one question, one answer, in `legality`.

#[allow(clippy::too_many_arguments)]
pub(super) fn bar_x_ticks_eff(
    bar_frames: &[&DataFrame],
    x_field: &str,
    data_min: f64, data_max: f64,
    // What a continuous-looking axis chooses its ticks over: the data's own span,
    // or the one a page shares (`build_axis`).
    ticks_over: (f64, f64),
    target_count: Option<usize>,
    // What the axis will show, for the ceiling on the count (`build_axis`).
    shown: (f64, f64),
) -> (TickSpec, (f64, f64)) {
    let mut vals: Vec<f64> = Vec::new();
    for df in bar_frames {
        let Some(col) = df.float_col(x_field) else { continue };
        for &v in col {
            if !vals.iter().any(|&e: &f64| (e - v).abs() < 1e-10) { vals.push(v); }
        }
    }
    vals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    if vals.is_empty() || vals.len() > 20 {
        // Too many distinct positions to tick one-per-bar; this axis behaves
        // like a continuous one, so it fits its data the same way (no baseline —
        // it carries positions, not amounts).
        let t = nice_ticks_within(ticks_over.0, ticks_over.1, target_count.unwrap_or(5), Some(shown));
        return fit_axis(t, data_min, data_max, false, false);
    }

    let step = if vals.len() > 1 {
        vals.windows(2)
            .map(|w| (w[1] - w[0]).abs())
            .filter(|&d| d > 1e-12)
            .fold(f64::INFINITY, f64::min)
    } else {
        (data_max - data_min).abs().max(1.0)
    };
    let step = if step.is_infinite() { 1.0 } else { step };
    let padding   = step * 0.5;
    // **The axis holds every layer's positions, not only the bars'.** It was built
    // from the bar slots alone, so a second table's points past the last bar were
    // placed beyond the panel and clipped away, in silence: `bar` over 2019 to
    // 2023 with a forecast `point` at 2024 to 2026 drew the points at cx 847 to
    // 1133 on an 800-wide canvas. The span of every layer (`data_min`/`data_max`)
    // now widens the range, with the same half slot of breathing as a bar. Where
    // the bars stand at an even step, a year each, the ticks go on at that step
    // into the added span, so the forecast years are labeled as the bars' are.
    let (first, last) = (vals[0], vals[vals.len() - 1]);
    let (lo, hi) = (first.min(data_min), last.max(data_max));
    let mut ticks = vals.clone();
    let even = vals.len() > 1
        && vals.windows(2).all(|w| ((w[1] - w[0]) - step).abs() <= 1e-9 * step.abs().max(1.0));
    if even {
        let slack = 1e-9 * step.abs().max(1.0);
        let mut t = first - step;
        while t >= lo - slack && ticks.len() < 20 {
            ticks.insert(0, t);
            t -= step;
        }
        let mut t = last + step;
        while t <= hi + slack && ticks.len() < 20 {
            ticks.push(t);
            t += step;
        }
    }
    (ticks_at(ticks), (lo - padding, hi + padding))
}
