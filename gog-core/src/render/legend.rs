//! Legends — the key that lets a reader decode a channel.
//!
//! One box per active channel. The shape of a box follows from what the channel
//! can depict rather than from taste: **show the whole scale if you can, sample
//! it if you cannot.** A continuous color is drawn as one gradient strip,
//! because a color ramp's entire range fits in a fixed space; `size` and
//! `opacity` are shown as three sampled examples, because there is no way to
//! draw a continuum of circles. Same rule, different shapes.
//!
//! A *set* value earns no box at all — `style(color = "red")` encodes nothing,
//! so there is nothing to decode. See `ir::StyleSpec`.

use std::collections::HashMap;
use std::fmt::Write;

use crate::data::{categories_across, DataFrame};
use crate::ir::{Channel, Mark};
use crate::legality::{Diagnostic, DiagnosticKind};
use crate::render::palette::{ramp_at, resolve_ramp, PALETTE_GOG};
use crate::render::pattern::{dash_for_index, fill_texture_for_index, pattern_dasharray, FillTexture};
use crate::render::shape::{shape_at_index, write_shape, ShapeKind};
use crate::render::text::{esc, estimate_cap_height, estimate_text_width};
use crate::render::ticks::auto_label;
use crate::render::{Layout, RenderContext};
use crate::render::encode::{opacity_at, radius_at, OPACITY_DEFAULT, SIZE_MAX_R};
use crate::scale::ChannelScale;

/// Format a value for a legend row.
///
/// Large magnitudes get a suffix. A log ramp over population runs from about
/// 200 thousand to 1.3 billion, and "1318683096" under a swatch is a number
/// nobody reads — it is measured, not read.
fn fmt_value(v: f64) -> String {
    let abs = v.abs();
    let trim = |x: f64, suffix: &str| {
        let s = format!("{x:.1}");
        format!("{}{suffix}", s.strip_suffix(".0").unwrap_or(&s))
    };
    if abs == 0.0           { "0".into() }
    else if abs >= 1e9      { trim(v / 1e9, "B") }
    else if abs >= 1e6      { trim(v / 1e6, "M") }
    else if abs >= 10_000.0 { trim(v / 1e3, "K") }
    else if abs >= 100.0    { format!("{v:.1}") }
    else                    { format!("{v:.2}") }
}

// ---------------------------------------------------------------------------
// Legend types — each active channel produces one LegendBox
// ---------------------------------------------------------------------------

pub(crate) const LEGEND_SWATCH_W: f64 = SIZE_MAX_R * 2.0; // swatch column width (covers all swatch kinds)
pub(crate) const LEGEND_SWATCH_GAP: f64 = 6.0;
pub(crate) const LEGEND_ROW_H: f64 = 20.0;
/// Row height inside a gradient legend — the strip spans the label rows, so
/// this is what decides how long the strip is. At the plain row height the
/// whole ramp got 40 px, a stub too short to see the gradient *as* a gradient,
/// which defeats the reason a strip is drawn instead of three swatches. The
/// labels still print at row centers, so they stay on the strip's ends and
/// middle whatever this value is.
pub(crate) const LEGEND_RAMP_ROW_H: f64 = LEGEND_ROW_H * 2.5;
pub(crate) const LEGEND_PLOT_GAP: f64 = 24.0;
pub(crate) const LEGEND_PADDING: f64 = 10.0;
pub(crate) const LEGEND_BOX_GAP: f64 = 12.0;

#[derive(Clone)]
pub(crate) enum LegendSwatch {
    ColorRect(String),
    ShapeMark(ShapeKind),
    SizeCircle(f64),  // pixel radius
    OpacityRect(f64), // fill-opacity 0..1
    /// A category's fill texture (spec §5's mapped `pattern` on a fill mark) — a
    /// small rect hatched in the category's color.
    PatternFill { texture: &'static str, color: String },
    /// A category's dash (mapped `pattern` on a stroke mark) — a short line.
    PatternStroke { dash: &'static str, color: String },
}

#[derive(Clone)]
pub(crate) struct LegendRow {
    pub(crate) label: String,
    pub(crate) swatch: LegendSwatch,
}

pub(crate) struct LegendBox {
    pub(crate) title: String,
    pub(crate) rows: Vec<LegendRow>,
    /// Ramp stops, when this legend decodes a continuous color.
    ///
    /// A legend should show the scale. Color is the only continuous channel
    /// whose *whole* range fits in a fixed space, so it is drawn as one strip;
    /// `size` and `opacity` have to be sampled, because there is no way to draw
    /// a continuum of circles. Different shapes, one rule — not an exception.
    ///
    /// The `rows` are still built, and still carry the min/mid/max labels and
    /// the widths the box is sized from; only the swatch column is replaced.
    pub(crate) gradient: Option<Vec<String>>,
}

impl LegendBox {
    /// **The title is measured against the row, not added to it.** It is drawn at
    /// `lx + LEGEND_PADDING`, flush with the swatch column rather than beside it,
    /// so folding its width in with the labels charged it for a swatch and a gap
    /// it never sits behind. Every legend in the book was wider than its contents
    /// by that much, and a title longer than its labels — which is the ordinary
    /// case, since a column name is longer than a number — paid the whole of it.
    ///
    /// Invisible at full size, where the key is a sixth of the plot either way. A
    /// **composed** cell is where it shows: the box does not shrink with the panel
    /// beside it, so on a page of four cubes the key was taking three fifths of
    /// its panel's width.
    pub(crate) fn width(&self, font_sm: f64, font_md: f64) -> f64 {
        let widest_row = self.rows.iter()
            .map(|r| LEGEND_SWATCH_W + LEGEND_SWATCH_GAP + estimate_text_width(&r.label, font_sm))
            .fold(0.0_f64, f64::max);
        let title = estimate_text_width(&self.title, font_md);
        LEGEND_PADDING + widest_row.max(title) + LEGEND_PADDING
    }

    pub(crate) fn height(&self, font_md: f64) -> f64 {
        let rows_h: f64 = self.rows.iter().map(|r| match r.swatch {
            LegendSwatch::SizeCircle(rad) => (rad * 2.0 + 6.0).max(LEGEND_ROW_H),
            _ if self.gradient.is_some() => LEGEND_RAMP_ROW_H,
            _ => LEGEND_ROW_H,
        }).sum();
        LEGEND_PADDING + estimate_cap_height(font_md) + 8.0 + rows_h + LEGEND_PADDING
    }
}


/// One row's label on a continuous legend: the value as the reader was given
/// it. For a temporal column that is a date — `fmt_value` would render epoch
/// seconds as "1.7B", a number that is true and useless. A legend row stands
/// alone, with no neighbors to borrow context from, so it gets the full
/// self-contained ISO form rather than an axis tick's `Mar 4`.
fn continuous_label(df: &DataFrame, field: &str, v: f64) -> String {
    match df.time_unit(field) {
        Some(unit) => crate::time::fmt_moment(v, unit),
        None => fmt_value(v),
    }
}

/// Collect one LegendBox per active channel (color → shape → size), in that order.
///
/// `eff` is the per-layer frame *after* transforms, parallel to `spec.layers`. Every
/// legend here reads the raw table, which is right for a binding that names a column
/// the user has — and wrong for the one channel whose column the engine invents. A
/// 2-D `bin` measures each cell by its count and puts that measurement on `color`, so
/// its legend can only be built from the transformed frame.
pub(crate) fn collect_legends(
    ctx: &RenderContext<'_>, color_map: &HashMap<String, String>, eff: &[DataFrame],
) -> Vec<LegendBox> {
    let mut boxes = Vec::new();

    // Color legend (categorical string column)
    'color: for layer in &ctx.spec.layers {
        let Some(def) = layer.encodings.get(&Channel::Color) else { continue };
        let Some(df)  = ctx.resolve_data(&layer.data)        else { continue };
        if df.str_col(&def.field).is_none() { continue }
        // If `pattern` maps the same column, its legend already shows each category's
        // hue *and* its texture — a complete key — so a separate color legend would
        // just repeat it. Suppress the duplicate (the redundant-encoding merge): the
        // pattern legend becomes the one key, which is exactly what a colorblind
        // reader needs.
        if ctx.spec.layers.iter().any(|l|
            l.encodings.get(&Channel::Pattern).is_some_and(|p| p.field == def.field))
        {
            break 'color;
        }
        let rows: Vec<LegendRow> = categories_across(&[df], &def.field).into_iter()
            .map(|label| {
                let color = color_map.get(label.as_str())
                    .cloned()
                    .unwrap_or_else(|| PALETTE_GOG[0].to_string());
                LegendRow { label, swatch: LegendSwatch::ColorRect(color) }
            })
            .collect();
        if !rows.is_empty() { boxes.push(LegendBox { title: auto_label(&def.field), rows, gradient: None }); break 'color; }
    }

    // Color legend (numeric column — a continuous strip, labeled min/mid/max)
    //
    // Drawn as the whole ramp rather than three sampled swatches: a legend
    // should show the scale, and color is the only continuous channel whose
    // entire range fits in a fixed space. `size` and `opacity` are sampled
    // because there is no way to draw a continuum of circles — same rule,
    // different shapes. Three swatches also under-describe a multi-hue ramp
    // like viridis, where the reader cannot guess what sits between the stops.
    'color_ramp: for (i, layer) in ctx.spec.layers.iter().enumerate() {
        // Only when no categorical color legend was produced above.
        if boxes.iter().any(|b| b.rows.iter().any(|r| matches!(r.swatch, LegendSwatch::ColorRect(_)))) {
            break 'color_ramp;
        }
        // A two-dimensional reading measures itself by a column no binding named —
        // a cell's count or density, a ring's level — so its key is the one that has
        // to look past the raw table: the column exists only downstream of the
        // transform. Every other legend keeps reading what the user actually bound,
        // which is what the raw table is right for.
        // Asked of `field_measure`, which follows the *geometry* rather than the mark:
        // rings are measured by the level they were cut at whether a `path` strokes
        // them or a `zone` fills them, cells by what was tallied or estimated in each.
        // Deriving it here instead is how a banded zone briefly drew no legend at all
        // — this looked up `density`, the banded frame publishes `level`, and a
        // missing column silently skipped the key. The panel then took the width the
        // legend would have used, so the two readings of one sentence did not even
        // line up on the page.
        let synthesized = crate::legality::field_measure(layer);
        let def = layer.encodings.get(&Channel::Color);
        let field = match (def, synthesized) {
            (Some(d), _) => d.field.as_str(),
            (None, Some(f)) => f,
            (None, None) => continue,
        };
        // **Which frame holds the numbers this key decodes**, and the rule is *is the
        // color column this layer's own measurement* — not *is its name synthesized*,
        // which is the same question only for the four transforms that invent one.
        // The five that reduce a column the reader named rewrite it in place, so the
        // raw table still has that column with every original row in it: reading the
        // domain there drew a heatmap of cell means beside a key spanning the raw
        // column's range, fills self-consistent under a legend that decoded them
        // wrongly. `color_is_the_measurement` covers both halves (spec §5).
        let src = if crate::legality::color_is_the_measurement(ctx.spec, layer) {
            eff.get(i)
        } else {
            ctx.resolve_data(&layer.data)
        };
        let Some(df)  = src                  else { continue };
        let Some(col) = df.float_col(field)  else { continue };
        let sc = ChannelScale::of(col, def);
        let ramp = resolve_ramp(&ctx.spec.palette);
        let stops: Vec<&str> = ramp.iter().map(String::as_str).collect();
        // Largest at the top, so the strip runs the way the axis does.
        //
        // The middle row is the value at *half way along the scale*, which on a
        // log ramp is the geometric mean — the arithmetic one would name a
        // color the strip does not paint there.
        let rows = [1.0, 0.5, 0.0].iter().map(|&f| LegendRow {
            label: continuous_label(df, field, sc.value_at(f)),
            swatch: LegendSwatch::ColorRect(ramp_at(&stops, f)),
        }).collect();
        boxes.push(LegendBox {
            title: auto_label(field),
            rows,
            gradient: Some(ramp.clone()),
        });
        break 'color_ramp;
    }

    // Shape legend (categorical string column)
    'shape: for layer in &ctx.spec.layers {
        let Some(def) = layer.encodings.get(&Channel::Shape) else { continue };
        let Some(df)  = ctx.resolve_data(&layer.data)        else { continue };
        if df.str_col(&def.field).is_none() { continue }
        let rows: Vec<LegendRow> = categories_across(&[df], &def.field).into_iter()
            .enumerate()
            .map(|(i, label)| LegendRow { label, swatch: LegendSwatch::ShapeMark(shape_at_index(i)) })
            .collect();
        if !rows.is_empty() { boxes.push(LegendBox { title: auto_label(&def.field), rows, gradient: None }); break 'shape; }
    }

    // Pattern legend (categorical) — the mapped `pattern` channel (spec §5),
    // `shape`'s twin one geometry class over: on a fill mark each category is a
    // texture swatch, on a stroke a dashed sample. Colored to match the mark — the
    // category's hue when `color` maps the same column (the redundant, colorblind-
    // safe encoding), else the default. This legend is what lets a texture-mapped
    // plot be read without relying on hue.
    'pattern: for layer in &ctx.spec.layers {
        let Some(def) = layer.encodings.get(&Channel::Pattern) else { continue };
        let Some(df)  = ctx.resolve_data(&layer.data)          else { continue };
        if df.str_col(&def.field).is_none() { continue }
        let stroke = matches!(layer.mark, Mark::Line | Mark::Step | Mark::Interval);
        let rows: Vec<LegendRow> = categories_across(&[df], &def.field).into_iter()
            .enumerate()
            .map(|(i, label)| {
                let color = color_map.get(&label).cloned().unwrap_or_else(|| PALETTE_GOG[0].to_string());
                let swatch = if stroke {
                    LegendSwatch::PatternStroke { dash: dash_for_index(i), color }
                } else {
                    LegendSwatch::PatternFill { texture: fill_texture_for_index(i), color }
                };
                LegendRow { label, swatch }
            })
            .collect();
        if !rows.is_empty() { boxes.push(LegendBox { title: auto_label(&def.field), rows, gradient: None }); break 'pattern; }
    }

    // Size legend (numeric column — show min / mid / max)
    'size: for layer in &ctx.spec.layers {
        let Some(def) = layer.encodings.get(&Channel::Size) else { continue };
        let Some(df)  = ctx.resolve_data(&layer.data)       else { continue };
        let Some(col) = df.float_col(&def.field)            else { continue };
        let sc = ChannelScale::of(col, Some(def));
        let rows = [0.0, 0.5, 1.0].iter().map(|&f| LegendRow {
            label:  continuous_label(df, &def.field, sc.value_at(f)),
            swatch: LegendSwatch::SizeCircle(radius_at(f)),
        }).collect();
        boxes.push(LegendBox { title: auto_label(&def.field), rows, gradient: None });
        break 'size;
    }

    // Opacity legend (numeric column — show min / mid / max), mirroring size.
    'opacity: for layer in &ctx.spec.layers {
        let Some(def) = layer.encodings.get(&Channel::Opacity) else { continue };
        let Some(df)  = ctx.resolve_data(&layer.data)          else { continue };
        let Some(col) = df.float_col(&def.field)               else { continue };
        let sc = ChannelScale::of(col, Some(def));
        let rows = [0.0, 0.5, 1.0].iter().map(|&f| LegendRow {
            label:  continuous_label(df, &def.field, sc.value_at(f)),
            swatch: LegendSwatch::OpacityRect(opacity_at(f)),
        }).collect();
        boxes.push(LegendBox { title: auto_label(&def.field), rows, gradient: None });
        break 'opacity;
    }

    boxes
}

/// Draw the legend panel to the right of the plot area.
///
/// A free function taking the font sizes rather than a method: the only thing
/// it ever wanted from the renderer was three numbers.
///
/// **A short panel squeezes the strip; it does not spill out of the box.** This
/// used to clamp the box's background to the room left (`.min(remaining)`) and
/// then draw the rows at full size regardless, so any plot shorter than a
/// legend's natural height — about 188px for a gradient — drew its strip and
/// its bottom label *outside the box*, and often outside the image. It was
/// reachable from `theme(height = )` on any plot with any legend, and from every
/// composed page with more than two rows, since the page canvas is fixed and the
/// cells divide it. Found while composing six ramps into one figure, which put
/// six of them on one page at 200px a row.
///
/// The gradient is the case that can give: a strip is a strip at any length, so
/// its rows shrink to the room available. Swatch rows cannot — a 12px swatch in
/// a 6px row is the same defect one level down — so when they do not fit, the
/// legend is **not drawn and says so**. A key drawn outside the picture is not a
/// key, and silently omitting one is what §12 forbids; a remark is the third
/// option both of those refuse.
/// `bottom` is the lowest y a legend may occupy, which is the **canvas**, not
/// the panel: `l` positions the stack (its top aligns with the panel's) but a
/// legend taller than the panel may run down into the margin the x axis labels
/// live beside, and always could. Measuring the room against `l.y1` instead
/// would drop legends that fit the image perfectly well.
pub(crate) fn write_legends(
    svg: &mut String, l: &Layout, boxes: &[LegendBox], fonts: (f64, f64),
    bottom: f64, remarks: &mut Vec<Diagnostic>,
) {
    let (font_sm, font_md) = fonts;
        let panel_w = boxes.iter()
            .map(|b| b.width(font_sm, font_md))
            .fold(0.0_f64, f64::max);

        let lx = l.x1 + LEGEND_PLOT_GAP;
        let mut cur_y = l.y0;
        let title_cap_h = estimate_cap_height(font_md);
        // Everything in a box that is not rows: two paddings, the title, and the
        // gap above the separator. Fixed — shrinking a title to fit a panel is
        // how a legend becomes unreadable while still technically being drawn.
        let overhead = LEGEND_PADDING + title_cap_h + 8.0 + LEGEND_PADDING;

        for lb in boxes {
            let remaining = bottom - cur_y;
            if remaining < 20.0 { break; }
            let natural = lb.height(font_md);
            let natural_rows = (natural - overhead).max(1.0);
            let available_rows = remaining - overhead;

            // How far the rows may be compressed. A row still has to hold its own
            // label, and for a gradient that is all it holds, so the floor is the
            // text itself plus enough air to keep two labels apart.
            let min_row = estimate_cap_height(font_sm) + 6.0;
            let n_rows = lb.rows.len().max(1) as f64;
            let squeezable = lb.gradient.is_some();
            let floor_rows = if squeezable { min_row * n_rows } else { natural_rows };

            if available_rows < floor_rows {
                remarks.push(Diagnostic {
                    kind: DiagnosticKind::Assumption,
                    message: format!(
                        "gog: the `{}` legend needs {:.0}px of height and there is room for \
                         {:.0}, so it was left out rather than drawn over the edge of the \
                         plot. Give the plot more room with `theme(height = )` — on a composed \
                         page that is a share of the page, so the other plots have to give \
                         some up.",
                        lb.title, floor_rows + overhead, remaining,
                    ),
                });
                continue;
            }

            // 1.0 whenever the legend fits, so nothing that fits today moves.
            let squeeze = if squeezable {
                (available_rows / natural_rows).min(1.0)
            } else {
                1.0
            };
            let ramp_row_h = LEGEND_RAMP_ROW_H * squeeze;
            let box_h = overhead + natural_rows * squeeze;

            // Box background + border
            writeln!(svg,
                r##"  <rect x="{lx:.2}" y="{cur_y:.2}" width="{panel_w:.2}" height="{box_h:.2}" fill="white" stroke="#d2d2da" stroke-width="1" rx="4"/>"##,
            ).unwrap();

            // Title
            let title_y = cur_y + LEGEND_PADDING + title_cap_h;
            writeln!(svg,
                r##"  <text x="{tx:.2}" y="{title_y:.2}" font-family="system-ui,sans-serif" font-size="{fs}" font-weight="600" fill="#28283a">{title}</text>"##,
                tx = lx + LEGEND_PADDING, fs = font_md, title = esc(&lb.title),
            ).unwrap();

            // Separator
            let sep_y = title_y + 5.0;
            writeln!(svg,
                r##"  <line x1="{lx:.2}" y1="{sep_y:.2}" x2="{x2:.2}" y2="{sep_y:.2}" stroke="#d2d2da" stroke-width="1"/>"##,
                x2 = lx + panel_w,
            ).unwrap();

            // Rows
            let mut row_y = sep_y + 6.0;
            let text_x = lx + LEGEND_PADDING + LEGEND_SWATCH_W + LEGEND_SWATCH_GAP;
            let swatch_cx = lx + LEGEND_PADDING + LEGEND_SWATCH_W / 2.0; // center of swatch column

            writeln!(svg,
                r##"  <g font-family="system-ui,sans-serif" font-size="{}" fill="#3c3c46">"##,
                font_sm
            ).unwrap();

            // A continuous color is drawn as one strip spanning all three
            // label rows, so the reader sees the scale itself rather than three
            // points on it. The labels below still print at their row centers,
            // which lines them up with the strip's ends and middle.
            if let Some(stops) = &lb.gradient {
                // Span center-of-first-row to center-of-last-row, so the strip's
                // ends line up with the labels that name them: the top of the
                // strip *is* the maximum, not half a row above it.
                let n = lb.rows.len().max(2) as f64;
                let strip_y = row_y + ramp_row_h / 2.0;
                let strip_h = ramp_row_h * (n - 1.0);
                let strip_w = LEGEND_SWATCH_W;
                let strip_x = swatch_cx - strip_w / 2.0;
                // The id must be unique per gradient *content*, not per box.
                // The book inlines many SVGs into one HTML document, and SVG
                // ids are document-global there: the first definition wins for
                // every reference on the page. The old geometry hash collided
                // the moment two plots shared a layout, and a viridis legend
                // upstream painted its colors into a white–navy strip
                // downstream. Hashing the stops instead means two ids only
                // collide when the gradients are identical — the one
                // collision that cannot mislead.
                let mut h: u64 = 0xcbf2_9ce4_8422_2325;
                for c in stops {
                    for b in c.bytes() {
                        h = (h ^ b as u64).wrapping_mul(0x0100_0000_01b3);
                    }
                    h = (h ^ 0xff).wrapping_mul(0x0100_0000_01b3);
                }
                let gid = format!("ramp{h:016x}");
                writeln!(svg, r#"    <defs><linearGradient id="{gid}" x1="0" y1="1" x2="0" y2="0">"#).unwrap();
                let last = stops.len().saturating_sub(1).max(1) as f64;
                for (i, c) in stops.iter().enumerate() {
                    writeln!(svg,
                        r#"      <stop offset="{o:.4}" stop-color="{c}"/>"#,
                        o = i as f64 / last
                    ).unwrap();
                }
                writeln!(svg, r#"    </linearGradient></defs>"#).unwrap();
                writeln!(svg,
                    r##"    <rect x="{strip_x:.2}" y="{strip_y:.2}" width="{strip_w:.2}" height="{strip_h:.2}" fill="url(#{gid})" fill-opacity="0.9" stroke="#d2d2da" stroke-width="0.5" rx="2"/>"##
                ).unwrap();
            }

            // Dedups any repeated (texture, color) tile across this box's swatches.
            let mut swatch_tex = FillTexture::new();
            for row in &lb.rows {
                // The strip already drew the swatch column for this box.
                if lb.gradient.is_some() {
                    let text_y = row_y + ramp_row_h / 2.0 + estimate_cap_height(font_sm) / 2.0;
                    writeln!(svg, r#"    <text x="{text_x:.2}" y="{text_y:.2}">{label}</text>"#,
                        label = esc(&row.label)).unwrap();
                    row_y += ramp_row_h;
                    continue;
                }
                let row_h = match row.swatch {
                    LegendSwatch::SizeCircle(r) => (r * 2.0 + 6.0).max(LEGEND_ROW_H),
                    _ => LEGEND_ROW_H,
                };
                let swatch_cy = row_y + row_h / 2.0;
                let text_y   = swatch_cy + estimate_cap_height(font_sm) / 2.0;

                match row.swatch {
                    LegendSwatch::ColorRect(ref color) => {
                        let s = 6.0;
                        writeln!(svg,
                            r#"    <rect x="{x:.2}" y="{y:.2}" width="{w:.2}" height="{w:.2}" fill="{color}" fill-opacity="0.82" rx="2"/>"#,
                            x = swatch_cx - s, y = swatch_cy - s, w = s * 2.0
                        ).unwrap();
                    }
                    LegendSwatch::ShapeMark(kind) => {
                        write_shape(svg, kind, swatch_cx, swatch_cy, 5.5, "#3c3c46", OPACITY_DEFAULT, None);
                    }
                    LegendSwatch::OpacityRect(o) => {
                        let s = 6.0;
                        writeln!(svg,
                            r##"    <rect x="{x:.2}" y="{y:.2}" width="{w:.2}" height="{w:.2}" fill="#3c3c46" fill-opacity="{o:.3}" rx="2"/>"##,
                            x = swatch_cx - s, y = swatch_cy - s, w = s * 2.0
                        ).unwrap();
                    }
                    LegendSwatch::SizeCircle(rad) => {
                        writeln!(svg,
                            r##"    <circle cx="{swatch_cx:.2}" cy="{swatch_cy:.2}" r="{rad:.2}" fill="#3c3c46" fill-opacity="0.60"/>"##,
                        ).unwrap();
                    }
                    LegendSwatch::PatternFill { texture, ref color } => {
                        // A hatched rect in the category's color — `solid` (index 0)
                        // draws a plain fill. The thin outline keeps a faint hatch
                        // legible in the small swatch.
                        let s = 6.0;
                        let fill = swatch_tex.fill(svg, Some(texture), color);
                        writeln!(svg,
                            r#"    <rect x="{x:.2}" y="{y:.2}" width="{w:.2}" height="{w:.2}" fill="{fill}" fill-opacity="0.82" stroke="{color}" stroke-width="0.6" rx="2"/>"#,
                            x = swatch_cx - s, y = swatch_cy - s, w = s * 2.0
                        ).unwrap();
                    }
                    LegendSwatch::PatternStroke { dash, ref color } => {
                        // A short line with the dash — round caps so `dotted` reads as
                        // dots, matching the stroke marks.
                        let dash_attr = pattern_dasharray(Some(dash));
                        let half = 8.0;
                        writeln!(svg,
                            r##"    <line x1="{:.2}" y1="{swatch_cy:.2}" x2="{:.2}" y2="{swatch_cy:.2}" stroke="{color}" stroke-width="2"{dash_attr} stroke-linecap="round"/>"##,
                            swatch_cx - half, swatch_cx + half
                        ).unwrap();
                    }
                }
                writeln!(svg, r#"    <text x="{text_x:.2}" y="{text_y:.2}">{label}</text>"#,
                    label = esc(&row.label)).unwrap();
                row_y += row_h;
            }
            writeln!(svg, "  </g>").unwrap();

            cur_y += box_h + LEGEND_BOX_GAP;
        }
    }
