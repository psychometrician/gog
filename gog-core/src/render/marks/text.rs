//! The `text` mark — a string glyph (the `label` channel) at each (x, y).
use std::collections::HashMap;
use std::fmt::Write;
use crate::data::DataFrame;
use crate::ir::{Channel, Layer};
use crate::legality::{Diagnostic, DiagnosticKind};
use crate::render::nest::Nest;
use crate::render::polar::Polar;
use crate::render::svg::{fmt_label_num, SvgRenderer, TEXT_FILL};
use crate::render::text::{esc, estimate_cap_height, estimate_text_width};
use crate::render::Layout;

impl SvgRenderer {
    // -----------------------------------------------------------------------
    // Mark: text
    // -----------------------------------------------------------------------

    /// A glyph mark whose glyph is a *string*: at each (x, y) it draws the value
    /// of the `label` column as text. `point`'s sibling — same per-row placement,
    /// but the glyph is data (the `label` channel) rather than one of the five
    /// shapes. `label` is the mark's minimum syllable, so `check` guarantees the
    /// encoding is present before this runs.
    ///
    /// Color resolves as a point's categorical color does: a set color wins,
    /// else the group's palette hue. A string label is drawn as-is; a numeric one
    /// is formatted. The baseline drops half a cap-height so the glyph sits
    /// centered on its point rather than resting its feet there.
    pub(crate) fn write_text(
        &self, svg: &mut String, layer: &Layer, df: &DataFrame,
        l: &Layout, xs: (f64, f64), ys: (f64, f64),
        x_field: &str, y_field: &str,
        cat_x: Option<&[String]>, cat_y: Option<&[String]>,
        color_map: &HashMap<String, String>,
        clip: &str,
        // Polar: the label sits at an angle and a radius. `style(nudge = )` still
        // moves it in page directions ("up" is up the page, not outward from the
        // center) — a nudge is a constant offset on the finished glyph, and the
        // page is where the reader sees the collision it exists to fix.
        polar: Option<&Polar>,
        // Nest: when `Some`, the label names a **region** rather than sitting at a
        // coordinate, and the two positions stop being positions — `y` is the
        // measure that sizes the region and `x`, if bound at all, is the outer
        // partition. Consulted once above the row loop, as `bar`'s is and for the
        // same reason: a packing is a property of all the rows together.
        nest: Option<&Nest>,
        // Where a label that could not be drawn is reported. A packing of many
        // shares has more regions than legible ones, so leaving the unfitted ones
        // out in silence would let a reader take the labeled cells for all of them
        // (§12). Empty in every other space, where a label always draws.
        remarks: &mut Vec<Diagnostic>,
    ) {
        // The strings: the label column — a string drawn as-is, a number
        // formatted (the "value on each point" case). Its presence is the mark's
        // minimum syllable, guaranteed by legality.
        let Some(label_field) = layer.encodings.get(&Channel::Label).map(|c| c.field.as_str()) else { return };
        let owned_labels: Vec<String>;
        let labels: &[String] = if let Some(s) = df.str_col(label_field) {
            s
        } else if let Some(nums) = df.float_col(label_field) {
            owned_labels = nums.iter().map(|v| fmt_label_num(*v)).collect();
            &owned_labels
        } else { return };

        // **The packed reading comes first, because it does not consult the axes
        // at all.** Everything below this branch is written in terms of a place on
        // two scales, and a packing has neither (spec §15) — so rather than
        // threading a third meaning through the position resolution, the region
        // case answers itself and returns.
        if let Some(nst) = nest {
            self.write_text_nest(svg, layer, df, x_field, y_field, cat_x, labels,
                                 color_map, clip, nst, remarks);
            return;
        }

        // Position: the one resolution every mark shares — `text` is `point`'s
        // sibling and places its glyph the same way, so no per-mark exception.
        let Some(x_vals) = super::positions(df, x_field, cat_x) else { return };
        let Some(y_vals) = super::positions(df, y_field, cat_y) else { return };

        // Color: a set color wins; else the group's palette hue (category →
        // color via the shared map). A numeric ramp on text is not drawn yet —
        // legality renders `color` as discrete here, so there is no ramp path.
        let st = &layer.style;
        let set_color = st.color.as_deref().map(esc);
        let group_vals = layer.encodings.get(&Channel::Color).and_then(|c| df.str_col(&c.field));

        let fs = st.size.unwrap_or(self.font_md);
        let opacity = st.opacity.unwrap_or(1.0);
        let dy = estimate_cap_height(fs) / 2.0; // center the glyph on its point

        // A nudge moves the label off its point, so a superposed dot is not
        // covered. The distance is derived from the font size — enough to clear a
        // default dot — never a parameter. A vertical nudge keeps the centered
        // anchor and shifts the baseline; a horizontal one anchors the glyph's
        // near edge to the point and shifts sideways. `check_nudge` has already
        // rejected any direction but these four, so `_` is the no-nudge default.
        let (anchor, ndx, ndy) = match st.nudge.as_deref() {
            Some("up")    => ("middle", 0.0, -fs),
            Some("down")  => ("middle", 0.0,  fs),
            Some("left")  => ("end",   -fs * 0.6, 0.0),
            Some("right") => ("start",  fs * 0.6, 0.0),
            _             => ("middle", 0.0, 0.0),
        };

        let n = x_vals.len().min(y_vals.len()).min(labels.len());
        writeln!(svg,
            r##"  <g clip-path="url(#{clip})" font-family="system-ui,sans-serif" font-size="{fs}" text-anchor="{anchor}">"##
        ).unwrap();
        for i in 0..n {
            if !(x_vals[i].is_finite() && y_vals[i].is_finite()) {
                continue;
            }
            let fill: String = if let Some(sc) = &set_color {
                sc.clone()
            } else if let Some(gv) = group_vals {
                gv.get(i).and_then(|g| color_map.get(g).cloned()).unwrap_or_else(|| TEXT_FILL.to_string())
            } else {
                TEXT_FILL.to_string()
            };
            let (bx, by) = super::place(l, polar, x_vals[i], y_vals[i], xs, ys);
            let px = bx + ndx;
            let py = by + dy + ndy;
            writeln!(svg,
                r#"    <text x="{px:.2}" y="{py:.2}" fill="{fill}" fill-opacity="{opacity:.3}">{}</text>"#,
                esc(&labels[i])
            ).unwrap();
        }
        writeln!(svg, "  </g>").unwrap();
    }

    // -----------------------------------------------------------------------
    // Mark: text, in `nest` — the name inside its own region
    // -----------------------------------------------------------------------

    /// A label at the centroid of the region its row was packed into — what makes
    /// a treemap readable once the split is too wide for a legend to decode (spec
    /// §15).
    ///
    /// **The cells come from the space, not from here.** `Nest::regions` is the
    /// one answer to *which rectangle does this row get*, and `bar` asks it the
    /// same question with the same two columns, so a label cannot land in a cell
    /// its bar did not draw. That is the whole reason the packing moved down into
    /// `nest.rs`: two marks computing it separately is two marks that can disagree,
    /// and a name sitting a pixel outside its own rectangle is worse than no name,
    /// because nothing on the page says which region it belongs to.
    ///
    /// **A label that does not fit is not drawn, and is counted.** A packing puts
    /// many shares on one panel and most of them are smaller than a word; printing
    /// the ones that fit and saying nothing about the rest would let a reader take
    /// the labeled cells for all of them, which is §12's silent drop wearing a
    /// typographic excuse. So the layer reports its tally, once, as an Assumption:
    /// the plot drew, and the reader is told what is missing from it. This is the
    /// rule `repel` already carries for the same failure in the plane (spec §5).
    #[allow(clippy::too_many_arguments)]
    fn write_text_nest(
        &self, svg: &mut String, layer: &Layer, df: &DataFrame,
        x_field: &str, y_field: &str, cat_x: Option<&[String]>,
        labels: &[String],
        color_map: &HashMap<String, String>,
        clip: &str,
        nest: &Nest,
        remarks: &mut Vec<Diagnostic>,
    ) {
        // The measure is read raw, exactly as the bar's is: this space has no
        // position to map through, and the share a region reports is arithmetic on
        // the values themselves (which is why `check_nest` refuses a log scale).
        let Some(weights) = df.float_col(y_field) else { return };
        // The outer partition, when one is bound. A one-level packing has a single
        // slot and every row stands in it — the same degenerate reading `bar` takes,
        // and what Law 7's third relaxation exists to allow (`legality.rs`).
        let slots = if x_field.is_empty() {
            std::borrow::Cow::Owned(vec![0.0; weights.len()])
        } else {
            let Some(p) = super::positions(df, x_field, cat_x) else { return };
            p
        };

        let n = slots.len().min(weights.len()).min(labels.len());
        if n == 0 { return; }
        let (cells, _) = nest.regions(&slots[..n], &weights[..n]);

        let st = &layer.style;
        let set_color = st.color.as_deref().map(esc);
        let group_vals = layer.encodings.get(&Channel::Color).and_then(|c| df.str_col(&c.field));
        let fs = st.size.unwrap_or(self.font_md);
        let opacity = st.opacity.unwrap_or(1.0);
        let dy = estimate_cap_height(fs) / 2.0;

        // `style(nudge = )` is not read here, and the anchor is always `middle`. A
        // nudge steps a label off the point it would otherwise cover, and a region
        // has no dot underneath to clear — so honoring one would shove the name at
        // its own border for no reason. It is **refused** in `check_nest` rather
        // than ignored here, which is what keeps this line from being the
        // accept-and-drop §12 forbids: nothing reaches this function carrying one.
        writeln!(svg,
            r##"  <g clip-path="url(#{clip})" font-family="system-ui,sans-serif" font-size="{fs}" text-anchor="middle">"##
        ).unwrap();
        let mut unfitted = 0usize;
        for i in 0..n {
            let c = cells[i];
            let label = &labels[i];
            // Does the name fit the rectangle that carries it? Width against the
            // ink the string will actually take, height against the cap height —
            // the two numbers the glyph is placed by, asked of the region rather
            // than of the panel. The one-pixel margin keeps a label off its own
            // border, which is where the reader looks to find the region's edge.
            let w = estimate_text_width(label, fs);
            if !(c.w >= w + 2.0 && c.h >= estimate_cap_height(fs) + 2.0) {
                // A region with no area at all is not an unfitted label — it is a
                // share too small to have a region, which the bar does not draw
                // either. Counting it would report a name the plot never had room
                // to consider.
                if c.w >= 0.5 && c.h >= 0.5 { unfitted += 1; }
                continue;
            }
            let fill: String = if let Some(sc) = &set_color {
                sc.clone()
            } else if let Some(gv) = group_vals {
                gv.get(i).and_then(|g| color_map.get(g).cloned()).unwrap_or_else(|| TEXT_FILL.to_string())
            } else {
                TEXT_FILL.to_string()
            };
            writeln!(svg,
                r#"    <text x="{:.2}" y="{:.2}" fill="{fill}" fill-opacity="{opacity:.3}">{}</text>"#,
                c.x + c.w / 2.0, c.y + c.h / 2.0 + dy, esc(label)
            ).unwrap();
        }
        writeln!(svg, "  </g>").unwrap();

        if unfitted > 0 {
            remarks.push(Diagnostic {
                kind: DiagnosticKind::Assumption,
                message: format!(
                    "gog: {unfitted} of {n} labels are wider than the region they name, and \
                     were left out — the packing drew every share, the names are what is \
                     missing. Fewer categories, a larger plot (`theme(width =, height =)`) or \
                     a smaller `style(size = )` fits more of them in."
                ),
            });
        }
    }
}
