//! Series a path cannot draw: a `line`, `area`, `step`, `path` or `ribbon` runs
//! through its positions, so a series that holds one draws nothing (§12).

use super::*;

/// What a path layer drew and left out, counted in every panel and at every
/// moment it plays, and said once after the loop.
///
/// **Counted in the drawing rather than in `legality`**, because the series a path
/// draws are the renderer's: the rows one panel keeps at one moment, after the
/// layer's transforms, split by every channel that splits. A count made anywhere
/// else would be a second copy of all three, free to disagree with the picture.
/// The drawing has refused before, for the panel left no room (ruled 2026-09-29).
///
/// **One condition, whatever reached it.** A one-row table, one row per color, one
/// per facet panel, one per frame of a line laid on its side: each drew an empty
/// panel, often under a legend, and said nothing. The two refusals that existed
/// named one cause each (`legality::splits_its_domain`), so every other road to
/// the same picture was open.
#[derive(Default)]
pub(super) struct SeriesTally {
    /// Series seen, one per series in each panel at each moment.
    seen: usize,
    /// Of those, the ones holding two positions or more, which draw.
    drawn: usize,
    /// A cell this count could not read: a position column it could not find, a
    /// split it does not know, a series with no position at all. A layer read
    /// with any doubt is never refused from here; what it did count is still said.
    unsure: bool,
    /// The channels that split the rows, as written, for the message.
    splits: Vec<String>,
    /// The series holding one position, each with where it held one.
    short: Vec<Short>,
}

/// One series that held a single position, and where.
struct Short {
    /// Its values of the columns that split it; empty when nothing splits.
    series: String,
    /// The panel, when the plot has more than one.
    panel: Option<String>,
    /// The moments it held one at, by the strip's name, when the layer plays.
    moments: Vec<String>,
}

/// Whether a layer reads its rows two at a time, as the low and high ends of one
/// position: a `ribbon`, and a `line` or `step` drawing a pair transform's two
/// boundary curves.
fn reads_pairs(layer: &Layer) -> bool {
    layer.mark == Mark::Ribbon
        || (matches!(layer.mark, Mark::Line | Mark::Step)
            && layer.transforms.iter().any(|t| matches!(t,
                Transform::Range | Transform::Confidence | Transform::Deviation
                | Transform::SmoothBand | Transform::Bounds)))
}

impl SeriesTally {
    /// A tally for a layer whose series need two positions, or `None` for any other.
    ///
    /// The path family, read as a path. A layer cut by its own domain column has its
    /// refusal already, and the hierarchy, flow, network and cluster readings lay
    /// their rows out in ways of their own.
    pub(super) fn for_layer(spec: &PlotSpec, layer: &Layer) -> Option<Self> {
        let path = matches!(layer.mark,
            Mark::Line | Mark::Area | Mark::Step | Mark::Path | Mark::Ribbon);
        let laid_out = layer.transforms.iter().any(|t| matches!(t,
            Transform::Partition | Transform::Flow | Transform::Layout | Transform::Cluster));
        (path && !laid_out && !crate::legality::splits_its_domain(spec, layer)).then(Self::default)
    }

    /// Count the series of one cell: this layer's rows in one panel at one moment.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn count(
        &mut self, spec: &PlotSpec, layer: &Layer, df: &DataFrame,
        x_field: &str, y_field: &str, panel: Option<&str>, moment: Option<&str>,
    ) {
        if df.is_empty() {
            return;
        }
        // A violin is drawn along its slot by a routine of its own.
        if crate::legality::slot_density(spec, layer, Some(df)).is_some() {
            self.unsure = true;
            return;
        }
        // A row has a place when its number is finite, or when it names a category.
        let placed = |field: &str| -> Option<Vec<bool>> {
            if let Some(v) = df.float_col(field) {
                return Some(v.iter().map(|v| v.is_finite()).collect());
            }
            df.str_col(field).map(|s| vec![true; s.len()])
        };
        let (Some(xs), Some(ys)) = (placed(x_field), placed(y_field)) else {
            self.unsure = true;
            return;
        };
        // Every channel that splits, splits, as `marks::split_series` reads them: a
        // categorical `color`, `group` and a mapped `pattern`. A measured color is a
        // ramp along the stroke and splits nothing.
        let mut fields: Vec<&str> = Vec::new();
        for ch in [Channel::Color, Channel::Group, Channel::Pattern] {
            let Some(d) = layer.encodings.get(&ch) else { continue };
            if df.str_col(&d.field).is_none() {
                if ch == Channel::Color { continue }
                self.unsure = true;
                return;
            }
            let written = format!("`{}({})`", crate::legality::channel_name(&ch), d.field);
            if !self.splits.contains(&written) { self.splits.push(written) }
            if !fields.contains(&d.field.as_str()) { fields.push(&d.field) }
        }
        let cols: Vec<&Vec<String>> = fields.iter().filter_map(|f| df.str_col(f)).collect();

        // The rows of each series, in the order the table first shows each one.
        let n = xs.len().min(ys.len());
        let mut index: HashMap<String, usize> = HashMap::new();
        let mut series: Vec<(String, Vec<usize>)> = Vec::new();
        for i in 0..n {
            let key = cols.iter()
                .map(|c| c.get(i).map_or("", String::as_str))
                .collect::<Vec<_>>()
                .join(", ");
            match index.get(&key) {
                Some(&at) => series[at].1.push(i),
                None => {
                    index.insert(key.clone(), series.len());
                    series.push((key, vec![i]));
                }
            }
        }

        let ok = |i: usize| xs[i] && ys[i];
        let pairs = reads_pairs(layer);
        for (key, rows) in series {
            let positions = if pairs {
                rows.chunks_exact(2).filter(|p| ok(p[0]) && ok(p[1])).count()
            } else {
                rows.iter().filter(|&&i| ok(i)).count()
            };
            self.seen += 1;
            match positions {
                0 => self.unsure = true,
                1 => {
                    let panel = panel.map(str::to_string);
                    match self.short.iter_mut().find(|s| s.series == key && s.panel == panel) {
                        Some(s) => s.moments.extend(moment.map(str::to_string)),
                        None => self.short.push(Short {
                            series: key,
                            panel,
                            moments: moment.map(str::to_string).into_iter().collect(),
                        }),
                    }
                }
                _ => self.drawn += 1,
            }
        }
    }
}

impl Short {
    /// How the message names it: the series, then where, when either says anything.
    fn name(&self, plays: bool) -> String {
        let mut place: Vec<String> = Vec::new();
        if let Some(p) = &self.panel {
            place.push(format!("panel `{p}`"));
        }
        if plays {
            place.push(match self.moments.as_slice() {
                [one] => format!("frame `{one}`"),
                many => format!("{} frames", many.len()),
            });
        }
        match (self.series.is_empty(), place.is_empty()) {
            (false, true) => format!("`{}`", self.series),
            (false, false) => format!("`{}` ({})", self.series, place.join(", ")),
            (true, _) => place.join(", "),
        }
    }
}

/// The mark that draws a lone position of this layer, with the layer's own
/// transforms: `point`, or for a pair of ends, the whisker an `interval` draws.
fn alone(layer: &Layer) -> String {
    let mark = if reads_pairs(layer) { "interval" } else { "point" };
    std::iter::once(mark)
        .chain(layer.transforms.iter()
            .filter(|t| **t != Transform::Stack)
            .map(crate::legality::transform_name))
        .collect::<Vec<_>>()
        .join(" * ")
}

/// Whether another layer already draws this one's positions on their own: a
/// `point` or `text` on the same two position columns, reading them through the
/// same transforms. `line + point` is how a series of one is drawn on purpose,
/// and a note there would report a row the reader can see.
///
/// **Any table**, because the dots of a drawing are often kept apart from the
/// lines between them: the book's UpSet matrix joins each combination's members
/// with a `path` from one table and draws the members with a `point` from
/// another, and a combination of one member has no line by design.
fn shown_elsewhere(spec: &PlotSpec, li: usize) -> bool {
    let layer = &spec.layers[li];
    let at = |l: &Layer, c: Channel| spec.position_for(l, &c).map(|d| d.field.clone());
    // An offset moves a glyph and keeps its row; `stack` piles a dot plot or an
    // area, and either way the row is still there to see.
    let reads = |l: &Layer| -> Vec<Transform> {
        l.transforms.iter()
            .filter(|t| !matches!(t,
                Transform::Jitter | Transform::Dodge | Transform::Repel | Transform::Stack))
            .cloned()
            .collect()
    };
    spec.layers.iter().enumerate().any(|(i, other)| {
        i != li
            && matches!(other.mark, Mark::Point | Mark::Text)
            && at(other, Channel::X) == at(layer, Channel::X)
            && at(other, Channel::Y) == at(layer, Channel::Y)
            && reads(other) == reads(layer)
    })
}

/// What the tallies add up to: a refusal when no layer of the plot draws a thing,
/// and otherwise a note for each layer that left a series out.
///
/// **Refused only when the plot would be empty.** One short series among many is a
/// fact about the data, not a fault in the sentence: the rest are drawn and the one
/// left out is named, as a dropped row with a missing value is. A plot with nothing
/// drawn is the empty panel §12 forbids, the reason the domain refusals exist.
pub(super) fn one_position(spec: &PlotSpec, tallies: &[Option<SeriesTally>]) -> Vec<Diagnostic> {
    // The written splits that cut every layer, for the refusal: the channels each
    // tally saw, the facet, and `play` on a layer that plays.
    let written_splits = |li: usize, t: &SeriesTally| -> Vec<String> {
        let mut out = t.splits.clone();
        if let Some(f) = &spec.facet {
            out.extend(f.col.as_ref().map(|c| format!("`| facet({c})`")));
            out.extend(f.row.as_ref().map(|r| format!("`/ facet({r})`")));
        }
        out.extend(spec.layers[li].encodings.get(&Channel::Play)
            .map(|d| format!("`play({})`", d.field)));
        out
    };

    let empty = !tallies.is_empty() && tallies.iter().all(|t| {
        t.as_ref().is_some_and(|t| !t.unsure && t.seen > 0 && t.drawn == 0)
    });
    if empty {
        let mut marks: Vec<String> = Vec::new();
        let mut splits: Vec<String> = Vec::new();
        for (li, t) in tallies.iter().enumerate() {
            let Some(t) = t else { continue };
            let m = format!("`{}`", crate::legality::mark_name(&spec.layers[li].mark));
            if !marks.contains(&m) { marks.push(m) }
            for s in written_splits(li, t) {
                if !splits.contains(&s) { splits.push(s) }
            }
        }
        let (draws, how) = match marks.len() {
            1 => ("draws", "it has"),
            _ => ("draw", "they have"),
        };
        let use_alone = alone(&spec.layers[0]);
        let message = if splits.is_empty() {
            format!(
                "gog: {} {draws} through at least two positions, and here {how} one, so \
                 nothing would be drawn. Use `{use_alone}`, which draws each position on its own.",
                crate::legality::and_list(&marks),
            )
        } else {
            format!(
                "gog: {} {draws} each series through at least two positions, and split by \
                 {}, every series has one, so nothing would be drawn. Use `{use_alone}`, which \
                 draws each position on its own, or remove a split so each series has more rows.",
                crate::legality::and_list(&marks),
                crate::legality::and_list(&splits),
            )
        };
        return vec![Diagnostic { kind: crate::legality::DiagnosticKind::Illegal, message }];
    }

    let mut out = Vec::new();
    for (li, t) in tallies.iter().enumerate() {
        let Some(t) = t else { continue };
        if t.short.is_empty() || shown_elsewhere(spec, li) {
            continue;
        }
        let layer = &spec.layers[li];
        let m = crate::legality::mark_name(&layer.mark);
        let fix = format!("Add `+ {}` to draw a position that stands alone.", alone(layer));
        let plays = layer.encodings.contains_key(&Channel::Play);
        let names: Vec<String> = t.short.iter().map(|s| s.name(plays)).collect();
        let message = if names.iter().all(String::is_empty) {
            // One series, one panel, one moment: there is nothing to name but the layer.
            format!("gog: `{m}` draws through at least two positions, and here it has one, \
                     so it draws nothing. {fix}")
        } else {
            const SHOWN: usize = 3;
            let mut listed: Vec<String> = names.iter().take(SHOWN).cloned().collect();
            if names.len() > SHOWN {
                listed.push(format!("{} more", names.len() - SHOWN));
            }
            let (which, are) = match names.len() {
                1 => ("the series".to_string(), "is"),
                k => (format!("the {k} series"), "are"),
            };
            format!("gog: `{m}` draws each series through at least two positions, so \
                     {which} with one {are} left out: {}. {fix}",
                    crate::legality::and_list(&listed))
        };
        out.push(Diagnostic { kind: crate::legality::DiagnosticKind::Assumption, message });
    }
    out
}
