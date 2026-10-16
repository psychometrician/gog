//! The brush on the picture: the frame a reader drags, and the dimmed pass for
//! the rows it leaves out.

use super::*;

impl SvgRenderer {
    // -----------------------------------------------------------------------
    // Play — the facet strip read in time, and the timing that swaps it
    //
    // The whole of the animation is three small writers, and their smallness is
    // the finding rather than an economy: a frame is a complete plot, so nothing
    // here interpolates, keys a datum, or knows what a mark is. It hides one
    // group and shows the next, which is the one operation that works identically
    // on all thirteen marks and in every coordinate space — Law 2 satisfied by
    // construction rather than by thirteen implementations agreeing.
    //
    // **Why SMIL and not a `<style>` block.** Two plots are inlined into one HTML
    // page all through this book, and CSS `@keyframes` are document-scoped: a
    // shared animation name would let the second plot's timing silently
    // reinterpret the first's. `<animate>` is a *child of the element it drives*,
    // so it needs no name, no id and no class, and two plots on a page cannot
    // reach each other. `clip_id` and `tile_id` exist because this crate has had
    // that collision before; this is the version of the problem that can simply
    // not be had.
    //
    // **Why the first frame's state is written statically.** Everything that
    // converts an SVG rather than animating it — `rsvg-convert`, which is how this
    // book's PDF is made — ignores `<animate>` and renders the attributes as
    // written. So the static `display` *is* the print fallback, and writing
    // `inline` on the first moment and `none` on the rest is what makes a printed
    // page show that moment with its strip, rather than every moment at once.
    // -----------------------------------------------------------------------

    // -----------------------------------------------------------------------
    // Selection — the marks drawn twice, once pushed back
    //
    // A brush is drawn as a **row partition**, which is the shape `play` and
    // `facet` already use: the rows outside the selection are drawn first inside
    // one dimmed group, then the rows inside it are drawn at full strength over
    // the top. Nothing per-element changes, so no mark writer learns that
    // selection exists, and the dim composes with whatever opacity each mark
    // resolved for itself.
    //
    // Everything here is silent when nothing is selected, on `open_frame`'s
    // discipline and for the same reason: a plot that names no brush, or names
    // one the reader has not moved, must be byte-for-byte the plot it was before
    // this code existed. That promise is what the book, the PDF and every parity
    // hash rest on. Spec §15.
    // -----------------------------------------------------------------------

    /// Where this panel is, and what its axes measure — the one fact a browser
    /// cannot work out for itself.
    ///
    /// A gesture arrives in pixels and a brush is written in data units, so
    /// something has to invert the scale. Doing it in JavaScript would be a
    /// second copy of `scale.rs` living in another language, which is the drift
    /// that cost this project its second renderer. Doing it with a second entry
    /// point into the engine would mean promoting `Layout` and `AxisFacts` out of
    /// the one door `plot.rs` keeps, and composed pages discard both. So the
    /// engine states the two numbers per axis and the browser does one affine
    /// division it can check against the rectangle it measured.
    ///
    /// An empty `<g>` rather than a `<rect>`, deliberately: the renderer's own
    /// tests fingerprint marks by counting rects with particular attributes, and
    /// a rect here would be counted as one. It carries no ink, so it cannot.
    ///
    /// **Silent unless a brush is declared**, including a brush the reader has
    /// not moved yet — the browser needs these before the first drag, and a plot
    /// that never mentions a brush must not pay a byte for one. **Or unless the
    /// drawing goes into a page that reads rows under the pointer**
    /// (`facts.pointer`), and only on a panel whose marks stand at their own
    /// values; such a frame says `data-gog-select="none"`, so no drag selects
    /// there. Neither is ever true of a file written to disk.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn write_brush_frame(&self, svg: &mut String, spec: &PlotSpec, l: &Layout,
                         xs: (f64, f64), ys: (f64, f64),
                         cat_x: Option<&Vec<String>>, cat_y: Option<&Vec<String>>,
                         x_field: &str, y_field: &str, plane: bool,
                         x_log: Option<f64>, y_log: Option<f64>,
                         facts: &PanelFacts<'_>) {
        // A packing has regions rather than coordinates, so there is no domain to
        // state and nothing for a gesture to invert. The dimming still works
        // there, because a predicate reads a column's values and never asks where
        // a row was drawn — only the *gesture* needs a plane.
        let brushed = !(spec.brush.is_empty() && spec.region.is_none());
        let pointed = !brushed && facts.pointer && facts.place.is_none();
        if !(brushed || pointed) || !plane {
            return;
        }
        // Named per axis rather than renamed after the fact. The log base below
        // is built with a `replace`, and the same trick applied here would have
        // emitted `data-cats` twice — once per axis, colliding — which is what it
        // did, so a drag on a categorical axis could never find its slots.
        let cats = |axis: &str, c: Option<&Vec<String>>| {
            c.map(|v| format!(" data-{axis}-cats=\"{}\"", v.iter()
                .map(|s| crate::render::text::esc(s)).collect::<Vec<_>>().join("|")))
                .unwrap_or_default()
        };
        // The column each axis measures travels with its domain. Which column an
        // axis reads is a *scope* question, and scope resolution is this engine's
        // job — a browser working it out from the spec would be a second copy of
        // `resolve_scopes` in another language.
        // **A log axis states its domain in log space**, because that is the
        // space positions are linear in — so a browser reading the two numbers
        // and interpolating between them gets a logarithm, not a value. The base
        // travels with the domain and says how to come back: `base^v`. Without
        // it a drag on a log axis produces a bound in the wrong units entirely,
        // and the engine then compares it against raw values.
        let base = |b: Option<f64>| b.map(|b| format!(" data-log=\"{b}\"")).unwrap_or_default();
        // Which slice of the table this panel holds. A faceted plot is one plot
        // over one table, so a browser walking the rows cannot tell which panel
        // a row belongs in: it answers a pointer in the Europe panel with an
        // African row, drawn at a position where Europe put nothing. With shared
        // scales the two coincide exactly, so the wrong answer looks like the
        // right one. These are the same two filters this panel's own frame was
        // built with, which is what keeps the attribute and the rows agreeing.
        let slice = |side: &str, pair: Option<(&str, &str)>| {
            pair.map(|(f, v)| format!(
                " data-facet-{side}-field=\"{}\" data-facet-{side}=\"{}\"",
                crate::render::text::esc(f), crate::render::text::esc(v)))
                .unwrap_or_default()
        };
        // Which moment is showing. Every moment of a played plot is in the
        // document at once and the clock chooses which one displays, so a
        // browser reading the table sees all of them and would answer with a row
        // from a frame nobody is looking at. These three say which frame that
        // is, given the time. The **keys** rather than the labels, because the
        // browser compares them against the column and a year is `1957` there
        // and "1957" on the strip.
        let play = facts.play.map(|(field, levels, clock)| {
            let keys = levels.iter().map(|lv| match &lv.key {
                crate::data::FrameKey::Str(s) => crate::render::text::esc(s),
                crate::data::FrameKey::Float(v) => format!("{v}"),
            }).collect::<Vec<_>>().join("|");
            // The same precision the animation's own `begin` and `dur` are
            // written at, or the frame the browser computes drifts from the
            // frame the reader sees wherever `speed` makes this repeat.
            let even = format!(concat!(r#" data-play-field="{f}" data-play-levels="{k}""#,
                                       r#" data-play-seconds="{s:.3}""#),
                               f = crate::render::text::esc(field), k = keys, s = clock.pace);
            // Frames held unevenly (a gap in the played column) are placed by their
            // own starts, not by one length: each start written exactly as that
            // frame's `<animate begin>` is, so the browser's clock and the
            // picture's cannot disagree about which moment is on show. Written only
            // when uneven, so an evenly played plot keeps its bytes.
            if clock.holds.iter().all(|&h| h == 1.0) {
                return even;
            }
            let begins = (0..levels.len()).map(|fi| format!("{:.3}", clock.window(fi).0))
                .collect::<Vec<_>>().join("|");
            let lap = clock.window(0).2;
            format!(r#"{even} data-play-begins="{begins}" data-play-loop="{lap:.3}""#)
        }).unwrap_or_default();
        // `row` is the promise that a value can be turned back into the place it
        // was drawn. Anything else names what broke it, so the page can say so
        // in a sentence rather than going quiet for no stated reason. A panel
        // carrying no such attribute at all — an old engine beside a new module
        // — reads as refused, which is the state a reader had before any of this
        // was built, and the safe way to be wrong.
        let place = format!(" data-gog-place=\"{}\"{}", facts.place.unwrap_or("row"),
                            if pointed { " data-gog-select=\"none\"" } else { "" });
        // **Every column a `bounds` pair lays along each axis**, which a brush on
        // either may move. Such an axis draws two columns at once, so it names no
        // single field, and a brush the page matched to no axis had every drag
        // dropped: a brushed `interval * bounds(lo, hi)` was right as first drawn
        // and no drag moved it. Which axis each pair lies along is the bindings'
        // answer (`legality::zone_orient`), as it is where the marks are drawn.
        let turned = crate::legality::zone_orient(cat_x.is_some(), cat_y.is_some())
            == crate::legality::Orient::Horizontal;
        let (mut along_x, mut along_y): (Vec<&str>, Vec<&str>) = (Vec::new(), Vec::new());
        for b in spec.layers.iter().filter_map(|l| l.bounds.as_ref()) {
            let (on_x, on_y) = if turned { (b.measure(), b.domain()) } else { (b.domain(), b.measure()) };
            if let Some((lo, hi)) = on_x { along_x.extend([lo, hi]); }
            if let Some((lo, hi)) = on_y { along_y.extend([lo, hi]); }
        }
        let fields = |axis: &str, cols: &[&str]| if cols.is_empty() { String::new() } else {
            format!(" data-{axis}-fields=\"{}\"", cols.iter()
                .map(|c| crate::render::text::esc(c)).collect::<Vec<_>>().join("|"))
        };
        writeln!(svg,
            concat!(r#"  <g data-gog-panel="{x0} {y0} {x1} {y1}" "#,
                    r#"data-x-field="{xn}"{xa} data-x="{xf} {xt}"{xc}{xl} "#,
                    r#"data-y-field="{yn}"{ya} data-y="{yf} {yt}"{yc}{yl}"#,
                    r#"{fc}{fr}{pp}{pl}/>"#),
            x0 = l.x0, y0 = l.y0, x1 = l.x1, y1 = l.y1,
            xn = crate::render::text::esc(x_field), xa = fields("x", &along_x), xf = xs.0, xt = xs.1,
            xc = cats("x", cat_x), xl = base(x_log).replace("data-log", "data-x-log"),
            yn = crate::render::text::esc(y_field), ya = fields("y", &along_y), yf = ys.0, yt = ys.1,
            yc = cats("y", cat_y), yl = base(y_log).replace("data-log", "data-y-log"),
            fc = slice("col", facts.facet_col), fr = slice("row", facts.facet_row),
            pp = play, pl = place,
        ).unwrap();
    }

    /// One pass when nothing is selected, two when something is — unselected
    /// first, so the selection paints over what it was taken from.
    pub(super) fn selection_passes(&self, spec: &PlotSpec) -> &'static [bool] {
        // Another brushed plot on this plot's table selects for it too: the page's
        // one selection per table (`Fit::selection_beside`).
        let selected = spec.brush.iter().any(|b| !b.is_resting())
            || spec.region.as_ref().is_some_and(|r| !r.is_resting())
            || !self.fit.selection_beside.is_empty();
        if selected {
            &[true, false]
        } else {
            &[false]
        }
    }

    /// The rows this layer contributes to this pass.
    ///
    /// `None` means it contributes none. A layer the selection cannot reach — a
    /// summarized one, a mark whose rows are vertices rather than
    /// elements — is drawn **once, whole, at full strength** rather than dimmed
    /// or drawn twice, which is what the Assumption in `legality::check_brush`
    /// tells the reader is happening.
    pub(super) fn pass_rows<'a>(
        &self,
        spec: &PlotSpec,
        layer: &Layer,
        df: &'a DataFrame,
        dim: bool,
    ) -> Option<std::borrow::Cow<'a, DataFrame>> {
        match self.pass_keep(spec, layer, df, dim)? {
            None => Some(std::borrow::Cow::Borrowed(df)),
            Some(side) => Some(std::borrow::Cow::Owned(df.keep_rows(&side))),
        }
    }

    /// Which of this share's rows the pass draws, as [`pass_rows`](Self::pass_rows)
    /// decides it: `None` for none of them, `Some(None)` for every one, and
    /// `Some(Some(mask))` for one side of the selection.
    ///
    /// The mask rather than the rows, for a writer that has to measure the whole
    /// share before it draws part of it. A `bar` takes its thickness from the
    /// smallest gap between neighbors and its dodge from the groups in its slot,
    /// so measured on half the rows, the selected and unselected bars came out at
    /// different widths and that is why a brushed bar was refused until 2026-10-09.
    pub(super) fn pass_keep(
        &self,
        spec: &PlotSpec,
        layer: &Layer,
        df: &DataFrame,
        dim: bool,
    ) -> Option<Option<Vec<bool>>> {
        // A flow's slots and their names stand one row per place, and only a
        // bound naming their own place can select one (`flow_slot_keeps`). Its
        // bands carry every stage's value on every row, so they take the plain
        // test, with a bound on the flow's `name` read across all of their stages
        // (`flow_band_keeps`).
        let flows = layer.transforms.contains(&crate::ir::Transform::Flow);
        let keeps = match (flows, layer.mark == Mark::Ribbon) {
            (true, false) => crate::legality::flow_slot_keeps(spec, layer, df),
            (true, true) => crate::legality::flow_band_keeps(spec, layer, df),
            (false, _) => crate::legality::brush_keeps(spec, df),
        };
        // The other brushed plots on the page that read this plot's table, tested
        // on this layer's rows when the layer reads that table too.
        let own_table = layer.data.as_ref().or(spec.data.as_ref()) == spec.data.as_ref();
        let keeps = match self.fit.selection_beside.is_empty() || !own_table {
            true => keeps,
            false => crate::legality::page_keeps(spec, df, keeps, &self.fit.selection_beside),
        };
        let Some(keep) = keeps else {
            // Nothing selected: one pass, the whole frame, the resting state.
            return (!dim).then_some(None);
        };
        let reachable = crate::legality::layer_takes_selection(layer)
            && crate::legality::layer_answers_selection(layer);
        if !reachable {
            return (!dim).then_some(None);
        }
        let side: Vec<bool> = if dim { keep.iter().map(|k| !k).collect() } else { keep };
        Some(Some(side))
    }

    /// Open the group that pushes the unselected rows back.
    pub(super) fn open_pass(&self, svg: &mut String, dim: bool) {
        if dim {
            writeln!(svg, r#"  <g opacity="{:.3}">"#, crate::render::encode::SELECTION_DIM).unwrap();
        }
    }

    pub(super) fn close_pass(&self, svg: &mut String, dim: bool) {
        if dim {
            writeln!(svg, "  </g>").unwrap();
        }
    }
}
