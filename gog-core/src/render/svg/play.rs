//! Play: which rows each moment draws, when each moment shows, and the strip
//! that names it.

use super::*;

/// One membership test a subset of the rows is defined by.
///
/// A plot's rows are divided twice before anything is drawn: across the page into
/// facet panels, and along the clock into `play` frames. Both divisions are the
/// same operation — keep the rows whose column equals this value — so they share
/// one type rather than each growing a filter path, which is what keeps the
/// statistics, the shared cut and the shared scale from having to know which
/// division they are inside.
///
/// Two variants because `play` accepts a numeric column where `facet` refuses one
/// (`data::frames_across` records why). A facet only ever produces `Str`.
#[derive(Clone, Copy)]
pub(super) enum Slice<'a> {
    Str(&'a str, &'a str),
    Float(&'a str, f64),
}

/// The filter that cuts one moment out of a layer's rows, or `None` for a layer that
/// does not bind `play`. The frame filter is per layer where a facet's are the
/// plot's: a layer without `play` is handed every row it ever had and so stands still
/// behind the ones that move (§8's nearest-wins rule, read on the clock).
pub(super) fn moment_filter<'a>(layer: &'a Layer, frame: Option<&'a crate::data::FrameLevel>) -> Option<Slice<'a>> {
    let (fr, def) = (frame?, layer.encodings.get(&Channel::Play)?);
    Some(match &fr.key {
        crate::data::FrameKey::Str(s) => Slice::Str(&def.field, s),
        crate::data::FrameKey::Float(v) => Slice::Float(&def.field, *v),
    })
}

/// The moments a played plot has, in order, given the tables its layers bind.
///
/// Split out of [`SvgRenderer::draw`] so that a caller assembling stills counts
/// the frames the same way the renderer cuts them. Two counts that could differ
/// is the whole failure mode here: one short, and the last moment is silently
/// missing from the file.
pub(crate) fn play_levels_of(
    spec: &PlotSpec,
    source_frames: &[&DataFrame],
) -> Vec<crate::data::FrameLevel> {
    spec.layers
        .iter()
        .find_map(|l| l.encodings.get(&Channel::Play))
        .map(|d| crate::data::frames_across(source_frames, &d.field))
        .unwrap_or_default()
}

/// When each moment of a played plot shows: its hold, in default holds
/// (`data::frame_holds`), and the seconds one default hold lasts at this `speed`.
#[derive(Clone, Copy)]
pub(super) struct Clock<'a> {
    pub(super) holds: &'a [f64],
    pub(super) pace: f64,
}

impl Clock<'_> {
    /// Moment `fi`'s start and hold, and the whole loop, all in seconds.
    ///
    /// Summed in default holds and scaled once, so an even sequence — every hold
    /// exactly 1.0 — computes `fi × pace` and `n × pace` exactly as it always did.
    pub(super) fn window(&self, fi: usize) -> (f64, f64, f64) {
        // Folded from `0.0` rather than `sum()`, whose empty sum is `-0.0` and
        // would write the first moment's `begin` as "-0.000".
        let before = self.holds[..fi].iter().fold(0.0, |a, h| a + h);
        let total = self.holds.iter().fold(0.0, |a, h| a + h);
        (before * self.pace, self.holds[fi] / total, total * self.pace)
    }
}

/// [`play_levels_of`] for a caller holding only the spec and its tables — the
/// door `plot::render_frames` comes in by. Resolves scope first, because `play`
/// is a channel and a layer that binds it after the mark binds it alone (§8).
pub(crate) fn play_levels(
    spec: &PlotSpec,
    data: &HashMap<String, DataFrame>,
) -> Vec<crate::data::FrameLevel> {
    let resolved = crate::legality::resolve_scopes(spec);
    let ctx = RenderContext::new(&resolved, data);
    let source_frames: Vec<&DataFrame> = resolved
        .layers
        .iter()
        .filter_map(|layer| ctx.resolve_data(&layer.data))
        .collect();
    play_levels_of(&resolved, &source_frames)
}

impl SvgRenderer {
    /// Open the group holding one moment's marks.
    ///
    /// Silent at one frame: a single-valued `play` column is a plot, not a
    /// sequence, and a group around it would cost every unplayed plot in the
    /// corpus its bytes to say nothing.
    pub(super) fn open_frame(&self, svg: &mut String, fi: usize, nframes: usize) {
        if nframes < 2 {
            return;
        }
        // The moment shown before any timing runs: the first, or the one a still
        // was asked for. One expression for both, because they are one idea —
        // which moment is on screen at the start — and a `still` that took a
        // second branch here could drift from what the sequence opens with.
        let shown = if fi == self.still.unwrap_or(0) { "inline" } else { "none" };
        writeln!(svg, r#"  <g display="{shown}">"#).unwrap();
    }

    /// Close it, and say when it shows.
    ///
    /// One `<animate>` per group, of constant size — the timing is carried by
    /// `begin` rather than by an N-entry `values` list per frame, so the markup
    /// grows with the number of frames and not with its square. Before its
    /// `begin` an element falls back to the `display` written on it, which is
    /// exactly the state [`open_frame`](Self::open_frame) set.
    pub(super) fn close_frame(&self, svg: &mut String, fi: usize, nframes: usize, clock: Clock<'_>) {
        if nframes < 2 {
            return;
        }
        // A still carries no timing at all. The caller holding the frames is the
        // clock now, and an `<animate>` left behind would switch the picture off
        // a fifth of a second after it was rasterized.
        if self.still.is_some() {
            writeln!(svg, "  </g>").unwrap();
            return;
        }
        // Shown from `begin` for its share of the loop, then hidden until the
        // loop comes round: a hold in proportion to the gap it stands for.
        let (begin, share, dur) = clock.window(fi);
        writeln!(svg,
            concat!(
                r#"    <animate attributeName="display" values="inline;none" "#,
                r#"keyTimes="0;{k:.6}" dur="{dur:.3}s" begin="{begin:.3}s" "#,
                r#"calcMode="discrete" repeatCount="indefinite"/>"#,
            ),
            k = share,
            dur = dur,
            begin = begin,
        ).unwrap();
        writeln!(svg, "  </g>").unwrap();
    }

    /// The band naming the moment on show.
    ///
    /// `play`'s guide, and the answer to what the channel *earns*. `x` earns an
    /// axis and `color` a legend; the two channels that earn nothing, `group` and
    /// `label`, earn nothing because neither adds a fact the reader has to decode.
    /// A frame does. Without it a reader watches points move with no way to say
    /// what they are moving *through*, which is the same plot as one with an
    /// unlabeled axis — so "earns: nothing" was the one line of the channel table
    /// this feature had to correct rather than fill in.
    ///
    /// It is deliberately the facet strip's strip: same band, same fill, same
    /// type, because it is the facet's guide read in time. The only difference is
    /// that it names the plot rather than a column of it, which is why it spans
    /// the panel area and sits above the facet's own names.
    pub(super) fn write_play_strip(
        &self,
        svg: &mut String,
        grid: &PanelGrid,
        levels: &[crate::data::FrameLevel],
        clock: Clock<'_>,
        theme: &ThemeSpec,
    ) {
        let Some(band) = grid.play_strip.as_ref() else { return };
        let strip_bg = theme.strip_or(STRIP_BG);
        let strip_fg = strip_ink(theme, &strip_bg);
        if levels.len() < 2 {
            return;
        }
        const STRIP_GAP: f64 = 4.0; // between the strip box and the panels
        let cap = estimate_cap_height(self.font_sm);
        let bh = crate::render::layout::STRIP_H - STRIP_GAP;
        let cx = band.x0 + band.w() / 2.0;
        let ty = band.y0 + bh / 2.0 + cap / 2.0;

        writeln!(svg,
            r##"  <g font-family="system-ui,sans-serif" font-size="{fs}" fill="{strip_fg}" text-anchor="middle">"##,
            fs = self.font_sm
        ).unwrap();
        // The box is drawn once, outside the moments: it is the same rectangle in
        // every frame, so swapping it would be a flicker carrying no information.
        writeln!(svg,
            r##"    <rect x="{x:.2}" y="{y:.2}" width="{w:.2}" height="{bh:.2}" fill="{strip_bg}"/>"##,
            x = band.x0, y = band.y0, w = band.w()
        ).unwrap();
        for (fi, level) in levels.iter().enumerate() {
            self.open_frame(svg, fi, levels.len());
            writeln!(svg,
                r#"    <text x="{cx:.2}" y="{ty:.2}">{v}</text>"#,
                v = esc(&level.label)
            ).unwrap();
            self.close_frame(svg, fi, levels.len(), clock);
        }
        writeln!(svg, "  </g>").unwrap();
    }
}
