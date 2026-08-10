/// Simple in-memory DataFrame — a named column store.
///
/// This is a thin stand-in until Apache Arrow integration lands.
/// Every column is either a float (f64) or a string sequence.
use crate::time::TimeUnit;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub enum Column {
    /// Numbers, optionally declared to be moments in time.
    ///
    /// `time` is how a date column keeps being a date across the wire: the
    /// values are epoch seconds — every numeric path works on them unchanged —
    /// and the marker is what tells an axis to cut them at calendar
    /// boundaries instead of round numbers. Without it a `Date` arrived as
    /// the category string `"2026-01-02"`, the same silent fall-through a
    /// factor's levels had.
    Float {
        values: Vec<f64>,
        time: Option<TimeUnit>,
    },
    /// Text, optionally carrying the category order it was declared in.
    ///
    /// `levels` is how an R factor's order survives the trip: writing
    /// `factor(x, levels = c("Low", "Medium", "High"))` is the normal way to say
    /// what order categories go in, and without somewhere to put it the order
    /// was silently lost and the axis fell back to the order of the rows.
    Str {
        values: Vec<String>,
        levels: Option<Vec<String>>,
    },
}

#[derive(Debug, Clone, Default)]
pub struct DataFrame {
    columns: HashMap<String, Column>,
    len: usize,
}

impl DataFrame {
    pub fn new() -> Self {
        Self::default()
    }

    /// Drop a column, if it is there. The one operation a *rename* needs that
    /// `with_*` cannot do: `transform::share_cells` divides a tally into shares and
    /// has to retire the old name with it, since the legend titles itself from the
    /// column and would otherwise find both.
    pub fn without_col(mut self, name: &str) -> Self {
        self.columns.remove(name);
        self
    }

    pub fn with_float(mut self, name: impl Into<String>, values: Vec<f64>) -> Self {
        self.len = self.len.max(values.len());
        self.columns.insert(name.into(), Column::Float { values, time: None });
        self
    }

    /// A numeric column whose values are moments — epoch seconds, declared at
    /// the given resolution.
    pub fn with_time(mut self, name: impl Into<String>, values: Vec<f64>, unit: TimeUnit) -> Self {
        self.len = self.len.max(values.len());
        self.columns.insert(name.into(), Column::Float { values, time: Some(unit) });
        self
    }

    pub fn with_str(mut self, name: impl Into<String>, values: Vec<String>) -> Self {
        self.len = self.len.max(values.len());
        self.columns.insert(name.into(), Column::Str { values, levels: None });
        self
    }

    /// A text column that declares what order its categories go in.
    pub fn with_levels(
        mut self,
        name: impl Into<String>,
        values: Vec<String>,
        levels: Vec<String>,
    ) -> Self {
        self.len = self.len.max(values.len());
        self.columns.insert(
            name.into(),
            Column::Str { values, levels: Some(levels) },
        );
        self
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn float_col(&self, name: &str) -> Option<&Vec<f64>> {
        match self.columns.get(name)? {
            Column::Float { values, .. } => Some(values),
            _ => None,
        }
    }

    /// The resolution this column's moments were declared at, if it is one of
    /// dates or timestamps. `None` for a plain number — the caller's signal to
    /// treat the values as quantities rather than calendar points.
    pub fn time_unit(&self, name: &str) -> Option<TimeUnit> {
        match self.columns.get(name)? {
            Column::Float { time, .. } => *time,
            _ => None,
        }
    }

    pub fn str_col(&self, name: &str) -> Option<&Vec<String>> {
        match self.columns.get(name)? {
            Column::Str { values, .. } => Some(values),
            _ => None,
        }
    }

    /// The order this column's categories were declared in, if any.
    pub fn levels(&self, name: &str) -> Option<&[String]> {
        match self.columns.get(name)? {
            Column::Str { levels, .. } => levels.as_deref(),
            _ => None,
        }
    }

    pub fn column_names(&self) -> impl Iterator<Item = &str> {
        self.columns.keys().map(String::as_str)
    }

    /// The rows whose value in `field` equals `value` — one facet panel's slice.
    ///
    /// Every column keeps its declarations (a factor's levels, a date column's
    /// resolution): a panel is a subset of the rows, not a new table, so nothing
    /// about the columns' meaning changes. A frame that does not have the column
    /// is returned whole — that is what lets a layer without the facet variable
    /// appear in every panel, the way a shared reference layer should.
    pub fn filter_str_eq(&self, field: &str, value: &str) -> DataFrame {
        let Some(keys) = self.str_col(field) else { return self.clone() };
        let keep: Vec<bool> = keys.iter().map(|k| k == value).collect();
        self.keep_rows(&keep)
    }

    /// The rows whose value in `field` equals `value` — one `play` frame's slice.
    ///
    /// [`filter_str_eq`](Self::filter_str_eq)'s numeric twin, and it inherits both
    /// of that method's contracts: the columns keep their declarations, and a frame
    /// that does not have the column is returned **whole**, which is what lets a
    /// layer with no `play` binding stand still behind one that moves.
    ///
    /// Exact equality is the right test rather than a tolerance, because `value`
    /// came out of this column by way of [`frames_across`] — comparing a value to
    /// itself, not to a computed neighbor.
    pub fn filter_float_eq(&self, field: &str, value: f64) -> DataFrame {
        let Some(keys) = self.float_col(field) else { return self.clone() };
        let keep: Vec<bool> = keys.iter().map(|k| *k == value).collect();
        self.keep_rows(&keep)
    }

    /// The rows `keep` marks true, every column keeping its declarations.
    ///
    /// Split out of [`filter_str_eq`](Self::filter_str_eq) when scale limits
    /// gained a second row filter (spec §10): a stated domain excludes rows the
    /// same way a facet does, and the two must agree about what surviving means
    /// — a factor's levels, a date column's resolution — or a limited facet
    /// would quietly lose one of them. `keep` is one entry per row; any other
    /// length is a caller bug and returns the frame untouched rather than
    /// misaligning the columns.
    pub fn keep_rows(&self, keep: &[bool]) -> DataFrame {
        if keep.len() != self.len {
            return self.clone();
        }
        let mut out = DataFrame::new();
        for (name, col) in &self.columns {
            let filtered = match col {
                Column::Float { values, time } => Column::Float {
                    values: values.iter().zip(keep).filter(|(_, &k)| k).map(|(v, _)| *v).collect(),
                    time: *time,
                },
                Column::Str { values, levels } => Column::Str {
                    values: values.iter().zip(keep).filter(|(_, &k)| k).map(|(v, _)| v.clone()).collect(),
                    levels: levels.clone(),
                },
            };
            out.columns.insert(name.clone(), filtered);
        }
        out.len = keep.iter().filter(|&&k| k).count();
        out
    }

    /// Each row repeated as many times as `times` says — the inverse of a tally.
    ///
    /// A counting statistic collapses rows into one row per slot carrying how many
    /// there were; a mark that draws **one glyph per observation** needs those rows
    /// back, which is the dot plot's pile (`transform::pile`, spec §5). Every column
    /// keeps its declaration, so a factor's order and a date's resolution survive
    /// the expansion, and a count of 0 drops the row — which is how an empty bin
    /// draws nothing. `times` is one entry per row; any other length is a caller
    /// bug and returns the frame untouched rather than misaligning the columns.
    pub fn repeat_rows(&self, times: &[usize]) -> DataFrame {
        if times.len() != self.len {
            return self.clone();
        }
        let idx: Vec<usize> = times.iter().enumerate()
            .flat_map(|(i, &n)| std::iter::repeat_n(i, n))
            .collect();
        let mut out = DataFrame::new();
        for (name, col) in &self.columns {
            let expanded = match col {
                Column::Float { values, time } => Column::Float {
                    values: idx.iter().map(|&i| values.get(i).copied().unwrap_or(f64::NAN)).collect(),
                    time: *time,
                },
                Column::Str { values, levels } => Column::Str {
                    values: idx.iter().map(|&i| values.get(i).cloned().unwrap_or_default()).collect(),
                    levels: levels.clone(),
                },
            };
            out.columns.insert(name.clone(), expanded);
        }
        out.len = idx.len();
        out
    }

    /// Stack frames of the same shape end to end — the rows of the second after
    /// the rows of the first, and so on.
    ///
    /// The schema is taken from the first frame: each of its columns is
    /// concatenated across every frame, keeping that column's declaration (a
    /// factor's levels, a date's resolution). This is the join a grouped
    /// transform makes when it runs a statistic within each color group and
    /// then reassembles one frame the renderer can color — every part carries
    /// the same `(key, out, group)` columns, so column-by-column concatenation is
    /// exactly right. A column missing from a later frame contributes no rows,
    /// which would misalign the columns; callers pass frames that agree.
    pub fn vconcat(frames: &[DataFrame]) -> DataFrame {
        let mut out = DataFrame::new();
        let Some(first) = frames.first() else { return out };
        let names: Vec<String> = first.columns.keys().cloned().collect();
        for name in names {
            let col = match &first.columns[&name] {
                Column::Float { time, .. } => {
                    let mut values = Vec::new();
                    for f in frames {
                        if let Some(c) = f.float_col(&name) { values.extend(c.iter().copied()); }
                    }
                    Column::Float { values, time: *time }
                }
                Column::Str { levels, .. } => {
                    let mut values = Vec::new();
                    for f in frames {
                        if let Some(c) = f.str_col(&name) { values.extend(c.iter().cloned()); }
                    }
                    Column::Str { values, levels: levels.clone() }
                }
            };
            let n = match &col {
                Column::Float { values, .. } => values.len(),
                Column::Str { values, .. } => values.len(),
            };
            out.len = out.len.max(n);
            out.columns.insert(name, col);
        }
        out
    }
}

/// The distinct values of a text column, in the order they should be shown.
///
/// One owner, because the axis, the color assignment, and every legend have to
/// agree: a chart whose bars run Low, Medium, High above a legend that runs
/// High, Low, Medium is worse than either order alone.
///
/// A **declared** order wins — an R factor's levels, which is how someone says
/// "Low, Medium, High" and means it. Otherwise it is first appearance in the
/// data, the only order the data itself supplies.
///
/// Levels decide the *order*, never the *membership*. A level with no rows draws
/// no mark, and a labeled gap a reader cannot interpret is worse than a shorter
/// axis; conversely a value present in the data but missing from the levels
/// keeps its place at the end rather than vanishing, because dropping a row in
/// silence is the one outcome the grammar refuses.
pub fn categories_across(frames: &[&DataFrame], field: &str) -> Vec<String> {
    let mut seen: Vec<String> = Vec::new();
    for df in frames {
        let Some(vals) = df.str_col(field) else { continue };
        for v in vals {
            if !seen.contains(v) {
                seen.push(v.clone());
            }
        }
    }

    let Some(levels) = frames.iter().find_map(|df| df.levels(field)) else {
        return seen;
    };

    let mut ordered: Vec<String> = levels.iter().filter(|l| seen.contains(l)).cloned().collect();
    for v in seen {
        if !ordered.contains(&v) {
            ordered.push(v);
        }
    }
    ordered
}

/// How a row is tested for membership in one frame of a `play` sequence.
///
/// Two variants because `play` accepts either column type, which is the one place
/// it parts company with `facet` (see [`frames_across`]). A category tests by
/// string; a number tests by exact `f64` equality, which is safe here for the
/// reason [`Lattice::of`] relies on it — every value compared came out of the
/// column itself, so the ones that should coincide are bit-identical rather than
/// merely close.
#[derive(Debug, Clone, PartialEq)]
pub enum FrameKey {
    Str(String),
    Float(f64),
}

/// One frame of a `play` sequence: which rows it holds, and what it is called.
///
/// The two are kept apart because they are not the same string. A year arrives as
/// the `f64` `1957.0` and must read `1957`; a date column arrives as epoch seconds
/// and must read as a date. Formatting at the point of comparison would have the
/// strip and the filter disagree the moment either changed.
#[derive(Debug, Clone)]
pub struct FrameLevel {
    pub key: FrameKey,
    /// What the play strip reads while this frame is showing.
    pub label: String,
}

/// A distinct value as a label a reader looks *up* rather than measures.
///
/// Deliberately not `ticks::format_tick`, which would print a year 1957 as "2K":
/// that function formats a point on a measured axis, where the step says how much
/// precision is meaningful and a suffix is a kindness. A frame level has no step
/// and no neighbors — it is a name — so the rule is the plain one: an exact
/// integer prints as an integer, and anything else keeps just the digits it needs.
fn fmt_level(v: f64) -> String {
    if !v.is_finite() {
        return String::new();
    }
    if v == v.trunc() && v.abs() < 1e15 {
        return format!("{v:.0}");
    }
    let s = format!("{v:.6}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    s.to_string()
}

/// The frames a column cuts a plot into — [`categories_across`] read in time.
///
/// `play` and `facet` are the same partition: both split the rows into subsets by
/// a column's distinct values. One lays the subsets out across the page, the other
/// in sequence. So this is `categories_across` with one deliberate difference —
/// **it accepts a numeric column, where `check_facet` refuses one.**
///
/// That difference is a ruling, not an oversight, and the reason is what the two
/// subsets compete for. Facet panels compete for **page area**: N panels each get
/// 1/N of it, so a hundred of them are unreadable at any canvas size, and refusing
/// a continuous column is the only way to say so before the picture is drawn.
/// Frames compete for **time**: each plays at full size, and a hundred of them is
/// a longer loop rather than a smaller picture. The cost function genuinely
/// differs, which is why the canonical sentence has always been `play(year)` on a
/// number. What a long sequence *does* earn is a word about the loop it implies,
/// and `legality::check_play` says it.
///
/// A text column delegates whole, so a declared factor order runs the frames the
/// same way it runs an axis. A numeric column takes its distinct values **sorted
/// ascending**, which is also what puts a year, or a date, in the only order it
/// could sensibly play in — there is nothing to declare and nothing to guess.
pub fn frames_across(frames: &[&DataFrame], field: &str) -> Vec<FrameLevel> {
    // A text column is the facet case exactly, levels and all.
    if frames.iter().any(|df| df.str_col(field).is_some()) {
        return categories_across(frames, field)
            .into_iter()
            .map(|v| FrameLevel { key: FrameKey::Str(v.clone()), label: v })
            .collect();
    }

    let unit = frames.iter().find_map(|df| df.time_unit(field));
    let mut seen: Vec<f64> = Vec::new();
    for df in frames {
        let Some(vals) = df.float_col(field) else { continue };
        for v in vals {
            if v.is_finite() {
                seen.push(*v);
            }
        }
    }
    seen.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    seen.dedup();

    seen.into_iter()
        .map(|v| FrameLevel {
            key: FrameKey::Float(v),
            label: match unit {
                Some(u) => crate::time::fmt_moment(v, u),
                None => fmt_level(v),
            },
        })
        .collect()
}

/// The grid two numeric columns describe — [`categories_across`]'s numeric
/// sibling, and what a `surface` reads instead of a footprint (spec §15).
///
/// A `bar` in the cube stands on a **cell** and reads its two pairs of edges off
/// the table. A surface has no cell: its rows are **nodes**, and the sheet is the
/// quads between adjacent ones — so what it needs is not an extent but an
/// *adjacency*, and the data already states one. The sorted distinct values of `x`
/// are the mesh's columns, of `y` its rows, and each row of the table lands at one
/// crossing.
///
/// **Recovered rather than declared, so one mechanism serves every source.** A
/// table the user built on a grid and a `density` estimate over a cut plane arrive
/// here the same shape, and neither has to say so. Exact equality is the right
/// test for both: `expand.grid` repeats the identical `f64`, and a cut mesh
/// computes each cell's center from the same cutpoints, so the values that should
/// coincide are bit-identical rather than merely close. Values that *don't*
/// coincide simply describe a finer lattice with holes in it, which is the case
/// [`Lattice::faces`] reports rather than papers over.
///
/// Lives here, below both callers, because `legality` asks whether a face can be
/// drawn and the mark writer draws it: two copies of this would be the drift that
/// `positions` was moved down to end.
pub struct Lattice {
    /// The distinct `x` values, ascending — the mesh's columns.
    pub xs: Vec<f64>,
    /// The distinct `y` values, ascending — the mesh's rows.
    pub ys: Vec<f64>,
    /// `node[j * xs.len() + i]` is the table row sitting at column `i`, row `j`,
    /// or `None` where the grid has a hole. A later row wins a collision, which
    /// only arises when a table states one crossing twice.
    node: Vec<Option<usize>>,
}

impl Lattice {
    /// Read the lattice two columns describe. `None` if either column is absent
    /// or holds no finite value — the caller then has nothing to draw and says so.
    pub fn of(xs_col: &[f64], ys_col: &[f64]) -> Option<Lattice> {
        let n = xs_col.len().min(ys_col.len());
        let axis = |col: &[f64]| -> Vec<f64> {
            let mut v: Vec<f64> = col[..n].iter().copied().filter(|f| f.is_finite()).collect();
            v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            v.dedup();
            v
        };
        let (xs, ys) = (axis(xs_col), axis(ys_col));
        if xs.is_empty() || ys.is_empty() {
            return None;
        }
        let mut node = vec![None; xs.len() * ys.len()];
        for r in 0..n {
            // NaN is skipped *before* the search, not by it: inside the
            // comparator a NaN would panic on the `unwrap`, never reach the
            // `else` arm the way ±infinity does — and a NaN coordinate is
            // reachable, through `GOG_STRICT=0` on a log axis.
            if !xs_col[r].is_finite() || !ys_col[r].is_finite() {
                continue; // a non-finite coordinate — no crossing to sit at
            }
            let (Ok(i), Ok(j)) = (
                xs.binary_search_by(|p| p.partial_cmp(&xs_col[r]).unwrap()),
                ys.binary_search_by(|p| p.partial_cmp(&ys_col[r]).unwrap()),
            ) else {
                continue; // finite values are all on the axes; kept for safety
            };
            node[j * xs.len() + i] = Some(r);
        }
        Some(Lattice { xs, ys, node })
    }

    /// The table row at column `i`, row `j`, if the grid has one there.
    pub fn at(&self, i: usize, j: usize) -> Option<usize> {
        *self.node.get(j * self.xs.len() + i)?
    }

    /// Every complete block of four adjacent nodes, as the table rows at its
    /// corners in `(i,j), (i+1,j), (i+1,j+1), (i,j+1)` order — counter-clockwise
    /// seen from above, matching the winding the projector's face culling expects.
    ///
    /// A block with any corner missing is skipped, which is what makes a hole an
    /// opening in the sheet rather than a face drawn across it.
    pub fn faces(&self) -> Vec<[usize; 4]> {
        let mut out = Vec::new();
        for j in 0..self.ys.len().saturating_sub(1) {
            for i in 0..self.xs.len().saturating_sub(1) {
                if let (Some(a), Some(b), Some(c), Some(d)) = (
                    self.at(i, j), self.at(i + 1, j), self.at(i + 1, j + 1), self.at(i, j + 1),
                ) {
                    out.push([a, b, c, d]);
                }
            }
        }
        out
    }

    /// How many crossings the lattice has, and how many the table filled — the two
    /// numbers the partial-mesh Assumption reports (spec §15).
    pub fn fill(&self) -> (usize, usize) {
        (self.node.len(), self.node.iter().filter(|n| n.is_some()).count())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(values: &[&str], levels: Option<&[&str]>) -> DataFrame {
        let v: Vec<String> = values.iter().map(|s| s.to_string()).collect();
        match levels {
            Some(l) => DataFrame::new().with_levels("sev", v, l.iter().map(|s| s.to_string()).collect()),
            None => DataFrame::new().with_str("sev", v),
        }
    }

    #[test]
    fn without_levels_the_data_order_stands() {
        let df = frame(&["High", "Low", "Medium"], None);
        assert_eq!(categories_across(&[&df], "sev"), ["High", "Low", "Medium"]);
    }

    /// A 3×2 grid stated in scrambled row order still recovers as a 3×2 grid with
    /// two faces — the whole point of *recovering* the lattice rather than trusting
    /// the table to arrive sorted.
    #[test]
    fn a_grid_recovers_its_own_lattice_whatever_order_the_rows_arrive_in() {
        let xs = [2.0, 0.0, 1.0, 1.0, 2.0, 0.0];
        let ys = [10.0, 20.0, 20.0, 10.0, 20.0, 10.0];
        let l = Lattice::of(&xs, &ys).unwrap();
        assert_eq!(l.xs, [0.0, 1.0, 2.0]);
        assert_eq!(l.ys, [10.0, 20.0]);
        assert_eq!(l.fill(), (6, 6));
        assert_eq!(l.faces().len(), 2, "a 3x2 grid has two quads");
        // Each face names its four table rows counter-clockwise from (i,j).
        assert_eq!(l.faces()[0], [5, 3, 2, 1]);
    }

    /// A NaN coordinate is a row with no crossing, never a panic. Reachable:
    /// `GOG_STRICT=0` on a log axis writes NaN into the column a `surface`
    /// then reads, and a panic here aborts the CLI and traps the wasm module.
    #[test]
    fn a_nan_coordinate_is_skipped_rather_than_a_panic() {
        let xs = [0.0, 1.0, f64::NAN, 0.0, 1.0, f64::NEG_INFINITY];
        let ys = [0.0, 0.0, 0.0, 1.0, 1.0, 1.0];
        let l = Lattice::of(&xs, &ys).unwrap();
        assert_eq!(l.xs, [0.0, 1.0]);
        assert_eq!(l.ys, [0.0, 1.0]);
        // Four finite crossings sit; the NaN row and the -inf row sit nowhere.
        assert_eq!(l.fill(), (4, 4));
    }

    /// A scatter is the case the fatal check exists for: *n* points in general
    /// position describe an *n*×*n* lattice holding *n* nodes, and not one complete
    /// block of four. A surface over it would be an empty panel (spec §15).
    #[test]
    fn a_scatter_describes_a_lattice_with_no_complete_face() {
        let xs = [0.11, 0.42, 0.77, 0.93, 0.28];
        let ys = [0.51, 0.13, 0.88, 0.34, 0.67];
        let l = Lattice::of(&xs, &ys).unwrap();
        assert_eq!(l.fill(), (25, 5));
        assert!(l.faces().is_empty(), "scattered points must yield no face");
    }

    /// A hole is an opening in the sheet, not a face drawn across it: drop one
    /// corner of a 3×3 grid and the three faces not touching it survive.
    #[test]
    fn a_hole_removes_only_the_faces_that_touch_it() {
        let mut xs = Vec::new();
        let mut ys = Vec::new();
        for j in 0..3 {
            for i in 0..3 {
                if (i, j) == (0, 0) { continue }
                xs.push(i as f64);
                ys.push(j as f64);
            }
        }
        let l = Lattice::of(&xs, &ys).unwrap();
        assert_eq!(l.fill(), (9, 8));
        assert_eq!(l.faces().len(), 3, "a full 3x3 has four quads; one corner kills one");
        assert_eq!(l.at(0, 0), None);
    }

    #[test]
    fn declared_levels_decide_the_order() {
        // The whole point: the rows arrive High, Low, Medium and the chart
        // still reads Low, Medium, High.
        let df = frame(&["High", "Low", "Medium"], Some(&["Low", "Medium", "High"]));
        assert_eq!(categories_across(&[&df], "sev"), ["Low", "Medium", "High"]);
    }

    #[test]
    fn a_level_with_no_rows_gets_no_slot() {
        // Levels say what order, the data says what is there.
        let df = frame(&["High", "Low"], Some(&["Low", "Medium", "High"]));
        assert_eq!(categories_across(&[&df], "sev"), ["Low", "High"]);
    }

    #[test]
    fn a_value_missing_from_the_levels_is_kept_not_dropped() {
        // Silently losing a row is the one outcome the grammar refuses, so an
        // unlisted value goes last rather than nowhere.
        let df = frame(&["High", "Other", "Low"], Some(&["Low", "High"]));
        assert_eq!(categories_across(&[&df], "sev"), ["Low", "High", "Other"]);
    }

    #[test]
    fn levels_carry_across_layers() {
        let a = frame(&["High"], Some(&["Low", "Medium", "High"]));
        let b = frame(&["Low", "Medium"], None);
        assert_eq!(categories_across(&[&a, &b], "sev"), ["Low", "Medium", "High"]);
    }

    #[test]
    fn a_column_with_no_levels_reports_none() {
        let df = frame(&["a"], None);
        assert!(df.levels("sev").is_none());
        assert!(df.levels("nope").is_none());
    }

    #[test]
    fn vconcat_stacks_rows_and_keeps_the_group_levels() {
        // Two per-group results reassembled into one frame: the key and count
        // columns run end to end, and the group column keeps its declared order
        // so colors and the legend follow the factor, not row order.
        let a = DataFrame::new()
            .with_float("x", vec![1.0, 2.0])
            .with_float("count", vec![3.0, 4.0])
            .with_levels("g", vec!["b".into(), "b".into()], vec!["b".into(), "a".into()]);
        let b = DataFrame::new()
            .with_float("x", vec![1.0, 2.0])
            .with_float("count", vec![5.0, 6.0])
            .with_levels("g", vec!["a".into(), "a".into()], vec!["b".into(), "a".into()]);
        let out = DataFrame::vconcat(&[a, b]);
        assert_eq!(out.len(), 4);
        assert_eq!(out.float_col("x").unwrap(), &[1.0, 2.0, 1.0, 2.0]);
        assert_eq!(out.float_col("count").unwrap(), &[3.0, 4.0, 5.0, 6.0]);
        assert_eq!(out.str_col("g").unwrap(), &["b", "b", "a", "a"]);
        assert_eq!(out.levels("g"), Some(["b".to_string(), "a".to_string()].as_slice()));
    }

    #[test]
    fn vconcat_of_nothing_is_empty() {
        assert!(DataFrame::vconcat(&[]).is_empty());
    }

    #[test]
    fn a_time_column_is_still_numeric_and_knows_its_unit() {
        // The marker rides on the Float variant precisely so every numeric
        // path — ranges, transforms, scales — works on a date unchanged.
        let df = DataFrame::new()
            .with_time("day", vec![0.0, 86_400.0], crate::time::TimeUnit::Day)
            .with_float("v", vec![1.0, 2.0]);
        assert_eq!(df.time_unit("day"), Some(crate::time::TimeUnit::Day));
        assert!(df.float_col("day").is_some());
        assert!(df.time_unit("v").is_none());
        assert!(df.time_unit("nope").is_none());
    }

    /// A year is an `f64` on the wire and must not read like one. This is the
    /// whole reason `FrameLevel` keeps the key and the label apart.
    #[test]
    fn a_numeric_frame_reads_as_a_number_and_plays_in_order() {
        let df = DataFrame::new()
            .with_float("year", vec![1967.0, 1957.0, 1967.0, 1962.0])
            .with_float("v", vec![1.0, 2.0, 3.0, 4.0]);
        let frames = frames_across(&[&df], "year");
        let labels: Vec<&str> = frames.iter().map(|f| f.label.as_str()).collect();
        assert_eq!(labels, ["1957", "1962", "1967"], "sorted ascending, and no `.0`");
        assert_eq!(frames[0].key, FrameKey::Float(1957.0));
    }

    /// A text column is the facet case exactly, declared order and all — the
    /// point being that `play` did not grow a second ordering rule.
    #[test]
    fn a_categorical_frame_honors_declared_levels() {
        let df = DataFrame::new().with_levels(
            "size",
            vec!["High".into(), "Low".into(), "Medium".into()],
            vec!["Low".into(), "Medium".into(), "High".into()],
        );
        let frames = frames_across(&[&df], "size");
        let labels: Vec<&str> = frames.iter().map(|f| f.label.as_str()).collect();
        assert_eq!(labels, ["Low", "Medium", "High"]);
    }

    /// A date column plays as dates. Without the `TimeUnit` branch a `Date` frame
    /// would read as epoch seconds — the same silent fall-through the marker
    /// exists to close.
    #[test]
    fn a_date_frame_reads_as_a_date() {
        let df = DataFrame::new()
            .with_time("day", vec![86_400.0, 0.0], crate::time::TimeUnit::Day);
        let frames = frames_across(&[&df], "day");
        let labels: Vec<&str> = frames.iter().map(|f| f.label.as_str()).collect();
        assert_eq!(labels, ["1970-01-01", "1970-01-02"]);
    }

    /// The static-backdrop rule, one level down: a table without the column is
    /// returned whole, so a layer that does not animate is drawn in every frame.
    #[test]
    fn a_frame_filter_leaves_a_table_without_the_column_alone() {
        let df = DataFrame::new().with_float("v", vec![1.0, 2.0, 3.0]);
        assert_eq!(df.filter_float_eq("year", 1957.0).len(), 3);

        let played = DataFrame::new()
            .with_float("year", vec![1957.0, 1962.0, 1957.0])
            .with_float("v", vec![1.0, 2.0, 3.0]);
        assert_eq!(played.filter_float_eq("year", 1957.0).len(), 2);
    }

    /// A fractional level keeps the digits it needs and no more.
    #[test]
    fn a_fractional_level_is_not_padded() {
        assert_eq!(fmt_level(0.5), "0.5");
        assert_eq!(fmt_level(-3.0), "-3");
        assert_eq!(fmt_level(1957.0), "1957");
    }
}
